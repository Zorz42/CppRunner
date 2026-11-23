use crate::hash::stable_hash;
use crate::result::improves_on;
use crate::{Limits, RunResult};
use log::warn;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::OpenOptions;
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
    fn improves_on(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Run(result), Self::Run(other)) => improves_on(result, other),
            (Self::Checked(result), Self::Checked(other)) => improves_on(result, other),
            _ => true,
        }
    }
}

/// What a run gives, as the result cache keeps it.
pub trait Cacheable: Sized {
    /// `None` for a result too large to be worth keeping.
    fn entry(result: &RunResult<Self>) -> Option<Entry>;
    fn from_entry(entry: Entry) -> Option<RunResult<Self>>;
}

impl Cacheable for String {
    fn entry(result: &RunResult) -> Option<Entry> {
        let too_large = matches!(result, RunResult::Ok { output, .. } if output.len() > MAX_CACHED_OUTPUT_BYTES);
        (!too_large).then(|| Entry::Run(result.clone()))
    }

    fn from_entry(entry: Entry) -> Option<RunResult> {
        match entry {
            Entry::Run(result) => Some(result),
            Entry::Checked(_) => None,
        }
    }
}

impl Cacheable for bool {
    fn entry(result: &RunResult<Self>) -> Option<Entry> {
        Some(Entry::Checked(*result))
    }

    fn from_entry(entry: Entry) -> Option<RunResult<Self>> {
        match entry {
            Entry::Checked(result) => Some(result),
            Entry::Run(_) => None,
        }
    }
}

/// Results of earlier runs, appended to a file in the build folder that several processes may share.
/// Of two results for the same run, the better is kept.
pub struct ResultCache {
    path: PathBuf,
    entries: OnceLock<Mutex<HashMap<Key, Entry>>>,
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
        Self { path, entries: OnceLock::new() }
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

    pub fn get<T: Cacheable>(&self, key: &Key) -> Option<RunResult<T>> {
        T::from_entry(self.entries().lock().ok()?.get(key)?.clone())
    }

    /// Writes while holding the lock, so that lines from this process never interleave.
    pub fn insert<T: Cacheable>(&self, key: Key, result: &RunResult<T>) {
        let Some(entry) = T::entry(result) else { return };
        let Ok(mut line) = serde_json::to_string(&(key, &entry)) else { return };
        line.push('\n');
        let Ok(mut entries) = self.entries().lock() else { return };
        if !keep_better(&mut entries, key, entry) {
            return;
        }
        let written = OpenOptions::new().create(true).append(true).open(&self.path).and_then(|mut file| file.write_all(line.as_bytes()));
        if let Err(err) = written {
            warn!("Could not write to the result cache {}: {err}", self.path.display());
        }
    }
}

/// Lines it cannot read are ignored, as a run cut short may leave half of one.
fn parse_line(line: &str) -> Option<(Key, Entry)> {
    let (key, entry): (Key, Entry) = serde_json::from_str(line).ok()?;
    (key.check.is_some() == matches!(entry, Entry::Checked(_))).then_some((key, entry))
}
