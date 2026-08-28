//! Minimal command-line interface.

use crate::codex::{CodexConfig, doctor};
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::TaskRequest;
use crate::runner::run_codex;
use lexopt::prelude::*;
use serde_json::json;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
enum CliCommand {
    DoctorCodex,
    RunCodex(PathBuf),
    Help,
    Version,
}

/// Parses process arguments, runs one command, and writes JSON to stdout.
pub async fn run() -> Result<()> {
    match parse(std::env::args_os().skip(1))? {
        CliCommand::DoctorCodex => {
            let report = doctor(CodexConfig::default()).await?;
            let json = serde_json::to_string_pretty(&report)?;
            println!("{json}");
        }
        CliCommand::RunCodex(path) => run_task(path).await?,
        CliCommand::Help => print_help(),
        CliCommand::Version => println!("spewer {}", env!("CARGO_PKG_VERSION")),
    }
    Ok(())
}

fn parse(arguments: impl IntoIterator<Item = std::ffi::OsString>) -> Result<CliCommand> {
    let mut parser = lexopt::Parser::from_args(arguments);
    let Some(argument) = parser
        .next()
        .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?
    else {
        return Ok(CliCommand::Help);
    };
    match argument {
        Value(value) if value == "doctor" => parse_doctor(&mut parser),
        Value(value) if value == "run" => parse_run(&mut parser),
        Long("help") | Short('h') => Ok(CliCommand::Help),
        Long("version") | Short('V') => Ok(CliCommand::Version),
        Value(value) => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("unknown command {}", value.to_string_lossy()),
        )),
        other => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("unexpected argument {other:?}"),
        )),
    }
}

fn parse_run(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    let mut task_path = None;
    let mut engine = None;
    while let Some(argument) = parser
        .next()
        .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?
    {
        match argument {
            Value(value) if task_path.is_none() => task_path = Some(PathBuf::from(value)),
            Long("engine") => {
                engine = Some(
                    parser
                        .value()
                        .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?,
                );
            }
            other => {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    format!("unexpected run argument {other:?}"),
                ));
            }
        }
    }
    let path = task_path
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "run requires a task JSON path"))?;
    match engine {
        Some(value) if value == "codex" => Ok(CliCommand::RunCodex(path)),
        Some(value) => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("unsupported engine {}", value.to_string_lossy()),
        )),
        None => Err(Error::new(
            ErrorKind::InvalidInput,
            "run requires --engine codex",
        )),
    }
}

async fn run_task(path: PathBuf) -> Result<()> {
    let task_json = tokio::task::spawn_blocking(move || std::fs::read_to_string(path)).await??;
    let request: TaskRequest = serde_json::from_str(&task_json)?;
    let result = run_codex(request, CodexConfig::default()).await?;
    println!(
        "{}",
        serde_json::to_string(&json!({"handle": result.handle}))?
    );
    for event in result.events {
        println!("{}", serde_json::to_string(&json!({"event": event}))?);
    }
    println!(
        "{}",
        serde_json::to_string(&json!({"receipt": result.receipt}))?
    );
    Ok(())
}

fn parse_doctor(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    let mut engine = None;
    while let Some(argument) = parser
        .next()
        .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?
    {
        match argument {
            Long("engine") => {
                let value = parser
                    .value()
                    .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?;
                engine = Some(value);
            }
            Long("help") | Short('h') => return Ok(CliCommand::Help),
            other => {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    format!("unexpected doctor argument {other:?}"),
                ));
            }
        }
    }
    match engine {
        Some(value) if value == "codex" => Ok(CliCommand::DoctorCodex),
        Some(value) => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("unsupported engine {}", value.to_string_lossy()),
        )),
        None => Err(Error::new(
            ErrorKind::InvalidInput,
            "doctor requires --engine codex",
        )),
    }
}

fn print_help() {
    println!(
        "spewer {}\n\nUSAGE:\n  spewer doctor --engine codex\n  spewer run <task.json> --engine codex\n  spewer --version",
        env!("CARGO_PKG_VERSION")
    );
}

#[cfg(test)]
mod tests {
    use super::{CliCommand, parse};
    use std::ffi::OsString;

    #[test]
    fn parses_doctor() -> Result<(), Box<dyn std::error::Error>> {
        let arguments = ["doctor", "--engine", "codex"]
            .into_iter()
            .map(OsString::from);
        assert_eq!(parse(arguments)?, CliCommand::DoctorCodex);
        Ok(())
    }

    #[test]
    fn parses_run() -> Result<(), Box<dyn std::error::Error>> {
        let arguments = ["run", "task.json", "--engine", "codex"]
            .into_iter()
            .map(OsString::from);
        assert_eq!(
            parse(arguments)?,
            CliCommand::RunCodex(std::path::PathBuf::from("task.json"))
        );
        Ok(())
    }
}
