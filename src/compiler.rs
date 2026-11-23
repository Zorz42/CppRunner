use crate::{Error, Result};
use log::{debug, trace};
use std::ffi::OsStr;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// Names the C++ compiler to use, as a path or a program on `PATH`. If unset, the first of `g++`,
/// `c++` and `clang++` found is used.
pub const COMPILER_ENV_VAR: &str = "GCC_PATH";

pub const SCRATCH_PREFIX: &str = ".scratch-";

pub fn canonicalize(path: &Path) -> Result<PathBuf> {
    dunce::canonicalize(path).map_err(Error::io(path))
}

pub fn find_compiler(from_env: Option<&OsStr>) -> Result<PathBuf> {
    if let Some(compiler) = from_env.filter(|compiler| !compiler.is_empty()) {
        return which::which(compiler).map_err(|_not_found| Error::CompilerNotFound);
    }

    let candidates = if cfg!(windows) {
        ["g++", "mingw32-g++", "x86_64-w64-mingw32-g++", "c++"].as_slice()
    } else {
        ["g++", "c++", "clang++"].as_slice()
    };

    if let Some(path) = candidates.iter().find_map(|candidate| which::which(candidate).ok()) {
        return Ok(path);
    }

    #[cfg(windows)]
    {
        let possible_dirs = [
            "C:\\msys64\\ucrt64\\bin",
            "C:\\msys64\\mingw64\\bin",
            "C:\\msys64\\mingw32\\bin",
            "C:\\msys32\\mingw32\\bin",
            "C:\\MinGW\\bin",
            "C:\\mingw64\\bin",
            "C:\\mingw-w64\\bin",
            "C:\\ProgramData\\chocolatey\\bin",
            "C:\\Program Files\\mingw64\\bin",
        ];

        for dir in possible_dirs {
            for candidate in candidates {
                let path = PathBuf::from(dir).join(format!("{candidate}.exe"));
                if path.is_file() {
                    return Ok(path);
                }
            }
        }
    }

    Err(Error::CompilerNotFound)
}

fn has_static_libraries(compiler: &Path) -> bool {
    ["libstdc++.a", "libc.a"].iter().all(|library| {
        let found = Command::new(compiler).arg(format!("-print-file-name={library}")).output();
        found.is_ok_and(|output| Path::new(String::from_utf8_lossy(&output.stdout).trim()).is_absolute())
    })
}

/// On Windows the rename fails while `destination` is open, but names are content hashes, so an
/// existing `destination` is already correct.
pub fn install(scratch: &Path, destination: &Path) -> Result<()> {
    std::fs::rename(scratch, destination).or_else(|err| {
        let _ = std::fs::remove_file(scratch);
        if destination.exists() { Ok(()) } else { Err(Error::io(destination)(err)) }
    })
}

pub struct CompiledProgram {
    pub source: PathBuf,
    pub executable: PathBuf,
}

pub struct Compiler {
    path: PathBuf,
    links_statically: OnceLock<bool>,
}

impl Compiler {
    pub fn detect() -> Result<Self> {
        let path = find_compiler(std::env::var_os(COMPILER_ENV_VAR).as_deref())?;
        debug!("Using the C++ compiler {}", path.display());
        Ok(Self {
            path,
            links_statically: OnceLock::new(),
        })
    }

    pub fn executable_path(source: &Path) -> Result<PathBuf> {
        let mut executable = source.with_extension(std::env::consts::EXE_EXTENSION);
        if executable == source {
            let mut file_name = executable.file_name().unwrap_or_default().to_os_string();
            file_name.push("_bin");
            executable.set_file_name(file_name);
        }

        let file_name = executable
            .file_name()
            .ok_or_else(|| Error::io(source)(std::io::Error::new(std::io::ErrorKind::InvalidInput, "the path has no file name")))?;
        let parent = executable.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
        Ok(canonicalize(parent)?.join(file_name))
    }

    pub fn compile(&self, source: &Path, executable: &Path) -> Result<()> {
        let mut command = Command::new(&self.path);
        command.args(["-std=c++17", "-O2"]);

        #[cfg(windows)]
        command.args(["-static", "-Wl,--stack,536870912"]);

        #[cfg(target_os = "macos")]
        command.arg("-Wl,-stack_size,0x20000000");

        if cfg!(target_os = "linux") {
            let links_statically = *self.links_statically.get_or_init(|| has_static_libraries(&self.path));
            command.arg(if links_statically { "-static" } else { "-Wl,--as-needed" });
        }

        command.arg(source).arg("-o").arg(executable);

        debug!("Running command: {command:?}");
        let process = command.output().map_err(Error::io(&self.path))?;

        if !process.status.success() {
            return Err(Error::CompilerError {
                stderr: String::from_utf8_lossy(&process.stderr).into_owned(),
                stdout: String::from_utf8_lossy(&process.stdout).into_owned(),
            });
        }

        if !executable.exists() {
            return Err(Error::CompilerError {
                stderr: "Output file was not created".to_owned(),
                stdout: String::new(),
            });
        }

        Ok(())
    }

    pub fn program_hash(&self, source_code: &str) -> u64 {
        let mut state = DefaultHasher::new();
        source_code.hash(&mut state);
        self.path.hash(&mut state);
        env!("CARGO_PKG_VERSION").hash(&mut state);
        state.finish()
    }

    /// `build_folder` must be canonical. Compiles in a private scratch folder and renames the results into place, so that threads and
    /// processes sharing the build folder never see half-written files.
    pub fn compile_cached(&self, build_folder: &Path, source_code: &str) -> Result<CompiledProgram> {
        let hash = self.program_hash(source_code);
        let source = build_folder.join(format!("p{hash}.cpp"));
        let executable = Self::executable_path(&source)?;
        if !executable.exists() {
            trace!("Compiling: {}", executable.display());
            let scratch = tempfile::Builder::new().prefix(SCRATCH_PREFIX).tempdir_in(build_folder).map_err(Error::io(build_folder))?;
            let scratch_source = scratch.path().join(format!("p{hash}.cpp"));
            std::fs::write(&scratch_source, source_code).map_err(Error::io(&scratch_source))?;
            let scratch_executable = Self::executable_path(&scratch_source)?;
            let compiled = self.compile(&scratch_source, &scratch_executable).and_then(|()| install(&scratch_executable, &executable));
            install(&scratch_source, &source)?;
            compiled?;
        }
        Ok(CompiledProgram { source, executable })
    }
}
