use cpprunner::{COMPILER_ENV_VAR, CppRunner, Error};
use tempfile::TempDir;

fn runner_with(build_folder: &TempDir, compiler: Option<&str>) -> cpprunner::Result<CppRunner> {
    match compiler {
        Some(value) => unsafe { std::env::set_var(COMPILER_ENV_VAR, value) },
        None => unsafe { std::env::remove_var(COMPILER_ENV_VAR) },
    }
    CppRunner::new(build_folder.path())
}

/// The only test in this binary that touches the environment, so it races with nothing.
#[test]
fn env_var_selects_the_compiler() {
    let dir = TempDir::new().unwrap();
    runner_with(&dir, None).unwrap();
    runner_with(&dir, Some("")).unwrap();
    assert!(matches!(runner_with(&dir, Some("no-such-compiler-cpprunner")), Err(Error::CompilerNotFound)));
    assert!(matches!(runner_with(&dir, Some("/no/such/dir/g++")), Err(Error::CompilerNotFound)));

    let compiler = if cfg!(windows) { "g++" } else { "c++" };
    let mut runner = runner_with(&dir, Some(compiler)).unwrap();
    runner.add_program("int main() {}").unwrap();

    runner_with(&dir, None).unwrap();
}

#[test]
fn compiler_not_found_names_the_env_var() {
    assert!(Error::CompilerNotFound.to_string().contains(COMPILER_ENV_VAR));
}
