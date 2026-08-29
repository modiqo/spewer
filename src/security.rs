//! Secret redaction and external-effect state transitions.

use crate::error::{Error, ErrorKind, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Redacts exact secret values before durable persistence.
#[derive(Clone, Debug, Default)]
pub struct Redactor {
    secrets: Vec<String>,
}

impl Redactor {
    /// Loads nonempty values only from explicitly named environment variables.
    pub fn from_environment(names: &[String]) -> Self {
        let secrets = names
            .iter()
            .filter_map(|name| std::env::var(name).ok())
            .filter(|value| !value.is_empty())
            .collect();
        Self { secrets }
    }

    /// Redacts secret-bearing keys and exact secret substrings recursively.
    pub fn redact(&self, value: &mut Value) {
        match value {
            Value::Object(fields) => {
                for (key, child) in fields {
                    if sensitive_key(key) {
                        *child = Value::String("[REDACTED]".to_owned());
                    } else {
                        self.redact(child);
                    }
                }
            }
            Value::Array(values) => {
                for child in values {
                    self.redact(child);
                }
            }
            Value::String(text) => {
                for secret in &self.secrets {
                    *text = text.replace(secret, "[REDACTED]");
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }
}

/// Durable external-effect lifecycle.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectState {
    /// Recorded but not attempted.
    Planned,
    /// Execution may have reached the external service.
    Started,
    /// External state was checked successfully.
    Verified,
    /// Outcome cannot be determined safely.
    Uncertain,
}

/// Validates one monotonic effect transition.
pub fn transition(current: Option<EffectState>, next: EffectState) -> Result<EffectState> {
    let valid = matches!(
        (current, next),
        (None, EffectState::Planned)
            | (
                Some(EffectState::Planned),
                EffectState::Planned | EffectState::Started
            )
            | (
                Some(EffectState::Started),
                EffectState::Started | EffectState::Verified | EffectState::Uncertain
            )
            | (Some(EffectState::Verified), EffectState::Verified)
            | (
                Some(EffectState::Uncertain),
                EffectState::Uncertain | EffectState::Verified
            )
    );
    if !valid {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "external effect transition would repeat or weaken authority evidence",
        ));
    }
    Ok(next)
}

/// Parent answer bound to the exact normalized action it reviewed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ApprovalDecision {
    /// SHA-256 of the normalized requested action.
    pub action_sha256: String,
    /// Parent decision.
    pub approved: bool,
}

/// Creates a decision that cannot authorize a different action later.
pub fn bind_approval(action: &Value, approved: bool) -> Result<ApprovalDecision> {
    Ok(ApprovalDecision {
        action_sha256: crate::util::sha256(&serde_json::to_vec(action)?)?,
        approved,
    })
}

/// Checks both the answer and its exact action fingerprint.
pub fn authorize(decision: &ApprovalDecision, action: &Value) -> Result<()> {
    let current = crate::util::sha256(&serde_json::to_vec(action)?)?;
    if !decision.approved || decision.action_sha256 != current {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "approval is denied or belongs to a different action",
        ));
    }
    Ok(())
}

fn sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    ["secret", "password", "authorization", "api_key"]
        .iter()
        .any(|marker| lower.contains(marker))
        || lower == "token"
        || lower.ends_with("_token")
}

#[cfg(test)]
mod tests {
    use super::{EffectState, Redactor, authorize, bind_approval, transition};
    use serde_json::json;

    #[test]
    fn redacts_nested_secret_keys() {
        let mut value = json!({"request":{"api_token":"unique-marker"}});
        Redactor::default().redact(&mut value);
        assert_eq!(
            value.pointer("/request/api_token"),
            Some(&json!("[REDACTED]"))
        );
    }

    #[test]
    fn verified_effect_cannot_restart() {
        assert!(transition(Some(EffectState::Verified), EffectState::Started).is_err());
    }

    #[test]
    fn stale_approval_cannot_authorize_changed_action() -> Result<(), Box<dyn std::error::Error>> {
        let original = json!({"command":"cargo test"});
        let changed = json!({"command":"cargo publish"});
        let decision = bind_approval(&original, true)?;
        authorize(&decision, &original)?;
        assert!(authorize(&decision, &changed).is_err());
        Ok(())
    }

    #[test]
    fn usage_counters_are_not_secret_tokens() {
        let mut value = json!({"input_tokens":42,"access_token":"secret"});
        Redactor::default().redact(&mut value);
        assert_eq!(value.get("input_tokens"), Some(&json!(42)));
        assert_eq!(value.get("access_token"), Some(&json!("[REDACTED]")));
    }
}
