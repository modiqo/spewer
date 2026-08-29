//! Parsing for inferred one-off question commands.

use super::{CliCommand, HelpTopic, next, unexpected, value};
use crate::error::{Error, ErrorKind, Result};
use lexopt::prelude::*;
use std::path::PathBuf;

pub(super) fn parse_init(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    let mut workspace = None;
    let mut overwrite = false;
    while let Some(argument) = next(parser)? {
        match argument {
            Long("help") | Short('h') => return Ok(CliCommand::Help(Some(HelpTopic::Init))),
            Long("workspace") => workspace = Some(PathBuf::from(value(parser)?)),
            Long("overwrite") => overwrite = true,
            other => return unexpected("init", &other),
        }
    }
    Ok(CliCommand::Init {
        workspace,
        overwrite,
    })
}

pub(super) fn parse_ask(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    let mut question = None;
    let mut workspace = None;
    let mut capsule_id = None;
    let mut text = false;
    let mut json = false;
    let mut detach = false;
    let mut socket = None;
    while let Some(argument) = next(parser)? {
        match argument {
            Long("help") | Short('h') => return Ok(CliCommand::Help(Some(HelpTopic::Ask))),
            Long("workspace") => workspace = Some(PathBuf::from(value(parser)?)),
            Long("capsule") => {
                capsule_id = Some(value(parser)?.to_string_lossy().into_owned());
            }
            Long("json") => json = true,
            Long("text") => text = true,
            Long("detach") => detach = true,
            Long("socket") => socket = Some(PathBuf::from(value(parser)?)),
            Value(value) if question.is_none() => {
                question = Some(value.into_string().map_err(|_| {
                    Error::new(ErrorKind::InvalidInput, "question is not valid UTF-8")
                })?);
            }
            other => return unexpected("ask", &other),
        }
    }
    let question = question
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "ask requires a quoted question"))?;
    if json && text {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "ask accepts either --json or --text",
        ));
    }
    if detach && text {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "detached ask always returns a JSON task handle",
        ));
    }
    if socket.is_some() && !detach {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "ask --socket requires --detach",
        ));
    }
    Ok(CliCommand::Ask {
        question,
        workspace,
        capsule_id,
        text,
        detach,
        socket,
    })
}
