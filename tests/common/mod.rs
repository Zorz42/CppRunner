#![allow(dead_code)]

use cpprunner::{CppRunner, Limits, RunResult};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::TempDir;

pub const HELLO_WORLD: &str = "#include <iostream>\nint main() { std::cout << \"Hello, World!\" << std::endl; }";
pub const ECHO: &str = "#include <iostream>\nint main() { long long n; std::cin >> n; std::cout << n; }";
pub const ALLOCATE: &str = "#include <iostream>\n#include <vector>
    int main() { size_t megabytes; std::cin >> megabytes; std::vector<char> memory(megabytes << 20, 1); long long sum = 0; for (char byte : memory) sum += byte; std::cout << sum; }";

pub fn run_within<F: FnOnce() -> T + Send + 'static, T: Send + 'static>(timeout: Duration, what: &str, body: F) -> T {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(body());
    });
    receiver.recv_timeout(timeout).unwrap_or_else(|_| unreachable!("{what} did not finish within {timeout:?}"))
}

pub fn runner() -> (TempDir, CppRunner) {
    let dir = TempDir::new().unwrap();
    let runner = CppRunner::new(dir.path()).unwrap();
    (dir, runner)
}

pub const fn limits(time_ms: u64) -> Limits {
    Limits::new().with_time_ms(time_ms)
}

pub fn run_with(source: &str, input: &str, limits: Limits) -> RunResult {
    let (_dir, mut runner) = runner();
    let program = runner.add_program(source).unwrap();
    runner.run(program, input, limits).unwrap()
}

pub fn run(source: &str, input: &str, time_ms: u64) -> RunResult {
    run_with(source, input, limits(time_ms))
}

pub fn assert_output(result: &RunResult, expected: &str) {
    assert!(matches!(result, RunResult::Ok { output, .. } if output.trim() == expected), "expected {expected:?}, got {result:?}");
}

pub fn files_in(folder: &Path) -> Vec<PathBuf> {
    let mut files: Vec<_> = std::fs::read_dir(folder).unwrap().map(|entry| entry.unwrap().path()).collect();
    files.sort();
    files
}

pub fn binaries_in(folder: &Path) -> Vec<PathBuf> {
    files_in(folder)
        .into_iter()
        .filter(|path| path.is_file() && path.extension().is_none_or(|extension| extension != "cpp"))
        .collect()
}
