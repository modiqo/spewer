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
        Some(value) if value == "codex" || value == "all" => Ok(CliCommand::Serve {
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
            "serve requires --engine all (or legacy --engine codex)",
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

pub(super) fn parse_task_socket(
    parser: &mut lexopt::Parser,
    command: &str,
    topic: HelpTopic,
    make: fn(String, Option<PathBuf>) -> CliCommand,
) -> Result<CliCommand> {
    let mut task_id = None;
    let mut socket = None;
    while let Some(argument) = next(parser)? {
        match argument {
            Long("help") | Short('h') => return Ok(CliCommand::Help(Some(topic))),
            Value(value) if task_id.is_none() => {
                task_id = Some(value.to_string_lossy().into_owned());
            }
            Long("socket") => socket = Some(PathBuf::from(value(parser)?)),
            other => return unexpected(command, &other),
        }
    }
    let task_id = task_id.ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            format!("{command} requires a task id"),
        )
    })?;
    Ok(make(task_id, socket))
}

pub(super) fn parse_observe(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    let mut task_id = None;
    let mut after = 0_u64;
    let mut socket = None;
    while let Some(argument) = next(parser)? {
        match argument {
            Long("help") | Short('h') => return Ok(CliCommand::Help(Some(HelpTopic::Observe))),
            Value(value) if task_id.is_none() => {
                task_id = Some(value.to_string_lossy().into_owned());
            }
            Long("after") => {
                after = value(parser)?
                    .to_string_lossy()
                    .parse::<u64>()
                    .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?;
            }
            Long("socket") => socket = Some(PathBuf::from(value(parser)?)),
            other => return unexpected("observe", &other),
        }
    }
    let task_id =
        task_id.ok_or_else(|| Error::new(ErrorKind::InvalidInput, "observe requires a task id"))?;
    Ok(CliCommand::Observe {
        task_id,
        after,
        socket,
    })
}

pub(super) fn parse_cancel(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    let mut task_id = None;
    let mut reason = "cancelled by parent harness".to_owned();
    let mut socket = None;
    while let Some(argument) = next(parser)? {
        match argument {
            Long("help") | Short('h') => return Ok(CliCommand::Help(Some(HelpTopic::Cancel))),
            Value(value) if task_id.is_none() => {
                task_id = Some(value.to_string_lossy().into_owned());
            }
            Long("reason") => reason = value(parser)?.to_string_lossy().into_owned(),
            Long("socket") => socket = Some(PathBuf::from(value(parser)?)),
            other => return unexpected("cancel", &other),
        }
    }
    let task_id =
        task_id.ok_or_else(|| Error::new(ErrorKind::InvalidInput, "cancel requires a task id"))?;
    Ok(CliCommand::Cancel {
        task_id,
        reason,
        socket,
    })
}

pub(super) fn parse_respond(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    let mut task_id = None;
    let mut request_id = None;
    let mut response = None;
    let mut socket = None;
    while let Some(argument) = next(parser)? {
        match argument {
            Long("help") | Short('h') => return Ok(CliCommand::Help(Some(HelpTopic::Respond))),
            Value(value) if task_id.is_none() => {
                task_id = Some(value.to_string_lossy().into_owned());
            }
            Value(value) if request_id.is_none() => {
                let text = value.to_string_lossy();
                request_id = Some(match serde_json::from_str(&text) {
                    Ok(value) => value,
                    Err(_) => serde_json::Value::String(text.into_owned()),
                });
            }
            Long("response") => {
                response = Some(serde_json::from_str(&value(parser)?.to_string_lossy())?);
            }
            Long("socket") => socket = Some(PathBuf::from(value(parser)?)),
            other => return unexpected("respond", &other),
        }
    }
    Ok(CliCommand::Respond {
        task_id: task_id
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "respond requires a task id"))?,
        request_id: request_id
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "respond requires a request id"))?,
        response: response.ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                "respond requires --response <json>",
            )
        })?,
        socket,
    })
}

fn parse_workers(value: &std::ffi::OsStr) -> Result<usize> {
    value
        .to_string_lossy()
        .parse::<usize>()
        .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))
}
