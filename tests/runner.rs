mod common;

use common::{ECHO, HELLO_WORLD, assert_output, binaries_in, files_in, limits, runner};
use cpprunner::{Compiler, CppRunner, Error, Job, MAX_DEFAULT_WORKERS, RunResult};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const fn assert_send_sync<T: Send + Sync>() {}
const _: () = assert_send_sync::<CppRunner>();
const _: () = assert_send_sync::<Compiler>();
const _: () = assert_send_sync::<Error>();

#[test]
fn run_program() {
    let (_dir, mut runner) = runner();
    let program = runner.add_program(HELLO_WORLD).unwrap();
    assert_output(&runner.run(program, "", limits(1000)).unwrap(), "Hello, World!");
}

#[test]
fn faulty_program_is_a_compiler_error() {
    let (_dir, mut runner) = runner();
    assert!(matches!(runner.add_program("int main() { compile error here }"), Err(Error::CompilerError { .. })));
    assert_eq!(runner.programs(), []);
}

#[test]
fn add_file() {
    let (dir, mut runner) = runner();
    let source = dir.path().join("hello.cpp");
    std::fs::write(&source, HELLO_WORLD).unwrap();

    let from_file = runner.add_file(&source).unwrap();

    assert_eq!(from_file, runner.add_program(HELLO_WORLD).unwrap());
    assert_output(&runner.run(from_file, "", limits(1000)).unwrap(), "Hello, World!");
}

#[test]
fn add_missing_file_is_an_io_error() {
    let (dir, mut runner) = runner();
    let missing = dir.path().join("missing.cpp");
    let Err(Error::Io { path, .. }) = runner.add_file(&missing) else { unreachable!() };
    assert_eq!(path, missing);
}

#[test]
fn many_programs_many_inputs() {
    let (_dir, mut runner) = runner();
    let programs: Vec<_> = (0..5)
        .map(|id| {
            runner
                .add_program(&format!("#include <iostream>\nint main() {{ int n; std::cin >> n; std::cout << {id} << ' ' << n; }}"))
                .unwrap()
        })
        .collect();
    assert_eq!(runner.programs(), programs);

    for i in 0..20 {
        for (j, result) in runner.check_programs(&format!("{i}\n"), &programs, limits(1000)).unwrap().iter().enumerate() {
            assert_output(result, &format!("{j} {i}"));
        }
    }
}

#[test]
fn same_program_is_compiled_once() {
    let (dir, mut runner) = runner();
    let start = Instant::now();
    let handles: Vec<_> = (0..100).map(|_| runner.add_program(HELLO_WORLD).unwrap()).collect();
    assert!(handles.windows(2).all(|pair| pair[0] == pair[1]));
    assert_eq!(runner.programs().len(), 1);
    assert_eq!(files_in(dir.path()).len(), 2);
    assert!(start.elapsed() < Duration::from_secs(10), "took {:?}", start.elapsed());
}

#[test]
fn executables_live_in_the_build_folder() {
    let (dir, mut runner) = runner();
    let program = runner.add_program(HELLO_WORLD).unwrap();
    let executable = runner.executable(program);
    assert!(executable.is_file());
    assert_eq!(executable.parent(), Some(runner.build_folder()));
    assert_eq!(runner.build_folder(), dunce::canonicalize(dir.path()).unwrap());
}

#[test]
fn program_hash_is_stable_across_runners() {
    let dir = TempDir::new().unwrap();
    let hash = |source: &str| {
        let mut runner = CppRunner::new(dir.path()).unwrap();
        let program = runner.add_program(source).unwrap();
        runner.program_hash(program)
    };
    assert_eq!(hash(HELLO_WORLD), hash(HELLO_WORLD));
    assert_ne!(hash(HELLO_WORLD), hash(ECHO));
    assert_eq!(hash(HELLO_WORLD), Compiler::detect().unwrap().program_hash(HELLO_WORLD));
}

#[test]
fn different_flags_share_a_build_folder_without_mixing_up_programs() {
    let dir = TempDir::new().unwrap();
    let source = "#include <iostream>\nint main() { std::cout << VALUE; }";
    let output = |value: u32| {
        let compiler = Compiler::detect().unwrap().with_arg(&format!("-DVALUE={value}"));
        let mut runner = CppRunner::with_compiler(dir.path(), compiler).unwrap();
        let program = runner.add_program(source).unwrap();
        runner.run(program, "", limits(1000)).unwrap()
    };
    assert_output(&output(1), "1");
    assert_output(&output(2), "2");
    assert_output(&output(1), "1");
    assert_eq!(binaries_in(dir.path()).len(), 2);
}

