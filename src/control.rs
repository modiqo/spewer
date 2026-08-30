//! JSON Lines control protocol over a private local socket.

#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use unix::{
    LocalService, acknowledge, cancel, capabilities, load, observe, respond, result, stop, submit,
};

use crate::capsule::CapsuleAdvertisement;
use crate::error::{ErrorKind, Result};
use crate::protocol::{TaskHandle, TaskInputResponse, TaskRequest};
use crate::reducer::Projection;
use crate::store::{CancelOutcome, Observation, TaskResult};
use crate::supervisor::SupervisorLoad;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Maximum encoded request accepted by the local control service.
pub const MAX_CONTROL_BYTES: u64 = 1_048_576;

/// Features implemented by this service version.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ServiceCapabilities {
    /// Spewer task protocol version.
    pub protocol_version: String,
    /// Stable service operations accepted by the control socket.
    pub operations: Vec<String>,
    /// Supported parent delivery modes.
    pub callback_modes: Vec<String>,
    /// Engine kinds that the service can schedule.
    pub engine_kinds: Vec<String>,
    /// Maximum encoded control request size.
    pub max_control_bytes: u64,
    /// Seconds an active worker waits for one human response.
    pub input_timeout_seconds: u64,
    /// Whether task cancellation is implemented.
    pub cancellation: bool,
    /// Whether callers can replay events from a durable cursor.
    pub cursor_replay: bool,
    /// Content revision of the live capsule catalog.
    pub capsule_revision: String,
    /// Generic and skill-specialized workers available now.
    pub capsules: Vec<CapsuleAdvertisement>,
}

/// Returns stable protocol features plus the live capsule catalog.
pub fn service_capabilities() -> Result<ServiceCapabilities> {
    let catalog = crate::capsule::catalog()?;
    Ok(ServiceCapabilities {
        protocol_version: crate::protocol::PROTOCOL_VERSION.to_owned(),
        operations: [
            "capabilities",
            "submit",
            "observe",
            "result",
            "respond",
            "cancel",
            "acknowledge",
            "load",
            "stop",
        ]
        .map(str::to_owned)
        .into(),
        callback_modes: ["stream", "wait", "poll"].map(str::to_owned).into(),
        engine_kinds: ["codex-app-server", crate::ollama::ENGINE_KIND]
            .map(str::to_owned)
            .into(),
        max_control_bytes: MAX_CONTROL_BYTES,
        input_timeout_seconds: crate::protocol::HUMAN_INPUT_TIMEOUT_SECONDS,
        cancellation: true,
        cursor_replay: true,
        capsule_revision: catalog.revision,
        capsules: catalog.capsules,
    })
}

/// Returns the default local control socket path.
pub fn default_socket_path() -> Result<PathBuf> {
    Ok(crate::util::data_root()?.join("spewer.sock"))
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum ControlRequest {
    Capabilities,
    Submit {
        request: Box<TaskRequest>,
    },
    Observe {
        task_id: String,
        after: u64,
    },
    Result {
        task_id: String,
    },
    Respond {
        task_id: String,
        response: TaskInputResponse,
    },
    Cancel {
        task_id: String,
        reason: String,
    },
    Acknowledge {
        message_id: String,
        consumer_id: String,
    },
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
    /// Features implemented by the running service.
    Capabilities {
        /// Negotiation result.
        capabilities: ServiceCapabilities,
    },
    /// Current task state plus replayed events.
    Observation {
        /// Consistent observation snapshot.
        observation: Observation,
    },
    /// Current task state and optional terminal message.
    Result {
        /// Non-consuming result snapshot.
        result: TaskResult,
    },
    /// A pending input request accepted one durable response.
    InputAccepted {
        /// Projection after the durable `input.resolved` event.
        projection: Projection,
    },
    /// Idempotent task cancellation result.
    Cancellation {
        /// Terminal projection and callback information.
        cancellation: CancelOutcome,
    },
    /// One consumer acknowledgement result.
    Acknowledged {
        /// True only when this call inserted the acknowledgement.
        applied: bool,
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
        /// Stable error category.
        kind: ErrorKind,
        /// Readable error context.
        message: String,
    },
}

#[cfg(not(unix))]
mod unsupported {
    use super::{
        CancelOutcome, ControlResponse, Observation, PathBuf, Projection, Result,
        ServiceCapabilities, SupervisorLoad, TaskHandle, TaskInputResponse, TaskRequest,
        TaskResult,
    };
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
            _ollama: crate::ollama::OllamaConfig,
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
    pub async fn capabilities(_path: PathBuf) -> Result<ServiceCapabilities> {
        Err(unsupported())
    }

    /// Returns an unsupported-platform error.
    pub async fn observe(_path: PathBuf, _task_id: String, _after: u64) -> Result<Observation> {
        Err(unsupported())
    }

    /// Returns an unsupported-platform error.
    pub async fn result(_path: PathBuf, _task_id: String) -> Result<TaskResult> {
        Err(unsupported())
    }

    /// Returns an unsupported-platform error.
    pub async fn respond(
        _path: PathBuf,
        _task_id: String,
        _response: TaskInputResponse,
    ) -> Result<Projection> {
        Err(unsupported())
    }

    /// Returns an unsupported-platform error.
    pub async fn cancel(
        _path: PathBuf,
        _task_id: String,
        _reason: String,
    ) -> Result<CancelOutcome> {
        Err(unsupported())
    }

    /// Returns an unsupported-platform error.
    pub async fn acknowledge(
        _path: PathBuf,
        _message_id: String,
        _consumer_id: String,
    ) -> Result<bool> {
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
pub use unsupported::{
    LocalService, acknowledge, cancel, capabilities, load, observe, respond, result, stop, submit,
};
