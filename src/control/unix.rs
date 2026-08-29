//! Unix-domain implementation of the private control protocol.

#[cfg(test)]
mod tests;

use super::{
    ControlRequest, ControlResponse, MAX_CONTROL_BYTES, ServiceCapabilities, service_capabilities,
};
use crate::codex::CodexConfig;
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::{TaskHandle, TaskRequest};
use crate::store::Database;
use crate::store::{CancelOutcome, Observation, TaskResult};
use crate::supervisor::{Supervisor, SupervisorConfig, SupervisorLoad};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

/// Foreground local service that owns the scheduler and its control socket.
#[derive(Debug)]
pub struct LocalService {
    path: PathBuf,
    listener: UnixListener,
    supervisor: Supervisor,
}

impl LocalService {
    /// Binds a private socket and starts the turn supervisor.
    pub async fn bind(
        path: PathBuf,
        database: Database,
        codex: CodexConfig,
        ollama: crate::ollama::OllamaConfig,
        config: SupervisorConfig,
    ) -> Result<Self> {
        let listener = bind_socket(path.clone()).await?;
        let supervisor = match Supervisor::start_engines(database, codex, ollama, config).await {
            Ok(supervisor) => supervisor,
            Err(error) => {
                remove_socket(path).await?;
                return Err(error);
            }
        };
        Ok(Self {
            path,
            listener,
            supervisor,
        })
    }

    /// Returns the bound socket path.
    pub fn socket_path(&self) -> &Path {
        &self.path
    }

    /// Serves requests until `stop` or an operating-system interrupt, then drains workers.
    pub async fn run(self) -> Result<()> {
        let Self {
            path,
            listener,
            supervisor,
        } = self;
        let handle = supervisor.handle();
        let service_result = loop {
            let accepted = tokio::select! {
                result = listener.accept() => Some(result),
                signal = tokio::signal::ctrl_c() => {
                    match signal {
                        Ok(()) => None,
                        Err(error) => break Err(error.into()),
                    }
                }
            };
            let Some(accepted) = accepted else {
                break Ok(());
            };
            let (stream, _address) = match accepted {
                Ok(value) => value,
                Err(error) => break Err(error.into()),
            };
            match serve_one(stream, &handle).await {
                Ok(true) => break Ok(()),
                Ok(false) => {}
                Err(error) => break Err(error),
            }
        };
        let shutdown = supervisor.shutdown().await;
        let cleanup = remove_socket(path).await;
        service_result.and(shutdown).and(cleanup)
    }
}

/// Submits one task and returns after its acceptance event commits.
pub async fn submit(path: PathBuf, request: TaskRequest) -> Result<TaskHandle> {
    match send(
        path,
        ControlRequest::Submit {
            request: Box::new(request),
        },
    )
    .await?
    {
        ControlResponse::Handle { handle } => Ok(handle),
        ControlResponse::Error { kind, message } => Err(Error::new(kind, message)),
        _ => Err(unexpected_response()),
    }
}

/// Reads the operations and limits implemented by the running service.
pub async fn capabilities(path: PathBuf) -> Result<ServiceCapabilities> {
    match send(path, ControlRequest::Capabilities).await? {
        ControlResponse::Capabilities { capabilities } => Ok(capabilities),
        ControlResponse::Error { kind, message } => Err(Error::new(kind, message)),
        _ => Err(unexpected_response()),
    }
}

/// Reads current state and committed events after one cursor.
pub async fn observe(path: PathBuf, task_id: String, after: u64) -> Result<Observation> {
    match send(path, ControlRequest::Observe { task_id, after }).await? {
        ControlResponse::Observation { observation } => Ok(observation),
        ControlResponse::Error { kind, message } => Err(Error::new(kind, message)),
        _ => Err(unexpected_response()),
    }
}

/// Reads one task's current state and stable terminal message.
pub async fn result(path: PathBuf, task_id: String) -> Result<TaskResult> {
    match send(path, ControlRequest::Result { task_id }).await? {
        ControlResponse::Result { result } => Ok(result),
        ControlResponse::Error { kind, message } => Err(Error::new(kind, message)),
        _ => Err(unexpected_response()),
    }
}

/// Cancels one queued or active task through its scheduler owner.
pub async fn cancel(path: PathBuf, task_id: String, reason: String) -> Result<CancelOutcome> {
    match send(path, ControlRequest::Cancel { task_id, reason }).await? {
        ControlResponse::Cancellation { cancellation } => Ok(cancellation),
        ControlResponse::Error { kind, message } => Err(Error::new(kind, message)),
        _ => Err(unexpected_response()),
    }
}

/// Acknowledges a terminal message after one consumer stores it.
pub async fn acknowledge(path: PathBuf, message_id: String, consumer_id: String) -> Result<bool> {
    match send(
        path,
        ControlRequest::Acknowledge {
            message_id,
            consumer_id,
        },
    )
    .await?
    {
        ControlResponse::Acknowledged { applied } => Ok(applied),
        ControlResponse::Error { kind, message } => Err(Error::new(kind, message)),
        _ => Err(unexpected_response()),
    }
}

/// Reads current queue depth and active-turn capacity.
pub async fn load(path: PathBuf) -> Result<SupervisorLoad> {
    match send(path, ControlRequest::Load).await? {
        ControlResponse::Load { load } => Ok(load),
        ControlResponse::Error { kind, message } => Err(Error::new(kind, message)),
        _ => Err(unexpected_response()),
    }
}

