use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("I/O error on {}: {source}", path.display())]
    Io { source: std::io::Error, path: PathBuf },

    #[error(
        "C++ compiler is not found. Make sure to install it first. If it is already installed, \
         specify the path to the compiler with the {} environment variable.",
        crate::COMPILER_ENV_VAR
    )]
    CompilerNotFound,

    #[error("Compiler error: {stderr}\n{stdout}")]
    CompilerError { stderr: String, stdout: String },

    #[error("Could not run a program: {details}")]
    RunFailed { details: String },
}

impl Error {
    pub(crate) fn io(path: &Path) -> impl FnOnce(std::io::Error) -> Self + '_ {
        |source| Self::Io { source, path: path.to_path_buf() }
    }

    pub(crate) fn run_failed(details: impl Into<String>) -> Self {
        Self::RunFailed { details: details.into() }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
