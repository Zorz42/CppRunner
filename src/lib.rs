#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

mod cache;
mod check;
mod compiler;
mod error;
mod exec;
mod limits;
mod result;
mod runner;
mod sys;
#[cfg(test)]
mod tests;

pub use cache::MAX_CACHED_OUTPUT_BYTES;
pub use check::{CHECKER_TIME_MS, Check};
pub use compiler::COMPILER_ENV_VAR;
pub use error::{Error, Result};
pub use limits::{DEFAULT_OUTPUT_BYTES, DEFAULT_TIME_MS, Limits};
pub use result::{CrashReason, RunResult, Verdict};
pub use runner::{MAX_DEFAULT_WORKERS, RunHandle, Runner};
