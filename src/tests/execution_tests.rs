use crate::tests::common::{ALLOCATE, CPU_MS, ECHO, HELLO_WORLD, assert_output, output, run, run_on, run_with, run_within};
use crate::{Limits, RunResult};
use std::time::{Duration, Instant};

fn busy_loop() -> String {
    format!("#include <iostream>\n{CPU_MS}\nint main() {{ long long ms; std::cin >> ms; while (cpu_ms() < ms) {{}} std::cout << \"done\"; }}")
}

#[test]
fn hello_world() {
    assert_output(&run(HELLO_WORLD, "", 1000), "Hello, World!");
}

#[test]
fn reads_stdin() {
    assert_output(&run(ECHO, "-123456789012\n", 1000), "-123456789012");
}

#[test]
fn empty_input_is_end_of_file() {
    let source = "#include <iostream>\nint main() { int x, count = 0; while (std::cin >> x) count++; std::cout << count; }";
    assert_output(&run(source, "", 1000), "0");
}

#[test]
fn empty_output() {
    assert_eq!(output(&run("int main() {}", "", 1000)), Some(""));
}

#[test]
fn output_is_exact() {
    let source = "#include <cstdio>\nint main() { std::printf(\"  a\\n\\nb  \\n\"); }";
    assert_eq!(output(&run(source, "", 1000)), Some("  a\n\nb  \n"));
}

#[test]
fn invalid_utf8_output_is_replaced() {
    let source = "#include <cstdio>\nint main() { std::printf(\"a\\xff\\xfe b\"); }";
    assert_eq!(output(&run(source, "", 1000)), Some("a\u{fffd}\u{fffd} b"));
}

#[test]
fn infinite_loop_times_out() {
    assert_eq!(run("int main() { while (true) {} }", "", 1000), RunResult::TimedOut);
}

#[test]
fn infinite_loop_is_stopped_soon_after_the_limit() {
    let start = Instant::now();
    assert_eq!(run("int main() { while (true) {} }", "", 500), RunResult::TimedOut);
    assert!(start.elapsed() < Duration::from_secs(20), "took {:?}", start.elapsed());
}

#[test]
fn over_a_sub_second_limit_is_a_timeout() {
    let (_dir, runner) = crate::tests::common::runner();
    assert_eq!(run_on(&runner, &busy_loop(), "400", Limits::new().with_time_ms(100)).unwrap(), RunResult::TimedOut);
    assert_output(&run_on(&runner, &busy_loop(), "400", Limits::new().with_time_ms(5000)).unwrap(), "done");
}

#[test]
fn cpu_time_is_measured() {
    let RunResult::Ok { time_ms, .. } = run(&busy_loop(), "300", 5000) else { unreachable!() };
    assert!((200..3000).contains(&time_ms), "a 300 ms busy loop was measured at {time_ms} ms");

    let RunResult::Ok { time_ms, .. } = run(HELLO_WORLD, "", 5000) else { unreachable!() };
    assert!(time_ms < 200, "hello world was measured at {time_ms} ms");
}

#[test]
fn blocked_program_is_stopped_by_the_wall_clock() {
    let source = "#include <chrono>\n#include <thread>\nint main() { std::this_thread::sleep_for(std::chrono::hours(1)); }";
    let start = Instant::now();
    assert_eq!(run_within(Duration::from_secs(60), "a program that sleeps", move || run(source, "", 100)), RunResult::TimedOut);
    assert!(start.elapsed() < Duration::from_secs(10), "took {:?}", start.elapsed());
}

#[test]
fn program_waiting_for_input_gets_end_of_file() {
    let source = "#include <iostream>\nint main() { std::string s; while (std::cin >> s) {} std::cout << \"eof\"; }";
    assert_output(&run_within(Duration::from_secs(30), "a program reading all input", move || run(source, "a b c", 1000)), "eof");
}

#[test]
fn deep_recursion_has_a_large_stack() {
    let source = "#include <iostream>
        void recurse(int depth) { volatile char frame[1024]; for (int i = 0; i < 1024; ++i) frame[i] = (char)i; if (depth > 0) recurse(depth - 1); if (frame[0] != 0) std::cout << 'x'; }
        int main() { recurse(100000); std::cout << \"Success\"; }";
    assert_output(&run(source, "", 2000), "Success");
}

#[test]
fn nonzero_exit_code_is_a_crash() {
    assert_eq!(run("int main() { return 3; }", "", 1000), RunResult::ExitCode(3));
}

#[test]
fn exit_code_with_output_is_still_a_crash() {
    let source = "#include <cstdio>\n#include <cstdlib>\nint main() { std::printf(\"partial\"); std::exit(1); }";
    assert_eq!(run(source, "", 1000), RunResult::ExitCode(1));
}

#[test]
fn segfault_is_a_crash() {
    let result = run("int main() { volatile int* p = nullptr; *p = 1; }", "", 1000);
    assert!(matches!(result, RunResult::Signal(_) | RunResult::ExitCode(_)), "{result:?}");
}

#[test]
#[cfg(unix)]
fn abort_is_a_signal() {
    assert_eq!(run("#include <cstdlib>\nint main() { std::abort(); }", "", 1000), RunResult::Signal(libc::SIGABRT));
}

#[test]
fn uncaught_exception_is_a_crash() {
    let result = run("#include <stdexcept>\nint main() { throw std::runtime_error(\"boom\"); }", "", 1000);
    assert!(matches!(result, RunResult::Signal(_) | RunResult::ExitCode(_)), "{result:?}");
}

#[test]
fn ignoring_most_of_the_input_does_not_block() {
    let input = format!("7\n{}", (0..500_000).map(|i| i.to_string()).collect::<Vec<_>>().join(" "));
    let result = run_within(Duration::from_secs(90), "running a program that ignores its input", move || run(ECHO, &input, 5000));
    assert_output(&result, "7");
}

#[test]
fn large_input_and_large_output() {
    let count = 200_000_usize;
    let input = (0..count).map(|i| i.to_string()).collect::<Vec<_>>().join("\n");
    let source = "#include <iostream>
        int main() { std::ios_base::sync_with_stdio(false); long long sum = 0, x; while (std::cin >> x) { sum += x; std::cout << x << '\\n'; } std::cout << \"sum \" << sum; }";

    let result = run_within(Duration::from_secs(90), "running a program with large input and output", move || run(source, &input, 10000));

    let RunResult::Ok { output, .. } = result else { unreachable!("expected OK, got {result:?}") };
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), count + 1);
    assert_eq!(lines[count], format!("sum {}", count * (count - 1) / 2));
}

