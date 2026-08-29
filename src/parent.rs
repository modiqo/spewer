//! Engine-neutral handoff contract for Play and other parent harnesses.

use crate::delivery::OutboxMessage;
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::{Receipt, TaskRequest};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

/// Task projection submitted by a parent that retains final-response ownership.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Handoff {
    /// Bounded public request passed to Spewer.
    pub task: TaskRequest,
}

impl Handoff {
    /// Creates a Play handoff while keeping Play's continuation owner-private.
    pub fn for_play(task: TaskRequest) -> Result<Self> {
        if task.private_continuation.is_some() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "Play continuation state must remain in Play's runtime",
            ));
        }
        Ok(Self { task })
    }
}

/// Parent-side idempotency state suitable for durable serialization.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ParentCursor {
    /// Highest contiguous normalized event consumed.
    pub event_seq: u64,
    /// Receipt identities already applied.
    pub applied_receipts: BTreeSet<String>,
}

/// Evidence returned to the frontier parent after one callback application.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Continuation {
    /// Whether this callback advanced parent-visible state.
    pub applied: bool,
    /// Opaque parent state returned unchanged.
    pub private_continuation: Option<Value>,
    /// Receipt available for verification or escalation.
    pub receipt: Receipt,
}

impl ParentCursor {
    /// Applies at-least-once delivery exactly once at the parent boundary.
    pub fn apply(&mut self, handoff: &Handoff, message: OutboxMessage) -> Continuation {
        let applied = self
            .applied_receipts
            .insert(message.receipt.receipt_id.clone());
        if applied {
            self.event_seq = self.event_seq.max(message.receipt.final_event_seq);
        }
        Continuation {
            applied,
            private_continuation: handoff.task.private_continuation.clone(),
            receipt: message.receipt,
        }
    }
}
