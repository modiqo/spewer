//! Small reusable client surface for frontier harness adapters.

use crate::capsule::{CapsuleAdvertisement, CapsuleRequest};
use crate::control::ServiceCapabilities;
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::{TaskHandle, TaskInputResponse, TaskRequest};
use crate::reducer::Projection;
use crate::store::{CancelOutcome, Observation, TaskResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Client for one running Spewer service.
#[derive(Clone, Debug)]
pub struct HarnessClient {
    socket: PathBuf,
}

/// Durable acceptance result plus the capsule selected by live lookup.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Delegation {
    /// Task handle committed before worker startup.
    pub handle: TaskHandle,
    /// Catalog revision observed during delegation.
    pub catalog_revision: String,
    /// Exact capsule advertisement bound to the task.
    pub capsule: CapsuleAdvertisement,
}

/// One combined observation and non-consuming result check.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HarnessCheck {
    /// Whether a stable terminal message is available.
    pub ready: bool,
    /// Projection, later events, next cursor, and polling delay.
    pub observation: Observation,
    /// Stable terminal message when ready.
    pub result: TaskResult,
}

impl HarnessClient {
    /// Connects operations to one local control socket.
    pub const fn new(socket: PathBuf) -> Self {
        Self { socket }
    }

    /// Reads stable operations and the current capsule catalog.
    pub async fn discover(&self) -> Result<ServiceCapabilities> {
        crate::control::capabilities(self.socket.clone()).await
    }

    /// Looks up one capsule, binds it to a request, and submits durably.
    pub async fn delegate(&self, mut request: TaskRequest, capsule_id: &str) -> Result<Delegation> {
        let capabilities = self.discover().await?;
        let capsule = capabilities
            .capsules
            .iter()
            .find(|capsule| capsule.id == capsule_id)
            .cloned()
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidInput,
                    format!("capsule {capsule_id} is not advertised"),
                )
            })?;
        ensure_authority_supported(&request, &capsule)?;
        request.engine = capsule.engine.clone();
        request.capsule = Some(CapsuleRequest {
            id: capsule.id.clone(),
            revision: capsule.revision.clone(),
            binding: None,
        });
        request.validate()?;
        let handle = crate::control::submit(self.socket.clone(), request).await?;
        Ok(Delegation {
            handle,
            catalog_revision: capabilities.capsule_revision,
            capsule,
        })
    }

    /// Combines cursor replay and stable terminal retrieval.
    pub async fn check(&self, task_id: String, after: u64) -> Result<HarnessCheck> {
        let observation =
            crate::control::observe(self.socket.clone(), task_id.clone(), after).await?;
        let result = crate::control::result(self.socket.clone(), task_id).await?;
        let ready = result.message.is_some();
        Ok(HarnessCheck {
            ready,
            observation,
            result,
        })
    }

    /// Cancels queued or active work idempotently.
    pub async fn cancel(&self, task_id: String, reason: String) -> Result<CancelOutcome> {
        crate::control::cancel(self.socket.clone(), task_id, reason).await
    }

    /// Answers a typed human boundary and continues the same delegated task.
    pub async fn respond(
        &self,
        task_id: String,
        response: TaskInputResponse,
    ) -> Result<Projection> {
        crate::control::respond(self.socket.clone(), task_id, response).await
    }
}

fn ensure_authority_supported(request: &TaskRequest, capsule: &CapsuleAdvertisement) -> Result<()> {
    if request.permissions.network == "allow" && !capsule.network {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("capsule {} advertises network=false", capsule.id),
        ));
    }
    if request.permissions.filesystem != "read-only"
        && !capsule.tools.iter().any(|tool| tool == "filesystem")
    {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!(
                "capsule {} does not advertise a filesystem tool",
                capsule.id
            ),
        ));
    }
    if request.permissions.commands == "allowlist"
        && !capsule.tools.iter().any(|tool| tool == "commands")
    {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("capsule {} does not advertise a command tool", capsule.id),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ensure_authority_supported;
    use crate::capsule::{CapsuleAdvertisement, CapsuleKind};
    use crate::protocol::{EngineRequest, TaskRequest};

    #[test]
    fn delegation_rejects_authority_missing_from_the_card() -> Result<(), serde_json::Error> {
        let mut request: TaskRequest =
            serde_json::from_str(include_str!("../tests/fixtures/task-request.json"))?;
        let capsule = CapsuleAdvertisement {
            id: "qwen3-local".to_owned(),
            revision: "revision".to_owned(),
            kind: CapsuleKind::Generic,
            description: "Local inference".to_owned(),
            engine: EngineRequest {
                kind: "ollama".to_owned(),
                model: "qwen3:30b-a3b".to_owned(),
                effort: None,
            },
            network: false,
            tools: Vec::new(),
            skill: None,
        };
        request.permissions.network = "allow".to_owned();
        assert!(ensure_authority_supported(&request, &capsule).is_err());
        request.permissions.network = "deny".to_owned();
        assert!(ensure_authority_supported(&request, &capsule).is_err());
        request.permissions.filesystem = "read-only".to_owned();
        request.permissions.writable_paths.clear();
        request.permissions.commands = "allowlist".to_owned();
        assert!(ensure_authority_supported(&request, &capsule).is_err());
        request.permissions.commands = "engine-policy".to_owned();
        assert!(ensure_authority_supported(&request, &capsule).is_ok());
        Ok(())
    }
}
