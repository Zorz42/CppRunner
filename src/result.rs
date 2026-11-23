use serde::{Deserialize, Serialize};

/// How a run ended. `T` is what a successful run gives: its output, or for a
/// [checked](crate::Runner::spawn_checked) run whether the output was accepted.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum RunResult<T = String> {
    /// Exited with code zero within all limits.
    Ok {
        /// CPU time used, in milliseconds.
        time_ms: u64,
        /// Peak memory used, in kilobytes.
        memory_kb: u64,
        /// Everything written to stdout, with invalid UTF-8 replaced; or whether it was accepted.
        output: T,
    },
    /// Used more CPU time than allowed, or blocked for far too long.
    TimedOut,
    /// Peak memory went over the limit.
    MemoryLimitExceeded,
    /// Exited with a non-zero code. On Windows this includes crashes, as exception codes.
    ExitCode(i32),
    /// Killed by this signal, such as `SIGSEGV` (Unix only).
    Signal(i32),
    /// Wrote more than [`Limits::output_bytes`](crate::Limits::output_bytes) to stdout.
    OutputLimitExceeded,
}

/// What a successful run gives: a plain run's output always passes, a checked run's only if accepted.
pub trait Output {
    fn passed(&self) -> bool;
}

impl Output for String {
    fn passed(&self) -> bool {
        true
    }
}

impl Output for bool {
    fn passed(&self) -> bool {
        *self
    }
}

fn passed_in_ms<O: Output>(result: &RunResult<O>) -> Option<u64> {
    match result {
        RunResult::Ok { time_ms, output, .. } if output.passed() => Some(*time_ms),
        _ => None,
    }
}

/// A pass beats a failure, and a faster pass a slower one.
pub fn improves_on<O: Output>(result: &RunResult<O>, other: &RunResult<O>) -> bool {
    match (passed_in_ms(result), passed_in_ms(other)) {
        (Some(time_ms), Some(other_ms)) => time_ms < other_ms,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

/// Whether timing noise could change the result: it timed out, or passed using over half of `time_ms`.
pub fn near_the_limit<O: Output>(result: &RunResult<O>, time_ms: u64) -> bool {
    matches!(result, RunResult::TimedOut) || passed_in_ms(result).is_some_and(|used| used * 2 > time_ms)
}
