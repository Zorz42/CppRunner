# CPP Runner

Compile and run C++ programs from Rust, with CPU time, memory and output limits. Works on Linux,
macOS and Windows.

```rust
use cpprunner::{Limits, RunResult, Runner};

let build_folder = tempfile::tempdir()?;
let runner = Runner::new(build_folder.path())?;
let sum = "#include <iostream>\nint main() { int a, b; std::cin >> a >> b; std::cout << a + b; }";

let limits = Limits::new().with_time_ms(1000).with_memory_mb(256);
match runner.spawn(sum, "1 2", limits)?.wait()? {
    RunResult::Ok { output, time_ms, memory_kb } => println!("{output} ({time_ms} ms, {memory_kb} kB)"),
    verdict => println!("{verdict:?}"),
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

- Programs are identified by their source and compiled on first use with `-std=c++17 -O2`, using
  `g++`, `c++`, `clang++` or MinGW, or the compiler in `GCC_PATH`. Compiled programs stay in the
  build folder, which can be shared between threads and processes.
- `spawn` queues a run and returns a handle to `wait` for or `cancel`; dropping the handle cancels
  it. Runs start in order and up to `set_workers` of them (by default the number of cores, at
  most 4) run at once.
- A run ends as `Ok`, `TimedOut`, `MemoryLimitExceeded` or `Crashed` (non-zero exit, signal or too
  much output). `Err` means something outside the program failed, such as a compile error.
- `spawn_checked` compares the output with an answer and gives a `Verdict`: `Accepted`,
  `WrongAnswer` or one of the failures. `Check::Tokens` compares whitespace-separated tokens, and
  `Check::Program` runs a C++ checker as `checker input output answer`, which exits with 0 to accept
  and 1 or 2 to reject, as testlib checkers do. `spawn_check` checks an output alone.
- Verdicts, time outs, memory limits, crashes and outputs of at most 64 bytes are cached in the
  build folder; of two results of the same run, the better is kept. `spawn_fresh` and
  `spawn_checked_fresh` run again regardless, and `set_result_cache(false)` turns caching off.
- Programs get a large stack for deep recursion: 512 MiB on Windows and macOS, and the system's
  maximum on Linux. On macOS memory is not capped while running, so a program over its
  limit is only reported as `MemoryLimitExceeded` once it finishes.
