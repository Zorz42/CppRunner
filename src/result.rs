#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RunResult {
    Ok { time_ms: u64, memory_kb: u64, output: String },
    TimedOut,
    MemoryLimitExceeded,
    Crashed(CrashReason),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CrashReason {
    ExitCode(i32),
    Signal(i32),
    OutputLimitExceeded,
}

impl RunResult {
    #[must_use]
    pub const fn is_ok(&self) -> bool {
        matches!(self, Self::Ok { .. })
    }

    #[must_use]
    pub fn output(&self) -> Option<&str> {
        match self {
            Self::Ok { output, .. } => Some(output),
            _ => None,
        }
    }

    #[must_use]
    pub fn into_output(self) -> Option<String> {
        match self {
            Self::Ok { output, .. } => Some(output),
            _ => None,
        }
    }

    #[must_use]
    pub const fn verdict(&self) -> &'static str {
        match self {
            Self::Ok { .. } => "OK",
            Self::TimedOut => "TLE",
            Self::MemoryLimitExceeded => "MLE",
            Self::Crashed(_) => "RTE",
        }
    }
}
