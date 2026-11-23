mod common;

use common::{ECHO, HELLO_WORLD, assert_output, limits};
use cpprunner::{CrashReason, Error, Limits, RunResult, run_file, run_source};
use tempfile::TempDir;

#[test]
fn run_source_compiles_and_runs() {
    assert_output(&run_source(HELLO_WORLD, "", Limits::default()).unwrap(), "Hello, World!");
    assert_output(&run_source(ECHO, "5", Limits::default()).unwrap(), "5");
}

#[test]
fn run_source_reports_verdicts() {
    assert_eq!(run_source("int main() { while (true) {} }", "", limits(100)).unwrap(), RunResult::TimedOut);
    assert_eq!(run_source("int main() { return 4; }", "", limits(1000)).unwrap(), RunResult::Crashed(CrashReason::ExitCode(4)));
}

#[test]
fn run_source_with_a_compile_error() {
    assert!(matches!(run_source("int main() {", "", Limits::default()), Err(Error::CompilerError { .. })));
}

#[test]
fn run_file_compiles_and_runs() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("solution.cpp");
    std::fs::write(&source, ECHO).unwrap();

    assert_output(&run_file(&source, "42", Limits::default()).unwrap(), "42");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "nothing is left next to the source");
}

#[test]
fn run_file_of_a_missing_file() {
    let missing = TempDir::new().unwrap().path().join("missing.cpp");
    let Err(Error::Io { path, .. }) = run_file(&missing, "", Limits::default()) else { unreachable!() };
    assert_eq!(path, missing);
}
