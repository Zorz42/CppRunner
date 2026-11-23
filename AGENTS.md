# When testing
- always run tests and clippy to make sure everything works
- run tests with `cargo nextest run`, and doctests with `cargo test --doc` (nextest skips them)
- run clippy with `cargo clippy --all-targets --all-features`, and also with
  `--target x86_64-unknown-linux-gnu` and `--target x86_64-pc-windows-gnu` so the code of the
  other platforms is at least checked

# Where tests go
- every test goes in `src/tests`, one module per file, declared in `src/tests/mod.rs`; never add
  tests in `tests/`, `benches/`, `examples/` or in `#[cfg(test)]` modules elsewhere in `src`
- `src/tests/layout_tests.rs` fails if a test is put anywhere else
