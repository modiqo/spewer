//! Public task selection and durable binding evidence.

use super::{CapsuleKind, MAX_SKILL_BYTES, SkillAdvertisement, validate_identifier};
use crate::error::{Error, ErrorKind, Result};
use serde::{Deserialize, Serialize};

/// Capsule identity selected by a harness for one task.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CapsuleRequest {
    /// Stable capsule identifier.
    pub id: String,
    /// Exact advertised capsule revision.
    pub revision: String,
    /// Spewer-populated execution snapshot retained with accepted work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) binding: Option<CapsuleBindingSnapshot>,
}

/// Private execution snapshot persisted with an accepted task.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CapsuleBindingSnapshot {
    /// Safe identity copied into the terminal receipt.
    pub evidence: CapsuleEvidence,
    /// Exact specialized instructions supplied to the worker.
    pub instructions: Option<String>,
}

/// Safe capsule identity retained in a receipt.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CapsuleEvidence {
    /// Stable capsule identifier.
    pub id: String,
    /// Exact capsule content revision.
    pub revision: String,
    /// Generic or specialized classification.
    pub kind: CapsuleKind,
    /// Bound skill identity when specialized.
    pub skill: Option<SkillAdvertisement>,
}

impl CapsuleRequest {
    pub(crate) fn validate(&self) -> Result<()> {
        validate_identifier(&self.id)?;
        validate_digest("capsule revision", &self.revision)?;
        if let Some(binding) = &self.binding {
            if binding.evidence.id != self.id || binding.evidence.revision != self.revision {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    "capsule binding does not match its selection",
                ));
            }
            if binding.instructions.as_ref().is_some_and(|instructions| {
                u64::try_from(instructions.len()).map_or(true, |length| length > MAX_SKILL_BYTES)
            }) {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    "capsule instructions exceed 1 MiB",
                ));
            }
            match binding.evidence.kind {
                CapsuleKind::Generic
                    if binding.evidence.skill.is_some() || binding.instructions.is_some() =>
                {
                    return Err(Error::new(
                        ErrorKind::InvalidInput,
                        "generic capsule binding contains specialized content",
                    ));
                }
                CapsuleKind::Specialized
                    if binding.evidence.skill.is_none() || binding.instructions.is_none() =>
                {
                    return Err(Error::new(
                        ErrorKind::InvalidInput,
                        "specialized capsule binding is incomplete",
                    ));
                }
                _ => {}
            }
            if let Some(skill) = &binding.evidence.skill {
                validate_digest("skill digest", &skill.digest)?;
            }
        }
        Ok(())
    }
}

pub(super) fn validate_digest(field: &str, digest: &str) -> Result<()> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("{field} must be a lowercase SHA-256 digest"),
        ));
    }
    Ok(())
}
