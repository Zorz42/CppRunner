use std::path::{Path, PathBuf};
use thiserror::Error;

/// Why a program could not be compiled or run. How a program behaved is a
/// [`RunResult`](crate::RunResult), not an error.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// A file or folder could not be read, written or created.
    #[error("I/O error on {}: {source}", path.display())]
    Io {
        /// The underlying error.
        source: std::io::Error,
        /// The file or folder it happened on.
        path: PathBuf,
    },

    /// No C++ compiler was found.
    #[error(
        "C++ compiler is not found. Make sure to install it first. If it is already installed, \
         specify the path to the compiler with the {} environment variable.",
        crate::compiler::COMPILER_ENV_VAR
    )]
    CompilerNotFound,

    /// The compiler rejected the program.
    #[error("Compiler error: {stderr}\n{stdout}")]
    CompilerError {
        /// The compiler's diagnostics.
        stderr: String,
        /// The compiler's standard output, usually empty.
        stdout: String,
    },

    /// A program could not be started or measured, for example because its executable is gone.
    #[error("Could not run a program: {details}")]
    RunFailed {
        /// What went wrong.
        details: String,
    },

    /// A checker program did not accept or reject: it crashed, timed out or exited with a code
    /// other than 0, 1 or 2.
    #[error("The checker failed: {details}")]
    CheckerFailed {
        /// How it ended.
        details: String,
    },

    /// The run was cancelled, or its runner dropped, before it started.
    #[error("The run was cancelled before it started")]
    Cancelled,
}

impl Error {
    pub(crate) fn io(path: &Path) -> impl FnOnce(std::io::Error) -> Self + '_ {
        |source| Self::Io { source, path: path.to_path_buf() }
    }

    pub(crate) fn run_failed(details: impl Into<String>) -> Self {
        Self::RunFailed { details: details.into() }
    }
}

/// A result with this crate's [`Error`](enum@Error).
pub type Result<T, E = Error> = std::result::Result<T, E>;
