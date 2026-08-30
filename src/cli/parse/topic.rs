//! Stable command help topics.

use crate::error::{Error, ErrorKind, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HelpTopic {
    Install,
    Capsule,
    Init,
    Ask,
    Doctor,
    Run,
    Serve,
    Submit,
    Delegate,
    Check,
    Load,
    Stop,
    Capabilities,
    Observe,
    Result,
    Respond,
    Cancel,
    Status,
    Tail,
    Watch,
    Rebuild,
    Recover,
    Resume,
    Outbox,
    Ack,
}

impl HelpTopic {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 25] = [
        Self::Install,
        Self::Capsule,
        Self::Init,
        Self::Ask,
        Self::Doctor,
        Self::Run,
        Self::Serve,
        Self::Submit,
        Self::Delegate,
        Self::Check,
        Self::Load,
        Self::Stop,
        Self::Capabilities,
        Self::Observe,
        Self::Result,
        Self::Respond,
        Self::Cancel,
        Self::Status,
        Self::Tail,
        Self::Watch,
        Self::Rebuild,
        Self::Recover,
        Self::Resume,
        Self::Outbox,
        Self::Ack,
    ];

    pub(super) fn parse(value: &std::ffi::OsStr) -> Result<Self> {
        match value.to_str() {
            Some("install") => Ok(Self::Install),
            Some("capsule") => Ok(Self::Capsule),
            Some("init") => Ok(Self::Init),
            Some("ask") => Ok(Self::Ask),
            Some("doctor") => Ok(Self::Doctor),
            Some("run") => Ok(Self::Run),
            Some("serve") => Ok(Self::Serve),
            Some("submit") => Ok(Self::Submit),
            Some("delegate") => Ok(Self::Delegate),
            Some("check") => Ok(Self::Check),
            Some("load") => Ok(Self::Load),
            Some("stop") => Ok(Self::Stop),
            Some("capabilities") => Ok(Self::Capabilities),
            Some("observe") => Ok(Self::Observe),
            Some("result") => Ok(Self::Result),
            Some("respond") => Ok(Self::Respond),
            Some("cancel") => Ok(Self::Cancel),
            Some("status") => Ok(Self::Status),
            Some("tail") => Ok(Self::Tail),
            Some("watch") => Ok(Self::Watch),
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
            Self::Install => "install",
            Self::Capsule => "capsule",
            Self::Init => "init",
            Self::Ask => "ask",
            Self::Doctor => "doctor",
            Self::Run => "run",
            Self::Serve => "serve",
            Self::Submit => "submit",
            Self::Delegate => "delegate",
            Self::Check => "check",
            Self::Load => "load",
            Self::Stop => "stop",
            Self::Capabilities => "capabilities",
            Self::Observe => "observe",
            Self::Result => "result",
            Self::Respond => "respond",
            Self::Cancel => "cancel",
            Self::Status => "status",
            Self::Tail => "tail",
            Self::Watch => "watch",
            Self::Rebuild => "rebuild",
            Self::Recover => "recover",
            Self::Resume => "resume",
            Self::Outbox => "outbox",
            Self::Ack => "ack",
        };
        formatter.write_str(name)
    }
}
