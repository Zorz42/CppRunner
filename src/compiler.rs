use crate::{Error, Result};
use log::{debug, trace};
use std::ffi::OsStr;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

pub const COMPILER_ENV_VAR: &str = "GCC_PATH";

pub const SCRATCH_PREFIX: &str = ".scratch-";

pub fn canonicalize(path: &Path) -> Result<PathBuf> {
    dunce::canonicalize(path).map_err(Error::io(path))
}

fn find_compiler(from_env: Option<&OsStr>) -> Result<PathBuf> {
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
fn install(scratch: &Path, destination: &Path) -> Result<()> {
    std::fs::rename(scratch, destination).or_else(|err| {
        let _ = std::fs::remove_file(scratch);
        if destination.exists() { Ok(()) } else { Err(Error::io(destination)(err)) }
    })
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct CompiledProgram {
    pub source: PathBuf,
    pub executable: PathBuf,
    pub hash: u64,
}

/// ```no_run
/// use cpprunner::Compiler;
///
/// let compiler = Compiler::detect()?.with_standard("c++20").with_optimization("3").with_arg("-DLOCAL");
/// compiler.compile("solution.cpp".as_ref(), "solution".as_ref())?;
/// # Ok::<(), cpprunner::Error>(())
/// ```
#[derive(Clone, Debug)]
pub struct Compiler {
    path: PathBuf,
    standard: String,
    optimization: String,
    extra_args: Vec<String>,
    links_statically: OnceLock<bool>,
}

impl Compiler {
    pub fn detect() -> Result<Self> {
        Ok(Self::at(find_compiler(std::env::var_os(COMPILER_ENV_VAR).as_deref())?))
    }

    pub fn with_path<P: AsRef<Path>>(compiler: P) -> Result<Self> {
        Ok(Self::at(which::which(compiler.as_ref()).map_err(|_not_found| Error::CompilerNotFound)?))
    }

    fn at(path: PathBuf) -> Self {
        debug!("Using the C++ compiler {}", path.display());
        Self {
            path,
            standard: "c++17".to_owned(),
            optimization: "2".to_owned(),
            extra_args: Vec::new(),
            links_statically: OnceLock::new(),
        }
    }

    #[must_use]
    pub fn with_standard(mut self, standard: &str) -> Self {
        standard.clone_into(&mut self.standard);
        self
    }

    #[must_use]
    pub fn with_optimization(mut self, level: &str) -> Self {
        level.clone_into(&mut self.optimization);
        self
    }

    /// Extra arguments go after the source file, so that libraries such as `-lm` link.
    #[must_use]
    pub fn with_arg(mut self, arg: &str) -> Self {
        self.extra_args.push(arg.to_owned());
        self
    }

    #[must_use]
    pub fn with_args<I: IntoIterator<Item = S>, S: AsRef<str>>(mut self, args: I) -> Self {
        self.extra_args.extend(args.into_iter().map(|arg| arg.as_ref().to_owned()));
        self
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn flags(&self) -> Vec<String> {
        let mut flags = vec![format!("-std={}", self.standard), format!("-O{}", self.optimization)];
        flags.extend(self.extra_args.iter().cloned());
        flags
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
        command.arg(format!("-std={}", self.standard)).arg(format!("-O{}", self.optimization));

        #[cfg(windows)]
        command.args(["-static", "-Wl,--stack,536870912"]);

        #[cfg(target_os = "macos")]
        command.arg("-Wl,-stack_size,0x20000000");

        if cfg!(target_os = "linux") {
            let links_statically = *self.links_statically.get_or_init(|| has_static_libraries(&self.path));
            command.arg(if links_statically { "-static" } else { "-Wl,--as-needed" });
        }

        command.arg(source).arg("-o").arg(executable).args(&self.extra_args);

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

    #[must_use]
    pub fn program_hash(&self, source_code: &str) -> u64 {
        let mut state = DefaultHasher::new();
        source_code.hash(&mut state);
        self.path.hash(&mut state);
        self.standard.hash(&mut state);
        self.optimization.hash(&mut state);
        self.extra_args.hash(&mut state);
        env!("CARGO_PKG_VERSION").hash(&mut state);
        state.finish()
    }

    pub fn compile_cached(&self, build_folder: &Path, source_code: &str) -> Result<CompiledProgram> {
        std::fs::create_dir_all(build_folder).map_err(Error::io(build_folder))?;
        self.compile_cached_in(&canonicalize(build_folder)?, source_code)
    }

    /// Compiles in a private scratch folder and renames the results into place, so that threads and
    /// processes sharing the build folder never see half-written files.
    pub(crate) fn compile_cached_in(&self, build_folder: &Path, source_code: &str) -> Result<CompiledProgram> {
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
        Ok(CompiledProgram { source, executable, hash })
    }
}
