use crate::cache::{Key, MAX_CACHED_OUTPUT_BYTES, ResultCache, is_cacheable};
use crate::{CrashReason, Limits, RunResult};
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
        RunResult::Crashed(CrashReason::ExitCode(-3)),
        RunResult::Crashed(CrashReason::Signal(11)),
        RunResult::Crashed(CrashReason::OutputLimitExceeded),
    ]
}

#[test]
fn what_is_cacheable() {
    assert!(is_cacheable(&ok(&"x".repeat(MAX_CACHED_OUTPUT_BYTES))));
    assert!(!is_cacheable(&ok(&"x".repeat(MAX_CACHED_OUTPUT_BYTES + 1))));
    for failure in &every_kind()[3..] {
        assert!(is_cacheable(failure), "{failure:?}");
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
            assert_eq!(reader.get(&Key::new(i as u64, &j.to_string(), limits)).as_ref(), Some(result));
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
    assert_eq!(cache.get(&key), None);
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
    good.insert(key, &RunResult::TimedOut);
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.insert_str(0, "garbage\n0000000000000001 0000000000000002 1000 - 5 OK 1 2 -abc\n\n1 2 3 4 5 TLE extra\n");
    text.push_str("0000000000000001 0000000000000002 10");
    std::fs::write(&path, text).unwrap();

    let reader = ResultCache::new(path);
    assert_eq!(reader.get(&key), Some(RunResult::TimedOut));
}

#[test]
fn missing_file_is_an_empty_cache() {
    let dir = TempDir::new().unwrap();
    assert_eq!(ResultCache::new(dir.path().join("missing")).get(&Key::new(1, "", Limits::new())), None);
}

#[test]
fn later_lines_win() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("results.cache");
    let key = Key::new(1, "in", Limits::new());
    let cache = ResultCache::new(path.clone());
    cache.insert(key, &RunResult::TimedOut);
    cache.insert(key, &ok("1"));
    assert_eq!(cache.get(&key), Some(ok("1")));
    assert_eq!(ResultCache::new(path).get(&key), Some(ok("1")));
}
