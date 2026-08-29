//! Strict command-line argument parsing.

mod question;
mod service;
#[cfg(test)]
mod tests;

use crate::error::{Error, ErrorKind, Result};
use lexopt::prelude::*;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum CliCommand {
    Init {
        workspace: Option<PathBuf>,
        overwrite: bool,
    },
    Ask {
        question: String,
        workspace: Option<PathBuf>,
        text: bool,
        detach: bool,
        socket: Option<PathBuf>,
    },
    DoctorCodex,
    RunCodex(PathBuf),
    Serve {
        max_workers: usize,
        socket: Option<PathBuf>,
        detach: bool,
    },
    Submit {
        path: PathBuf,
        socket: Option<PathBuf>,
    },
    Load {
        socket: Option<PathBuf>,
    },
    Stop {
        socket: Option<PathBuf>,
    },
    Capabilities {
        socket: Option<PathBuf>,
    },
    Observe {
        task_id: String,
        after: u64,
        socket: Option<PathBuf>,
    },
    Result {
        task_id: String,
        socket: Option<PathBuf>,
    },
    Cancel {
        task_id: String,
        reason: String,
        socket: Option<PathBuf>,
    },
    Status(String),
    Tail {
        task_id: String,
        after: u64,
    },
    Rebuild(String),
    Resume(String),
    Recover,
    Outbox(String),
    Acknowledge {
        message_id: String,
        consumer_id: String,
        socket: Option<PathBuf>,
    },
    Help(Option<HelpTopic>),
    Version,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum HelpTopic {
    Init,
    Ask,
    Doctor,
    Run,
    Serve,
    Submit,
    Load,
    Stop,
    Capabilities,
    Observe,
    Result,
    Cancel,
    Status,
    Tail,
    Rebuild,
    Recover,
    Resume,
    Outbox,
    Ack,
}

impl HelpTopic {
    #[cfg(test)]
    pub(super) const ALL: [Self; 19] = [
        Self::Init,
        Self::Ask,
        Self::Doctor,
        Self::Run,
        Self::Serve,
        Self::Submit,
        Self::Load,
        Self::Stop,
        Self::Capabilities,
        Self::Observe,
        Self::Result,
        Self::Cancel,
        Self::Status,
        Self::Tail,
        Self::Rebuild,
        Self::Recover,
        Self::Resume,
        Self::Outbox,
        Self::Ack,
    ];

    fn parse(value: &std::ffi::OsStr) -> Result<Self> {
        match value.to_str() {
            Some("init") => Ok(Self::Init),
            Some("ask") => Ok(Self::Ask),
            Some("doctor") => Ok(Self::Doctor),
            Some("run") => Ok(Self::Run),
            Some("serve") => Ok(Self::Serve),
            Some("submit") => Ok(Self::Submit),
            Some("load") => Ok(Self::Load),
            Some("stop") => Ok(Self::Stop),
            Some("capabilities") => Ok(Self::Capabilities),
            Some("observe") => Ok(Self::Observe),
            Some("result") => Ok(Self::Result),
            Some("cancel") => Ok(Self::Cancel),
            Some("status") => Ok(Self::Status),
            Some("tail") => Ok(Self::Tail),
            Some("rebuild") => Ok(Self::Rebuild),
            Some("recover") => Ok(Self::Recover),
            Some("resume") => Ok(Self::Resume),
            Some("outbox") => Ok(Self::Outbox),
            Some("ack") => Ok(Self::Ack),
            Some(other) => Err(Error::new(
                ErrorKind::InvalidInput,
                format!("unknown help topic {other}"),
            )),
            None => Err(Error::new(
                ErrorKind::InvalidInput,
                "help topic is not valid UTF-8",
            )),
        }
    }
}

impl std::fmt::Display for HelpTopic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Init => "init",
            Self::Ask => "ask",
            Self::Doctor => "doctor",
            Self::Run => "run",
            Self::Serve => "serve",
            Self::Submit => "submit",
            Self::Load => "load",
            Self::Stop => "stop",
            Self::Capabilities => "capabilities",
            Self::Observe => "observe",
            Self::Result => "result",
            Self::Cancel => "cancel",
            Self::Status => "status",
            Self::Tail => "tail",
            Self::Rebuild => "rebuild",
            Self::Recover => "recover",
            Self::Resume => "resume",
            Self::Outbox => "outbox",
            Self::Ack => "ack",
        };
        formatter.write_str(name)
    }
}

