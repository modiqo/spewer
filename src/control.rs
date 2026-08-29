//! JSON Lines control protocol over a private local socket.

#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use unix::{LocalService, load, stop, submit};

use crate::error::Result;
use crate::protocol::{TaskHandle, TaskRequest};
use crate::supervisor::SupervisorLoad;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Returns the default local control socket path.
pub fn default_socket_path() -> Result<PathBuf> {
    Ok(crate::util::data_root()?.join("spewer.sock"))
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum ControlRequest {
    Submit { request: Box<TaskRequest> },
    Load,
    Stop,
}

/// One response from the local supervisor service.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlResponse {
    /// A task was durably accepted.
    Handle {
        /// Stable task handle committed before scheduling.
        handle: TaskHandle,
    },
    /// Current scheduler capacity and queue depth.
    Load {
        /// Point-in-time load report.
        load: SupervisorLoad,
    },
    /// The service accepted a graceful stop request.
    Stopping,
    /// The service rejected the request.
    Error {
        /// Typed server error rendered for the local caller.
        message: String,
    },
}

#[cfg(not(unix))]
mod unsupported {
    use super::{ControlResponse, PathBuf, Result, SupervisorLoad, TaskHandle, TaskRequest};
    use crate::codex::CodexConfig;
    use crate::error::{Error, ErrorKind};
    use crate::store::Database;
    use crate::supervisor::SupervisorConfig;

    /// Placeholder on platforms without Unix sockets.
    #[derive(Debug)]
    pub struct LocalService;

    impl LocalService {
        /// Returns an unsupported-platform error.
        pub async fn bind(
            _path: PathBuf,
            _database: Database,
            _codex: CodexConfig,
            _config: SupervisorConfig,
        ) -> Result<Self> {
            Err(unsupported())
        }

        /// Returns an empty path on unsupported platforms.
        pub fn socket_path(&self) -> &std::path::Path {
            std::path::Path::new("")
        }

        /// Returns an unsupported-platform error.
        pub async fn run(self) -> Result<()> {
            Err(unsupported())
        }
    }

    /// Returns an unsupported-platform error.
    pub async fn submit(_path: PathBuf, _request: TaskRequest) -> Result<TaskHandle> {
        Err(unsupported())
    }

    /// Returns an unsupported-platform error.
    pub async fn load(_path: PathBuf) -> Result<SupervisorLoad> {
        Err(unsupported())
    }

    /// Returns an unsupported-platform error.
    pub async fn stop(_path: PathBuf) -> Result<ControlResponse> {
        Err(unsupported())
    }

    fn unsupported() -> Error {
        Error::new(
            ErrorKind::InvalidInput,
            "local service requires Unix domain sockets",
        )
    }
}

#[cfg(not(unix))]
pub use unsupported::{LocalService, load, stop, submit};
