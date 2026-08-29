//! Codex App Server integration.

mod discovery;
mod mapper;
mod params;
mod process;
mod wire;

pub use crate::engine::NormalizedEvent;
pub use mapper::Normalizer;
pub(crate) use params::{thread as thread_params, turn as turn_params};
pub use process::{CodexClient, CodexConfig, CodexMessage};

use crate::error::{Error, ErrorKind, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;
use tokio::process::Command;

/// Result returned by `spewer doctor --engine codex`.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DoctorReport {
    /// Whether the handshake completed.
    pub ready: bool,
    /// Installed Codex CLI version string.
    pub codex_version: String,
    /// App Server initialization response.
    pub initialization: serde_json::Value,
}

/// Starts App Server, performs the handshake, and shuts it down.
pub async fn doctor(config: CodexConfig) -> Result<DoctorReport> {
    let version_output = tokio::time::timeout(
        Duration::from_secs(5),
        Command::new(&config.program).arg("--version").output(),
    )
    .await
    .map_err(|_| Error::new(ErrorKind::Timeout, "Codex version probe exceeded 5 seconds"))??;
    if !version_output.status.success() {
        return Err(Error::new(
            ErrorKind::EngineProtocol,
            String::from_utf8_lossy(&version_output.stderr).into_owned(),
        ));
    }
    let codex_version = String::from_utf8(version_output.stdout)
        .map_err(|error| Error::new(ErrorKind::EngineProtocol, error.to_string()))?
        .trim()
        .to_owned();

    let mut client = CodexClient::connect(config).await?;
    let initialization = client.initialization().clone();
    let _models = client
        .request("model/list", json!({"limit": 1, "includeHidden": false}))
        .await?;
    client.close().await?;
    Ok(DoctorReport {
        ready: true,
        codex_version,
        initialization,
    })
}
