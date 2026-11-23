use crate::compiler::SCRATCH_PREFIX;
use crate::exec::run_executable;
use crate::hash::stable_hash;
use crate::{CrashReason, Error, Limits, Result, RunResult, Verdict};
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
    /// [`Error::CheckerFailed`]. It may use [`CHECKER_TIME_MS`].
    Program(String),
}

pub fn tokens_match(output: &str, answer: &str) -> bool {
    output.split_whitespace().eq(answer.split_whitespace())
}

/// Identifies the check of an output against `answer`, for the result cache.
pub fn check_hash(checker: Option<u64>, answer: &str) -> u64 {
    let checker = checker.map(u64::to_le_bytes);
    stable_hash(&[checker.as_ref().map_or(&[][..], |checker| &checker[..]), answer.as_bytes()])
}

/// Whether `output` is accepted: by token comparison without a checker executable.
pub fn accepts(checker: Option<&Path>, build_folder: &Path, input: &str, answer: &str, output: &str) -> Result<bool> {
    let Some(checker) = checker else {
        return Ok(tokens_match(output, answer));
    };
    let scratch = tempfile::Builder::new().prefix(SCRATCH_PREFIX).tempdir_in(build_folder).map_err(Error::io(build_folder))?;
    let files = [("input.txt", input), ("output.txt", output), ("answer.txt", answer)].map(|(name, contents)| {
        let path = scratch.path().join(name);
        std::fs::write(&path, contents).map_err(Error::io(&path)).map(|()| path)
    });
    let [input, output, answer] = files;
    let (input, output, answer) = (input?, output?, answer?);
    let limits = Limits::new().with_time_ms(CHECKER_TIME_MS);
    match run_executable(checker, &[&input, &output, &answer], "", limits)? {
        RunResult::Ok { .. } => Ok(true),
        RunResult::Crashed(CrashReason::ExitCode(1 | 2)) => Ok(false),
        ending => Err(Error::CheckerFailed { details: format!("{ending:?}") }),
    }
}

pub fn verdict<F: FnOnce(&str) -> Result<bool>>(result: RunResult, accepted: F) -> Result<Verdict> {
    Ok(match result {
        RunResult::Ok { time_ms, memory_kb, output } => {
            if accepted(&output)? {
                Verdict::Accepted { time_ms, memory_kb }
            } else {
                Verdict::WrongAnswer
            }
        }
        RunResult::TimedOut => Verdict::TimedOut,
        RunResult::MemoryLimitExceeded => Verdict::MemoryLimitExceeded,
        RunResult::Crashed(reason) => Verdict::Crashed(reason),
    })
}
