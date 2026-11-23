use cpprunner::{DEFAULT_OUTPUT_BYTES, DEFAULT_TIME_MS, Limits};

#[test]
fn defaults() {
    let limits = Limits::default();
    assert_eq!(limits, Limits::new());
    assert_eq!(limits.time_ms, DEFAULT_TIME_MS);
    assert_eq!(limits.memory_kb, None);
    assert_eq!(limits.output_bytes, DEFAULT_OUTPUT_BYTES);
}

#[test]
fn builders_set_fields() {
    let limits = Limits::new().with_time_ms(5).with_memory_mb(2).with_output_bytes(7);
    assert_eq!(
        limits,
        Limits {
            time_ms: 5,
            memory_kb: Some(2048),
            output_bytes: 7
        }
    );
    assert_eq!(limits.with_memory_kb(None).memory_kb, None);
    assert_eq!(limits.with_memory_kb(Some(3)).memory_kb, Some(3));
}

#[test]
fn memory_mb_saturates() {
    assert_eq!(Limits::new().with_memory_mb(u64::MAX).memory_kb, Some(u64::MAX));
}