#[test]
fn runner_finds_what_compile_cached_compiled() {
    let dir = TempDir::new().unwrap();
    let compiler = Compiler::detect().unwrap();
    let compiled = compiler.compile_cached(dir.path(), HELLO_WORLD).unwrap();
    let modified = std::fs::metadata(&compiled.executable).unwrap().modified().unwrap();

    let mut runner = CppRunner::with_compiler(dir.path(), compiler).unwrap();
    let program = runner.add_program(HELLO_WORLD).unwrap();

    assert_eq!(runner.executable(program), compiled.executable);
    assert_eq!(runner.program_hash(program), compiled.hash);
    assert_eq!(std::fs::metadata(&compiled.executable).unwrap().modified().unwrap(), modified);
}

#[test]
fn runners_sharing_a_build_folder() {
    let dir = TempDir::new().unwrap();
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let build_folder = dir.path().to_path_buf();
            std::thread::spawn(move || -> cpprunner::Result<RunResult> {
                let mut runner = CppRunner::new(&build_folder)?;
                let program = runner.add_program(HELLO_WORLD)?;
                runner.run(program, "", limits(5000))
            })
        })
        .collect();

    for worker in workers {
        assert_output(&worker.join().unwrap().unwrap(), "Hello, World!");
    }
    assert_eq!(files_in(dir.path()).len(), 2, "{:?}", files_in(dir.path()));
}

#[test]
#[cfg(not(windows))]
fn cache_is_picked_up_by_new_runners() {
    let dir = TempDir::new().unwrap();
    let run_once = || {
        let mut runner = CppRunner::new(dir.path()).unwrap();
        let program = runner.add_program(HELLO_WORLD).unwrap();
        assert_output(&runner.run(program, "", limits(1000)).unwrap(), "Hello, World!");
    };

    run_once();
    let start = Instant::now();
    for _ in 0..29 {
        run_once();
    }
    assert!(start.elapsed() < Duration::from_secs(10), "took {:?}", start.elapsed());
}

#[test]
fn missing_binary_is_an_error_not_a_crash() {
    let (dir, mut runner) = runner();
    let program = runner.add_program(HELLO_WORLD).unwrap();
    std::fs::remove_file(&binaries_in(dir.path())[0]).unwrap();

    let result = runner.check_programs("", &[program], limits(1000));
    assert!(matches!(result, Err(Error::RunFailed { .. })), "got {result:?}");
}

#[test]
fn foreign_handle_is_an_error() {
    let (_dir, mut runner) = runner();
    let (_other_dir, mut other) = common::runner();
    other.add_program(HELLO_WORLD).unwrap();
    let foreign = other.add_program(ECHO).unwrap();
    runner.add_program(HELLO_WORLD).unwrap();

    assert!(matches!(runner.run(foreign, "", limits(1000)), Err(Error::RunFailed { .. })));
    assert!(matches!(runner.run_all(&[Job { program: foreign, input: "" }], limits(1000)), Err(Error::RunFailed { .. })));
}

#[test]
#[cfg(not(windows))]
fn run_all_reports_failure_without_hanging() {
    let result = common::run_within(Duration::from_secs(60), "running jobs that all fail to start", || {
        let (dir, mut runner) = runner();
        let program = runner.add_program(HELLO_WORLD).unwrap();
        std::fs::remove_file(&binaries_in(dir.path())[0]).unwrap();
        runner.run_all(&[Job { program, input: "" }; 20], limits(1000))
    });
    assert!(result.is_err(), "got {result:?}");
}

#[test]
fn clean_build_folder_keeps_compiled_programs() {
    let (dir, mut runner) = runner();
    let program = runner.add_program(HELLO_WORLD).unwrap();
    let stray = dir.path().join("stray.txt");
    std::fs::write(&stray, "junk").unwrap();
    let scratch = dir.path().join(".scratch-abc");
    std::fs::create_dir_all(&scratch).unwrap();
    std::fs::write(scratch.join("p1.cpp"), "junk").unwrap();
    let other_folder = dir.path().join("subfolder");
    std::fs::create_dir_all(&other_folder).unwrap();

    runner.clean_build_folder().unwrap();

    assert!(!stray.exists());
    assert!(!scratch.exists());
    assert!(other_folder.exists());
    assert_output(&runner.run(program, "", limits(1000)).unwrap(), "Hello, World!");
}

#[test]
fn clean_build_folder_removes_programs_of_other_runners() {
    let dir = TempDir::new().unwrap();
    CppRunner::new(dir.path()).unwrap().add_program(ECHO).unwrap();

    let mut runner = CppRunner::new(dir.path()).unwrap();
    runner.add_program(HELLO_WORLD).unwrap();
    runner.clean_build_folder().unwrap();

    assert_eq!(files_in(dir.path()).len(), 2);
    assert_eq!(binaries_in(runner.build_folder()), [runner.executable(runner.programs()[0]).to_path_buf()]);
}

