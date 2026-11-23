use crate::cache::{CACHE_FILE, Entry, Key, ResultCache};
use crate::check::{accepts, check_hash, checked, tokens_match};
use crate::compiler::{Compiler, SCRATCH_PREFIX, canonicalize, is_program_source};
use crate::exec::run_executable;
use crate::result::{Output, improves_on, near_the_limit};
use crate::{Check, Error, Limits, Result, RunResult};
use log::{debug, warn};
use std::collections::{HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

/// The default number of runs at once is the number of cores, but no more than this, so that
/// contention between parallel runs does not change verdicts.
pub const MAX_DEFAULT_WORKERS: usize = 4;

/// A compiled program or scratch folder not used for this long is removed when a runner is dropped.
pub const UNUSED_FILE_LIFETIME: Duration = Duration::from_secs(7 * 24 * 60 * 60);

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A program's last use is the modification time of its source, as its executable cannot be
/// written to while it runs on Windows.
fn mark_used(source: &Path) {
    let marked = std::fs::File::options().write(true).open(source).and_then(|file| file.set_modified(SystemTime::now()));
    if let Err(err) = marked {
        warn!("Could not mark {} as used: {err}", source.display());
    }
}

fn unused_for(path: &Path) -> Option<Duration> {
    SystemTime::now().duration_since(path.metadata().ok()?.modified().ok()?).ok()
}

fn best_of<O: Output, F: FnMut() -> Result<RunResult<O>>>(retries: usize, limits: Limits, mut run: F) -> Result<RunResult<O>> {
    let mut best = run()?;
    if near_the_limit(&best, limits.effective_time_ms()) {
        for _ in 0..retries {
            let again = run()?;
            if improves_on(&again, &best) {
                best = again;
            }
        }
    }
    Ok(best)
}

struct Task {
    cancelled: Arc<AtomicBool>,
    job: Box<dyn FnOnce(&ResultCache) + Send>,
}

#[derive(Default)]
struct Queue {
    tasks: VecDeque<Task>,
    threads: usize,
    shutdown: bool,
}

struct Shared {
    queue: Mutex<Queue>,
    work: Condvar,
    workers: AtomicUsize,
    cache: ResultCache,
}

impl Shared {
    fn next_task(&self) -> Option<Task> {
        let mut queue = lock(&self.queue);
        loop {
            if queue.shutdown || queue.threads > self.workers.load(Ordering::Relaxed) {
                queue.threads -= 1;
                return None;
            }
            if let Some(task) = queue.tasks.pop_front() {
                return Some(task);
            }
            queue = self.work.wait(queue).unwrap_or_else(PoisonError::into_inner);
        }
    }

    fn work(&self) {
        while let Some(task) = self.next_task() {
            if !task.cancelled.load(Ordering::Relaxed) {
                (task.job)(&self.cache);
            }
        }
    }
}

/// A run queued by [`Runner::spawn`], [`Runner::spawn_checked`] or [`Runner::spawn_check`].
#[must_use = "a run is only useful if it is waited for"]
pub struct RunHandle<T = RunResult> {
    receiver: mpsc::Receiver<Result<T>>,
    cancelled: Arc<AtomicBool>,
    cached: bool,
}

impl<T> RunHandle<T> {
    fn finished(result: T, cached: bool) -> Self {
        let (sender, receiver) = mpsc::channel();
        let _ = sender.send(Ok(result));
        Self {
            receiver,
            cancelled: Arc::new(AtomicBool::new(false)),
            cached,
        }
    }

    /// Waits for the run to end. [`Error::Cancelled`] if it was cancelled before it started.
    pub fn wait(self) -> Result<T> {
        self.receiver.recv().unwrap_or(Err(Error::Cancelled))
    }

    /// Whether the result came from the result cache, without running the program.
    #[must_use]
    pub const fn is_cached(&self) -> bool {
        self.cached
    }

    /// Skips the run if it has not started yet; a run already going is not stopped. Dropping the
    /// handle does the same.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl<T> Drop for RunHandle<T> {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Compiles C++ programs into a build folder and runs them.
///
/// Programs are named by their source and compiled the first time they are used. Compiled programs
/// and small results stay in the build folder for later runners, which may be in other processes.
/// Dropping a runner, once its runs have ended, removes the programs no runner used for a week.
/// Every method takes `&self`, so a runner can be shared between threads.
///
/// ```
/// use cpprunner::{Limits, RunResult, Runner};
///
/// let build_folder = tempfile::tempdir()?;
/// let runner = Runner::new(build_folder.path())?;
/// let double = "#include <iostream>\nint main() { int n; std::cin >> n; std::cout << 2 * n; }";
///
/// let handles = ["1", "2", "3"].map(|input| runner.spawn(double, input, Limits::default()));
/// let mut outputs = Vec::new();
/// for handle in handles {
///     match handle?.wait()? {
///         RunResult::Ok { output, .. } => outputs.push(output),
///         other => panic!("{other:?}"),
///     }
/// }
/// assert_eq!(outputs, ["2", "4", "6"]);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct Runner {
    compiler: Compiler,
    build_folder: PathBuf,
    caching: AtomicBool,
    timing_retries: AtomicUsize,
    executables: Mutex<HashMap<u64, Arc<Path>>>,
    shared: Arc<Shared>,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

impl Runner {
    /// Uses `build_folder`, created if missing, and the compiler `GCC_PATH` names.
    pub fn new(build_folder: &Path) -> Result<Self> {
        std::fs::create_dir_all(build_folder).map_err(Error::io(build_folder))?;
        let build_folder = canonicalize(build_folder)?;
        let workers = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get).min(MAX_DEFAULT_WORKERS);
        Ok(Self {
            compiler: Compiler::detect()?,
            caching: AtomicBool::new(true),
            timing_retries: AtomicUsize::new(0),
            executables: Mutex::new(HashMap::new()),
            shared: Arc::new(Shared {
                queue: Mutex::new(Queue::default()),
                work: Condvar::new(),
                workers: AtomicUsize::new(workers),
                cache: ResultCache::new(build_folder.join(CACHE_FILE)),
            }),
            threads: Mutex::new(Vec::new()),
            build_folder,
        })
    }

    /// Sets how many programs run at once; at least one.
    pub fn set_workers(&self, workers: usize) {
        self.shared.workers.store(workers.max(1), Ordering::Relaxed);
        self.shared.work.notify_all();
    }

    /// Whether results are looked up in and added to the result cache, which holds every checked run,
    /// time out, memory limit and crash, and successful runs whose output is at most 64 bytes. Of
    /// two results of the same run, it keeps the better: a pass over a failure, then the faster. On
    /// by default.
    pub fn set_result_cache(&self, enabled: bool) {
        self.caching.store(enabled, Ordering::Relaxed);
    }

    /// How many more times a run is measured when it timed out or used over half its time limit, as
    /// timing varies between runs. The best measurement counts and is cached. None by default.
    pub fn set_timing_retries(&self, retries: usize) {
        self.timing_retries.store(retries, Ordering::Relaxed);
    }

    fn executable(&self, source_code: &str) -> Result<(u64, Arc<Path>)> {
        let hash = self.compiler.program_hash(source_code);
        if let Some(executable) = lock(&self.executables).get(&hash) {
            return Ok((hash, executable.clone()));
        }
        let compiled = self.compiler.compile_cached(&self.build_folder, source_code)?;
        mark_used(&compiled.source);
        let executable: Arc<Path> = compiled.executable.into();
        lock(&self.executables).insert(hash, executable.clone());
        Ok((hash, executable))
    }

    /// Queues a run of `source_code` on `input`, compiling it first if needed. Runs start in the
    /// order they are queued. A cached result is returned without running the program.
    pub fn spawn<I: Into<Arc<str>>>(&self, source_code: &str, input: I, limits: Limits) -> Result<RunHandle> {
        self.queue_run(source_code, input.into(), limits)
    }

    /// As [`spawn`](Self::spawn), but the output is checked against `answer`, and only whether it
    /// was accepted is kept. That is small, so checked runs are always cached.
    pub fn spawn_checked<I: Into<Arc<str>>, A: Into<Arc<str>>>(&self, source_code: &str, input: I, answer: A, check: &Check, limits: Limits) -> Result<RunHandle<RunResult<bool>>> {
        self.queue_checked(source_code, input.into(), answer.into(), check, limits)
    }

    /// Queues checking `output` against `answer`: whether `check` accepts it. Only a checker program
    /// is really queued.
    pub fn spawn_check<I: Into<Arc<str>>, A: Into<Arc<str>>, O: Into<Arc<str>>>(&self, check: &Check, input: I, answer: A, output: O) -> Result<RunHandle<bool>> {
        let (input, answer, output) = (input.into(), answer.into(), output.into());
        let Some((_, checker)) = self.checker(check)? else {
            return Ok(RunHandle::finished(tokens_match(&output, &answer), false));
        };
        let build_folder = self.build_folder.clone();
        self.queue(move |_cache| accepts(Some(&checker), &build_folder, &input, &answer, &output))
    }

    fn caching(&self) -> bool {
        self.caching.load(Ordering::Relaxed)
    }

    fn checker(&self, check: &Check) -> Result<Option<(u64, Arc<Path>)>> {
        match check {
            Check::Tokens => Ok(None),
            Check::Program(source_code) => self.executable(source_code).map(Some),
        }
    }

    fn queue_run(&self, source_code: &str, input: Arc<str>, limits: Limits) -> Result<RunHandle> {
        let (hash, executable) = self.executable(source_code)?;
        let key = self.caching().then(|| Key::new(hash, &input, limits));
        if let Some(Entry::Run(cached)) = key.and_then(|key| self.shared.cache.get(&key)) {
            return Ok(RunHandle::finished(cached, true));
        }
        let retries = self.timing_retries.load(Ordering::Relaxed);
        self.queue(move |cache| {
            let result = best_of(retries, limits, || run_executable(&executable, &[], &input, limits))?;
            if let Some(key) = key {
                cache.insert_run(key, &result);
            }
            Ok(result)
        })
    }

    fn queue_checked(&self, source_code: &str, input: Arc<str>, answer: Arc<str>, check: &Check, limits: Limits) -> Result<RunHandle<RunResult<bool>>> {
        let (hash, executable) = self.executable(source_code)?;
        let checker = self.checker(check)?;
        let key = self
            .caching()
            .then(|| Key::checked(hash, &input, check_hash(checker.as_ref().map_or(0, |&(hash, _)| hash), &answer), limits));
        if let Some(Entry::Checked(cached)) = key.and_then(|key| self.shared.cache.get(&key)) {
            return Ok(RunHandle::finished(cached, true));
        }
        let build_folder = self.build_folder.clone();
        let retries = self.timing_retries.load(Ordering::Relaxed);
        self.queue(move |cache| {
            let checker = checker.as_ref().map(|(_, checker)| &**checker);
            let run = || checked(run_executable(&executable, &[], &input, limits)?, |output| accepts(checker, &build_folder, &input, &answer, output));
            let checked = best_of(retries, limits, run)?;
            if let Some(key) = key {
                cache.insert_checked(key, checked);
            }
            Ok(checked)
        })
    }

    fn queue<T: Send + 'static, W: FnOnce(&ResultCache) -> Result<T> + Send + 'static>(&self, work: W) -> Result<RunHandle<T>> {
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let job = move |cache: &ResultCache| {
            let result = catch_unwind(AssertUnwindSafe(|| work(cache))).unwrap_or_else(|_panic| Err(Error::run_failed("the worker thread panicked")));
            let _ = sender.send(result);
        };
        let mut queue = lock(&self.shared.queue);
        queue.tasks.push_back(Task {
            cancelled: cancelled.clone(),
            job: Box::new(job),
        });
        if queue.threads < self.shared.workers.load(Ordering::Relaxed) {
            let shared = self.shared.clone();
            match std::thread::Builder::new().name("cpprunner-worker".to_owned()).spawn(move || shared.work()) {
                Ok(thread) => {
                    queue.threads += 1;
                    lock(&self.threads).push(thread);
                }
                Err(err) if queue.threads == 0 => {
                    queue.tasks.pop_back();
                    return Err(Error::run_failed(format!("could not start a worker thread: {err}")));
                }
                Err(err) => warn!("Could not start another worker thread: {err}"),
            }
        }
        drop(queue);
        self.shared.work.notify_one();
        Ok(RunHandle { receiver, cancelled, cached: false })
    }

    /// A program's executable goes before its source, so that one still running elsewhere keeps its
    /// source and is tried again next time. A scratch folder that old belongs to no live compile.
    fn remove_unused_files(&self) {
        let Ok(entries) = std::fs::read_dir(&self.build_folder) else { return };
        for path in entries.filter_map(|entry| Some(entry.ok()?.path())) {
            if unused_for(&path).is_none_or(|unused| unused < UNUSED_FILE_LIFETIME) {
                continue;
            }
            let removed = if is_program_source(&path) {
                match std::fs::remove_file(Compiler::executable_path(&path)) {
                    Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err),
                    _ => std::fs::remove_file(&path),
                }
            } else if path.is_dir() && path.file_name().is_some_and(|name| name.to_string_lossy().starts_with(SCRATCH_PREFIX)) {
                std::fs::remove_dir_all(&path)
            } else {
                continue;
            };
            match removed {
                Ok(()) => debug!("Removed {}, unused for over a week", path.display()),
                Err(err) => warn!("Could not remove {} from the build folder: {err}", path.display()),
            }
        }
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        let unstarted = {
            let mut queue = lock(&self.shared.queue);
            queue.shutdown = true;
            std::mem::take(&mut queue.tasks)
        };
        drop(unstarted);
        self.shared.work.notify_all();
        for thread in lock(&self.threads).drain(..) {
            let _ = thread.join();
        }
        self.remove_unused_files();
    }
}
