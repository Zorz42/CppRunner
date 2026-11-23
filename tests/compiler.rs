mod common;

use common::{HELLO_WORLD, files_in};
use cpprunner::{COMPILER_ENV_VAR, Compiler, Error};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn compile_with(compiler: &Compiler, dir: &Path, source_code: &str) -> cpprunner::Result<PathBuf> {
    let source = dir.join("test.cpp");
    std::fs::write(&source, source_code).unwrap();
    let executable = Compiler::executable_path(&source)?;
    compiler.compile(&source, &executable)?;
    Ok(executable)
}

fn compile(dir: &Path, source_code: &str) -> cpprunner::Result<PathBuf> {
    compile_with(&Compiler::detect()?, dir, source_code)
}

fn stdout_of(executable: &Path) -> String {
    let output = Command::new(executable).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn detect_finds_an_executable() {
    let compiler = Compiler::detect().unwrap();
    assert!(compiler.path().is_absolute(), "{:?}", compiler.path());
    assert!(compiler.path().is_file(), "{:?}", compiler.path());
}

#[test]
fn with_path_accepts_a_name_or_a_path() {
    let detected = Compiler::detect().unwrap();
    let by_path = Compiler::with_path(detected.path()).unwrap();
    assert_eq!(by_path.path(), detected.path());

    let name = detected.path().file_name().unwrap().to_owned();
    let by_name = Compiler::with_path(&name).unwrap();
    assert_eq!(by_name.path().file_name().unwrap(), name);
}

#[test]
fn with_path_to_nothing_is_compiler_not_found() {
    assert!(matches!(Compiler::with_path("no-such-compiler-cpprunner"), Err(Error::CompilerNotFound)));
    assert!(matches!(Compiler::with_path("/no/such/dir/g++"), Err(Error::CompilerNotFound)));
}

#[test]
fn compiler_not_found_names_the_env_var() {
    assert!(Error::CompilerNotFound.to_string().contains(COMPILER_ENV_VAR));
}

#[test]
fn default_flags() {
    assert_eq!(Compiler::detect().unwrap().flags(), ["-std=c++17", "-O2"]);
}

#[test]
fn builder_sets_flags() {
    let compiler = Compiler::detect().unwrap().with_standard("c++20").with_optimization("0").with_arg("-DA").with_args(["-DB", "-DC"]);
    assert_eq!(compiler.flags(), ["-std=c++20", "-O0", "-DA", "-DB", "-DC"]);
}

#[test]
fn compile_and_run() {
    let dir = TempDir::new().unwrap();
    let executable = compile(dir.path(), "#include <iostream>\nint main() { std::cout << 1234567 << std::endl; }").unwrap();
    assert!(stdout_of(&executable).contains("1234567"));
}

#[test]
fn compile_error_keeps_the_messages() {
    let dir = TempDir::new().unwrap();
    let result = compile(dir.path(), "int main() { undeclared_identifier_xyz; }");
    let Err(Error::CompilerError { stderr, .. }) = result else {
        unreachable!("expected a compiler error, got {result:?}")
    };
    assert!(stderr.contains("undeclared_identifier_xyz"), "{stderr}");
    assert!(!dir.path().join("test").exists());
}

#[test]
fn missing_source_is_a_compiler_error() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("missing.cpp");
    let result = Compiler::detect().unwrap().compile(&source, &dir.path().join("missing"));
    assert!(matches!(result, Err(Error::CompilerError { .. })), "{result:?}");
}

#[test]
fn standard_is_applied() {
    let dir = TempDir::new().unwrap();
    let cpp20 = "consteval int one() { return 1; }\nint main() { return one() - 1; }";
    let detected = Compiler::detect().unwrap();
    assert!(matches!(compile_with(&detected.clone().with_standard("c++11"), dir.path(), cpp20), Err(Error::CompilerError { .. })));
    compile_with(&detected.with_standard("c++20"), dir.path(), cpp20).unwrap();
}

#[test]
fn extra_args_are_applied() {
    let dir = TempDir::new().unwrap();
    let source = "#include <iostream>\n#include <cmath>\nint main() { std::cout << VALUE << ' ' << (int)std::sqrt(49.0); }";
    let compiler = Compiler::detect().unwrap().with_args(["-DVALUE=42", "-lm"]);
    assert_eq!(stdout_of(&compile_with(&compiler, dir.path(), source).unwrap()), "42 7");
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
fn program_hash_depends_on_source_and_flags() {
    let compiler = Compiler::detect().unwrap();
    assert_eq!(compiler.program_hash(HELLO_WORLD), compiler.program_hash(HELLO_WORLD));
    assert_eq!(compiler.program_hash(HELLO_WORLD), Compiler::detect().unwrap().program_hash(HELLO_WORLD));
    assert_ne!(compiler.program_hash(HELLO_WORLD), compiler.program_hash("int main() {}"));
    assert_ne!(compiler.program_hash(HELLO_WORLD), compiler.clone().with_standard("c++20").program_hash(HELLO_WORLD));
    assert_ne!(compiler.program_hash(HELLO_WORLD), compiler.clone().with_optimization("0").program_hash(HELLO_WORLD));
    assert_ne!(compiler.program_hash(HELLO_WORLD), compiler.with_arg("-DX").program_hash(HELLO_WORLD));
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

    assert_eq!(first, second);
    assert_eq!(first.hash, compiler.program_hash(HELLO_WORLD));
    assert_eq!(std::fs::metadata(&second.executable).unwrap().modified().unwrap(), modified);
    assert_eq!(files_in(dir.path()).len(), 2, "{:?}", files_in(dir.path()));
}

#[test]
fn compile_cached_failure_keeps_only_the_source() {
    let dir = TempDir::new().unwrap();
    let compiler = Compiler::detect().unwrap();
    let result = compiler.compile_cached(dir.path(), "int main() { oops }");
    assert!(matches!(result, Err(Error::CompilerError { .. })), "{result:?}");

    let files = files_in(dir.path());
    assert_eq!(files.len(), 1, "{files:?}");
    assert_eq!(files[0].extension().unwrap(), "cpp");
}
