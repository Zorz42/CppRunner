#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

mod compiler;
mod error;
mod exec;
mod limits;
mod result;
mod runner;
mod sys;
#[cfg(test)]
mod tests;

pub use compiler::COMPILER_ENV_VAR;
use compiler::Compiler;
pub use error::{Error, Result};
pub use exec::run_executable;
pub use limits::{DEFAULT_OUTPUT_BYTES, DEFAULT_TIME_MS, Limits};
pub use result::{CrashReason, RunResult};
pub use runner::{CppRunner, Job, MAX_DEFAULT_WORKERS, ProgramHandle};

use std::path::Path;

/// Compiles `source_code` in a temporary folder and runs it once with `input` on stdin.
pub fn run_source(source_code: &str, input: &str, limits: Limits) -> Result<RunResult> {
    let build_folder = tempfile::tempdir().map_err(Error::io(&std::env::temp_dir()))?;
    let compiled = Compiler::detect()?.compile_cached(build_folder.path(), source_code)?;
    run_executable(&compiled.executable, input, limits)
}

/// Reads the C++ file at `source` and runs it as [`run_source`] does.
pub fn run_file(source: &Path, input: &str, limits: Limits) -> Result<RunResult> {
    let source_code = std::fs::read_to_string(source).map_err(Error::io(source))?;
    run_source(&source_code, input, limits)
}
