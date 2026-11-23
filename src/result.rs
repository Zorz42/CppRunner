use serde::{Deserialize, Serialize};

/// How a run ended. `O` is what a successful run gives: its output, or for a
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
