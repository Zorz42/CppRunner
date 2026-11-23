use crate::Limits;
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
    assert_eq!(Limits::new().with_memory_kb(Some(u64::MAX)).memory_cap_bytes(), Some(u64::MAX));
}

#[test]
fn wall_clock_is_generous() {
    assert_eq!(Limits::new().with_time_ms(1000).wall_clock(), Duration::from_secs(4));
    assert_eq!(Limits::new().with_time_ms(u64::MAX).wall_clock(), Duration::from_millis(u64::MAX));
}
