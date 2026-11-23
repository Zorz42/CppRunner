use crate::result::{improves_on, near_the_limit};
use crate::tests::common::{CPU_MS, runner, runs_logged};
use crate::{Check, Limits, RunResult, Runner};
use std::path::Path;

/// Logs a line every time it really runs, then uses as many milliseconds of CPU time as its input.
fn logging_busy_loop(log: &Path) -> String {
    let log = log.to_string_lossy().replace('\\', "/");
    format!(
        "#include <fstream>\n#include <iostream>\n{CPU_MS}
        int main() {{ long long ms; std::cin >> ms; std::ofstream(\"{log}\", std::ios::app) << \"run\\n\"; while (cpu_ms() < ms) {{}} std::cout << \"done\"; }}"
    )
}

fn runs(runner: &Runner, program: &str, input: &str, time_ms: u64) -> RunResult {
    runner.spawn(program, input, Limits::new().with_time_ms(time_ms)).unwrap().wait().unwrap()
}

#[test]
fn runs_near_the_limit_are_measured_again() {
    let (dir, runner) = runner();
    let log = dir.path().join("log");
    runner.set_timing_retries(2);
    assert!(matches!(runs(&runner, &logging_busy_loop(&log), "300", 500), RunResult::Ok { .. }));
    assert_eq!(runs_logged(&log).len(), 3);
}

#[test]
fn runs_far_below_the_limit_are_measured_once() {
    let (dir, runner) = runner();
    let log = dir.path().join("log");
    runner.set_timing_retries(2);
    assert!(matches!(runs(&runner, &logging_busy_loop(&log), "0", 5000), RunResult::Ok { .. }));
    assert_eq!(runs_logged(&log).len(), 1);
}

#[test]
fn time_outs_are_measured_again() {
    let (dir, runner) = runner();
    let log = dir.path().join("log");
    runner.set_timing_retries(1);
    assert_eq!(runs(&runner, &logging_busy_loop(&log), "5000", 100), RunResult::TimedOut);
    assert_eq!(runs_logged(&log).len(), 2);
}

#[test]
fn there_are_no_retries_by_default() {
    let (dir, runner) = runner();
    let log = dir.path().join("log");
    runs(&runner, &logging_busy_loop(&log), "300", 500);
    assert_eq!(runs_logged(&log).len(), 1);
}

#[test]
fn cached_results_are_not_measured_again() {
    let (dir, runner) = runner();
    let log = dir.path().join("log");
    runner.set_timing_retries(2);
    let program = logging_busy_loop(&log);
    runs(&runner, &program, "300", 500);
    let again = runner.spawn(&program, "300", Limits::new().with_time_ms(500)).unwrap();
    assert!(again.is_cached());
    again.wait().unwrap();
    assert_eq!(runs_logged(&log).len(), 3);
}

#[test]
fn checked_runs_are_measured_again() {
    let (dir, runner) = runner();
    let log = dir.path().join("log");
    runner.set_timing_retries(2);
    let limits = Limits::new().with_time_ms(500);
    let result = runner.spawn_checked(&logging_busy_loop(&log), "300", "done", &Check::Tokens, limits).unwrap().wait().unwrap();
    assert!(matches!(result, RunResult::Ok { output: true, .. }), "{result:?}");
    assert_eq!(runs_logged(&log).len(), 3);
}

#[test]
fn wrong_answers_are_not_measured_again() {
    let (dir, runner) = runner();
    let log = dir.path().join("log");
    runner.set_timing_retries(2);
    let limits = Limits::new().with_time_ms(500);
    let result = runner.spawn_checked(&logging_busy_loop(&log), "300", "other", &Check::Tokens, limits).unwrap().wait().unwrap();
    assert!(matches!(result, RunResult::Ok { output: false, .. }), "{result:?}");
    assert_eq!(runs_logged(&log).len(), 1);
}

fn ok<O>(time_ms: u64, output: O) -> RunResult<O> {
    RunResult::Ok { time_ms, memory_kb: 1, output }
}

#[test]
fn a_pass_beats_a_failure_and_a_faster_pass_a_slower_one() {
    assert!(improves_on(&ok(900, true), &RunResult::TimedOut));
    assert!(improves_on(&ok(700, true), &ok(900, true)));
    assert!(improves_on(&ok(900, true), &ok(100, false)));
    assert!(!improves_on(&ok(900, true), &ok(700, true)));
    assert!(!improves_on(&RunResult::TimedOut, &ok(900, true)));
    assert!(!improves_on(&ok(100, false), &RunResult::<bool>::TimedOut));
    assert!(improves_on(&ok(5, String::new()), &RunResult::ExitCode(1)));
}

#[test]
fn near_the_limit_means_timed_out_or_over_half_of_it() {
    assert!(near_the_limit(&RunResult::<bool>::TimedOut, 1000));
    assert!(near_the_limit(&ok(501, true), 1000));
    assert!(!near_the_limit(&ok(500, true), 1000));
    assert!(!near_the_limit(&ok(900, false), 1000));
    assert!(near_the_limit(&ok(900, String::new()), 1000));
    assert!(!near_the_limit(&RunResult::<bool>::ExitCode(1), 1000));
}
