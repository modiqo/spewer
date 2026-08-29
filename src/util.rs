use crate::error::{Error, ErrorKind, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

pub(crate) fn now() -> Result<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))
}

pub(crate) fn new_id(prefix: &str) -> Result<String> {
    let timestamp = OffsetDateTime::now_utc().unix_timestamp_nanos();
    let counter = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    let material = format!("{prefix}:{timestamp}:{}:{counter}", std::process::id());
    let digest = sha256(material.as_bytes())?;
    let suffix = digest
        .get(..24)
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "identifier digest is too short"))?;
    Ok(format!("{prefix}_{suffix}"))
}

pub(crate) fn sha256(bytes: &[u8]) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}")
            .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?;
    }
    Ok(encoded)
}

pub(crate) fn required_pointer(value: &Value, pointer: &str) -> Result<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| Error::new(ErrorKind::EngineProtocol, format!("missing {pointer}")))
}

pub(crate) fn data_root() -> Result<PathBuf> {
    if let Some(root) = std::env::var_os("SPEWER_HOME") {
        return Ok(PathBuf::from(root));
    }
    let home = std::env::var_os("HOME")
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "HOME or SPEWER_HOME is required"))?;
    Ok(PathBuf::from(home).join(".local/share/spewer"))
}
