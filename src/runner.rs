use crate::compiler::{Compiler, SCRATCH_PREFIX, canonicalize};
use crate::{Error, Limits, Result, RunResult, run_executable};
use log::{trace, warn};
use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;

/// Kept low so that contention between parallel runs does not change verdicts.
pub const MAX_DEFAULT_WORKERS: usize = 4;

/// A program added to a [`CppRunner`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProgramHandle(usize);

/// One run of a program on an input.
#[derive(Clone, Copy, Debug)]
pub struct Job<'input> {
    /// The program to run.
    pub program: ProgramHandle,
    /// What it gets on stdin.
    pub input: &'input str,
}

struct StopOnDrop<'flag>(&'flag AtomicBool);

impl Drop for StopOnDrop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// Compiles programs into a cached build folder and runs them, in parallel when given several jobs.
///
/// ```
/// use cpprunner::{CppRunner, Job, Limits, RunResult};
///
/// let build_folder = tempfile::tempdir()?;
/// let mut runner = CppRunner::new(build_folder.path())?;
/// let double = runner.add_program("#include <iostream>\nint main() { int n; std::cin >> n; std::cout << 2 * n; }")?;
///
/// let inputs = ["1", "2", "3"];
/// let jobs: Vec<Job> = inputs.iter().map(|input| Job { program: double, input }).collect();
/// let outputs: Vec<_> = runner.run_all(&jobs, Limits::default())?.into_iter().map(|result| match result {
///     RunResult::Ok { output, .. } => output,
///     other => panic!("{other:?}"),
/// }).collect();
/// assert_eq!(outputs, ["2", "4", "6"]);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct CppRunner {
    compiler: Compiler,
    build_folder: PathBuf,
    programs: Vec<PathBuf>,
    hashes: Vec<u64>,
    hash_to_handle: HashMap<u64, ProgramHandle>,
    necessary_files: HashSet<PathBuf>,
    workers: usize,
}

