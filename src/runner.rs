use crate::cache::{CACHE_FILE, Key, ResultCache};
use crate::compiler::{Compiler, SCRATCH_PREFIX, canonicalize};
use crate::exec::run_executable;
use crate::{Error, Limits, Result, RunResult};
use log::warn;
use std::collections::{HashMap, HashSet, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, mpsc};
use std::thread::JoinHandle;

/// The default number of runs at once is the number of cores, but no more than this, so that
/// contention between parallel runs does not change verdicts.
pub const MAX_DEFAULT_WORKERS: usize = 4;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

struct Task {
    executable: Arc<Path>,
    input: Arc<str>,
    limits: Limits,
    cache_key: Option<Key>,
    cancelled: Arc<AtomicBool>,
    sender: mpsc::Sender<Result<RunResult>>,
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
            if task.cancelled.load(Ordering::Relaxed) {
                continue;
            }
            let result = catch_unwind(AssertUnwindSafe(|| run_executable(&task.executable, &task.input, task.limits))).unwrap_or_else(|_panic| Err(Error::run_failed("the worker thread panicked")));
            if let (Some(key), Ok(result)) = (task.cache_key, &result) {
                self.cache.insert(key, result);
            }
            let _ = task.sender.send(result);
        }
    }
}

/// A run queued by [`Runner::spawn`].
#[must_use = "a run is only useful if it is waited for"]
pub struct RunHandle {
    receiver: mpsc::Receiver<Result<RunResult>>,
    cancelled: Arc<AtomicBool>,
}

impl RunHandle {
    fn finished(result: RunResult) -> Self {
        let (sender, receiver) = mpsc::channel();
        let _ = sender.send(Ok(result));
        Self {
            receiver,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Waits for the run to end. [`Error::Cancelled`] if it was cancelled before it started.
    pub fn wait(self) -> Result<RunResult> {
        self.receiver.recv().unwrap_or(Err(Error::Cancelled))
    }

    /// Skips the run if it has not started yet; a run already going is not stopped. Dropping the
    /// handle does the same.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl Drop for RunHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Compiles C++ programs into a build folder and runs them.
///
/// Programs are named by their source and compiled the first time they are used. Compiled programs
/// and small results stay in the build folder for later runners, which may be in other processes.
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
    executables: Mutex<HashMap<u64, Arc<Path>>>,
    kept: Mutex<HashSet<PathBuf>>,
    shared: Arc<Shared>,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

impl Runner {
    /// Uses `build_folder`, created if missing, and the compiler [`COMPILER_ENV_VAR`](crate::COMPILER_ENV_VAR) names.
    pub fn new(build_folder: &Path) -> Result<Self> {
        std::fs::create_dir_all(build_folder).map_err(Error::io(build_folder))?;
        let build_folder = canonicalize(build_folder)?;
        let workers = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get).min(MAX_DEFAULT_WORKERS);
        Ok(Self {
            compiler: Compiler::detect()?,
            caching: AtomicBool::new(true),
            executables: Mutex::new(HashMap::new()),
            kept: Mutex::new(HashSet::from([build_folder.join(CACHE_FILE)])),
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

    /// The build folder, canonicalized.
    #[must_use]
    pub fn build_folder(&self) -> &Path {
        &self.build_folder
    }

    /// How many programs run at once.
    #[must_use]
    pub fn workers(&self) -> usize {
        self.shared.workers.load(Ordering::Relaxed)
    }

    /// Sets how many programs run at once; at least one.
    pub fn set_workers(&self, workers: usize) {
        self.shared.workers.store(workers.max(1), Ordering::Relaxed);
        self.shared.work.notify_all();
    }

    /// Whether results are looked up in and added to the result cache, which holds every time out,
    /// memory limit and crash, and successful runs whose output is at most
    /// [`MAX_CACHED_OUTPUT_BYTES`](crate::MAX_CACHED_OUTPUT_BYTES). On by default.
    pub fn set_result_cache(&self, enabled: bool) {
        self.caching.store(enabled, Ordering::Relaxed);
    }

    /// Identifies `source_code` and the compiler; stable across runs.
    #[must_use]
    pub fn program_hash(&self, source_code: &str) -> u64 {
        self.compiler.program_hash(source_code)
    }

    /// Compiles `source_code` unless it is compiled already. Running a program compiles it too; this
    /// only reports compile errors sooner.
    pub fn compile(&self, source_code: &str) -> Result<()> {
        self.executable(source_code).map(drop)
    }

    fn executable(&self, source_code: &str) -> Result<(u64, Arc<Path>)> {
        let hash = self.compiler.program_hash(source_code);
        if let Some(executable) = lock(&self.executables).get(&hash) {
            return Ok((hash, executable.clone()));
        }
        let compiled = self.compiler.compile_cached(&self.build_folder, source_code)?;
        self.keep(compiled.source);
        self.keep(compiled.executable.clone());
        let executable: Arc<Path> = compiled.executable.into();
        lock(&self.executables).insert(hash, executable.clone());
        Ok((hash, executable))
    }

    /// Queues a run of `source_code` on `input`, compiling it first if needed. Runs start in the
    /// order they are queued. A cached result is returned without running the program.
    pub fn spawn<I: Into<Arc<str>>>(&self, source_code: &str, input: I, limits: Limits) -> Result<RunHandle> {
        self.queue_run(source_code, input.into(), limits, true)
    }

    /// As [`spawn`](Self::spawn), but never uses a cached result, to measure the program again.
    pub fn spawn_fresh<I: Into<Arc<str>>>(&self, source_code: &str, input: I, limits: Limits) -> Result<RunHandle> {
        self.queue_run(source_code, input.into(), limits, false)
    }

    fn queue_run(&self, source_code: &str, input: Arc<str>, limits: Limits, use_cached: bool) -> Result<RunHandle> {
        let (hash, executable) = self.executable(source_code)?;
        let cache_key = self.caching.load(Ordering::Relaxed).then(|| Key::new(hash, &input, limits));
        if use_cached && let Some(cached) = cache_key.and_then(|key| self.shared.cache.get(&key)) {
            return Ok(RunHandle::finished(cached));
        }

        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut queue = lock(&self.shared.queue);
        queue.tasks.push_back(Task {
            executable,
            input,
            limits,
            cache_key,
            cancelled: cancelled.clone(),
            sender,
        });
        if queue.threads < self.workers() {
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
        Ok(RunHandle { receiver, cancelled })
    }

    /// Keeps `file` when [`clean_build_folder`](Self::clean_build_folder) runs.
    pub fn keep(&self, file: PathBuf) {
        lock(&self.kept).insert(file);
    }

    /// Removes every file of the build folder that belongs to no program this runner compiled, except
    /// the result cache and kept files.
    pub fn clean_build_folder(&self) -> Result<()> {
        let kept = lock(&self.kept).clone();
        for entry in std::fs::read_dir(&self.build_folder).map_err(Error::io(&self.build_folder))? {
            let path = entry.map_err(Error::io(&self.build_folder))?.path();
            let removed = if path.is_file() && !kept.contains(&path) {
                std::fs::remove_file(&path)
            } else if path.is_dir() && path.file_name().is_some_and(|name| name.to_string_lossy().starts_with(SCRATCH_PREFIX)) {
                std::fs::remove_dir_all(&path)
            } else {
                continue;
            };
            if let Err(err) = removed {
                warn!("Could not remove {} from the build folder: {err}", path.display());
            }
        }
        Ok(())
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
    }
}
