use crate::exec::{Exit, Usage, poll_interval};
use crate::{Limits, RunResult};
use nix::errno::Errno;
use nix::sys::resource::{Resource, getrlimit, setrlimit};
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use std::io;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Child, Command, ExitStatus};
use std::time::Instant;

pub fn limit(command: &mut Command, limits: Limits) {
    let cpu_seconds = limits.cpu_seconds();
    let memory_cap = if cfg!(target_os = "linux") { limits.memory_cap_bytes() } else { None };
    #[cfg(target_os = "linux")]
    let parent = nix::unistd::getpid();

    let raise = |resource, cap: Option<u64>| {
        if let Ok((soft, hard)) = getrlimit(resource) {
            let wanted = cap.map_or(hard, |cap| cap.min(hard));
            if soft != wanted {
                let _ = setrlimit(resource, wanted, hard);
            }
        }
    };
    // Only async-signal-safe calls between fork and exec.
    unsafe {
        command.pre_exec(move || {
            #[cfg(target_os = "linux")]
            {
                let _ = nix::sys::prctl::set_pdeathsig(Signal::SIGKILL);
                if nix::unistd::getppid() != parent {
                    return Err(Errno::ESRCH.into());
                }
            }
            let _ = setrlimit(Resource::RLIMIT_CPU, cpu_seconds, cpu_seconds + 5);
            raise(Resource::RLIMIT_STACK, None);
            raise(Resource::RLIMIT_AS, memory_cap);
            Ok(())
        });
    }
}

pub fn is_out_of_memory(err: &io::Error) -> bool {
    err.raw_os_error() == Some(Errno::ENOMEM as i32)
}

#[allow(clippy::needless_pass_by_ref_mut, reason = "the Windows version needs it")]
pub fn wait(program: &mut Child, _limits: Limits, deadline: Instant) -> io::Result<Usage> {
    let pid = program.id() as libc::pid_t;
    let start = Instant::now();
    loop {
        if let Some(usage) = wait4(pid, libc::WNOHANG)? {
            return Ok(usage);
        }
        if Instant::now() >= deadline {
            let _ = kill(Pid::from_raw(pid), Signal::SIGKILL);
            return wait4(pid, 0)?.ok_or_else(|| io::Error::other("the program was killed but did not end"));
        }
        std::thread::sleep(poll_interval(start.elapsed()));
    }
}

fn wait4(pid: libc::pid_t, options: libc::c_int) -> io::Result<Option<Usage>> {
    let mut status = 0;
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    loop {
        match unsafe { libc::wait4(pid, &raw mut status, options, &raw mut usage) } {
            0 => return Ok(None),
            -1 if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted => {}
            -1 => return Err(io::Error::last_os_error()),
            _ => break,
        }
    }

    let status = ExitStatus::from_raw(status);
    let exit = match (status.code(), status.signal()) {
        (Some(0), _) => Exit::Success,
        (_, Some(signal)) if signal == Signal::SIGXCPU as i32 || signal == Signal::SIGKILL as i32 => Exit::OutOfTime,
        (_, Some(signal)) => Exit::Failure(RunResult::Signal(signal)),
        (Some(code), None) => Exit::Failure(RunResult::ExitCode(code)),
        (None, None) => Exit::Failure(RunResult::ExitCode(-1)),
    };
    let ms = |time: libc::timeval| time.tv_sec as u64 * 1000 + time.tv_usec as u64 / 1000;
    let peak_memory = usage.ru_maxrss as u64;
    Ok(Some(Usage {
        exit,
        cpu_time_ms: ms(usage.ru_utime) + ms(usage.ru_stime),
        // ru_maxrss is in bytes on macOS and in kilobytes elsewhere.
        peak_memory_kb: if cfg!(target_os = "macos") { peak_memory / 1024 } else { peak_memory },
    }))
}
