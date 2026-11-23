use crate::exec::{Exit, Usage, poll_interval};
use crate::{CrashReason, Limits};
use std::io;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command};
use std::sync::Once;
use std::time::Instant;
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
use windows_sys::Win32::System::Diagnostics::Debug::{SEM_FAILCRITICALERRORS, SEM_NOGPFAULTERRORBOX, SEM_NOOPENFILEERRORBOX, SetErrorMode};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOB_OBJECT_LIMIT_PROCESS_TIME, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
};
use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, GetProcessTimes};

pub fn limit(command: &mut Command, _limits: Limits) {
    static ERROR_MODE: Once = Once::new();
    ERROR_MODE.call_once(|| unsafe {
        SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX | SEM_NOOPENFILEERRORBOX);
    });
    command.creation_flags(CREATE_NO_WINDOW);
}

pub const fn is_out_of_memory(_err: &io::Error) -> bool {
    false
}

struct Job(HANDLE);

impl Job {
    fn new(program: HANDLE, limits: Limits) -> Option<Self> {
        let job = Self(unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) });
        if job.0.is_null() {
            return None;
        }

        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_PROCESS_TIME;
        info.BasicLimitInformation.PerProcessUserTimeLimit = (limits.cpu_seconds().saturating_add(5).saturating_mul(10_000_000)).min(i64::MAX as u64) as i64;
        if let Some(cap) = limits.memory_cap_bytes() {
            info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_PROCESS_MEMORY;
            info.ProcessMemoryLimit = usize::try_from(cap).unwrap_or(usize::MAX);
        }
        unsafe {
            SetInformationJobObject(job.0, JobObjectExtendedLimitInformation, (&raw const info).cast(), size_of_val(&info) as u32);
            AssignProcessToJobObject(job.0, program);
        }
        Some(job)
    }

    fn peak_memory_kb(&self) -> u64 {
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        let ok = unsafe { QueryInformationJobObject(self.0, JobObjectExtendedLimitInformation, (&raw mut info).cast(), size_of_val(&info) as u32, std::ptr::null_mut()) };
        if ok == 0 { 0 } else { info.PeakProcessMemoryUsed as u64 / 1024 }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn cpu_time_ms(process: HANDLE) -> u64 {
    let (mut creation, mut exit, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    let ok = unsafe { GetProcessTimes(process, &raw mut creation, &raw mut exit, &raw mut kernel, &raw mut user) };
    let ticks = |time: FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
    if ok == 0 { 0 } else { (ticks(kernel) + ticks(user)) / 10_000 }
}

pub fn wait(program: &mut Child, limits: Limits, deadline: Instant) -> io::Result<Usage> {
    let process = program.as_raw_handle();
    let job = Job::new(process, limits);
    let cpu_limit_ms = limits.cpu_seconds().saturating_mul(1000);
    let start = Instant::now();
    let exit = loop {
        if let Some(status) = program.try_wait()? {
            break match status.code() {
                Some(0) => Exit::Success,
                code => Exit::Failure(CrashReason::ExitCode(code.unwrap_or(-1))),
            };
        }
        if cpu_time_ms(process) > cpu_limit_ms || Instant::now() >= deadline {
            let _ = program.kill();
            program.wait()?;
            break Exit::OutOfTime;
        }
        std::thread::sleep(poll_interval(start.elapsed()));
    };
    Ok(Usage {
        exit,
        cpu_time_ms: cpu_time_ms(process),
        peak_memory_kb: job.as_ref().map_or(0, Job::peak_memory_kb),
    })
}