#[test]
fn clean_build_folder_spares_kept_files() {
    let (_dir, mut runner) = runner();
    let kept = runner.build_folder().join("cache.json");
    std::fs::write(&kept, "{}").unwrap();
    runner.keep(kept.clone());

    runner.clean_build_folder().unwrap();

    assert!(kept.exists());
}

#[test]
fn workers() {
    let (_dir, mut runner) = runner();
    assert!((1..=MAX_DEFAULT_WORKERS).contains(&runner.workers()));
    runner.set_workers(7);
    assert_eq!(runner.workers(), 7);
    runner.set_workers(0);
    assert_eq!(runner.workers(), 1);
    assert_eq!(runner.with_workers(3).workers(), 3);
}

#[test]
fn jobs_are_answered_in_order_whatever_the_number_of_workers() {
    let (_dir, mut runner) = runner();
    let echo = runner.add_program(ECHO).unwrap();
    let inputs: Vec<String> = (0..40).map(|i| i.to_string()).collect();
    let jobs: Vec<_> = inputs.iter().map(|input| Job { program: echo, input }).collect();

    for workers in [1, 3, 16] {
        runner.set_workers(workers);
        let results = runner.run_all(&jobs, limits(5000)).unwrap();
        assert_eq!(results.len(), inputs.len());
        for (result, input) in results.iter().zip(&inputs) {
            assert_output(result, input);
        }
    }
}

#[test]
fn run_all_mixes_programs_and_verdicts() {
    let (_dir, mut runner) = runner();
    let echo = runner.add_program(ECHO).unwrap();
    let crash = runner.add_program("int main() { return 1; }").unwrap();
    let hang = runner.add_program("int main() { while (true) {} }").unwrap();
    let jobs = [
        Job { program: echo, input: "1" },
        Job { program: crash, input: "" },
        Job { program: hang, input: "" },
        Job { program: echo, input: "2" },
    ];

    let results = runner.run_all(&jobs, limits(300)).unwrap();

    assert_output(&results[0], "1");
    assert!(matches!(results[1], RunResult::Crashed(_)));
    assert_eq!(results[2], RunResult::TimedOut);
    assert_output(&results[3], "2");
}

#[test]
fn run_all_of_nothing() {
    let (_dir, runner) = runner();
    assert_eq!(runner.run_all(&[], limits(1000)).unwrap(), []);
    assert_eq!(runner.check_programs("", &[], limits(1000)).unwrap(), []);
}

#[test]
fn jobs_run_in_parallel() {
    let (_dir, mut runner) = runner();
    let sleep = runner
        .add_program("#include <chrono>\n#include <thread>\nint main() { std::this_thread::sleep_for(std::chrono::milliseconds(500)); }")
        .unwrap();
    runner.set_workers(4);
    let jobs = [Job { program: sleep, input: "" }; 4];

    let start = Instant::now();
    assert!(runner.run_all(&jobs, limits(1000)).unwrap().iter().all(RunResult::is_ok));
    assert!(start.elapsed() < Duration::from_millis(1500), "took {:?}", start.elapsed());
}

#[test]
fn skipped_jobs_are_not_run() {
    let (_dir, mut runner) = runner();
    let program = runner.add_program(HELLO_WORLD).unwrap();
    let jobs = [Job { program, input: "" }; 10];
    let asked = Mutex::new(Vec::new());

    let mut answered = Vec::new();
    runner
        .run_jobs(
            &jobs,
            limits(5000),
            &|job| {
                asked.lock().unwrap().push(job);
                job % 2 == 1
            },
            |job, result| {
                assert_eq!(result.is_none(), job % 2 == 1, "job {job} got {result:?}");
                answered.push(job);
            },
        )
        .unwrap();

    answered.sort_unstable();
    let mut asked = asked.into_inner().unwrap();
    asked.sort_unstable();
    assert_eq!(answered, (0..10).collect::<Vec<_>>());
    assert_eq!(asked, (0..10).collect::<Vec<_>>());
}

#[test]
fn on_result_can_change_what_skip_answers() {
    let (_dir, mut runner) = runner();
    let program = runner.add_program(HELLO_WORLD).unwrap();
    runner.set_workers(1);
    let jobs = [Job { program, input: "" }; 200];
    let (finished, ran) = (AtomicUsize::new(0), AtomicUsize::new(0));

    runner
        .run_jobs(&jobs, limits(5000), &|_job| finished.load(Ordering::SeqCst) > 0, |_job, result| {
            if result.is_some() {
                ran.fetch_add(1, Ordering::SeqCst);
            }
            finished.store(1, Ordering::SeqCst);
        })
        .unwrap();

    assert!(ran.load(Ordering::SeqCst) < jobs.len(), "skip never took effect");
}
