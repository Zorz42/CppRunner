use crate::compiler::COMPILER_ENV_VAR;
use crate::{Error, Runner};
use std::process::Command;

const IN_CHILD: &str = "CPPRUNNER_TEST_IN_CHILD";

/// The environment is shared by the whole process, so the runner is made in a child process of this
/// test binary, running only `runner_in_child`.
fn finds_compiler(compiler: Option<&str>) -> bool {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", "tests::env_var_tests::runner_in_child", "--nocapture", "--test-threads=1"]).env(IN_CHILD, "1");
    match compiler {
        Some(compiler) => command.env(COMPILER_ENV_VAR, compiler),
        None => command.env_remove(COMPILER_ENV_VAR),
    };
    let output = command.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}{}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("1 passed"), "the child test did not run: {stdout}");
    stdout.contains("compiler found")
}

#[test]
fn runner_in_child() {
    if std::env::var_os(IN_CHILD).is_none() {
        return;
    }
    let dir = tempfile::TempDir::new().unwrap();
    let runner = Runner::new(dir.path());
    if matches!(runner, Err(Error::CompilerNotFound)) {
        println!("no compiler");
        return;
    }
    runner.unwrap().spawn("int main() {}", "", crate::Limits::default()).unwrap().wait().unwrap();
    println!("compiler found");
}

#[test]
fn env_var_selects_the_compiler() {
    assert!(finds_compiler(None));
    assert!(finds_compiler(Some("")));
    assert!(!finds_compiler(Some("no-such-compiler-cpprunner")));
    assert!(!finds_compiler(Some("/no/such/dir/g++")));
    assert!(finds_compiler(Some(if cfg!(windows) { "g++" } else { "c++" })));
}

#[test]
fn compiler_not_found_names_the_env_var() {
    assert!(Error::CompilerNotFound.to_string().contains(COMPILER_ENV_VAR));
}
