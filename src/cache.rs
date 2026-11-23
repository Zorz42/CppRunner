use crate::{CrashReason, Limits, RunResult};
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
/// `YES`/`NO`; caching long outputs would make the cache too large to be worth it.
pub const MAX_CACHED_OUTPUT_BYTES: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Key {
    program: u64,
    input: u64,
    limits: Limits,
}

impl Key {
    pub fn new(program: u64, input: &str, limits: Limits) -> Self {
        let mut state = DefaultHasher::new();
        input.hash(&mut state);
        Self {
            program,
            input: state.finish(),
            limits,
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
pub struct ResultCache {
    path: PathBuf,
    entries: OnceLock<Mutex<HashMap<Key, RunResult>>>,
    file: Mutex<Option<File>>,
}

impl ResultCache {
    pub const fn new(path: PathBuf) -> Self {
        Self {
            path,
            entries: OnceLock::new(),
            file: Mutex::new(None),
        }
    }

    fn entries(&self) -> &Mutex<HashMap<Key, RunResult>> {
        self.entries.get_or_init(|| {
            let text = std::fs::read_to_string(&self.path).unwrap_or_default();
            Mutex::new(text.lines().filter_map(parse_line).collect())
        })
    }

    pub fn get(&self, key: &Key) -> Option<RunResult> {
        self.entries().lock().ok()?.get(key).cloned()
    }

    pub fn insert(&self, key: Key, result: &RunResult) {
        if !is_cacheable(result) {
            return;
        }
        if let Ok(mut entries) = self.entries().lock() {
            entries.insert(key, result.clone());
        }
        let Ok(mut file) = self.file.lock() else { return };
        if file.is_none() {
            match OpenOptions::new().create(true).append(true).open(&self.path) {
                Ok(opened) => *file = Some(opened),
                Err(err) => return warn!("Could not open the result cache {}: {err}", self.path.display()),
            }
        }
        if let Some(file) = file.as_mut()
            && let Err(err) = file.write_all(format_line(&key, result).as_bytes())
        {
            warn!("Could not write to the result cache {}: {err}", self.path.display());
        }
    }
}

fn format_line(key: &Key, result: &RunResult) -> String {
    let Limits { time_ms, memory_kb, output_bytes } = key.limits;
    let memory_limit = memory_kb.map_or_else(|| "-".to_owned(), |memory_kb| memory_kb.to_string());
    let mut line = format!("{:016x} {:016x} {time_ms} {memory_limit} {output_bytes} ", key.program, key.input);
    match result {
        RunResult::Ok { time_ms, memory_kb, output } => {
            write!(line, "OK {time_ms} {memory_kb} -").ok();
            for byte in output.as_bytes() {
                write!(line, "{byte:02x}").ok();
            }
        }
        RunResult::TimedOut => line.push_str("TLE"),
        RunResult::MemoryLimitExceeded => line.push_str("MLE"),
        RunResult::Crashed(CrashReason::ExitCode(code)) => write!(line, "EXIT {code}").unwrap_or_default(),
        RunResult::Crashed(CrashReason::Signal(signal)) => write!(line, "SIGNAL {signal}").unwrap_or_default(),
        RunResult::Crashed(CrashReason::OutputLimitExceeded) => line.push_str("OUTPUT"),
    }
    line.push('\n');
    line
}

/// Lines it cannot read are ignored, as a run cut short may leave half of one.
fn parse_line(line: &str) -> Option<(Key, RunResult)> {
    let fields: Vec<&str> = line.split(' ').collect();
    let [program, input, time_limit, memory_limit, output_limit, verdict, rest @ ..] = fields.as_slice() else {
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
        limits,
    };
    let result = match (*verdict, rest) {
        ("OK", [time_ms, memory_kb, output]) => RunResult::Ok {
            time_ms: time_ms.parse().ok()?,
            memory_kb: memory_kb.parse().ok()?,
            output: decode_hex(output.strip_prefix('-')?)?,
        },
        ("TLE", []) => RunResult::TimedOut,
        ("MLE", []) => RunResult::MemoryLimitExceeded,
        ("EXIT", [code]) => RunResult::Crashed(CrashReason::ExitCode(code.parse().ok()?)),
        ("SIGNAL", [signal]) => RunResult::Crashed(CrashReason::Signal(signal.parse().ok()?)),
        ("OUTPUT", []) => RunResult::Crashed(CrashReason::OutputLimitExceeded),
        _ => return None,
    };
    Some((key, result))
}

fn decode_hex(hex: &str) -> Option<String> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    let bytes = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok()).collect::<Option<Vec<u8>>>()?;
    String::from_utf8(bytes).ok()
}
