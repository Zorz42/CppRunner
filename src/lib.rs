#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

mod cache;
mod check;
mod compiler;
mod error;
mod exec;
mod hash;
mod limits;
mod result;
mod runner;
mod sys;
#[cfg(test)]
mod tests;

pub use check::Check;
pub use compiler::COMPILER_ENV_VAR;
pub use error::{Error, Result};
pub use limits::Limits;
pub use result::RunResult;
pub use runner::{RunHandle, Runner};
