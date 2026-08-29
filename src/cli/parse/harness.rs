//! Parsing for the small frontier harness surface.

use super::{CliCommand, HelpTopic, next, unexpected, value};
use crate::error::{Error, ErrorKind, Result};
use lexopt::prelude::*;
use std::path::PathBuf;

pub(super) fn parse_delegate(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    let mut path = None;
    let mut capsule_id = "default".to_owned();
    let mut socket = None;
    while let Some(argument) = next(parser)? {
        match argument {
            Long("help") | Short('h') => return Ok(CliCommand::Help(Some(HelpTopic::Delegate))),
            Long("capsule") => capsule_id = value(parser)?.to_string_lossy().into_owned(),
            Long("socket") => socket = Some(PathBuf::from(value(parser)?)),
            Value(value) if path.is_none() => path = Some(PathBuf::from(value)),
            other => return unexpected("delegate", &other),
        }
    }
    let path = path.ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            "delegate requires a task JSON path",
        )
    })?;
    Ok(CliCommand::Delegate {
        path,
        capsule_id,
        socket,
    })
}

pub(super) fn parse_check(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    let mut task_id = None;
    let mut after = 0;
    let mut socket = None;
    while let Some(argument) = next(parser)? {
        match argument {
            Long("help") | Short('h') => return Ok(CliCommand::Help(Some(HelpTopic::Check))),
            Long("after") => {
                after = value(parser)?
                    .to_string_lossy()
                    .parse::<u64>()
                    .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?;
            }
            Long("socket") => socket = Some(PathBuf::from(value(parser)?)),
            Value(value) if task_id.is_none() => {
                task_id = Some(value.to_string_lossy().into_owned());
            }
            other => return unexpected("check", &other),
        }
    }
    let task_id =
        task_id.ok_or_else(|| Error::new(ErrorKind::InvalidInput, "check requires a task id"))?;
    Ok(CliCommand::Check {
        task_id,
        after,
        socket,
    })
}
