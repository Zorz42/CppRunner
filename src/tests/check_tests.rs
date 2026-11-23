use crate::check::{check_hash, tokens_match, verdict};
use crate::{CrashReason, Error, RunResult, Verdict};

#[test]
fn tokens_ignore_whitespace_only() {
    assert!(tokens_match("1 2\n3\n", "1  2 3"));
    assert!(tokens_match("", " \n"));
    assert!(!tokens_match("1 2", "1 2 3"));
    assert!(!tokens_match("12", "1 2"));
    assert!(!tokens_match("yes", "YES"));
}

#[test]
fn check_hashes_differ_by_checker_and_answer() {
    assert_eq!(check_hash(None, "1"), check_hash(None, "1"));
    assert_ne!(check_hash(None, "1"), check_hash(None, "2"));
    assert_ne!(check_hash(None, "1"), check_hash(Some(5), "1"));
    assert_ne!(check_hash(Some(5), "1"), check_hash(Some(6), "1"));
}

#[test]
fn only_successful_runs_are_checked() {
    let ok = RunResult::Ok {
        time_ms: 3,
        memory_kb: 4,
        output: "out".to_owned(),
    };
    assert_eq!(verdict(ok.clone(), |output| Ok(output == "out")).unwrap(), Verdict::Accepted { time_ms: 3, memory_kb: 4 });
    assert_eq!(verdict(ok.clone(), |_| Ok(false)).unwrap(), Verdict::WrongAnswer);
    assert!(matches!(verdict(ok, |_| Err(Error::Cancelled)), Err(Error::Cancelled)));
    let unchecked = |_: &str| -> crate::Result<bool> { unreachable!("a failed run was checked") };
    assert_eq!(verdict(RunResult::TimedOut, unchecked).unwrap(), Verdict::TimedOut);
    assert_eq!(verdict(RunResult::MemoryLimitExceeded, unchecked).unwrap(), Verdict::MemoryLimitExceeded);
    let crash = CrashReason::Signal(11);
    assert_eq!(verdict(RunResult::Crashed(crash), unchecked).unwrap(), Verdict::Crashed(crash));
}
