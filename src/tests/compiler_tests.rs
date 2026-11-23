use crate::compiler::{Compiler, canonicalize, find_compiler, install};
use crate::tests::common::{HELLO_WORLD, files_in};
use crate::{Error, Runner};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn compile(dir: &Path, source_code: &str) -> crate::Result<PathBuf> {
    let source = dir.join("test.cpp");
    std::fs::write(&source, source_code).unwrap();
    let executable = Compiler::executable_path(&source);
    Compiler::detect()?.compile(&source, &executable)?;
    Ok(executable)
}

fn stdout_of(executable: &Path) -> String {
    let output = Command::new(executable).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn env_var_names_the_compiler() {
    let fallback = find_compiler(None).unwrap();
    let name = fallback.file_name().unwrap();
    assert_eq!(find_compiler(Some(fallback.as_os_str())).unwrap(), fallback);
    assert_eq!(find_compiler(Some(name)).unwrap().file_name().unwrap(), name);
}

#[test]
fn env_var_naming_no_compiler_is_an_error_without_fallback() {
    assert!(matches!(find_compiler(Some(OsStr::new("no-such-compiler-cpprunner"))), Err(Error::CompilerNotFound)));
    assert!(matches!(find_compiler(Some(OsStr::new("/no/such/dir/g++"))), Err(Error::CompilerNotFound)));
}

#[test]
fn empty_env_var_counts_as_unset() {
    assert_eq!(find_compiler(Some(OsStr::new(""))).unwrap(), find_compiler(None).unwrap());
}

#[test]
fn detect_finds_an_executable() {
    let path = find_compiler(None).unwrap();
    assert!(path.is_absolute(), "{path:?}");
    assert!(path.is_file(), "{path:?}");
}

#[test]
fn compile_and_run() {
    let dir = TempDir::new().unwrap();
    let executable = compile(dir.path(), "#include <iostream>\nint main() { std::cout << 1234567 << std::endl; }").unwrap();
    assert!(stdout_of(&executable).contains("1234567"));
}

#[test]
fn compiles_as_cpp17() {
    let dir = TempDir::new().unwrap();
    compile(dir.path(), "#include <optional>\nint main() { std::optional<int> x = 1; if constexpr (true) { return *x - 1; } }").unwrap();
}

#[test]
fn compile_error_keeps_the_messages() {
    let dir = TempDir::new().unwrap();
    let result = compile(dir.path(), "int main() { undeclared_identifier_xyz; }");
    let Err(Error::CompilerError { stderr, .. }) = result else {
        unreachable!("expected a compiler error, got {result:?}")
    };
    assert!(stderr.contains("undeclared_identifier_xyz"), "{stderr}");
}

#[test]
fn missing_source_is_a_compiler_error() {
    let dir = TempDir::new().unwrap();
    let result = Compiler::detect().unwrap().compile(&dir.path().join("missing.cpp"), &dir.path().join("missing"));
    assert!(matches!(result, Err(Error::CompilerError { .. })), "{result:?}");
}

#[test]
fn executable_path_sits_next_to_the_source() {
    let executable = Compiler::executable_path(Path::new("/build/p1.cpp"));
    #[cfg(windows)]
    assert_eq!(executable, Path::new("/build/p1.exe"));
    #[cfg(unix)]
    assert_eq!(executable, Path::new("/build/p1"));
}

#[test]
fn program_hash_depends_on_the_source() {
    let compiler = Compiler::detect().unwrap();
    assert_eq!(compiler.program_hash(HELLO_WORLD), compiler.program_hash(HELLO_WORLD));
    assert_eq!(compiler.program_hash(HELLO_WORLD), Compiler::detect().unwrap().program_hash(HELLO_WORLD));
    assert_ne!(compiler.program_hash(HELLO_WORLD), compiler.program_hash("int main() {}"));
}

#[test]
fn compile_cached_reuses_the_executable() {
    let dir = TempDir::new().unwrap();
    let build_folder = canonicalize(dir.path()).unwrap();
    let compiler = Compiler::detect().unwrap();
    let first = compiler.compile_cached(&build_folder, HELLO_WORLD).unwrap();
    let modified = std::fs::metadata(&first.executable).unwrap().modified().unwrap();

    let second = compiler.compile_cached(&build_folder, HELLO_WORLD).unwrap();

    assert_eq!((&first.source, &first.executable), (&second.source, &second.executable));
    assert_eq!(std::fs::read_to_string(&first.source).unwrap(), HELLO_WORLD);
    assert!(stdout_of(&first.executable).contains("Hello, World!"));
    assert_eq!(std::fs::metadata(&second.executable).unwrap().modified().unwrap(), modified);
    assert_eq!(files_in(dir.path()).len(), 2, "{:?}", files_in(dir.path()));
}

#[test]
fn compile_cached_failure_keeps_only_the_source() {
    let dir = TempDir::new().unwrap();
    let result = Compiler::detect().unwrap().compile_cached(&canonicalize(dir.path()).unwrap(), "int main() { oops }");
    assert!(matches!(result, Err(Error::CompilerError { .. })));

    let files = files_in(dir.path());
    assert_eq!(files.len(), 1, "{files:?}");
    assert_eq!(files[0].extension().unwrap(), "cpp");
}

#[test]
fn install_keeps_an_existing_destination() {
    let dir = TempDir::new().unwrap();
    let (scratch, destination) = (dir.path().join("scratch"), dir.path().join("destination"));
    std::fs::write(&scratch, "new").unwrap();
    install(&scratch, &destination).unwrap();
    assert_eq!(std::fs::read_to_string(&destination).unwrap(), "new");
    assert!(!scratch.exists());

    install(&scratch, &destination).unwrap();
    assert!(install(&scratch, &dir.path().join("missing")).is_err());
}

#[test]
fn runner_reuses_what_another_compiled() {
    let dir = TempDir::new().unwrap();
    let compiled = Compiler::detect().unwrap().compile_cached(&canonicalize(dir.path()).unwrap(), HELLO_WORLD).unwrap();
    let modified = std::fs::metadata(&compiled.executable).unwrap().modified().unwrap();

    Runner::new(dir.path()).unwrap().spawn(HELLO_WORLD, "", crate::Limits::default()).unwrap().wait().unwrap();

    assert_eq!(std::fs::metadata(&compiled.executable).unwrap().modified().unwrap(), modified);
    assert_eq!(files_in(dir.path()).into_iter().filter(|file| !file.ends_with("results.cache")).count(), 2);
}
