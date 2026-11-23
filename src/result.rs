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

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(output: &str) -> RunResult {
        RunResult::Ok {
            time_ms: 1,
            memory_kb: 2,
            output: output.to_owned(),
        }
    }

    #[test]
    fn accessors() {
        assert!(ok("x").is_ok());
        assert_eq!(ok("x").output(), Some("x"));
        assert_eq!(ok("x").into_output().as_deref(), Some("x"));
        for failed in [RunResult::TimedOut, RunResult::MemoryLimitExceeded, RunResult::Crashed(CrashReason::ExitCode(1))] {
            assert!(!failed.is_ok());
            assert_eq!(failed.output(), None);
            assert_eq!(failed.into_output(), None);
        }
    }

    #[test]
    fn verdicts() {
        assert_eq!(ok("").verdict(), "OK");
        assert_eq!(RunResult::TimedOut.verdict(), "TLE");
        assert_eq!(RunResult::MemoryLimitExceeded.verdict(), "MLE");
        assert_eq!(RunResult::Crashed(CrashReason::OutputLimitExceeded).verdict(), "RTE");
    }
}
