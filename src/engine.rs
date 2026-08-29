//! Public adapter contract implemented without provider-specific wire types.

use crate::error::Result;
use crate::protocol::{EventSource, TaskRequest};
use crate::util::{now, sha256};
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

/// One engine-neutral event before durable task sequencing.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct NormalizedEvent {
    /// Stable normalized event type.
    pub kind: String,
    /// Normalized event data.
    pub data: Value,
    /// Engine source provenance.
    pub source: EventSource,
    /// Deterministic source deduplication key.
    pub source_key: String,
    /// RFC 3339 observation time.
    pub observed_at: String,
}

impl EngineEvent {
    /// Adds common provenance and observation time to one adapter event.
    pub fn normalize(self, engine: &str) -> Result<NormalizedEvent> {
        let payload_hash = sha256(&serde_json::to_vec(&self.data)?)?;
        Ok(NormalizedEvent {
            kind: self.kind,
            data: self.data,
            source: EventSource {
                engine: engine.to_owned(),
                method: self.method,
                thread_id: None,
                turn_id: None,
                item_id: None,
                payload_hash,
            },
            source_key: self.source_key,
            observed_at: now()?,
        })
    }
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
