use crate::compiler::{Compiler, find_compiler, install};
use crate::{CppRunner, Error};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

const HELLO_WORLD: &str = "#include <iostream>\nint main() { std::cout << \"Hello, World!\" << std::endl; }";

fn compile(dir: &Path, source_code: &str) -> crate::Result<PathBuf> {
    let source = dir.join("test.cpp");
    std::fs::write(&source, source_code).unwrap();
    let executable = Compiler::executable_path(&source)?;
    Compiler::detect()?.compile(&source, &executable)?;
    Ok(executable)
}

fn stdout_of(executable: &Path) -> String {
    let output = Command::new(executable).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

fn files_in(folder: &Path) -> Vec<PathBuf> {
    let mut files: Vec<_> = std::fs::read_dir(folder).unwrap().map(|entry| entry.unwrap().path()).collect();
    files.sort();
    files
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
fn executable_path_extension() {
    let executable = Compiler::executable_path(&TempDir::new().unwrap().path().join("foo.cpp")).unwrap();
    #[cfg(windows)]
    assert_eq!(executable.file_name().unwrap(), "foo.exe");
    #[cfg(unix)]
    assert_eq!(executable.file_name().unwrap(), "foo");
}

#[test]
fn executable_path_never_overwrites_the_source() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("noextension");
    std::fs::write(&source, "int main(){return 0;}").unwrap();

    let executable = Compiler::executable_path(&source).unwrap();

    assert_ne!(executable, source);
    assert!(executable.is_absolute(), "{executable:?}");
    assert!(!executable.exists(), "{executable:?}");
}

#[test]
fn executable_path_stays_in_its_directory() {
    let dir = TempDir::new().unwrap();
    let executable = Compiler::executable_path(&dir.path().join("prog.cpp")).unwrap();
    assert_eq!(executable.parent(), Some(dunce::canonicalize(dir.path()).unwrap().as_path()));
}

#[test]
fn executable_path_of_a_relative_source_is_absolute() {
    let executable = Compiler::executable_path(Path::new("prog.cpp")).unwrap();
    assert!(executable.is_absolute(), "{executable:?}");
    assert_eq!(executable.parent(), Some(dunce::canonicalize(".").unwrap().as_path()));
}

#[test]
fn executable_path_of_a_missing_directory_is_an_error() {
    assert!(matches!(Compiler::executable_path(Path::new("/no/such/dir/prog.cpp")), Err(Error::Io { .. })));
}

#[test]
fn program_hash_depends_on_the_source() {
    let compiler = Compiler::detect().unwrap();
    assert_eq!(compiler.program_hash(HELLO_WORLD), compiler.program_hash(HELLO_WORLD));
    assert_eq!(compiler.program_hash(HELLO_WORLD), Compiler::detect().unwrap().program_hash(HELLO_WORLD));
    assert_ne!(compiler.program_hash(HELLO_WORLD), compiler.program_hash("int main() {}"));
}

#[test]
fn compile_cached_creates_the_build_folder() {
    let dir = TempDir::new().unwrap();
    let build_folder = dir.path().join("a").join("b");
    let compiled = Compiler::detect().unwrap().compile_cached(&build_folder, HELLO_WORLD).unwrap();
    assert!(compiled.executable.is_file());
    assert_eq!(std::fs::read_to_string(&compiled.source).unwrap(), HELLO_WORLD);
    assert_eq!(compiled.executable.parent(), Some(dunce::canonicalize(&build_folder).unwrap().as_path()));
    assert!(stdout_of(&compiled.executable).contains("Hello, World!"));
}

#[test]
fn compile_cached_reuses_the_executable() {
    let dir = TempDir::new().unwrap();
    let compiler = Compiler::detect().unwrap();
    let first = compiler.compile_cached(dir.path(), HELLO_WORLD).unwrap();
    let modified = std::fs::metadata(&first.executable).unwrap().modified().unwrap();

    let second = compiler.compile_cached(dir.path(), HELLO_WORLD).unwrap();

    assert_eq!((&first.source, &first.executable, first.hash), (&second.source, &second.executable, second.hash));
    assert_eq!(first.hash, compiler.program_hash(HELLO_WORLD));
    assert_eq!(std::fs::metadata(&second.executable).unwrap().modified().unwrap(), modified);
    assert_eq!(files_in(dir.path()).len(), 2, "{:?}", files_in(dir.path()));
}

#[test]
fn compile_cached_failure_keeps_only_the_source() {
    let dir = TempDir::new().unwrap();
    let result = Compiler::detect().unwrap().compile_cached(dir.path(), "int main() { oops }");
    assert!(matches!(result, Err(Error::CompilerError { .. })));

    let files = files_in(dir.path());
    assert_eq!(files.len(), 1, "{files:?}");
    assert_eq!(files[0].extension().unwrap(), "cpp");
}

#[test]
fn runner_finds_what_compile_cached_compiled() {
    let dir = TempDir::new().unwrap();
    let compiled = Compiler::detect().unwrap().compile_cached(dir.path(), HELLO_WORLD).unwrap();
    let modified = std::fs::metadata(&compiled.executable).unwrap().modified().unwrap();

    let mut runner = CppRunner::new(dir.path()).unwrap();
    let program = runner.add_program(HELLO_WORLD).unwrap();

    assert_eq!(runner.executable(program), compiled.executable);
    assert_eq!(runner.program_hash(program), compiled.hash);
    assert_eq!(std::fs::metadata(&compiled.executable).unwrap().modified().unwrap(), modified);
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