#[test]
fn stderr_is_discarded() {
    let source = "#include <iostream>\nint main() { std::cerr << \"debug 123\\ntrailing\"; std::cout << 42; }";
    assert_eq!(output(&run(source, "", 1000)), Some("42"));
}

#[test]
fn lots_of_stderr_does_not_block() {
    let source = "#include <iostream>\n#include <string>
        int main() { std::string line(1 << 12, 'd'); for (int i = 0; i < 4096; i++) std::cerr << line << '\\n'; std::cout << 42; }";
    assert_output(&run(source, "", 5000), "42");
}

#[test]
fn runaway_output_is_stopped() {
    let source = "#include <cstdio>\n#include <string>
        int main() { std::string chunk(1 << 20, 'x'); for (;;) fwrite(chunk.data(), 1, chunk.size(), stdout); }";
    assert_eq!(
        run_within(Duration::from_secs(60), "a program that never stops printing", move || run(source, "", 5000)),
        RunResult::OutputLimitExceeded
    );
}

#[test]
fn output_limit_is_exact() {
    let source = "#include <iostream>\n#include <string>\nint main() { int n; std::cin >> n; std::cout << std::string(n, 'x'); }";
    let limits = Limits::new().with_output_bytes(100);
    assert_eq!(output(&run_with(source, "100", limits)).map(str::len), Some(100));
    assert_eq!(run_with(source, "101", limits), RunResult::OutputLimitExceeded);
}

#[test]
fn output_limit_extremes() {
    let source = "#include <iostream>\n#include <string>\nint main() { int n; std::cin >> n; std::cout << std::string(n, 'x'); }";
    assert_eq!(output(&run_with(source, "0", Limits::new().with_output_bytes(0))), Some(""));
    assert_eq!(run_with(source, "1", Limits::new().with_output_bytes(0)), RunResult::OutputLimitExceeded);
    assert_eq!(output(&run_with(source, "5", Limits::new().with_output_bytes(1 << 40))), Some("xxxxx"));
}

#[test]
fn peak_memory_is_reported() {
    let peak = |megabytes: usize| match run(ALLOCATE, &megabytes.to_string(), 5000) {
        RunResult::Ok { memory_kb, .. } => memory_kb,
        other => unreachable!("expected OK, got {other:?}"),
    };
    let (small, large) = (peak(1), peak(100));
    assert!(small > 0);
    assert!(small < 50 << 10, "a program using 1 MB was measured at {small} kB");
    assert!((100 << 10..200 << 10).contains(&large), "a program using 100 MB was measured at {large} kB");
}

#[test]
fn memory_limit() {
    let limited = |megabytes: usize, limit_megabytes: u64| run_with(ALLOCATE, &megabytes.to_string(), Limits::new().with_time_ms(5000).with_memory_mb(limit_megabytes));
    assert_output(&limited(10, 64), &(10 << 20).to_string());
    assert_eq!(limited(200, 64), RunResult::MemoryLimitExceeded);
}

#[test]
fn no_memory_limit_allows_large_allocations() {
    assert_output(&run(ALLOCATE, "300", 5000), &(300 << 20).to_string());
}

#[test]
#[cfg(not(target_os = "macos"))]
fn runaway_memory_is_stopped() {
    let result = run_with(ALLOCATE, "8192", Limits::new().with_time_ms(5000).with_memory_mb(64));
    assert!(matches!(result, RunResult::MemoryLimitExceeded | RunResult::ExitCode(_) | RunResult::Signal(_)), "got {result:?}");
}

#[test]
fn time_limit_beats_memory_limit() {
    let source = "#include <vector>\nint main() { std::vector<char> v(200 << 20, 1); volatile char* p = v.data(); while (p[0]) {} }";
    assert_eq!(run_with(source, "", Limits::new().with_time_ms(500).with_memory_mb(64)), RunResult::TimedOut);
}
