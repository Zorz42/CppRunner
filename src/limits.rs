use std::time::Duration;

pub const DEFAULT_TIME_MS: u64 = 1000;
pub const DEFAULT_OUTPUT_BYTES: u64 = 256 << 20;

/// ```
/// use cpprunner::Limits;
///
/// let limits = Limits::new().with_time_ms(2000).with_memory_mb(256);
/// assert_eq!(limits.time_ms, 2000);
/// assert_eq!(limits.memory_kb, Some(256 << 10));
/// assert_eq!(Limits::default(), Limits::new());
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Limits {
    pub time_ms: u64,
    pub memory_kb: Option<u64>,
    pub output_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self::new()
    }
}

impl Limits {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            time_ms: DEFAULT_TIME_MS,
            memory_kb: None,
            output_bytes: DEFAULT_OUTPUT_BYTES,
        }
    }

    #[must_use]
    pub const fn with_time_ms(mut self, time_ms: u64) -> Self {
        self.time_ms = time_ms;
        self
    }

    #[must_use]
    pub const fn with_memory_kb(mut self, memory_kb: Option<u64>) -> Self {
        self.memory_kb = memory_kb;
        self
    }

    #[must_use]
    pub const fn with_memory_mb(self, memory_mb: u64) -> Self {
        self.with_memory_kb(Some(memory_mb.saturating_mul(1024)))
    }

    #[must_use]
    pub const fn with_output_bytes(mut self, output_bytes: u64) -> Self {
        self.output_bytes = output_bytes;
        self
    }

    pub(crate) const fn effective_time_ms(self) -> u64 {
        if self.time_ms == 0 { 1 } else { self.time_ms }
    }

    pub(crate) const fn cpu_seconds(self) -> u64 {
        self.effective_time_ms().div_ceil(1000)
    }

    /// Only stops runaway programs; the measured peak is what is compared with the limit.
    pub(crate) fn memory_cap_bytes(self) -> Option<u64> {
        self.memory_kb.map(|memory_kb| memory_kb.saturating_mul(2048).saturating_add(1 << 30))
    }

    pub(crate) const fn wall_clock(self) -> Duration {
        Duration::from_millis(self.effective_time_ms().saturating_mul(2).saturating_add(2000))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_seconds_round_up() {
        assert_eq!(Limits::new().with_time_ms(1).cpu_seconds(), 1);
        assert_eq!(Limits::new().with_time_ms(1000).cpu_seconds(), 1);
        assert_eq!(Limits::new().with_time_ms(1001).cpu_seconds(), 2);
        assert_eq!(Limits::new().with_time_ms(0).cpu_seconds(), 1);
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
    }
}