pub(super) fn parse(arguments: impl IntoIterator<Item = std::ffi::OsString>) -> Result<CliCommand> {
    let mut parser = lexopt::Parser::from_args(arguments);
    let Some(argument) = next(&mut parser)? else {
        return Ok(CliCommand::Help(None));
    };
    match argument {
        Value(value) if value == "init" => question::parse_init(&mut parser),
        Value(value) if value == "ask" => question::parse_ask(&mut parser),
        Value(value) if value == "doctor" => parse_doctor(&mut parser),
        Value(value) if value == "run" => parse_run(&mut parser),
        Value(value) if value == "serve" => service::parse_serve(&mut parser),
        Value(value) if value == "submit" => service::parse_submit(&mut parser),
        Value(value) if value == "load" => {
            service::parse_socket_command(&mut parser, "load", HelpTopic::Load, |socket| {
                CliCommand::Load { socket }
            })
        }
        Value(value) if value == "stop" => {
            service::parse_socket_command(&mut parser, "stop", HelpTopic::Stop, |socket| {
                CliCommand::Stop { socket }
            })
        }
        Value(value) if value == "capabilities" => service::parse_socket_command(
            &mut parser,
            "capabilities",
            HelpTopic::Capabilities,
            |socket| CliCommand::Capabilities { socket },
        ),
        Value(value) if value == "observe" => service::parse_observe(&mut parser),
        Value(value) if value == "result" => service::parse_task_socket(
            &mut parser,
            "result",
            HelpTopic::Result,
            |task_id, socket| CliCommand::Result { task_id, socket },
        ),
        Value(value) if value == "cancel" => service::parse_cancel(&mut parser),
        Value(value) if value == "help" => parse_help(&mut parser),
        Value(value) if value == "status" => {
            parse_task_command(&mut parser, "status", HelpTopic::Status, CliCommand::Status)
        }
        Value(value) if value == "tail" => parse_tail(&mut parser),
        Value(value) if value == "rebuild" => parse_task_command(
            &mut parser,
            "rebuild",
            HelpTopic::Rebuild,
            CliCommand::Rebuild,
        ),
        Value(value) if value == "resume" => {
            parse_task_command(&mut parser, "resume", HelpTopic::Resume, CliCommand::Resume)
        }
        Value(value) if value == "recover" => parse_no_arguments(
            &mut parser,
            "recover",
            CliCommand::Recover,
            HelpTopic::Recover,
        ),
        Value(value) if value == "outbox" => {
            parse_task_command(&mut parser, "outbox", HelpTopic::Outbox, CliCommand::Outbox)
        }
        Value(value) if value == "ack" => parse_acknowledgement(&mut parser),
        Long("help") | Short('h') => Ok(CliCommand::Help(None)),
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
    while let Some(argument) = next(parser)? {
        match argument {
            Long("help") | Short('h') => return Ok(CliCommand::Help(Some(HelpTopic::Run))),
            Value(value) if task_path.is_none() => task_path = Some(PathBuf::from(value)),
            Long("engine") => engine = Some(value(parser)?),
            other => return unexpected("run", &other),
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

fn parse_task_command(
    parser: &mut lexopt::Parser,
    command: &str,
    topic: HelpTopic,
    make: fn(String) -> CliCommand,
) -> Result<CliCommand> {
    let Some(task_id) = parse_identifier(parser, command, topic)? else {
        return Ok(CliCommand::Help(Some(topic)));
    };
    if next(parser)?.is_some() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("{command} accepts one identifier"),
        ));
    }
    Ok(make(task_id))
}

fn parse_tail(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    let Some(task_id) = parse_identifier(parser, "tail", HelpTopic::Tail)? else {
        return Ok(CliCommand::Help(Some(HelpTopic::Tail)));
    };
    let mut after = 0;
    while let Some(argument) = next(parser)? {
        match argument {
            Long("help") | Short('h') => return Ok(CliCommand::Help(Some(HelpTopic::Tail))),
            Long("after") => {
                after = value(parser)?
                    .to_string_lossy()
                    .parse::<u64>()
                    .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?;
            }
            other => return unexpected("tail", &other),
        }
    }
    Ok(CliCommand::Tail { task_id, after })
}

fn parse_no_arguments(
    parser: &mut lexopt::Parser,
    command: &str,
    result: CliCommand,
    topic: HelpTopic,
) -> Result<CliCommand> {
    let argument = next(parser)?;
    if matches!(argument, Some(Long("help") | Short('h'))) {
        return Ok(CliCommand::Help(Some(topic)));
    }
    if argument.is_some() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("{command} accepts no arguments"),
        ));
    }
    Ok(result)
}

