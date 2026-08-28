//! Minimal command-line interface.

use crate::codex::{CodexConfig, doctor};
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::TaskRequest;
use crate::runner::run_codex_durable;
use crate::store::Database;
use lexopt::prelude::*;
use serde_json::json;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
enum CliCommand {
    DoctorCodex,
    RunCodex(PathBuf),
    Status(String),
    Tail { task_id: String, after: u64 },
    Rebuild(String),
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
        CliCommand::Status(task_id) => show_status(task_id).await?,
        CliCommand::Tail { task_id, after } => tail(task_id, after).await?,
        CliCommand::Rebuild(task_id) => rebuild(task_id).await?,
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
        Value(value) if value == "status" => {
            parse_task_id(&mut parser, "status").map(CliCommand::Status)
        }
        Value(value) if value == "tail" => parse_tail(&mut parser),
        Value(value) if value == "rebuild" => {
            parse_task_id(&mut parser, "rebuild").map(CliCommand::Rebuild)
        }
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
    let database = Database::open(Database::default_path()?).await?;
    let outcome = run_codex_durable(request, CodexConfig::default(), &database).await;
    let close = database.close().await;
    let result = outcome?;
    close?;
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

async fn show_status(task_id: String) -> Result<()> {
    let database = Database::open(Database::default_path()?).await?;
    let projection = database.get(task_id).await?;
    database.close().await?;
    println!("{}", serde_json::to_string_pretty(&projection)?);
    Ok(())
}

async fn tail(task_id: String, after: u64) -> Result<()> {
    let database = Database::open(Database::default_path()?).await?;
    let events = database.events_after(task_id, after).await?;
    database.close().await?;
    for event in events {
        println!("{}", serde_json::to_string(&event)?);
    }
    Ok(())
}

async fn rebuild(task_id: String) -> Result<()> {
    let database = Database::open(Database::default_path()?).await?;
    let projection = database.rebuild(task_id).await?;
    database.close().await?;
    println!("{}", serde_json::to_string_pretty(&projection)?);
    Ok(())
}

fn parse_task_id(parser: &mut lexopt::Parser, command: &str) -> Result<String> {
    let value = parser
        .value()
        .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?;
    if parser
        .next()
        .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?
        .is_some()
    {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("{command} accepts one task id"),
        ));
    }
    Ok(value.to_string_lossy().into_owned())
}

fn parse_tail(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    let task_id = parser
        .value()
        .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?
        .to_string_lossy()
        .into_owned();
    let mut after = 0;
    while let Some(argument) = parser
        .next()
        .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?
    {
        match argument {
            Long("after") => {
                let value = parser
                    .value()
                    .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?;
                after = value
                    .to_string_lossy()
                    .parse::<u64>()
                    .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?;
            }
            other => {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    format!("unexpected tail argument {other:?}"),
                ));
            }
        }
    }
    Ok(CliCommand::Tail { task_id, after })
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
        "spewer {}\n\nUSAGE:\n  spewer doctor --engine codex\n  spewer run <task.json> --engine codex\n  spewer status <task-id>\n  spewer tail <task-id> [--after <seq>]\n  spewer rebuild <task-id>\n  spewer --version",
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

    #[test]
    fn parses_durable_queries() -> Result<(), Box<dyn std::error::Error>> {
        let status = ["status", "task-one"].into_iter().map(OsString::from);
        assert_eq!(parse(status)?, CliCommand::Status("task-one".to_owned()));
        let tail = ["tail", "task-one", "--after", "12"]
            .into_iter()
            .map(OsString::from);
        assert_eq!(
            parse(tail)?,
            CliCommand::Tail {
                task_id: "task-one".to_owned(),
                after: 12
            }
        );
        Ok(())
    }
}
