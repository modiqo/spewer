//! Public adapter contract implemented without provider-specific wire types.

use crate::error::Result;
use crate::protocol::TaskRequest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;

/// Capabilities negotiated before a task is accepted by an engine.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EngineCapabilities {
    /// Engine adapter identifier.
    pub kind: String,
    /// Models available for dispatch.
    pub models: Vec<String>,
    /// Whether stored conversations can resume.
    pub resumable: bool,
    /// Whether usage events include provider token counts.
    pub usage: bool,
}

/// Provider-neutral source message consumed by the common controller.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EngineEvent {
    /// Native event name retained as provenance.
    pub method: String,
    /// Provider-neutral normalized event type.
    pub kind: String,
    /// Normalized event data.
    pub data: Value,
    /// Stable source key used for deduplication.
    pub source_key: String,
}

/// Minimal harness boundary for Codex, local, or hosted engines.
pub trait EngineAdapter {
    /// Returns capabilities without starting a task.
    fn capabilities(&self) -> &EngineCapabilities;

    /// Executes one bounded request and yields a finite event stream.
    fn execute(&mut self, request: &TaskRequest) -> impl Future<Output = Result<Vec<EngineEvent>>>;
}

/// Rejects an unsupported model or recovery requirement before dispatch.
pub fn negotiate(
    capabilities: &EngineCapabilities,
    request: &TaskRequest,
    needs_resume: bool,
) -> Result<()> {
    if !capabilities.models.contains(&request.engine.model) {
        return Err(crate::error::Error::new(
            crate::error::ErrorKind::InvalidInput,
            "engine does not advertise the requested model",
        ));
    }
    if needs_resume && !capabilities.resumable {
        return Err(crate::error::Error::new(
            crate::error::ErrorKind::InvalidInput,
            "engine does not support task resumption",
        ));
    }
    Ok(())
}
