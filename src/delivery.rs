//! Stable terminal messages and idempotent parent acknowledgements.

use crate::protocol::Receipt;
use serde::{Deserialize, Serialize};

/// One durable result callback retained until parent acknowledgement.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct OutboxMessage {
    /// Stable delivery identity reused on retries.
    pub message_id: String,
    /// Task whose terminal result is ready.
    pub task_id: String,
    /// Stable receipt identity.
    pub receipt_id: String,
    /// Requested `stream`, `wait`, or `poll` delivery mode.
    pub mode: String,
    /// Typed terminal result.
    pub receipt: Receipt,
    /// RFC 3339 creation time.
    pub created_at: String,
}
