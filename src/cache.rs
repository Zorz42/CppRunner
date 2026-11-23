use crate::hash::stable_hash;
use crate::{Limits, RunResult};
use log::warn;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

pub const CACHE_FILE: &str = "results.cache";
pub const MAX_CACHED_OUTPUT_BYTES: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct Key {
    program: u64,
    input: u64,
    check: Option<u64>,
    limits: Limits,
}

impl Key {
    pub fn new(program: u64, input: &str, limits: Limits) -> Self {
        Self {
            program,
            input: stable_hash(&[input.as_bytes()]),
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

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Entry {
    Run(RunResult),
    Checked(RunResult<bool>),
}

impl Entry {
    const fn passed_in_ms(&self) -> Option<u64> {
        match self {
            Self::Run(RunResult::Ok { time_ms, .. }) | Self::Checked(RunResult::Ok { time_ms, output: true, .. }) => Some(*time_ms),
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
        RunResult::TimedOut | RunResult::MemoryLimitExceeded | RunResult::ExitCode(_) | RunResult::Signal(_) | RunResult::OutputLimitExceeded => true,
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

    pub fn insert_checked(&self, key: Key, result: RunResult<bool>) {
        self.insert(key, Entry::Checked(result));
    }

    fn insert(&self, key: Key, entry: Entry) {
        let Ok(mut line) = serde_json::to_string(&(key, &entry)) else { return };
        line.push('\n');
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

/// Lines it cannot read are ignored, as a run cut short may leave half of one.
fn parse_line(line: &str) -> Option<(Key, Entry)> {
    let (key, entry): (Key, Entry) = serde_json::from_str(line).ok()?;
    (key.check.is_some() == matches!(entry, Entry::Checked(_))).then_some((key, entry))
}
