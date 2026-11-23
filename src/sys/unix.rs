use crate::exec::{Exit, Usage, poll_interval};
use crate::{CrashReason, Limits};
use std::io;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Child, Command, ExitStatus};
use std::time::Instant;

pub fn limit(command: &mut Command, limits: Limits) {
    let cpu_seconds = limits.cpu_seconds() as libc::rlim_t;
    let memory_cap = if cfg!(target_os = "linux") { limits.memory_cap_bytes() } else { None };
    #[cfg(target_os = "linux")]
    let parent = std::process::id() as libc::pid_t;

    unsafe {
        command.pre_exec(move || {
            #[cfg(target_os = "linux")]
            {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL as libc::c_ulong);
                if libc::getppid() != parent {
                    return Err(io::Error::from_raw_os_error(libc::ESRCH));
                }
            }

            let cpu = libc::rlimit {
                rlim_cur: cpu_seconds,
                rlim_max: cpu_seconds + 5,
            };
            libc::setrlimit(libc::RLIMIT_CPU, &raw const cpu);

            let raise = |resource, cap: Option<u64>| {
                let mut limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
                if libc::getrlimit(resource, &raw mut limit) == 0 {
                    let wanted = cap.map_or(limit.rlim_max, |cap| (cap as libc::rlim_t).min(limit.rlim_max));
                    if limit.rlim_cur != wanted {
                        limit.rlim_cur = wanted;
                        libc::setrlimit(resource, &raw const limit);
                    }
                }
            };
            raise(libc::RLIMIT_STACK, None);
            raise(libc::RLIMIT_AS, memory_cap);
            Ok(())
        });
    }
}

pub fn is_out_of_memory(err: &io::Error) -> bool {
    err.raw_os_error() == Some(libc::ENOMEM)
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
            unsafe {
                libc::kill(pid, libc::SIGKILL);
            }
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
        (_, Some(libc::SIGXCPU | libc::SIGKILL)) => Exit::OutOfTime,
        (_, Some(signal)) => Exit::Failure(CrashReason::Signal(signal)),
        (Some(code), None) => Exit::Failure(CrashReason::ExitCode(code)),
        (None, None) => Exit::Failure(CrashReason::ExitCode(-1)),
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
