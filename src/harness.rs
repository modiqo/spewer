//! Small reusable client surface for frontier harness adapters.

use crate::capsule::{CapsuleAdvertisement, CapsuleRequest};
use crate::control::ServiceCapabilities;
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::{TaskHandle, TaskRequest};
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
}
