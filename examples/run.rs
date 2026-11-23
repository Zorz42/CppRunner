use cpprunner::{Limits, RunResult, Runner};
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: run <file.cpp> [--time-ms N] [--memory-mb N] [--build-folder DIR] < input";

struct Args {
    source: PathBuf,
    limits: Limits,
    build_folder: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let (mut source, mut build_folder) = (None, None);
    let mut limits = Limits::default();
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        match arg.as_str() {
            "--time-ms" => limits = limits.with_time_ms(value("--time-ms")?.parse().map_err(|err| format!("--time-ms: {err}"))?),
            "--memory-mb" => limits = limits.with_memory_mb(value("--memory-mb")?.parse().map_err(|err| format!("--memory-mb: {err}"))?),
            "--build-folder" => build_folder = Some(PathBuf::from(value("--build-folder")?)),
            _ if source.is_none() => source = Some(PathBuf::from(arg)),
            _ => return Err(format!("unexpected argument {arg}")),
        }
    }
    Ok(Args {
        source: source.ok_or_else(|| USAGE.to_owned())?,
        limits,
        build_folder,
    })
}

fn run(args: &Args) -> Result<RunResult, String> {
    let source = std::fs::read_to_string(&args.source).map_err(|err| format!("could not read {}: {err}", args.source.display()))?;
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).map_err(|err| format!("could not read stdin: {err}"))?;
    let temporary = tempfile::tempdir().map_err(|err| format!("could not create a build folder: {err}"))?;
    let runner = Runner::new(args.build_folder.as_deref().unwrap_or_else(|| temporary.path())).map_err(|err| err.to_string())?;
    runner.run(&source, &input, args.limits).map_err(|err| err.to_string())
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };

    match run(&args) {
        Ok(RunResult::Ok { time_ms, memory_kb, output }) => {
            print!("{output}");
            eprintln!("OK: {time_ms} ms, {memory_kb} kB");
            ExitCode::SUCCESS
        }
        Ok(other) => {
            eprintln!("{other:?}");
            ExitCode::FAILURE
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
