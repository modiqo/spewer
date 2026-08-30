//! Typed responses to one pending human-input request.

use super::ProtocolError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Maximum encoded response accepted for one human-input boundary.
const MAX_INPUT_RESPONSE_BYTES: usize = 65_536;

/// Typed parent response to one exact worker input request.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TaskInputResponse {
    /// Provider request identity copied from `projection.pending_input.request_id`.
    pub request_id: Value,
    /// Provider-shaped response validated against the pending request method.
    pub response: Value,
}

impl TaskInputResponse {
    /// Validates size and top-level shape before service dispatch.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if !matches!(self.request_id, Value::String(_) | Value::Number(_)) {
            return Err(ProtocolError::new(
                "input response request_id must be a string or number",
            ));
        }
        if !self.response.is_object() {
            return Err(ProtocolError::new("input response must be a JSON object"));
        }
        let encoded = serde_json::to_vec(self).map_err(|error| {
            ProtocolError::new(format!("input response encoding failed: {error}"))
        })?;
        if encoded.len() > MAX_INPUT_RESPONSE_BYTES {
            return Err(ProtocolError::new("input response exceeds 65536 bytes"));
        }
        Ok(())
    }
}
