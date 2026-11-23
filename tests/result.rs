use cpprunner::{CrashReason, RunResult};

fn ok(output: &str) -> RunResult {
    RunResult::Ok {
        time_ms: 1,
        memory_kb: 2,
        output: output.to_owned(),
    }
}

#[test]
fn accessors() {
    assert!(ok("x").is_ok());
    assert_eq!(ok("x").output(), Some("x"));
    assert_eq!(ok("x").into_output().as_deref(), Some("x"));
    for failed in [RunResult::TimedOut, RunResult::MemoryLimitExceeded, RunResult::Crashed(CrashReason::ExitCode(1))] {
        assert!(!failed.is_ok());
        assert_eq!(failed.output(), None);
        assert_eq!(failed.into_output(), None);
    }
}

#[test]
fn verdicts() {
    assert_eq!(ok("").verdict(), "OK");
    assert_eq!(RunResult::TimedOut.verdict(), "TLE");
    assert_eq!(RunResult::MemoryLimitExceeded.verdict(), "MLE");
    assert_eq!(RunResult::Crashed(CrashReason::OutputLimitExceeded).verdict(), "RTE");
    assert_eq!(RunResult::Crashed(CrashReason::Signal(11)).verdict(), "RTE");
}
