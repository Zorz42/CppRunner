# cpprunner

Compile and run C++ programs from Rust.

- Finds `g++`, `c++` or `clang++` (or MinGW on Windows), or the compiler named by `GCC_PATH`.
- Caches compiled programs in a build folder keyed by source, compiler and flags; several threads
  and processes can share one folder safely.
- Runs programs with CPU time, peak memory and output limits, measuring the time and memory they
  used, and stops ones that block or print forever.
- Runs many jobs in parallel, returning results in order.
- Gives programs a large stack, since competitive programming solutions often recurse deeply.

Supported on Linux, macOS and Windows.

## Running a file

```rust,no_run
use cpprunner::{Limits, RunResult};

let result = cpprunner::run_file("solution.cpp".as_ref(), "1 2\n", Limits::default())?;
match result {
    RunResult::Ok { output, time_ms, memory_kb } => println!("{output} ({time_ms} ms, {memory_kb} kB)"),
    other => println!("{}", other.verdict()),
}
# Ok::<(), cpprunner::Error>(())
```

## Running source code

```rust
use cpprunner::{Limits, RunResult};

let result = cpprunner::run_source("int main() { while (true) {} }", "", Limits::new().with_time_ms(100))?;
assert_eq!(result, RunResult::TimedOut);
# Ok::<(), cpprunner::Error>(())
```

## Many programs and inputs, with a cached build folder

```rust
use cpprunner::{Compiler, CppRunner, Limits};

let build_folder = tempfile::tempdir()?;
let compiler = Compiler::detect()?.with_standard("c++20");
let mut runner = CppRunner::with_compiler(build_folder.path(), compiler)?;

let echo = runner.add_program("#include <iostream>\nint main() { int n; std::cin >> n; std::cout << n; }")?;
let square = runner.add_program("#include <iostream>\nint main() { int n; std::cin >> n; std::cout << n * n; }")?;

let results = runner.check_programs("7", &[echo, square], Limits::new().with_memory_mb(256))?;
assert_eq!(results[0].output(), Some("7"));
assert_eq!(results[1].output(), Some("49"));

runner.clean_build_folder()?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Verdicts

| `RunResult`             | Meaning                                                              |
|-------------------------|----------------------------------------------------------------------|
| `Ok`                    | Exited with code zero within the limits                              |
| `TimedOut`              | Used more CPU time than allowed, or blocked for over twice the limit plus two seconds |
| `MemoryLimitExceeded`   | Peak memory over the limit                                           |
| `Crashed(reason)`       | Non-zero exit code, killed by a signal, or too much output           |

Errors (`cpprunner::Error`) are reserved for problems outside the program: no compiler, a
compile error, I/O failures, or a run that could not be measured.

Notes:

- macOS cannot cap a process's address space, so a program far over its memory limit runs to
  completion before it is reported as `MemoryLimitExceeded`.
- Stderr of programs is discarded.

## Command line

```sh
cargo run --example run -- solution.cpp --time-ms 2000 --memory-mb 256 < input.txt
```

## Testing

```sh
cargo nextest run && cargo test --doc
```

## License

GPL-3.0-only
