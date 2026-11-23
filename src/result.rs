use serde::{Deserialize, Serialize};
/// How a run ended.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum RunResult {
    /// Exited with code zero within all limits.
    Ok {
        /// CPU time used, in milliseconds.
        time_ms: u64,
        /// Peak memory used, in kilobytes.
        memory_kb: u64,
        /// Everything written to stdout; invalid UTF-8 is replaced.
        output: String,
    },
    /// Used more CPU time than allowed, or blocked for far too long.
    TimedOut,
    /// Peak memory went over the limit.
    MemoryLimitExceeded,
    /// The program failed.
    Crashed(CrashReason),
}

/// Why a program [crashed](RunResult::Crashed).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum CrashReason {
    /// Exited with a non-zero code. On Windows this includes crashes, as exception codes.
    ExitCode(i32),
    /// Killed by this signal, such as `SIGSEGV` (Unix only).
    Signal(i32),
    /// Wrote more than [`Limits::output_bytes`](crate::Limits::output_bytes) to stdout.
    OutputLimitExceeded,
}

/// How a run ended, with its output [checked](crate::Check) against an answer.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Verdict {
    /// Exited with code zero within all limits, and the output was accepted.
    Accepted {
        /// CPU time used, in milliseconds.
        time_ms: u64,
        /// Peak memory used, in kilobytes.
        memory_kb: u64,
    },
    /// Exited with code zero within all limits, but the output was rejected.
    WrongAnswer,
    /// Used more CPU time than allowed, or blocked for far too long.
    TimedOut,
    /// Peak memory went over the limit.
    MemoryLimitExceeded,
    /// The program failed.
    Crashed(CrashReason),
}
