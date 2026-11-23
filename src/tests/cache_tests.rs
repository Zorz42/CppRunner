use crate::cache::{Cacheable, Entry, Key, MAX_CACHED_OUTPUT_BYTES, ResultCache};
use crate::{Limits, RunResult};
use tempfile::TempDir;

fn ok(output: &str) -> RunResult {
    RunResult::Ok {
        time_ms: 12,
        memory_kb: 3456,
        output: output.to_owned(),
    }
}

fn every_kind() -> Vec<RunResult> {
    vec![
        ok(""),
        ok("YES\n"),
        ok("multi word\nline \u{fffd} end"),
        RunResult::TimedOut,
        RunResult::MemoryLimitExceeded,
        RunResult::ExitCode(-3),
        RunResult::Signal(11),
        RunResult::OutputLimitExceeded,
    ]
}

#[test]
fn what_is_cacheable() {
    assert!(String::entry(&ok(&"x".repeat(MAX_CACHED_OUTPUT_BYTES))).is_some());
    assert!(String::entry(&ok(&"x".repeat(MAX_CACHED_OUTPUT_BYTES + 1))).is_none());
    for failure in &every_kind()[3..] {
        assert!(String::entry(failure).is_some(), "{failure:?}");
    }
}

#[test]
fn every_kind_of_result_survives_the_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("results.cache");
    let limits = [Limits::new(), Limits::new().with_time_ms(7).with_memory_mb(64).with_output_bytes(9)];
    let writer = ResultCache::new(path.clone());
    for (i, result) in every_kind().iter().enumerate() {
        for (j, &limits) in limits.iter().enumerate() {
            writer.insert(Key::new(i as u64, &j.to_string(), limits), result);
        }
    }

    let reader = ResultCache::new(path);
    for (i, result) in every_kind().iter().enumerate() {
        for (j, &limits) in limits.iter().enumerate() {
            assert_eq!(reader.get(&Key::new(i as u64, &j.to_string(), limits)), Some(result.clone()));
        }
    }
}

#[test]
fn uncacheable_results_are_not_stored() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("results.cache");
    let cache = ResultCache::new(path.clone());
    let key = Key::new(1, "input", Limits::new());
    cache.insert(key, &ok(&"x".repeat(MAX_CACHED_OUTPUT_BYTES + 1)));
    assert_eq!(cache.get::<String>(&key), None);
    assert!(!path.exists());
}

#[test]
fn keys_differ_by_program_input_and_limits() {
    let key = Key::new(1, "input", Limits::new());
    assert_eq!(key, Key::new(1, "input", Limits::new()));
    assert_ne!(key, Key::new(2, "input", Limits::new()));
    assert_ne!(key, Key::new(1, "input2", Limits::new()));
    assert_ne!(key, Key::new(1, "input", Limits::new().with_time_ms(2)));
}

#[test]
fn damaged_lines_are_ignored() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("results.cache");
    let good = ResultCache::new(path.clone());
    let key = Key::new(5, "in", Limits::new());
    good.insert(key, &RunResult::<String>::TimedOut);

    let better = serde_json::to_string(&(key, Entry::Run(ok("1")))).unwrap();
    let checked = Key::checked(5, "in", 7, Limits::new());
    let mismatched = serde_json::to_string(&(checked, Entry::Run(ok("1")))).unwrap();
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.insert_str(0, &format!("garbage\n\n[1, 2]\n{mismatched}\n{better} extra\n"));
    text.push_str(&better[..better.len() / 2]);
    std::fs::write(&path, text).unwrap();

    let reader = ResultCache::new(path);
    assert_eq!(reader.get(&key), Some(RunResult::<String>::TimedOut));
    assert_eq!(reader.get::<String>(&checked), None);
    assert_eq!(reader.get::<bool>(&checked), None);
}

#[test]
fn missing_file_is_an_empty_cache() {
    let dir = TempDir::new().unwrap();
    assert_eq!(ResultCache::new(dir.path().join("missing")).get::<String>(&Key::new(1, "", Limits::new())), None);
}

#[test]
fn the_better_result_is_kept() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("results.cache");
    let key = Key::new(1, "in", Limits::new());
    let cache = ResultCache::new(path.clone());
    let fast = RunResult::Ok {
        time_ms: 5,
        memory_kb: 1,
        output: "1".to_owned(),
    };
    for result in [RunResult::TimedOut, ok("1"), RunResult::ExitCode(1), fast.clone(), ok("1")] {
        cache.insert(key, &result);
    }
    assert_eq!(cache.get(&key), Some(fast.clone()));
    assert_eq!(ResultCache::new(path.clone()).get(&key), Some(fast));
    assert_eq!(std::fs::read_to_string(path).unwrap().lines().count(), 3);
}

#[test]
fn a_failure_does_not_replace_another() {
    let dir = TempDir::new().unwrap();
    let key = Key::new(1, "in", Limits::new());
    let cache = ResultCache::new(dir.path().join("results.cache"));
    cache.insert(key, &RunResult::<String>::TimedOut);
    cache.insert(key, &RunResult::<String>::MemoryLimitExceeded);
    assert_eq!(cache.get(&key), Some(RunResult::<String>::TimedOut));
}

fn every_checked_result() -> Vec<RunResult<bool>> {
    vec![
        RunResult::Ok {
            time_ms: 7,
            memory_kb: 890,
            output: true,
        },
        RunResult::Ok {
            time_ms: 7,
            memory_kb: 890,
            output: false,
        },
        RunResult::TimedOut,
        RunResult::MemoryLimitExceeded,
        RunResult::ExitCode(3),
        RunResult::Signal(6),
        RunResult::OutputLimitExceeded,
    ]
}

#[test]
fn every_checked_result_survives_the_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("results.cache");
    let writer = ResultCache::new(path.clone());
    for (i, &result) in every_checked_result().iter().enumerate() {
        writer.insert(Key::checked(i as u64, "in", 42, Limits::new()), &result);
    }
    let reader = ResultCache::new(path);
    for (i, &result) in every_checked_result().iter().enumerate() {
        assert_eq!(reader.get(&Key::checked(i as u64, "in", 42, Limits::new())), Some(result));
        assert_eq!(reader.get::<bool>(&Key::new(i as u64, "in", Limits::new())), None);
    }
}

#[test]
fn checked_keys_differ_by_check() {
    let key = Key::checked(1, "input", 2, Limits::new());
    assert_eq!(key, Key::checked(1, "input", 2, Limits::new()));
    assert_ne!(key, Key::checked(1, "input", 3, Limits::new()));
    assert_ne!(key, Key::new(1, "input", Limits::new()));
}

#[test]
fn a_rejected_run_does_not_replace_an_accepted_one() {
    let dir = TempDir::new().unwrap();
    let key = Key::checked(1, "in", 2, Limits::new());
    let cache = ResultCache::new(dir.path().join("results.cache"));
    let run = |time_ms, output| RunResult::Ok { time_ms, memory_kb: 1, output };
    cache.insert(key, &run(5, false));
    cache.insert(key, &run(9, true));
    cache.insert(key, &run(1, false));
    assert_eq!(cache.get(&key), Some(run(9, true)));
}
