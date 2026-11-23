use crate::cache::MAX_CACHED_OUTPUT_BYTES;
use crate::tests::common::{ECHO, HELLO_WORLD, assert_output, binaries_in, files_in, limits, logging_echo, precompile, run_on, runner, runs_logged};
use crate::{Error, Limits, RunHandle, RunResult, Runner};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const SLEEP_300_MS: &str = "#include <chrono>\n#include <iostream>\n#include <thread>
    int main() { std::string s; std::cin >> s; std::this_thread::sleep_for(std::chrono::milliseconds(300)); std::cout << s; }";

const fn assert_send_sync<T: Send + Sync>() {}
const fn assert_send<T: Send>() {}

#[test]
fn types_are_thread_safe() {
    assert_send_sync::<Runner>();
    assert_send_sync::<Error>();
    assert_send::<RunHandle>();
}

#[test]
fn run_program() {
    let (_dir, runner) = runner();
    assert_output(&run_on(&runner, HELLO_WORLD, "", limits(1000)).unwrap(), "Hello, World!");
}

#[test]
fn compile_error() {
    let (_dir, runner) = runner();
    let faulty = "int main() { compile error here }";
    assert!(matches!(run_on(&runner, faulty, "", limits(1000)), Err(Error::CompilerError { .. })));
    assert!(matches!(runner.spawn(faulty, "", limits(1000)), Err(Error::CompilerError { .. })));
}

#[test]
fn a_program_is_compiled_when_first_run() {
    let (dir, runner) = runner();
    assert_eq!(binaries_in(dir.path()).len(), 0);
    assert_output(&run_on(&runner, ECHO, "5", limits(1000)).unwrap(), "5");
    assert_output(&run_on(&runner, ECHO, "6", limits(1000)).unwrap(), "6");
    assert_eq!(binaries_in(dir.path()).len(), 1);
}

#[test]
fn many_programs_many_inputs() {
    let (_dir, runner) = runner();
    let programs: Vec<String> = (0..5)
        .map(|id| format!("#include <iostream>\nint main() {{ int n; std::cin >> n; std::cout << {id} << ' ' << n; }}"))
        .collect();

    let handles: Vec<Vec<RunHandle>> = (0..20)
        .map(|i| programs.iter().map(|program| runner.spawn(program, format!("{i}\n"), limits(1000)).unwrap()).collect())
        .collect();

    for (i, row) in handles.into_iter().enumerate() {
        for (j, handle) in row.into_iter().enumerate() {
            assert_output(&handle.wait().unwrap(), &format!("{j} {i}"));
        }
    }
}

#[test]
fn same_program_is_compiled_once() {
    let (dir, runner) = runner();
    let start = Instant::now();
    for _ in 0..100 {
        run_on(&runner, HELLO_WORLD, "", limits(1000)).unwrap();
    }
    assert_eq!(binaries_in(dir.path()).len(), 1);
    assert!(start.elapsed() < Duration::from_secs(10), "took {:?}", start.elapsed());
}

#[test]
fn build_folder_is_created() {
    let dir = TempDir::new().unwrap();
    let nested = dir.path().join("a").join("b");
    let runner = Runner::new(&nested).unwrap();
    assert_output(&run_on(&runner, HELLO_WORLD, "", limits(1000)).unwrap(), "Hello, World!");
    assert!(nested.is_dir());
}

#[test]
fn runners_sharing_a_build_folder() {
    let dir = TempDir::new().unwrap();
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let build_folder = dir.path().to_path_buf();
            std::thread::spawn(move || run_on(&Runner::new(&build_folder)?, HELLO_WORLD, "", limits(5000)))
        })
        .collect();

    for worker in workers {
        assert_output(&worker.join().unwrap().unwrap(), "Hello, World!");
    }
    let files: Vec<_> = files_in(dir.path()).into_iter().filter(|path| path.file_name().unwrap() != "results.cache").collect();
    assert_eq!(files.len(), 2, "{files:?}");
}

#[test]
fn one_runner_shared_between_threads() {
    let (_dir, runner) = runner();
    let runner = &runner;
    std::thread::scope(|scope| {
        let threads: Vec<_> = (0..8).map(|i| (i, scope.spawn(move || run_on(runner, ECHO, &i.to_string(), limits(5000))))).collect();
        for (i, thread) in threads {
            assert_output(&thread.join().unwrap().unwrap(), &i.to_string());
        }
    });
}

