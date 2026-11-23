#![allow(dead_code)]

use cpprunner::{Limits, RunResult, Runner};
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

pub fn runner() -> (TempDir, Runner) {
    let dir = TempDir::new().unwrap();
    let runner = Runner::new(dir.path()).unwrap();
    (dir, runner)
}

pub fn run_on(runner: &Runner, source: &str, input: &str, limits: Limits) -> cpprunner::Result<RunResult> {
    runner.spawn(source, input, limits)?.wait()
}

pub const fn limits(time_ms: u64) -> Limits {
    Limits::new().with_time_ms(time_ms)
}

pub fn run_with(source: &str, input: &str, limits: Limits) -> RunResult {
    let (_dir, runner) = runner();
    run_on(&runner, source, input, limits).unwrap()
}

pub fn run(source: &str, input: &str, time_ms: u64) -> RunResult {
    run_with(source, input, limits(time_ms))
}

pub fn output(result: &RunResult) -> Option<&str> {
    match result {
        RunResult::Ok { output, .. } => Some(output),
        _ => None,
    }
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
        .filter(|path| path.is_file() && path.file_name().is_some_and(|name| name != "results.cache") && path.extension().is_none_or(|extension| extension != "cpp"))
        .collect()
}

/// A program that appends a line to `log` every time it really runs, then echoes its input.
pub fn logging_echo(log: &Path) -> String {
    let log = log.to_string_lossy().replace('\\', "/");
    format!(
        "#include <fstream>\n#include <iostream>\n#include <string>
        int main() {{ std::string s; std::cin >> s; std::ofstream(\"{log}\", std::ios::app) << s << '\\n'; std::cout << s; }}"
    )
}

pub fn runs_logged(log: &Path) -> Vec<String> {
    std::fs::read_to_string(log).unwrap_or_default().lines().map(str::to_owned).collect()
}
