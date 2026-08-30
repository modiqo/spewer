//! Parsing for installation and capsule administration.

use super::{CliCommand, HelpTopic, next, unexpected, value};
use crate::error::{Error, ErrorKind, Result};
use lexopt::prelude::*;
use std::path::PathBuf;

pub(super) fn parse_install(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    let mut workspace = None;
    let mut max_workers = 1;
    let mut skip_codex_install = false;
    while let Some(argument) = next(parser)? {
        match argument {
            Long("help") | Short('h') => return Ok(CliCommand::Help(Some(HelpTopic::Install))),
            Long("workspace") => workspace = Some(PathBuf::from(value(parser)?)),
            Long("max-workers") => max_workers = parse_workers(&value(parser)?)?,
            Long("skip-codex-install") => skip_codex_install = true,
            other => return unexpected("install", &other),
        }
    }
    Ok(CliCommand::Install {
        workspace,
        max_workers,
        skip_codex_install,
    })
}

pub(super) fn parse_capsule(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    let Some(operation) = next(parser)? else {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "capsule requires add, list, show, default, bind, or unbind",
        ));
    };
    if matches!(operation, Long("help") | Short('h')) {
        return Ok(CliCommand::Help(Some(HelpTopic::Capsule)));
    }
    let Value(operation) = operation else {
        return unexpected("capsule", &operation);
    };
    match operation.to_str() {
        Some("add") => parse_capsule_add(parser),
        Some("list") => parse_capsule_list(parser),
        Some("show") => parse_capsule_show(parser),
        Some("default") => parse_capsule_default(parser),
        Some("bind") => parse_capsule_bind(parser),
        Some("unbind") => parse_capsule_unbind(parser),
        Some(other) => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("unknown capsule operation {other}"),
        )),
        None => Err(Error::new(
            ErrorKind::InvalidInput,
            "capsule operation is not valid UTF-8",
        )),
    }
}

fn parse_capsule_show(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    let capsule_id = match next(parser)? {
        Some(Value(value)) => Some(
            value
                .into_string()
                .map_err(|_| Error::new(ErrorKind::InvalidInput, "capsule id is not UTF-8"))?,
        ),
        Some(Long("help") | Short('h')) => {
            return Ok(CliCommand::Help(Some(HelpTopic::Capsule)));
        }
        Some(other) => return unexpected("capsule show", &other),
        None => None,
    };
    if let Some(argument) = next(parser)? {
        return unexpected("capsule show", &argument);
    }
    Ok(CliCommand::CapsuleShow(capsule_id))
}

fn parse_capsule_default(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    let capsule_id = positional(parser, "capsule default requires a capsule id")?;
    if let Some(argument) = next(parser)? {
        return unexpected("capsule default", &argument);
    }
    Ok(CliCommand::CapsuleDefault(capsule_id))
}

fn parse_capsule_add(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    let capsule_id = positional(parser, "capsule add requires a capsule id")?;
    let mut engine = None;
    let mut model = None;
    while let Some(argument) = next(parser)? {
        match argument {
            Long("engine") => engine = Some(value(parser)?.to_string_lossy().into_owned()),
            Long("model") => model = Some(value(parser)?.to_string_lossy().into_owned()),
            Long("help") | Short('h') => {
                return Ok(CliCommand::Help(Some(HelpTopic::Capsule)));
            }
            other => return unexpected("capsule add", &other),
        }
    }
    let engine = engine.ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            "capsule add requires --engine codex-app-server or --engine ollama",
        )
    })?;
    let model =
        model.ok_or_else(|| Error::new(ErrorKind::InvalidInput, "capsule add requires --model"))?;
    Ok(CliCommand::CapsuleAdd {
        capsule_id,
        engine,
        model,
    })
}

fn parse_capsule_list(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    match next(parser)? {
        None => Ok(CliCommand::CapsuleList),
        Some(Long("help") | Short('h')) => Ok(CliCommand::Help(Some(HelpTopic::Capsule))),
        Some(other) => unexpected("capsule list", &other),
    }
}

fn parse_capsule_bind(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    let capsule_id = positional(parser, "capsule bind requires a capsule id")?;
    let skill = PathBuf::from(positional(parser, "capsule bind requires a skill path")?);
    if let Some(argument) = next(parser)? {
        return unexpected("capsule bind", &argument);
    }
    Ok(CliCommand::CapsuleBind { capsule_id, skill })
}

fn parse_capsule_unbind(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    let capsule_id = positional(parser, "capsule unbind requires a capsule id")?;
    if let Some(argument) = next(parser)? {
        return unexpected("capsule unbind", &argument);
    }
    Ok(CliCommand::CapsuleUnbind(capsule_id))
}

fn positional(parser: &mut lexopt::Parser, message: &str) -> Result<String> {
    match next(parser)? {
        Some(Value(value)) => value
            .into_string()
            .map_err(|_| Error::new(ErrorKind::InvalidInput, message)),
        Some(Long("help") | Short('h')) => Err(Error::new(
            ErrorKind::InvalidInput,
            "use 'spewer help capsule' for capsule forms",
        )),
        Some(argument) => unexpected("capsule", &argument),
        None => Err(Error::new(ErrorKind::InvalidInput, message)),
    }
}

fn parse_workers(value: &std::ffi::OsStr) -> Result<usize> {
    let workers = value
        .to_string_lossy()
        .parse::<usize>()
        .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?;
    if !(1..=64).contains(&workers) {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "max-workers must be between 1 and 64",
        ));
    }
    Ok(workers)
}
