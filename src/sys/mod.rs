#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::{is_out_of_memory, limit, wait};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{is_out_of_memory, limit, wait};

#[cfg(not(any(unix, windows)))]
compile_error!("cpprunner supports only Unix and Windows");
