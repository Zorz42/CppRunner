# When testing
- always run tests and clippy to make sure everything works
- run tests with `cargo nextest run`, and doctests with `cargo test --doc` (nextest skips them)
- run clippy with `cargo clippy --all-targets --all-features`, and also with
  `--target x86_64-unknown-linux-gnu` and `--target x86_64-pc-windows-gnu` so the code of the
  other platforms is at least checked
