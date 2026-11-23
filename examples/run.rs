use cpprunner::{Limits, RunResult};
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: run <file.cpp> [--time-ms N] [--memory-mb N] < input";

fn parse_args() -> Result<(PathBuf, Limits), String> {
    let mut args = std::env::args().skip(1);
    let mut source = None;
    let mut limits = Limits::default();
    while let Some(arg) = args.next() {
        let mut number = |name: &str| args.next().and_then(|value| value.parse::<u64>().ok()).ok_or_else(|| format!("{name} needs a number"));
        match arg.as_str() {
            "--time-ms" => limits = limits.with_time_ms(number("--time-ms")?),
            "--memory-mb" => limits = limits.with_memory_mb(number("--memory-mb")?),
            _ if source.is_none() => source = Some(PathBuf::from(arg)),
            _ => return Err(format!("unexpected argument {arg}")),
        }
    }
    Ok((source.ok_or_else(|| USAGE.to_owned())?, limits))
}

fn main() -> ExitCode {
    let (source, limits) = match parse_args() {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };

    let mut input = String::new();
    if let Err(err) = std::io::stdin().read_to_string(&mut input) {
        eprintln!("could not read stdin: {err}");
        return ExitCode::FAILURE;
    }

    match cpprunner::run_file(&source, &input, limits) {
        Ok(RunResult::Ok { time_ms, memory_kb, output }) => {
            print!("{output}");
            eprintln!("OK: {time_ms} ms, {memory_kb} kB");
            ExitCode::SUCCESS
        }
        Ok(RunResult::Crashed(reason)) => {
            eprintln!("RTE: {reason:?}");
            ExitCode::FAILURE
        }
        Ok(other) => {
            eprintln!("{other:?}");
            ExitCode::FAILURE
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}
