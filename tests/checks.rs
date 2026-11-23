mod common;

use common::{ECHO, logging_echo, runner, runs_logged};
use cpprunner::{Check, CrashReason, Error, Limits, Runner, Verdict};

/// Accepts any output that is a number at least the input, whatever the answer.
const AT_LEAST_INPUT: &str = "#include <fstream>
    int main(int argc, char** argv) { std::ifstream in(argv[1]), out(argv[2]); long long n, got; in >> n; if (!(out >> got)) return 1; return got >= n ? 0 : 1; }";
/// Accepts the output if its first token is the answer's, and reports a failure on an empty answer.
const FIRST_TOKEN: &str = "#include <fstream>\n#include <string>
    int main(int argc, char** argv) { std::ifstream out(argv[2]), ans(argv[3]); std::string got, want; if (!(ans >> want)) return 3; out >> got; return got == want ? 0 : 2; }";
const PRINT_LINES: &str = "#include <iostream>\nint main() { int n; std::cin >> n; for (int i = 0; i < n; i++) std::cout << i << '\\n'; }";

fn checked(runner: &Runner, source: &str, input: &str, answer: &str, check: &Check) -> cpprunner::Result<Verdict> {
    runner.spawn_checked(source, input, answer, check, Limits::default())?.wait()
}

fn accepted(verdict: &Verdict) -> bool {
    matches!(verdict, Verdict::Accepted { .. })
}

#[test]
fn tokens_are_compared() {
    let (_dir, runner) = runner();
    assert!(accepted(&checked(&runner, ECHO, "42", "42", &Check::Tokens).unwrap()));
    assert!(accepted(&checked(&runner, ECHO, "42", "  42\n\n", &Check::Tokens).unwrap()));
    assert_eq!(checked(&runner, ECHO, "42", "43", &Check::Tokens).unwrap(), Verdict::WrongAnswer);
    assert_eq!(checked(&runner, ECHO, "42", "42 0", &Check::Tokens).unwrap(), Verdict::WrongAnswer);
    assert_eq!(checked(&runner, ECHO, "42", "4 2", &Check::Tokens).unwrap(), Verdict::WrongAnswer);
}

#[test]
fn the_default_check_compares_tokens() {
    assert_eq!(Check::default(), Check::Tokens);
}

#[test]
fn accepted_runs_are_measured() {
    let (_dir, runner) = runner();
    let verdict = checked(&runner, ECHO, "1", "1", &Check::Tokens).unwrap();
    assert!(matches!(verdict, Verdict::Accepted { memory_kb, .. } if memory_kb > 0), "{verdict:?}");
}

#[test]
fn failed_runs_keep_their_verdict() {
    let (_dir, runner) = runner();
    let limits = Limits::new().with_time_ms(100);
    let hang = runner.spawn_checked("int main() { while (true) {} }", "", "", &Check::Tokens, limits).unwrap();
    assert_eq!(hang.wait().unwrap(), Verdict::TimedOut);
    assert_eq!(
        checked(&runner, "int main() { return 3; }", "", "", &Check::Tokens).unwrap(),
        Verdict::Crashed(CrashReason::ExitCode(3))
    );
    let limits = Limits::new().with_output_bytes(10);
    let flood = runner.spawn_checked(PRINT_LINES, "1000", "", &Check::Tokens, limits).unwrap();
    assert_eq!(flood.wait().unwrap(), Verdict::Crashed(CrashReason::OutputLimitExceeded));
}

#[test]
fn a_failed_run_is_not_checked() {
    let (_dir, runner) = runner();
    let failing_checker = Check::Program("int main() { return 3; }".to_owned());
    assert_eq!(
        checked(&runner, "int main() { return 1; }", "", "", &failing_checker).unwrap(),
        Verdict::Crashed(CrashReason::ExitCode(1))
    );
}