#[test]
#[cfg(not(windows))]
fn compiled_programs_are_picked_up_by_new_runners() {
    let dir = TempDir::new().unwrap();
    let run_once = |input: &str| {
        let runner = Runner::new(dir.path()).unwrap();
        assert_output(&run_on(&runner, ECHO, input, limits(1000)).unwrap(), input);
    };

    run_once("0");
    let start = Instant::now();
    for i in 1..30 {
        run_once(&i.to_string());
    }
    assert!(start.elapsed() < Duration::from_secs(10), "took {:?}", start.elapsed());
}

#[test]
fn missing_binary_is_an_error_not_a_crash() {
    let (dir, runner) = runner();
    runner.set_result_cache(false);
    run_on(&runner, HELLO_WORLD, "", limits(1000)).unwrap();
    std::fs::remove_file(&binaries_in(dir.path())[0]).unwrap();

    let result = run_on(&runner, HELLO_WORLD, "", limits(1000));
    assert!(matches!(result, Err(Error::RunFailed { .. })), "got {result:?}");
}

#[test]
fn runs_start_in_the_order_they_are_spawned() {
    let (dir, runner) = runner();
    runner.set_workers(1);
    let log = dir.path().join("log");
    let program = logging_echo(&log);

    let handles: Vec<_> = (0..20).map(|i| runner.spawn(&program, i.to_string(), limits(5000)).unwrap()).collect();
    for (i, handle) in handles.into_iter().enumerate() {
        assert_output(&handle.wait().unwrap(), &i.to_string());
    }
    assert_eq!(runs_logged(&log), (0..20).map(|i| i.to_string()).collect::<Vec<_>>());
}

#[test]
fn results_come_back_whatever_the_number_of_workers() {
    let (_dir, runner) = runner();
    for workers in [1, 3, 16] {
        runner.set_workers(workers);
        let handles: Vec<_> = (0..40).map(|i| runner.spawn(ECHO, format!("{}", workers * 100 + i), limits(5000)).unwrap()).collect();
        for (i, handle) in handles.into_iter().enumerate() {
            assert_output(&handle.wait().unwrap(), &(workers * 100 + i).to_string());
        }
    }
}

#[test]
fn zero_workers_count_as_one() {
    let (_dir, runner) = runner();
    runner.set_workers(0);
    assert_output(&run_on(&runner, ECHO, "5", limits(1000)).unwrap(), "5");
}

#[test]
fn runs_go_in_parallel() {
    let (dir, runner) = runner();
    runner.set_workers(4);
    precompile(dir.path(), SLEEP_300_MS);
    let start = Instant::now();
    let handles: Vec<_> = (0..4).map(|i| runner.spawn(SLEEP_300_MS, i.to_string(), limits(1000)).unwrap()).collect();
    for handle in handles {
        assert!(matches!(handle.wait().unwrap(), RunResult::Ok { .. }));
    }
    assert!(start.elapsed() < Duration::from_millis(1000), "took {:?}", start.elapsed());
}

#[test]
fn fewer_workers_run_fewer_at_once() {
    let (_dir, runner) = runner();
    runner.set_workers(4);
    run_on(&runner, SLEEP_300_MS, "warm", limits(1000)).unwrap();
    runner.set_workers(1);
    let start = Instant::now();
    let handles: Vec<_> = (0..3).map(|i| runner.spawn(SLEEP_300_MS, i.to_string(), limits(1000)).unwrap()).collect();
    for handle in handles {
        handle.wait().unwrap();
    }
    assert!(start.elapsed() >= Duration::from_millis(850), "took {:?}", start.elapsed());
}

#[test]
fn mixed_verdicts() {
    let (_dir, runner) = runner();
    let crash = "int main() { return 1; }";
    let hang = "int main() { volatile bool forever = true; while (forever) {} }";
    let handles = [
        runner.spawn(ECHO, "1", limits(300)).unwrap(),
        runner.spawn(crash, "", limits(300)).unwrap(),
        runner.spawn(hang, "", limits(300)).unwrap(),
        runner.spawn(ECHO, "2", limits(300)).unwrap(),
    ];
    let [echo_1, crashed, hung, echo_2] = handles.map(|handle| handle.wait().unwrap());

    assert_output(&echo_1, "1");
    assert_eq!(crashed, RunResult::ExitCode(1));
    assert_eq!(hung, RunResult::TimedOut);
    assert_output(&echo_2, "2");
}

