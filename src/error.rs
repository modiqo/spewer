//! Explicit Spewer error values.

use std::fmt;

/// The error categories exposed by Spewer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    /// Input failed public validation.
    InvalidInput,
    /// An operating-system or filesystem call failed.
    Io,
    /// JSON could not be encoded or decoded.
    Json,
    /// `SQLite` rejected an operation.
    Storage,
    /// An engine violated or rejected its protocol.
    EngineProtocol,
    /// An operation exceeded its explicit deadline.
    Timeout,
    /// A bounded internal channel closed.
    ChannelClosed,
    /// A spawned task or thread failed.
    Join,
}

/// One typed Spewer failure with stable category and readable context.
#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    message: String,
}

impl Error {
    /// Creates an error in a stable category.
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// Returns the stable failure category.
    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", kind_name(self.kind), self.message)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::new(ErrorKind::Io, error.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::new(ErrorKind::Json, error.to_string())
    }
}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Self::new(ErrorKind::Storage, error.to_string())
    }
}

impl From<tokio::task::JoinError> for Error {
    fn from(error: tokio::task::JoinError) -> Self {
        Self::new(ErrorKind::Join, error.to_string())
    }
}

const fn kind_name(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::InvalidInput => "invalid input",
        ErrorKind::Io => "I/O",
        ErrorKind::Json => "JSON",
        ErrorKind::Storage => "storage",
        ErrorKind::EngineProtocol => "engine protocol",
        ErrorKind::Timeout => "timeout",
        ErrorKind::ChannelClosed => "channel closed",
        ErrorKind::Join => "join",
    }
}

/// The result type used throughout Spewer.
pub type Result<T> = std::result::Result<T, Error>;