#[test]
fn verdicts_of_long_outputs_are_cached() {
    let (dir, runner) = runner();
    let answer = (0..1000).map(|i| i.to_string()).collect::<Vec<_>>().join("\n");
    let first = runner.spawn_checked(PRINT_LINES, "1000", answer.as_str(), &Check::Tokens, Limits::default()).unwrap();
    assert!(!first.is_cached());
    assert!(accepted(&first.wait().unwrap()));

    let again = runner.spawn_checked(PRINT_LINES, "1000", answer.as_str(), &Check::Tokens, Limits::default()).unwrap();
    assert!(again.is_cached());
    assert!(accepted(&again.wait().unwrap()));
    assert!(std::fs::metadata(dir.path().join("results.cache")).unwrap().len() < 200);
}

#[test]
fn checked_and_unchecked_runs_are_cached_apart() {
    let (dir, runner) = runner();
    let log = dir.path().join("log");
    let program = logging_echo(&log);
    runner.spawn(&program, "a", Limits::default()).unwrap().wait().unwrap();
    checked(&runner, &program, "a", "a", &Check::Tokens).unwrap();
    checked(&runner, &program, "a", "a", &Check::Tokens).unwrap();
    runner.spawn(&program, "a", Limits::default()).unwrap().wait().unwrap();
    assert_eq!(runs_logged(&log), ["a", "a"]);
}

#[test]
fn the_cache_key_holds_the_answer_and_the_checker() {
    let (dir, runner) = runner();
    let log = dir.path().join("log");
    let program = logging_echo(&log);
    let first_token = Check::Program(FIRST_TOKEN.to_owned());
    assert!(accepted(&checked(&runner, &program, "a", "a", &Check::Tokens).unwrap()));
    assert_eq!(checked(&runner, &program, "a", "b", &Check::Tokens).unwrap(), Verdict::WrongAnswer);
    assert!(accepted(&checked(&runner, &program, "a", "a", &first_token).unwrap()));
    assert!(accepted(&checked(&runner, &program, "a", "a", &Check::Tokens).unwrap()));
    assert!(accepted(&checked(&runner, &program, "a", "a", &first_token).unwrap()));
    assert_eq!(runs_logged(&log), ["a", "a", "a"]);
}

#[test]
fn fresh_checked_runs_run_again() {
    let (dir, runner) = runner();
    let log = dir.path().join("log");
    let program = logging_echo(&log);
    checked(&runner, &program, "a", "a", &Check::Tokens).unwrap();
    let fresh = runner.spawn_checked_fresh(&program, "a", "a", &Check::Tokens, Limits::default()).unwrap();
    assert!(!fresh.is_cached());
    assert!(accepted(&fresh.wait().unwrap()));
    assert_eq!(runs_logged(&log), ["a", "a"]);
}

#[test]
fn checked_runs_are_not_cached_when_caching_is_off() {
    let (dir, runner) = runner();
    runner.set_result_cache(false);
    let log = dir.path().join("log");
    let program = logging_echo(&log);
    checked(&runner, &program, "a", "a", &Check::Tokens).unwrap();
    checked(&runner, &program, "a", "a", &Check::Tokens).unwrap();
    assert_eq!(runs_logged(&log), ["a", "a"]);
}

#[test]
fn verdicts_outlive_the_runner() {
    let (dir, runner) = runner();
    let log = dir.path().join("log");
    let program = logging_echo(&log);
    checked(&runner, &program, "a", "b", &Check::Tokens).unwrap();
    drop(runner);
    let runner = Runner::new(dir.path()).unwrap();
    let again = runner.spawn_checked(&program, "a", "b", &Check::Tokens, Limits::default()).unwrap();
    assert!(again.is_cached());
    assert_eq!(again.wait().unwrap(), Verdict::WrongAnswer);
    assert_eq!(runs_logged(&log), ["a"]);
}