/// Requests graceful drain and shutdown.
pub async fn stop(path: PathBuf) -> Result<ControlResponse> {
    match send(path, ControlRequest::Stop).await? {
        response @ ControlResponse::Stopping => Ok(response),
        ControlResponse::Error { kind, message } => Err(Error::new(kind, message)),
        _ => Err(unexpected_response()),
    }
}

async fn serve_one(
    stream: UnixStream,
    handle: &crate::supervisor::SupervisorHandle,
) -> Result<bool> {
    let mut reader = BufReader::new(stream).take(MAX_CONTROL_BYTES.saturating_add(1));
    let mut line = String::new();
    let bytes = reader.read_line(&mut line).await?;
    let (response, stop) =
        if bytes == 0 || u64::try_from(bytes).map_or(true, |n| n > MAX_CONTROL_BYTES) {
            (
                ControlResponse::Error {
                    kind: ErrorKind::InvalidInput,
                    message: "control request is empty or exceeds 1 MiB".to_owned(),
                },
                false,
            )
        } else {
            match serde_json::from_str::<ControlRequest>(&line) {
                Ok(request) => execute(request, handle).await,
                Err(error) => (
                    ControlResponse::Error {
                        kind: ErrorKind::Json,
                        message: format!("invalid control request: {error}"),
                    },
                    false,
                ),
            }
        };
    write_response(reader.get_mut().get_mut(), &response).await?;
    Ok(stop)
}

async fn execute(
    request: ControlRequest,
    handle: &crate::supervisor::SupervisorHandle,
) -> (ControlResponse, bool) {
    match request {
        ControlRequest::Capabilities => match service_capabilities() {
            Ok(capabilities) => (ControlResponse::Capabilities { capabilities }, false),
            Err(error) => (error_response(&error), false),
        },
        ControlRequest::Submit { request } => match handle.submit(*request).await {
            Ok(task) => (ControlResponse::Handle { handle: task }, false),
            Err(error) => (error_response(&error), false),
        },
        ControlRequest::Observe { task_id, after } => match handle.observe(task_id, after).await {
            Ok(observation) => (ControlResponse::Observation { observation }, false),
            Err(error) => (error_response(&error), false),
        },
        ControlRequest::Result { task_id } => match handle.result(task_id).await {
            Ok(result) => (ControlResponse::Result { result }, false),
            Err(error) => (error_response(&error), false),
        },
        ControlRequest::Cancel { task_id, reason } => match handle.cancel(task_id, reason).await {
            Ok(cancellation) => (ControlResponse::Cancellation { cancellation }, false),
            Err(error) => (error_response(&error), false),
        },
        ControlRequest::Acknowledge {
            message_id,
            consumer_id,
        } => match handle.acknowledge(message_id, consumer_id).await {
            Ok(applied) => (ControlResponse::Acknowledged { applied }, false),
            Err(error) => (error_response(&error), false),
        },
        ControlRequest::Load => match handle.load().await {
            Ok(load) => (ControlResponse::Load { load }, false),
            Err(error) => (error_response(&error), false),
        },
        ControlRequest::Stop => (ControlResponse::Stopping, true),
    }
}

fn error_response(error: &Error) -> ControlResponse {
    ControlResponse::Error {
        kind: error.kind(),
        message: error.message().to_owned(),
    }
}

async fn send(path: PathBuf, request: ControlRequest) -> Result<ControlResponse> {
    let stream = UnixStream::connect(path).await?;
    let mut reader = BufReader::new(stream);
    let mut encoded = serde_json::to_vec(&request)?;
    encoded.push(b'\n');
    reader.get_mut().write_all(&encoded).await?;
    reader.get_mut().flush().await?;
    let mut line = String::new();
    let bytes = reader.read_line(&mut line).await?;
    if bytes == 0 {
        return Err(Error::new(
            ErrorKind::ChannelClosed,
            "local service closed without a response",
        ));
    }
    Ok(serde_json::from_str(&line)?)
}

async fn write_response(stream: &mut UnixStream, response: &ControlResponse) -> Result<()> {
    let mut encoded = serde_json::to_vec(response)?;
    encoded.push(b'\n');
    stream.write_all(&encoded).await?;
    stream.flush().await?;
    Ok(())
}

async fn bind_socket(path: PathBuf) -> Result<UnixListener> {
    let inspect = path.clone();
    tokio::task::spawn_blocking(move || prepare_path(&inspect)).await??;
    let listener = UnixListener::bind(&path)?;
    let permissions = std::fs::Permissions::from_mode(0o600);
    if let Err(error) = std::fs::set_permissions(&path, permissions) {
        let _removed = std::fs::remove_file(&path);
        return Err(error.into());
    }
    Ok(listener)
}

fn prepare_path(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let Some(metadata) = metadata else {
        return Ok(());
    };
    if !metadata.file_type().is_socket() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "control path exists and is not a Unix socket",
        ));
    }
    match std::os::unix::net::UnixStream::connect(path) {
        Ok(_stream) => Err(Error::new(
            ErrorKind::InvalidInput,
            "a Spewer service is already listening",
        )),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
            ) =>
        {
            std::fs::remove_file(path)?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

async fn remove_socket(path: PathBuf) -> Result<()> {
    tokio::task::spawn_blocking(move || match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    })
    .await?
}

fn unexpected_response() -> Error {
    Error::new(
        ErrorKind::EngineProtocol,
        "unexpected local control response",
    )
}