#[test]
fn cancelled_runs_do_not_start() {
    let (dir, runner) = runner();
    runner.set_workers(1);
    let log = dir.path().join("log");
    let program = logging_echo(&log);
    precompile(dir.path(), &program);

    let blocker = runner.spawn(SLEEP_300_MS, "blocker", limits(1000)).unwrap();
    let handles: Vec<_> = (0..6).map(|i| runner.spawn(&program, i.to_string(), limits(1000)).unwrap()).collect();
    for handle in handles.iter().skip(1).step_by(2) {
        handle.cancel();
    }

    assert_output(&blocker.wait().unwrap(), "blocker");
    for (i, handle) in handles.into_iter().enumerate() {
        let result = handle.wait();
        if i % 2 == 1 {
            assert!(matches!(result, Err(Error::Cancelled)), "run {i} got {result:?}");
        } else {
            assert_output(&result.unwrap(), &i.to_string());
        }
    }
    assert_eq!(runs_logged(&log), ["0", "2", "4"]);
}

#[test]
fn dropped_handles_cancel_their_runs() {
    let (dir, runner) = runner();
    runner.set_workers(1);
    let log = dir.path().join("log");
    let program = logging_echo(&log);
    precompile(dir.path(), &program);

    let blocker = runner.spawn(SLEEP_300_MS, "blocker", limits(1000)).unwrap();
    drop(runner.spawn(&program, "dropped", limits(1000)).unwrap());
    let kept = runner.spawn(&program, "kept", limits(1000)).unwrap();

    blocker.wait().unwrap();
    assert_output(&kept.wait().unwrap(), "kept");
    assert_eq!(runs_logged(&log), ["kept"]);
}

#[test]
fn cancelling_a_finished_run_changes_nothing() {
    let (_dir, runner) = runner();
    let handle = runner.spawn(ECHO, "3", limits(1000)).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    handle.cancel();
    assert_output(&handle.wait().unwrap(), "3");
}

#[test]
fn dropping_the_runner_cancels_queued_runs() {
    let (_dir, runner) = runner();
    runner.set_workers(1);
    let blocker = runner.spawn(SLEEP_300_MS, "blocker", limits(1000)).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let queued = runner.spawn(SLEEP_300_MS, "queued", limits(1000)).unwrap();

    drop(runner);

    assert_output(&blocker.wait().unwrap(), "blocker");
    assert!(matches!(queued.wait(), Err(Error::Cancelled)));
}

#[test]
fn handles_can_be_waited_for_on_other_threads() {
    let (_dir, runner) = runner();
    let handles: Vec<_> = (0..4).map(|i| runner.spawn(ECHO, i.to_string(), limits(1000)).unwrap()).collect();
    let threads: Vec<_> = handles.into_iter().map(|handle| std::thread::spawn(move || handle.wait())).collect();
    for (i, thread) in threads.into_iter().enumerate() {
        assert_output(&thread.join().unwrap().unwrap(), &i.to_string());
    }
}

#[test]
fn small_outputs_are_cached() {
    let (dir, runner) = runner();
    let log = dir.path().join("log");
    let program = logging_echo(&log);

    for _ in 0..3 {
        assert_output(&run_on(&runner, &program, "YES", limits(1000)).unwrap(), "YES");
    }
    assert_eq!(runs_logged(&log), ["YES"]);
}

#[test]
fn cached_results_keep_their_measurements() {
    let (_dir, runner) = runner();
    let first = run_on(&runner, ECHO, "42", limits(1000)).unwrap();
    assert_eq!(run_on(&runner, ECHO, "42", limits(1000)).unwrap(), first);
}

#[test]
fn large_outputs_are_not_cached() {
    let (dir, runner) = runner();
    let log = dir.path().join("log");
    let program = logging_echo(&log);
    let long = "x".repeat(MAX_CACHED_OUTPUT_BYTES + 1);
    let longest_cached = "y".repeat(MAX_CACHED_OUTPUT_BYTES);

    for _ in 0..2 {
        assert_output(&run_on(&runner, &program, &long, limits(1000)).unwrap(), &long);
        assert_output(&run_on(&runner, &program, &longest_cached, limits(1000)).unwrap(), &longest_cached);
    }
    assert_eq!(runs_logged(&log), [long.clone(), longest_cached, long]);
}