#[test]
fn a_checker_reads_the_input_output_and_answer() {
    let (_dir, runner) = runner();
    let at_least_input = Check::Program(AT_LEAST_INPUT.to_owned());
    let print_five = "#include <iostream>\nint main() { std::cout << 5; }";
    assert!(accepted(&checked(&runner, print_five, "3", "unused", &at_least_input).unwrap()));
    assert!(accepted(&checked(&runner, print_five, "5", "unused", &at_least_input).unwrap()));
    assert_eq!(checked(&runner, print_five, "6", "unused", &at_least_input).unwrap(), Verdict::WrongAnswer);

    let first_token = Check::Program(FIRST_TOKEN.to_owned());
    assert!(accepted(&checked(&runner, print_five, "", "5 and more", &first_token).unwrap()));
    assert_eq!(checked(&runner, print_five, "", "4", &first_token).unwrap(), Verdict::WrongAnswer);
}

#[test]
fn a_checker_failing_is_an_error() {
    let (_dir, runner) = runner();
    let first_token = Check::Program(FIRST_TOKEN.to_owned());
    let result = checked(&runner, ECHO, "1", "", &first_token);
    assert!(matches!(result, Err(Error::CheckerFailed { .. })), "{result:?}");
    let crashing = Check::Program("#include <cstdlib>\nint main() { std::abort(); }".to_owned());
    let result = checked(&runner, ECHO, "1", "1", &crashing);
    assert!(matches!(result, Err(Error::CheckerFailed { .. })), "{result:?}");
}

#[test]
fn a_checker_that_fails_is_not_cached() {
    let (dir, runner) = runner();
    let log = dir.path().join("log");
    let program = logging_echo(&log);
    let first_token = Check::Program(FIRST_TOKEN.to_owned());
    checked(&runner, &program, "a", "", &first_token).unwrap_err();
    checked(&runner, &program, "a", "", &first_token).unwrap_err();
    assert_eq!(runs_logged(&log), ["a", "a"]);
}

#[test]
fn a_checker_that_does_not_compile_is_reported_when_queued() {
    let (_dir, runner) = runner();
    let broken = Check::Program("int main() { syntax error }".to_owned());
    let result = runner.spawn_checked(ECHO, "1", "1", &broken, Limits::default());
    assert!(matches!(result, Err(Error::CompilerError { .. })), "{:?}", result.err());
}

#[test]
fn a_checker_leaves_no_files_behind() {
    let (dir, runner) = runner();
    let first_token = Check::Program(FIRST_TOKEN.to_owned());
    checked(&runner, ECHO, "1", "1", &first_token).unwrap();
    let scratch: Vec<_> = common::files_in(dir.path()).into_iter().filter(|path| path.is_dir()).collect();
    assert_eq!(scratch, Vec::<std::path::PathBuf>::new());
}

#[test]
fn outputs_can_be_checked_alone() {
    let (_dir, runner) = runner();
    let tokens = runner.spawn_check(&Check::Tokens, "", "1 2", "1\n2\n").unwrap();
    assert!(tokens.wait().unwrap());
    assert!(!runner.spawn_check(&Check::Tokens, "", "1 2", "1").unwrap().wait().unwrap());

    let at_least_input = Check::Program(AT_LEAST_INPUT.to_owned());
    assert!(runner.spawn_check(&at_least_input, "3", "", "4").unwrap().wait().unwrap());
    assert!(!runner.spawn_check(&at_least_input, "3", "", "2").unwrap().wait().unwrap());
    let failed = runner.spawn_check(&Check::Program(FIRST_TOKEN.to_owned()), "", "", "x").unwrap().wait();
    assert!(matches!(failed, Err(Error::CheckerFailed { .. })), "{failed:?}");
}

#[test]
fn checked_runs_can_be_cancelled() {
    let (_dir, runner) = runner();
    runner.set_workers(1);
    runner.compile(ECHO).unwrap();
    let limits = Limits::new().with_time_ms(300);
    let busy = runner.spawn_checked("int main() { while (true) {} }", "", "", &Check::Tokens, limits).unwrap();
    let queued = runner.spawn_checked(ECHO, "1", "1", &Check::Tokens, Limits::default()).unwrap();
    queued.cancel();
    assert_eq!(busy.wait().unwrap(), Verdict::TimedOut);
    assert!(matches!(queued.wait(), Err(Error::Cancelled)));
}
