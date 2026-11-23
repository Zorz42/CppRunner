use crate::sys;
use crate::{CrashReason, Error, Limits, Result, RunResult};
use log::{debug, trace};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Exit {
    Success,
    Failure(CrashReason),
    OutOfTime,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Usage {
    pub exit: Exit,
    pub cpu_time_ms: u64,
    pub peak_memory_kb: u64,
}

pub fn poll_interval(elapsed: Duration) -> Duration {
    (elapsed / 8).clamp(Duration::from_micros(50), Duration::from_millis(2))
}

fn read_output<R: Read>(stdout: R, max_bytes: u64) -> std::io::Result<Option<Vec<u8>>> {
    let mut output = Vec::new();
    stdout.take(max_bytes.saturating_add(1)).read_to_end(&mut output)?;
    Ok((output.len() as u64 <= max_bytes).then_some(output))
}

pub fn run_executable(executable: &Path, input: &str, limits: Limits) -> Result<RunResult> {
    let mut command = Command::new(executable);
    command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    sys::limit(&mut command, limits);
    trace!("Running command: {command:?}");
    let mut program = match command.spawn() {
        Ok(program) => program,
        Err(err) if limits.memory_kb.is_some() && sys::is_out_of_memory(&err) => return Ok(RunResult::MemoryLimitExceeded),
        Err(err) => return Err(Error::run_failed(format!("could not start {}: {err}", executable.display()))),
    };
    let deadline = Instant::now() + limits.wall_clock();
    let (stdin, stdout) = (program.stdin.take(), program.stdout.take());

    // Both pipes are serviced concurrently: one filling up unread would deadlock.
    let (stdout, usage) = std::thread::scope(|scope| {
        if let Some(mut stdin) = stdin {
            scope.spawn(move || drop(stdin.write_all(input.as_bytes())));
        }
        let stdout_reader = scope.spawn(move || stdout.map_or_else(|| Ok(Some(Vec::new())), |stdout| read_output(stdout, limits.output_bytes)));
        let usage = sys::wait(&mut program, limits, deadline).inspect_err(|_| drop(program.kill()));
        let stdout = stdout_reader.join().unwrap_or_else(|_panic| Err(std::io::Error::other("the thread reading stdout panicked")));
        (stdout, usage)
    });

    let usage = usage.map_err(|err| Error::run_failed(format!("could not wait for {}: {err}", executable.display())))?;
    let stdout = stdout.map_err(|err| Error::run_failed(format!("could not read the output of {}: {err}", executable.display())))?;
    trace!("Program ended: {usage:?}");

    let Some(stdout) = stdout else {
        debug!("Program wrote more than {} bytes to stdout and was stopped", limits.output_bytes);
        return Ok(RunResult::Crashed(CrashReason::OutputLimitExceeded));
    };

    let time_ms = usage.cpu_time_ms;
    let over_memory_limit = limits.memory_kb.is_some_and(|limit| usage.peak_memory_kb > limit);

    Ok(match usage.exit {
        Exit::OutOfTime => RunResult::TimedOut,
        _ if over_memory_limit => RunResult::MemoryLimitExceeded,
        Exit::Failure(reason) => RunResult::Crashed(reason),
        Exit::Success if time_ms > limits.effective_time_ms() => RunResult::TimedOut,
        Exit::Success => {
            let output = String::from_utf8(stdout).unwrap_or_else(|err| String::from_utf8_lossy(err.as_bytes()).into_owned());
            #[cfg(windows)]
            let output = output.replace("\r\n", "\n");
            RunResult::Ok {
                time_ms,
                memory_kb: usage.peak_memory_kb,
                output,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_interval_is_bounded() {
        assert_eq!(poll_interval(Duration::ZERO), Duration::from_micros(50));
        assert_eq!(poll_interval(Duration::from_millis(8)), Duration::from_millis(1));
        assert_eq!(poll_interval(Duration::from_secs(100)), Duration::from_millis(2));
    }

    #[test]
    fn read_output_stops_past_the_limit() {
        assert_eq!(read_output(&b"12345"[..], 5).unwrap(), Some(b"12345".to_vec()));
        assert_eq!(read_output(&b"123456"[..], 5).unwrap(), None);
        assert_eq!(read_output(&b""[..], 0).unwrap(), Some(Vec::new()));
        assert_eq!(read_output(&b"x"[..], 0).unwrap(), None);
        assert_eq!(read_output(&b"x"[..], u64::MAX).unwrap(), Some(b"x".to_vec()));
    }
}
