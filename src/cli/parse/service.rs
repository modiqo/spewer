//! Parsing for local service and submission commands.

use super::{CliCommand, HelpTopic, next, unexpected, value};
use crate::error::{Error, ErrorKind, Result};
use lexopt::prelude::*;
use std::path::PathBuf;

pub(super) fn parse_serve(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    let mut engine = None;
    let mut max_workers = 1_usize;
    let mut socket = None;
    let mut explicit_detach = false;
    let mut foreground = false;
    while let Some(argument) = next(parser)? {
        match argument {
            Long("help") | Short('h') => return Ok(CliCommand::Help(Some(HelpTopic::Serve))),
            Long("engine") => engine = Some(value(parser)?),
            Long("max-workers") => max_workers = parse_workers(&value(parser)?)?,
            Long("socket") => socket = Some(PathBuf::from(value(parser)?)),
            Long("detach") => explicit_detach = true,
            Long("foreground") => foreground = true,
            Long("json") => {}
            other => return unexpected("serve", &other),
        }
    }
    if explicit_detach && foreground {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "serve accepts either --detach or --foreground",
        ));
    }
    match engine {
        Some(value) if value == "codex" => Ok(CliCommand::Serve {
            max_workers,
            socket,
            detach: !foreground,
        }),
        Some(value) => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("unsupported engine {}", value.to_string_lossy()),
        )),
        None => Err(Error::new(
            ErrorKind::InvalidInput,
            "serve requires --engine codex",
        )),
    }
}

pub(super) fn parse_submit(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    let mut path = None;
    let mut socket = None;
    while let Some(argument) = next(parser)? {
        match argument {
            Long("help") | Short('h') => return Ok(CliCommand::Help(Some(HelpTopic::Submit))),
            Value(value) if path.is_none() => path = Some(PathBuf::from(value)),
            Long("socket") => socket = Some(PathBuf::from(value(parser)?)),
            other => return unexpected("submit", &other),
        }
    }
    let path = path
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "submit requires a task JSON path"))?;
    Ok(CliCommand::Submit { path, socket })
}

pub(super) fn parse_socket_command(
    parser: &mut lexopt::Parser,
    command: &str,
    topic: HelpTopic,
    make: fn(Option<PathBuf>) -> CliCommand,
) -> Result<CliCommand> {
    let mut socket = None;
    while let Some(argument) = next(parser)? {
        match argument {
            Long("help") | Short('h') => return Ok(CliCommand::Help(Some(topic))),
            Long("socket") => socket = Some(PathBuf::from(value(parser)?)),
            other => return unexpected(command, &other),
        }
    }
    Ok(make(socket))
}

fn parse_workers(value: &std::ffi::OsStr) -> Result<usize> {
    value
        .to_string_lossy()
        .parse::<usize>()
        .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))
}