fn parse_acknowledgement(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    let first = next(parser)?
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "ack requires a message id"))?;
    if matches!(first, Long("help") | Short('h')) {
        return Ok(CliCommand::Help(Some(HelpTopic::Ack)));
    }
    let Value(message_id) = first else {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "ack requires a message id",
        ));
    };
    let consumer_id = value(parser)?.to_string_lossy().into_owned();
    let mut socket = None;
    while let Some(argument) = next(parser)? {
        match argument {
            Long("socket") => socket = Some(PathBuf::from(value(parser)?)),
            other => return unexpected("ack", &other),
        }
    }
    Ok(CliCommand::Acknowledge {
        message_id: message_id.to_string_lossy().into_owned(),
        consumer_id,
        socket,
    })
}

fn parse_doctor(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    let mut engine = None;
    while let Some(argument) = next(parser)? {
        match argument {
            Long("help") | Short('h') => return Ok(CliCommand::Help(Some(HelpTopic::Doctor))),
            Long("engine") => engine = Some(value(parser)?),
            other => return unexpected("doctor", &other),
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

fn parse_help(parser: &mut lexopt::Parser) -> Result<CliCommand> {
    let Some(argument) = next(parser)? else {
        return Ok(CliCommand::Help(None));
    };
    let Value(value) = argument else {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "help accepts one command name",
        ));
    };
    if next(parser)?.is_some() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "help accepts one command name",
        ));
    }
    Ok(CliCommand::Help(Some(HelpTopic::parse(&value)?)))
}

fn parse_identifier(
    parser: &mut lexopt::Parser,
    command: &str,
    topic: HelpTopic,
) -> Result<Option<String>> {
    let argument = next(parser)?.ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            format!("{command} requires an identifier"),
        )
    })?;
    if matches!(argument, Long("help") | Short('h')) {
        return Ok(None);
    }
    let Value(value) = argument else {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("unexpected {command} argument {argument:?}; use spewer help {topic}"),
        ));
    };
    Ok(Some(value.to_string_lossy().into_owned()))
}

fn next(parser: &mut lexopt::Parser) -> Result<Option<lexopt::Arg<'_>>> {
    parser
        .next()
        .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))
}

fn value(parser: &mut lexopt::Parser) -> Result<std::ffi::OsString> {
    parser
        .value()
        .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))
}

fn unexpected<T>(command: &str, argument: &lexopt::Arg<'_>) -> Result<T> {
    Err(Error::new(
        ErrorKind::InvalidInput,
        format!("unexpected {command} argument {argument:?}"),
    ))
}