impl CppRunner {
    /// Uses `build_folder`, created if missing, and the compiler [`COMPILER_ENV_VAR`](crate::COMPILER_ENV_VAR) names.
    pub fn new(build_folder: &Path) -> Result<Self> {
        std::fs::create_dir_all(build_folder).map_err(Error::io(build_folder))?;
        Ok(Self {
            compiler: Compiler::detect()?,
            build_folder: canonicalize(build_folder)?,
            programs: Vec::new(),
            hashes: Vec::new(),
            hash_to_handle: HashMap::new(),
            necessary_files: HashSet::new(),
            workers: std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get).min(MAX_DEFAULT_WORKERS),
        })
    }

    /// The build folder, canonicalized.
    #[must_use]
    pub fn build_folder(&self) -> &Path {
        &self.build_folder
    }

    /// Compiles `source_code` unless it is cached. Adding the same source again returns the same handle.
    pub fn add_program(&mut self, source_code: &str) -> Result<ProgramHandle> {
        let hash = self.compiler.program_hash(source_code);
        if let Some(&handle) = self.hash_to_handle.get(&hash) {
            return Ok(handle);
        }

        let compiled = self.compiler.compile_cached_in(&self.build_folder, source_code)?;
        self.necessary_files.insert(compiled.source);
        self.necessary_files.insert(compiled.executable.clone());
        let handle = ProgramHandle(self.programs.len());
        self.programs.push(compiled.executable);
        self.hashes.push(compiled.hash);
        self.hash_to_handle.insert(compiled.hash, handle);
        Ok(handle)
    }

    /// Reads the C++ file at `source` and adds it as [`add_program`](Self::add_program) does.
    pub fn add_file(&mut self, source: &Path) -> Result<ProgramHandle> {
        let source_code = std::fs::read_to_string(source).map_err(Error::io(source))?;
        self.add_program(&source_code)
    }

    /// Identifies the program's source and compiler; stable across runs.
    #[must_use]
    pub fn program_hash(&self, program: ProgramHandle) -> u64 {
        self.hashes[program.0]
    }

    /// The program's executable.
    #[must_use]
    pub fn executable(&self, program: ProgramHandle) -> &Path {
        &self.programs[program.0]
    }

    /// Every added program, in the order added.
    #[must_use]
    pub fn programs(&self) -> Vec<ProgramHandle> {
        (0..self.programs.len()).map(ProgramHandle).collect()
    }

    /// Keeps `file` when [`clean_build_folder`](Self::clean_build_folder) runs.
    pub fn keep(&mut self, file: PathBuf) {
        self.necessary_files.insert(file);
    }

    /// Sets how many jobs run at once; at least one.
    pub fn set_workers(&mut self, workers: usize) {
        self.workers = workers.max(1);
    }

    /// Sets how many jobs run at once, as [`set_workers`](Self::set_workers) does.
    #[must_use]
    pub fn with_workers(mut self, workers: usize) -> Self {
        self.set_workers(workers);
        self
    }

    /// How many jobs run at once.
    #[must_use]
    pub const fn workers(&self) -> usize {
        self.workers
    }

    /// Runs one program on this thread.
    pub fn run(&self, program: ProgramHandle, input: &str, limits: Limits) -> Result<RunResult> {
        let executable = self.programs.get(program.0).ok_or_else(|| Error::run_failed(format!("{program:?} does not belong to this runner")))?;
        run_executable(executable, input, limits)
    }

    /// Runs every program on the same input, returning the results in order.
    pub fn check_programs(&self, input: &str, programs: &[ProgramHandle], limits: Limits) -> Result<Vec<RunResult>> {
        let jobs: Vec<_> = programs.iter().map(|&program| Job { program, input }).collect();
        self.run_all(&jobs, limits)
    }

    /// Runs every job, returning the results in order.
    pub fn run_all(&self, jobs: &[Job], limits: Limits) -> Result<Vec<RunResult>> {
        let mut results = vec![None; jobs.len()];
        self.run_jobs(jobs, limits, &|_job| false, |job, result| results[job] = result)?;
        results.into_iter().collect::<Option<Vec<_>>>().ok_or_else(|| Error::run_failed("a run ended without a result"))
    }

    /// Workers take jobs in order and ask `skip` right before starting each one. `on_result` runs on
    /// the calling thread as each job ends (`None` if skipped), so it may change what `skip` answers.
    /// After an error no new job starts.
    pub fn run_jobs<F: FnMut(usize, Option<RunResult>)>(&self, jobs: &[Job], limits: Limits, skip: &(dyn Fn(usize) -> bool + Sync), mut on_result: F) -> Result<()> {
        let (next_job, stopped) = (AtomicUsize::new(0), AtomicBool::new(false));
        let (sender, receiver) = mpsc::channel();
        let mut first_error = None;

        std::thread::scope(|scope| {
            let _stop_on_unwind = StopOnDrop(&stopped);
            for _ in 0..self.workers.min(jobs.len()) {
                let (sender, next_job, stopped) = (sender.clone(), &next_job, &stopped);
                scope.spawn(move || {
                    while !stopped.load(Ordering::Relaxed) {
                        let idx = next_job.fetch_add(1, Ordering::Relaxed);
                        let Some(job) = jobs.get(idx) else { break };
                        let outcome = if skip(idx) {
                            Ok(None)
                        } else {
                            catch_unwind(AssertUnwindSafe(|| self.run(job.program, job.input, limits)))
                                .unwrap_or_else(|_panic| Err(Error::run_failed(format!("the worker thread for job {idx} panicked"))))
                                .map(Some)
                        };
                        if outcome.is_err() {
                            stopped.store(true, Ordering::Relaxed);
                        }
                        if sender.send((idx, outcome)).is_err() {
                            break;
                        }
                    }
                });
            }
            drop(sender);

            for (idx, outcome) in receiver {
                match outcome {
                    Ok(result) => {
                        trace!("Job {idx} finished with result: {result:?}");
                        on_result(idx, result);
                    }
                    Err(err) => {
                        first_error.get_or_insert(err);
                    }
                }
            }
        });

        first_error.map_or(Ok(()), Err)
    }

    /// Removes every file of the build folder that belongs to no added program and was not kept.
    pub fn clean_build_folder(&self) -> Result<()> {
        for entry in std::fs::read_dir(&self.build_folder).map_err(Error::io(&self.build_folder))? {
            let path = entry.map_err(Error::io(&self.build_folder))?.path();
            let removed = if path.is_file() && !self.necessary_files.contains(&path) {
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
