//! Minimal command-line interface.

use crate::codex::{CodexConfig, doctor};
use crate::error::{Error, ErrorKind, Result};
use lexopt::prelude::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CliCommand {
    DoctorCodex,
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
        "spewer {}\n\nUSAGE:\n  spewer doctor --engine codex\n  spewer --version",
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
}
