use crate::compiler::{Compiler, is_program_source};
use crate::runner::UNUSED_PROGRAM_LIFETIME;
use crate::tests::common::{ECHO, HELLO_WORLD, run_on};
use crate::{Limits, Runner};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

const OTHER: &str = "int main() { return 0; }";

fn source_of(runner: &Runner, source_code: &str) -> PathBuf {
    runner.build_folder().join(format!("p{}.cpp", runner.program_hash(source_code)))
}

fn exists(source: &Path) -> bool {
    source.exists() && Compiler::executable_path(source).unwrap().exists()
}

fn age(source: &Path, by: Duration) {
    let file = std::fs::File::options().write(true).open(source).unwrap();
    file.set_modified(SystemTime::now() - by).unwrap();
}

fn compiled_and_aged(dir: &TempDir, programs: &[&str], old: &[&str]) -> Vec<PathBuf> {
    let runner = Runner::new(dir.path()).unwrap();
    for program in programs {
        runner.compile(program).unwrap();
    }
    let sources = programs.iter().map(|program| source_of(&runner, program)).collect();
    for program in old {
        age(&source_of(&runner, program), UNUSED_PROGRAM_LIFETIME + Duration::from_secs(3600));
    }
    sources
}

#[test]
fn programs_unused_for_a_week_are_removed_when_the_runner_is_dropped() {
    let dir = TempDir::new().unwrap();
    let [old, recent] = compiled_and_aged(&dir, &[HELLO_WORLD, OTHER], &[HELLO_WORLD]).try_into().unwrap();

    let runner = Runner::new(dir.path()).unwrap();
    runner.compile(ECHO).unwrap();
    let echo = source_of(&runner, ECHO);
    assert!(exists(&old), "removed before the runner was dropped");
    drop(runner);

    assert!(!old.exists());
    assert!(!Compiler::executable_path(&old).unwrap().exists());
    assert!(exists(&recent));
    assert!(exists(&echo));
}

#[test]
fn programs_unused_for_under_a_week_stay() {
    let dir = TempDir::new().unwrap();
    let [source] = compiled_and_aged(&dir, &[HELLO_WORLD], &[]).try_into().unwrap();
    age(&source, UNUSED_PROGRAM_LIFETIME - Duration::from_secs(3600));
    drop(Runner::new(dir.path()).unwrap());
    assert!(exists(&source));
}

#[test]
fn running_a_program_counts_as_using_it() {
    let dir = TempDir::new().unwrap();
    let [source] = compiled_and_aged(&dir, &[ECHO], &[ECHO]).try_into().unwrap();
    let runner = Runner::new(dir.path()).unwrap();
    run_on(&runner, ECHO, "1", Limits::default()).unwrap();
    drop(runner);
    assert!(exists(&source));
    assert!(source.metadata().unwrap().modified().unwrap().elapsed().unwrap() < Duration::from_secs(3600));
}

#[test]
fn a_cached_result_counts_as_using_the_program() {
    let dir = TempDir::new().unwrap();
    let runner = Runner::new(dir.path()).unwrap();
    run_on(&runner, ECHO, "1", Limits::default()).unwrap();
    let source = source_of(&runner, ECHO);
    drop(runner);
    age(&source, UNUSED_PROGRAM_LIFETIME * 2);

    let runner = Runner::new(dir.path()).unwrap();
    assert!(runner.spawn(ECHO, "1", Limits::default()).unwrap().is_cached());
    drop(runner);
    assert!(exists(&source));
}

#[test]
fn a_program_without_its_executable_is_removed() {
    let dir = TempDir::new().unwrap();
    let [source] = compiled_and_aged(&dir, &[HELLO_WORLD], &[HELLO_WORLD]).try_into().unwrap();
    std::fs::remove_file(Compiler::executable_path(&source).unwrap()).unwrap();
    drop(Runner::new(dir.path()).unwrap());
    assert!(!source.exists());
}

#[test]
fn other_files_are_left_alone() {
    let dir = TempDir::new().unwrap();
    let other = dir.path().join("notes.cpp");
    std::fs::write(&other, "").unwrap();
    age(&other, UNUSED_PROGRAM_LIFETIME * 2);
    drop(Runner::new(dir.path()).unwrap());
    assert!(other.exists());
}

#[test]
fn program_sources_are_recognised_by_name() {
    for name in ["p0.cpp", "p12345678901234567890.cpp"] {
        assert!(is_program_source(Path::new(name)), "{name}");
    }
    for name in ["p.cpp", "pabc.cpp", "q12.cpp", "p12.exe", "p12", "notes.cpp", "p12.cpp.bak"] {
        assert!(!is_program_source(Path::new(name)), "{name}");
    }
}