#[test]
fn failures_are_cached() {
    let (dir, runner) = runner();
    let log = dir.path().join("log");
    let log_in_source = log.to_string_lossy().replace('\\', "/");
    let failing = |body: &str| format!("#include <fstream>\n#include <vector>\nint main() {{ std::ofstream(\"{log_in_source}\", std::ios::app) << \"run\\n\"; {body} }}");
    let programs = [
        failing("volatile bool forever = true; while (forever) {}"),
        failing("return 3;"),
        failing("std::vector<char> v(100 << 20, 1); volatile char* p = v.data(); long long sum = 0; for (size_t i = 0; i < v.size(); i += 4096) sum += p[i]; return sum == 0;"),
    ];
    let limits = Limits::new().with_time_ms(200).with_memory_mb(32);

    let first: Vec<_> = programs.iter().map(|program| run_on(&runner, program, "", limits).unwrap()).collect();
    assert_eq!(first[0], RunResult::TimedOut);
    assert_eq!(first[1], RunResult::ExitCode(3));
    assert_eq!(first[2], RunResult::MemoryLimitExceeded);

    let start = Instant::now();
    let again: Vec<_> = programs.iter().map(|program| run_on(&runner, program, "", limits).unwrap()).collect();
    assert_eq!(again, first);
    assert!(start.elapsed() < Duration::from_millis(150), "cached results took {:?}", start.elapsed());
    assert_eq!(runs_logged(&log).len(), 3);
}

#[test]
fn cache_is_keyed_by_input_and_limits() {
    let (dir, runner) = runner();
    let log = dir.path().join("log");
    let program = logging_echo(&log);

    run_on(&runner, &program, "a", limits(1000)).unwrap();
    run_on(&runner, &program, "b", limits(1000)).unwrap();
    run_on(&runner, &program, "a", limits(2000)).unwrap();
    run_on(&runner, &program, "a", limits(1000).with_memory_mb(512)).unwrap();
    run_on(&runner, &program, "a", limits(1000).with_output_bytes(10)).unwrap();
    run_on(&runner, &program, "a", limits(1000)).unwrap();

    assert_eq!(runs_logged(&log), ["a", "b", "a", "a", "a"]);
}

#[test]
fn cache_is_keyed_by_program() {
    let (dir, runner) = runner();
    let (log_1, log_2) = (dir.path().join("log1"), dir.path().join("log2"));
    run_on(&runner, &logging_echo(&log_1), "a", limits(1000)).unwrap();
    run_on(&runner, &logging_echo(&log_2), "a", limits(1000)).unwrap();
    assert_eq!((runs_logged(&log_1).len(), runs_logged(&log_2).len()), (1, 1));
}

#[test]
fn result_cache_can_be_turned_off() {
    let (dir, runner) = runner();
    let log = dir.path().join("log");
    let program = logging_echo(&log);
    run_on(&runner, &program, "a", limits(1000)).unwrap();

    runner.set_result_cache(false);
    run_on(&runner, &program, "a", limits(1000)).unwrap();
    run_on(&runner, &program, "b", limits(1000)).unwrap();
    runner.set_result_cache(true);
    run_on(&runner, &program, "a", limits(1000)).unwrap();
    run_on(&runner, &program, "b", limits(1000)).unwrap();

    assert_eq!(runs_logged(&log), ["a", "a", "b", "b"]);
}

#[test]
fn result_cache_is_shared_through_the_build_folder() {
    let dir = TempDir::new().unwrap();
    let log = dir.path().join("log");
    let program = logging_echo(&log);
    let build_folder = dir.path().join("build");

    run_on(&Runner::new(&build_folder).unwrap(), &program, "a", limits(1000)).unwrap();
    let later = Runner::new(&build_folder).unwrap();
    assert_output(&run_on(&later, &program, "a", limits(1000)).unwrap(), "a");

    assert_eq!(runs_logged(&log), ["a"]);
}

#[test]
fn damaged_cache_file_is_ignored() {
    let (dir, runner) = runner();
    std::fs::write(dir.path().join("results.cache"), "garbage\n0 1 2\n\u{0}\u{1}\nffff ffff 1 - 1 OK 1 1 -zz\n").unwrap();
    assert_output(&run_on(&runner, ECHO, "9", limits(1000)).unwrap(), "9");
}
