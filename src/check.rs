use crate::compiler::SCRATCH_PREFIX;
use crate::exec::run_executable;
use crate::hash::stable_hash;
use crate::{Error, Limits, Result, RunResult};
use std::path::Path;

/// The CPU time a checker program may use on one output.
pub const CHECKER_TIME_MS: u64 = 10_000;

/// How the output of a run is compared with the answer.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum Check {
    /// The output's whitespace-separated tokens have to equal the answer's.
    #[default]
    Tokens,
    /// The source of a C++ checker, run as `checker input output answer` on three files. It accepts
    /// by exiting with 0 and rejects with 1 or 2, as testlib checkers do; any other ending is an
    /// [`Error::CheckerFailed`]. It may use ten seconds of CPU time.
    Program(String),
}

pub fn tokens_match(output: &str, answer: &str) -> bool {
    output.split_whitespace().eq(answer.split_whitespace())
}

/// Identifies the check of an output against `answer` for the result cache; `checker` is the hash of
/// the checker program, or zero for token comparison.
pub fn check_hash(checker: u64, answer: &str) -> u64 {
    stable_hash(&[&checker.to_le_bytes(), answer.as_bytes()])
}

/// Whether `output` is accepted: by token comparison without a checker executable.
pub fn accepts(checker: Option<&Path>, build_folder: &Path, input: &str, answer: &str, output: &str) -> Result<bool> {
    let Some(checker) = checker else {
        return Ok(tokens_match(output, answer));
    };
    let scratch = tempfile::Builder::new().prefix(SCRATCH_PREFIX).tempdir_in(build_folder).map_err(Error::io(build_folder))?;
    let write = |name: &str, contents: &str| {
        let path = scratch.path().join(name);
        std::fs::write(&path, contents).map_err(Error::io(&path)).map(|()| path)
    };
    let (input, output, answer) = (write("input.txt", input)?, write("output.txt", output)?, write("answer.txt", answer)?);
    let limits = Limits::new().with_time_ms(CHECKER_TIME_MS);
    match run_executable(checker, &[&input, &output, &answer], "", limits)? {
        RunResult::Ok { .. } => Ok(true),
        RunResult::ExitCode(1 | 2) => Ok(false),
        ending => Err(Error::CheckerFailed { details: format!("{ending:?}") }),
    }
}

pub fn checked<F: FnOnce(&str) -> Result<bool>>(result: RunResult, accepted: F) -> Result<RunResult<bool>> {
    Ok(match result {
        RunResult::Ok { time_ms, memory_kb, output } => RunResult::Ok {
            time_ms,
            memory_kb,
            output: accepted(&output)?,
        },
        RunResult::TimedOut => RunResult::TimedOut,
        RunResult::MemoryLimitExceeded => RunResult::MemoryLimitExceeded,
        RunResult::ExitCode(code) => RunResult::ExitCode(code),
        RunResult::Signal(signal) => RunResult::Signal(signal),
        RunResult::OutputLimitExceeded => RunResult::OutputLimitExceeded,
    })
}
