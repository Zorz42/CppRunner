use crate::{CrashReason, Limits, RunResult, Verdict};
use log::warn;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

pub const CACHE_FILE: &str = "results.cache";
/// Successful runs are cached only if their output is at most this long, such as a single number or
/// `YES`/`NO`.
///
/// Caching long outputs would make the cache too large to be worth it. Checked runs are always
/// cached, as only their verdict is kept.
pub const MAX_CACHED_OUTPUT_BYTES: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Key {
    program: u64,
    input: u64,
    check: Option<u64>,
    limits: Limits,
}

impl Key {
    pub fn new(program: u64, input: &str, limits: Limits) -> Self {
        let mut state = DefaultHasher::new();
        input.hash(&mut state);
        Self {
            program,
            input: state.finish(),
            check: None,
            limits,
        }
    }

    pub fn checked(program: u64, input: &str, check: u64, limits: Limits) -> Self {
        Self {
            check: Some(check),
            ..Self::new(program, input, limits)
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Entry {
    Run(RunResult),
    Checked(Verdict),
}

impl Entry {
    const fn passed_in_ms(&self) -> Option<u64> {
        match self {
            Self::Run(RunResult::Ok { time_ms, .. }) | Self::Checked(Verdict::Accepted { time_ms, .. }) => Some(*time_ms),
            _ => None,
        }
    }

    /// A pass beats a failure, and a faster pass a slower one.
    pub const fn improves_on(&self, other: &Self) -> bool {
        match (self.passed_in_ms(), other.passed_in_ms()) {
            (Some(time_ms), Some(other_ms)) => time_ms < other_ms,
            (Some(_), None) => true,
            (None, _) => false,
        }
    }
}

pub const fn is_cacheable(result: &RunResult) -> bool {
    match result {
        RunResult::Ok { output, .. } => output.len() <= MAX_CACHED_OUTPUT_BYTES,
        RunResult::TimedOut | RunResult::MemoryLimitExceeded | RunResult::Crashed(_) => true,
    }
}

/// Results of earlier runs, appended to a file in the build folder that several processes may share.
/// Of two results for the same run, the better is kept.
pub struct ResultCache {
    path: PathBuf,
    entries: OnceLock<Mutex<HashMap<Key, Entry>>>,
    file: Mutex<Option<File>>,
}

fn keep_better(entries: &mut HashMap<Key, Entry>, key: Key, entry: Entry) -> bool {
    if entries.get(&key).is_some_and(|old| !entry.improves_on(old)) {
        return false;
    }
    entries.insert(key, entry);
    true
}

impl ResultCache {
    pub const fn new(path: PathBuf) -> Self {
        Self {
            path,
            entries: OnceLock::new(),
            file: Mutex::new(None),
        }
    }

    fn entries(&self) -> &Mutex<HashMap<Key, Entry>> {
        self.entries.get_or_init(|| {
            let text = std::fs::read_to_string(&self.path).unwrap_or_default();
            let mut entries = HashMap::new();
            for (key, entry) in text.lines().filter_map(parse_line) {
                keep_better(&mut entries, key, entry);
            }
            Mutex::new(entries)
        })
    }

    pub fn get(&self, key: &Key) -> Option<Entry> {
        self.entries().lock().ok()?.get(key).cloned()
    }

    pub fn insert_run(&self, key: Key, result: &RunResult) {
        if is_cacheable(result) {
            self.insert(key, Entry::Run(result.clone()));
        }
    }

    pub fn insert_verdict(&self, key: Key, verdict: Verdict) {
        self.insert(key, Entry::Checked(verdict));
    }

    fn insert(&self, key: Key, entry: Entry) {
        let line = format_line(&key, &entry);
        let Ok(mut entries) = self.entries().lock() else { return };
        if !keep_better(&mut entries, key, entry) {
            return;
        }
        drop(entries);
        let Ok(mut file) = self.file.lock() else { return };
        if file.is_none() {
            match OpenOptions::new().create(true).append(true).open(&self.path) {
                Ok(opened) => *file = Some(opened),
                Err(err) => return warn!("Could not open the result cache {}: {err}", self.path.display()),
            }
        }
        if let Some(file) = file.as_mut()
            && let Err(err) = file.write_all(line.as_bytes())
        {
            warn!("Could not write to the result cache {}: {err}", self.path.display());
        }
    }
}

fn format_line(key: &Key, entry: &Entry) -> String {
    let Limits { time_ms, memory_kb, output_bytes } = key.limits;
    let memory_limit = memory_kb.map_or_else(|| "-".to_owned(), |memory_kb| memory_kb.to_string());
    let check = key.check.map_or_else(|| "-".to_owned(), |check| format!("{check:016x}"));
    let mut line = format!("{:016x} {:016x} {check} {time_ms} {memory_limit} {output_bytes} ", key.program, key.input);
    match entry {
        Entry::Run(RunResult::Ok { time_ms, memory_kb, output }) => {
            write!(line, "OK {time_ms} {memory_kb} -").ok();
            for byte in output.as_bytes() {
                write!(line, "{byte:02x}").ok();
            }
        }
        Entry::Checked(Verdict::Accepted { time_ms, memory_kb }) => write!(line, "AC {time_ms} {memory_kb}").unwrap_or_default(),
        Entry::Checked(Verdict::WrongAnswer) => line.push_str("WA"),
        Entry::Run(RunResult::TimedOut) | Entry::Checked(Verdict::TimedOut) => line.push_str(&Failure::TimedOut.code()),
        Entry::Run(RunResult::MemoryLimitExceeded) | Entry::Checked(Verdict::MemoryLimitExceeded) => line.push_str(&Failure::MemoryLimitExceeded.code()),
        Entry::Run(RunResult::Crashed(reason)) | Entry::Checked(Verdict::Crashed(reason)) => line.push_str(&Failure::Crashed(*reason).code()),
    }
    line.push('\n');
    line
}

enum Failure {
    TimedOut,
    MemoryLimitExceeded,
    Crashed(CrashReason),
}

impl Failure {
    fn code(&self) -> String {
        match self {
            Self::TimedOut => "TLE".to_owned(),
            Self::MemoryLimitExceeded => "MLE".to_owned(),
            Self::Crashed(CrashReason::ExitCode(code)) => format!("EXIT {code}"),
            Self::Crashed(CrashReason::Signal(signal)) => format!("SIGNAL {signal}"),
            Self::Crashed(CrashReason::OutputLimitExceeded) => "OUTPUT".to_owned(),
        }
    }
}

fn parse_failure(verdict: &str, rest: &[&str]) -> Option<Failure> {
    Some(match (verdict, rest) {
        ("TLE", []) => Failure::TimedOut,
        ("MLE", []) => Failure::MemoryLimitExceeded,
        ("EXIT", [code]) => Failure::Crashed(CrashReason::ExitCode(code.parse().ok()?)),
        ("SIGNAL", [signal]) => Failure::Crashed(CrashReason::Signal(signal.parse().ok()?)),
        ("OUTPUT", []) => Failure::Crashed(CrashReason::OutputLimitExceeded),
        _ => return None,
    })
}

/// Lines it cannot read are ignored, as a run cut short may leave half of one.
fn parse_line(line: &str) -> Option<(Key, Entry)> {
    let fields: Vec<&str> = line.split(' ').collect();
    let [program, input, check, time_limit, memory_limit, output_limit, verdict, rest @ ..] = fields.as_slice() else {
        return None;
    };
    let limits = Limits {
        time_ms: time_limit.parse().ok()?,
        memory_kb: if *memory_limit == "-" { None } else { Some(memory_limit.parse().ok()?) },
        output_bytes: output_limit.parse().ok()?,
    };
    let key = Key {
        program: u64::from_str_radix(program, 16).ok()?,
        input: u64::from_str_radix(input, 16).ok()?,
        check: if *check == "-" { None } else { Some(u64::from_str_radix(check, 16).ok()?) },
        limits,
    };
    let entry = match (key.check, *verdict, rest) {
        (None, "OK", [time_ms, memory_kb, output]) => Entry::Run(RunResult::Ok {
            time_ms: time_ms.parse().ok()?,
            memory_kb: memory_kb.parse().ok()?,
            output: decode_hex(output.strip_prefix('-')?)?,
        }),
        (Some(_), "AC", [time_ms, memory_kb]) => Entry::Checked(Verdict::Accepted {
            time_ms: time_ms.parse().ok()?,
            memory_kb: memory_kb.parse().ok()?,
        }),
        (Some(_), "WA", []) => Entry::Checked(Verdict::WrongAnswer),
        (None, verdict, rest) => Entry::Run(match parse_failure(verdict, rest)? {
            Failure::TimedOut => RunResult::TimedOut,
            Failure::MemoryLimitExceeded => RunResult::MemoryLimitExceeded,
            Failure::Crashed(reason) => RunResult::Crashed(reason),
        }),
        (Some(_), verdict, rest) => Entry::Checked(match parse_failure(verdict, rest)? {
            Failure::TimedOut => Verdict::TimedOut,
            Failure::MemoryLimitExceeded => Verdict::MemoryLimitExceeded,
            Failure::Crashed(reason) => Verdict::Crashed(reason),
        }),
    };
    Some((key, entry))
}

fn decode_hex(hex: &str) -> Option<String> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    let bytes = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok()).collect::<Option<Vec<u8>>>()?;
    String::from_utf8(bytes).ok()
}
