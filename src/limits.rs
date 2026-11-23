use serde::{Deserialize, Serialize};
use std::time::Duration;

/// The default [`Limits::time_ms`].
pub const DEFAULT_TIME_MS: u64 = 1000;
/// The default [`Limits::output_bytes`]: 256 MiB.
pub const DEFAULT_OUTPUT_BYTES: u64 = 256 << 20;

/// What a single run may use.
///
/// ```
/// use cpprunner::Limits;
///
/// let limits = Limits::new().with_time_ms(2000).with_memory_mb(256);
/// assert_eq!(limits.time_ms, 2000);
/// assert_eq!(limits.memory_kb, Some(256 << 10));
/// assert_eq!(Limits::default(), Limits::new());
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct Limits {
    /// CPU time in milliseconds; zero counts as one. A program that blocks is stopped after twice
    /// this plus two seconds of wall time.
    pub time_ms: u64,
    /// Peak memory in kilobytes, or `None` for no limit.
    pub memory_kb: Option<u64>,
    /// The most bytes a program may write to stdout.
    pub output_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self::new()
    }
}

impl Limits {
    /// [`DEFAULT_TIME_MS`], no memory limit and [`DEFAULT_OUTPUT_BYTES`].
    #[must_use]
    pub const fn new() -> Self {
        Self {
            time_ms: DEFAULT_TIME_MS,
            memory_kb: None,
            output_bytes: DEFAULT_OUTPUT_BYTES,
        }
    }

    /// Sets [`Limits::time_ms`].
    #[must_use]
    pub const fn with_time_ms(mut self, time_ms: u64) -> Self {
        self.time_ms = time_ms;
        self
    }

    /// Sets [`Limits::memory_kb`].
    #[must_use]
    pub const fn with_memory_kb(mut self, memory_kb: Option<u64>) -> Self {
        self.memory_kb = memory_kb;
        self
    }

    /// Sets [`Limits::memory_kb`] in megabytes.
    #[must_use]
    pub const fn with_memory_mb(self, memory_mb: u64) -> Self {
        self.with_memory_kb(Some(memory_mb.saturating_mul(1024)))
    }

    /// Sets [`Limits::output_bytes`].
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
