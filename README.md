# cpprunner

Compile and run C++ programs from Rust.

- One object, [`Runner`], does everything. Programs are named by their source: they are compiled the
  first time they run, with `g++`, `c++` or `clang++` (or MinGW on Windows), or the compiler named
  by `GCC_PATH`, as `-std=c++17 -O2`.
- Compiled programs are kept in a build folder that several threads and processes can share.
- Runs have CPU time, peak memory and output limits; the time and memory used are measured, and
  programs that block or print forever are stopped.
- Every run is queued: [`Runner::spawn`] returns a [`RunHandle`] to wait for or cancel. Runs go in
  parallel, starting in the order they were queued; a queued run can be cancelled before it starts.
- Results that are small to store are cached in the build folder: every time out, memory limit and
  crash, and successful runs whose output is at most [`MAX_CACHED_OUTPUT_BYTES`] long, such as a
  single number or `YES`/`NO`.
- Programs get a large stack, since competitive programming solutions often recurse deeply.

Supported on Linux, macOS and Windows.

## Running a program

```rust
use cpprunner::{Limits, RunResult, Runner};

let build_folder = tempfile::tempdir()?;
let runner = Runner::new(build_folder.path())?;
let sum = "#include <iostream>\nint main() { int a, b; std::cin >> a >> b; std::cout << a + b; }";

match runner.spawn(sum, "1 2", Limits::default())?.wait()? {
    RunResult::Ok { output, time_ms, memory_kb } => println!("{output} ({time_ms} ms, {memory_kb} kB)"),
    other => println!("{other:?}"),
}
let hang = runner.spawn("int main() { while (true) {} }", "", Limits::new().with_time_ms(100))?;
assert_eq!(hang.wait()?, RunResult::TimedOut);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Queueing runs

```rust
use cpprunner::{Limits, RunResult, Runner};

let build_folder = tempfile::tempdir()?;
let runner = Runner::new(build_folder.path())?;
let square = "#include <iostream>\nint main() { long long n; std::cin >> n; std::cout << n * n; }";

let limits = Limits::new().with_time_ms(2000).with_memory_mb(256);
let handles: Vec<_> = (1..=10).map(|n| runner.spawn(square, n.to_string(), limits)).collect::<cpprunner::Result<_>>()?;
for (n, handle) in (1..=10).zip(handles) {
    assert!(matches!(handle.wait()?, RunResult::Ok { output, .. } if output == (n * n).to_string()));
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

`runner.spawn_fresh` ignores cached results, to measure a program again, and
`runner.set_result_cache(false)` turns the cache off.

## Verdicts

| `RunResult`           | Meaning                                                                                |
|-----------------------|----------------------------------------------------------------------------------------|
| `Ok`                  | Exited with code zero within the limits                                                |
| `TimedOut`            | Used more CPU time than allowed, or blocked for over twice the limit plus two seconds  |
| `MemoryLimitExceeded` | Peak memory over the limit                                                             |
| `Crashed(reason)`     | Non-zero exit code, killed by a signal, or too much output                             |

Errors (`cpprunner::Error`) are reserved for problems outside the program: no compiler, a compile
error, I/O failures, a run that could not be measured, or a cancelled run.

Notes:

- macOS cannot cap a process's address space, so a program far over its memory limit runs to
  completion before it is reported as `MemoryLimitExceeded`.
- Stderr of programs is discarded.

## Testing

```sh
cargo nextest run && cargo test --doc
```

## License

GPL-3.0-only
