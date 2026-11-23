use crate::Limits;
use crate::limits::{DEFAULT_OUTPUT_BYTES, DEFAULT_TIME_MS};
use std::time::Duration;

#[test]
fn cpu_seconds_round_up() {
    assert_eq!(Limits::new().with_time_ms(1).cpu_seconds(), 1);
    assert_eq!(Limits::new().with_time_ms(1000).cpu_seconds(), 1);
    assert_eq!(Limits::new().with_time_ms(1001).cpu_seconds(), 2);
    assert_eq!(Limits::new().with_time_ms(0).cpu_seconds(), 1);
}

#[test]
fn zero_time_counts_as_one_millisecond() {
    assert_eq!(Limits::new().with_time_ms(0).effective_time_ms(), 1);
    assert_eq!(Limits::new().with_time_ms(7).effective_time_ms(), 7);
}

#[test]
fn memory_cap_is_far_above_the_limit() {
    assert_eq!(Limits::new().memory_cap_bytes(), None);
    assert_eq!(Limits::new().with_memory_mb(64).memory_cap_bytes(), Some((128 << 20) + (1 << 30)));
}

#[test]
fn wall_clock_is_generous() {
    assert_eq!(Limits::new().with_time_ms(1000).wall_clock(), Duration::from_secs(4));
}

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
