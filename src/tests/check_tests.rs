use crate::check::{check_hash, checked, tokens_match};
use crate::{Error, RunResult};

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
    assert_eq!(
        checked(ok.clone(), |output| Ok(output == "out")).unwrap(),
        RunResult::Ok {
            time_ms: 3,
            memory_kb: 4,
            output: true
        }
    );
    assert_eq!(
        checked(ok.clone(), |_| Ok(false)).unwrap(),
        RunResult::Ok {
            time_ms: 3,
            memory_kb: 4,
            output: false
        }
    );
    assert!(matches!(checked(ok, |_| Err(Error::Cancelled)), Err(Error::Cancelled)));
    let unchecked = |_: &str| -> crate::Result<bool> { unreachable!("a failed run was checked") };
    assert_eq!(checked(RunResult::TimedOut, unchecked).unwrap(), RunResult::TimedOut);
    assert_eq!(checked(RunResult::MemoryLimitExceeded, unchecked).unwrap(), RunResult::MemoryLimitExceeded);
    assert_eq!(checked(RunResult::Signal(11), unchecked).unwrap(), RunResult::Signal(11));
    assert_eq!(checked(RunResult::ExitCode(3), unchecked).unwrap(), RunResult::ExitCode(3));
    assert_eq!(checked(RunResult::OutputLimitExceeded, unchecked).unwrap(), RunResult::OutputLimitExceeded);
}
