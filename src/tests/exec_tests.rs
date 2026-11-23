use crate::exec::{poll_interval, read_output};
use std::time::Duration;

#[test]
fn poll_interval_is_bounded() {
    assert_eq!(poll_interval(Duration::ZERO), Duration::from_micros(50));
    assert_eq!(poll_interval(Duration::from_millis(8)), Duration::from_millis(1));
    assert_eq!(poll_interval(Duration::from_secs(100)), Duration::from_millis(2));
}

#[test]
fn read_output_stops_past_the_limit() {
    assert_eq!(read_output(&b"12345"[..], 5).unwrap(), Some(b"12345".to_vec()));
    assert_eq!(read_output(&b"123456"[..], 5).unwrap(), None);
    assert_eq!(read_output(&b""[..], 0).unwrap(), Some(Vec::new()));
    assert_eq!(read_output(&b"x"[..], 0).unwrap(), None);
    assert_eq!(read_output(&b"x"[..], u64::MAX).unwrap(), Some(b"x".to_vec()));
}

#[test]
fn missing_executable_is_an_error() {
    let result = crate::exec::run_executable(std::path::Path::new("/no/such/executable"), &[], "", crate::Limits::default());
    assert!(matches!(result, Err(crate::Error::RunFailed { .. })), "{result:?}");
}

#[test]
fn non_executable_file_is_an_error() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), "not a program").unwrap();
    let result = crate::exec::run_executable(file.path(), &[], "", crate::Limits::default());
    assert!(matches!(result, Err(crate::Error::RunFailed { .. })), "{result:?}");
}
