//! Durable worker descriptions advertised through live capability lookup.

mod advertisement;
mod binding;
mod creation;
mod selection;

pub use selection::{CapsuleBindingSnapshot, CapsuleEvidence, CapsuleRequest};

use crate::error::{Error, ErrorKind, Result};
use crate::protocol::{DEFAULT_MODEL, EngineRequest};
use crate::util::{data_root, new_id, sha256};
use serde::{Deserialize, Serialize};
use std::fs::{OpenOptions, read_dir};
use std::io::Write;
use std::path::{Path, PathBuf};

use advertisement::advertisement;
use creation::{create_at, ensure_default_at};

const MANIFEST_VERSION: u32 = 1;
const MAX_SKILL_BYTES: u64 = 1_048_576;
const DEFAULT_CAPSULE: &str = "default";

/// Persisted description of one dispatchable worker.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CapsuleManifest {
    /// Manifest schema version.
    pub version: u32,
    /// Stable capsule identifier.
    pub id: String,
    /// Human-readable worker purpose.
    pub description: String,
    /// Engine selected for this worker.
    pub engine: EngineRequest,
    /// Optional skill that specializes the worker.
    pub skill: Option<SkillBinding>,
}

/// Persisted identity of one bound skill.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SkillBinding {
    /// Skill name from `SKILL.md` front matter.
    pub name: String,
    /// Short routing description from `SKILL.md` front matter.
    pub description: String,
    /// Declared version or deterministic digest prefix.
    pub revision: String,
    /// SHA-256 digest of the complete skill file.
    pub digest: String,
    /// Canonical local `SKILL.md` path, never advertised remotely.
    pub source: String,
}

/// Public classification of a capsule.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapsuleKind {
    /// No skill is bound.
    Generic,
    /// A named, revision-bound skill is present.
    Specialized,
}

/// Public skill identity safe to expose to a harness adapter.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SkillAdvertisement {
    /// Bound skill name.
    pub name: String,
    /// Short routing description.
    pub description: String,
    /// Bound skill revision.
    pub revision: String,
    /// Bound content digest.
    pub digest: String,
}

/// Public worker description returned by capability lookup.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CapsuleAdvertisement {
    /// Stable capsule identifier.
    pub id: String,
    /// Content revision used to bind task selection.
    pub revision: String,
    /// Generic or specialized state derived from the binding.
    pub kind: CapsuleKind,
    /// Human-readable worker purpose.
    pub description: String,
    /// Engine selected for the capsule.
    pub engine: EngineRequest,
    /// Whether the adapter can provide network access when a task authorizes it.
    #[serde(default)]
    pub network: bool,
    /// Tool categories the adapter can provide when a task authorizes them.
    #[serde(default)]
    pub tools: Vec<String>,
    /// Bound skill identity when specialized.
    pub skill: Option<SkillAdvertisement>,
}

/// Content-addressed view of every installed capsule.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CapsuleCatalog {
    /// SHA-256 of the sorted public advertisements.
    pub revision: String,
    /// Capsules sorted by identifier.
    pub capsules: Vec<CapsuleAdvertisement>,
}

/// Creates an unbound selection from the current catalog.
pub fn select(capsule_id: &str) -> Result<CapsuleRequest> {
    binding::select_at(&catalog_root()?, capsule_id)
}

pub(crate) fn resolve_external_request(request: &mut crate::protocol::TaskRequest) -> Result<()> {
    binding::resolve_external_at(&catalog_root()?, request)
}

pub(crate) fn ensure_request_bound(request: &mut crate::protocol::TaskRequest) -> Result<()> {
    binding::ensure_bound_at(&catalog_root()?, request)
}

pub(crate) fn receipt_evidence(request: &crate::protocol::TaskRequest) -> Option<CapsuleEvidence> {
    request
        .capsule
        .as_ref()
        .and_then(|capsule| capsule.binding.as_ref())
        .map(|binding| binding.evidence.clone())
}

/// Ensures the default generic Luna capsule is persisted.
pub fn ensure_default() -> Result<CapsuleManifest> {
    ensure_default_at(&catalog_root()?)
}

/// Reads a fresh public catalog from disk.
pub fn catalog() -> Result<CapsuleCatalog> {
    catalog_at(&catalog_root()?)
}

/// Creates one additional generic capsule without changing existing capsules.
pub fn create(
    capsule_id: &str,
    description: String,
    engine: EngineRequest,
) -> Result<CapsuleManifest> {
    create_at(&catalog_root()?, capsule_id, description, engine)
}

/// Binds one `SKILL.md` to an existing capsule.
pub fn bind_skill(capsule_id: &str, source: &Path) -> Result<CapsuleManifest> {
    bind_skill_at(&catalog_root()?, capsule_id, source)
}

/// Removes a capsule's skill binding while preserving the worker.
pub fn unbind_skill(capsule_id: &str) -> Result<CapsuleManifest> {
    unbind_skill_at(&catalog_root()?, capsule_id)
}

fn catalog_root() -> Result<PathBuf> {
    Ok(data_root()?.join("capsules"))
}

fn default_manifest() -> CapsuleManifest {
    CapsuleManifest {
        version: MANIFEST_VERSION,
        id: DEFAULT_CAPSULE.to_owned(),
        description: "General bounded work through Codex App Server".to_owned(),
        engine: EngineRequest {
            kind: "codex-app-server".to_owned(),
            model: DEFAULT_MODEL.to_owned(),
            effort: None,
        },
        skill: None,
    }
}

fn catalog_at(root: &Path) -> Result<CapsuleCatalog> {
    let mut manifests = Vec::new();
    match read_dir(root) {
        Ok(entries) => {
            for entry in entries {
                let path = entry?.path();
                if path.extension().and_then(std::ffi::OsStr::to_str) == Some("json") {
                    manifests.push(load_manifest(&path)?);
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            manifests.push(default_manifest());
        }
        Err(error) => return Err(error.into()),
    }
    if manifests.is_empty() {
        manifests.push(default_manifest());
    }
    manifests.sort_by(|left, right| left.id.cmp(&right.id));
    let capsules = manifests
        .iter()
        .map(advertisement)
        .collect::<Result<Vec<_>>>()?;
    let revision = sha256(&serde_json::to_vec(&capsules)?)?;
    Ok(CapsuleCatalog { revision, capsules })
}

fn bind_skill_at(root: &Path, capsule_id: &str, source: &Path) -> Result<CapsuleManifest> {
    validate_identifier(capsule_id)?;
    if capsule_id == DEFAULT_CAPSULE {
        let _default = ensure_default_at(root)?;
    }
    let path = manifest_path(root, capsule_id)?;
    let mut manifest = load_manifest(&path)?;
    manifest.skill = Some(read_skill(source)?);
    write_replace_manifest(root, &path, &manifest)?;
    Ok(manifest)
}

fn unbind_skill_at(root: &Path, capsule_id: &str) -> Result<CapsuleManifest> {
    validate_identifier(capsule_id)?;
    if capsule_id == DEFAULT_CAPSULE {
        let _default = ensure_default_at(root)?;
    }
    let path = manifest_path(root, capsule_id)?;
    let mut manifest = load_manifest(&path)?;
    manifest.skill = None;
    write_replace_manifest(root, &path, &manifest)?;
    Ok(manifest)
}

fn read_skill(source: &Path) -> Result<SkillBinding> {
    let path = if source.is_dir() {
        source.join("SKILL.md")
    } else {
        source.to_owned()
    };
    let path = std::fs::canonicalize(path)?;
    let metadata = std::fs::metadata(&path)?;
    if !metadata.is_file() || metadata.len() > MAX_SKILL_BYTES {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "skill must be a regular SKILL.md no larger than 1 MiB",
        ));
    }
    let bytes = std::fs::read(&path)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| Error::new(ErrorKind::InvalidInput, "SKILL.md is not valid UTF-8"))?;
    let metadata = parse_front_matter(text)?;
    let digest = sha256(&bytes)?;
    let revision = match metadata.version {
        Some(version) => version,
        None => digest
            .get(..12)
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "skill digest is too short"))?
            .to_owned(),
    };
    Ok(SkillBinding {
        name: metadata.name,
        description: metadata.description,
        revision,
        digest,
        source: path_string(&path)?,
    })
}

struct SkillMetadata {
    name: String,
    description: String,
    version: Option<String>,
}

fn parse_front_matter(text: &str) -> Result<SkillMetadata> {
    let mut lines = text.lines();
    if lines.next() != Some("---") {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "SKILL.md must start with YAML front matter",
        ));
    }
    let mut name = None;
    let mut description = None;
    let mut version = None;
    let mut closed = false;
    for line in lines {
        if line.trim() == "---" {
            closed = true;
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            let value = unquote(value.trim());
            match key.trim() {
                "name" if !value.is_empty() => name = Some(value.to_owned()),
                "description" if !value.is_empty() => description = Some(value.to_owned()),
                "version" if !value.is_empty() => version = Some(value.to_owned()),
                _ => {}
            }
        }
    }
    if !closed {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "SKILL.md front matter is not closed",
        ));
    }
    Ok(SkillMetadata {
        name: name.ok_or_else(|| Error::new(ErrorKind::InvalidInput, "skill name is missing"))?,
        description: description
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "skill description is missing"))?,
        version,
    })
}

fn unquote(value: &str) -> &str {
    let quoted = value
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|inner| inner.strip_suffix('\''))
        });
    match quoted {
        Some(inner) => inner,
        None => value,
    }
}

fn load_manifest(path: &Path) -> Result<CapsuleManifest> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            Error::new(
                ErrorKind::InvalidInput,
                format!("capsule not found at {}", path.display()),
            )
        } else {
            error.into()
        }
    })?;
    if !metadata.file_type().is_file() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("capsule manifest is not a regular file: {}", path.display()),
        ));
    }
    let bytes = std::fs::read(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            Error::new(
                ErrorKind::InvalidInput,
                format!("capsule not found at {}", path.display()),
            )
        } else {
            error.into()
        }
    })?;
    let manifest: CapsuleManifest = serde_json::from_slice(&bytes)?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn validate_manifest(manifest: &CapsuleManifest) -> Result<()> {
    if manifest.version != MANIFEST_VERSION {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "unsupported capsule version",
        ));
    }
    validate_identifier(&manifest.id)?;
    if manifest.description.trim().is_empty()
        || manifest.engine.kind.trim().is_empty()
        || manifest.engine.model.trim().is_empty()
    {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "capsule fields cannot be empty",
        ));
    }
    Ok(())
}

fn validate_identifier(identifier: &str) -> Result<()> {
    let valid = !identifier.is_empty()
        && identifier.len() <= 64
        && identifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if !valid {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "capsule id must use 1-64 letters, digits, hyphens, or underscores",
        ));
    }
    Ok(())
}

fn manifest_path(root: &Path, identifier: &str) -> Result<PathBuf> {
    validate_identifier(identifier)?;
    Ok(root.join(format!("{identifier}.json")))
}

fn write_new_manifest(root: &Path, path: &Path, manifest: &CapsuleManifest) -> Result<()> {
    prepare_root(root)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    private_options(&mut options);
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };
    write_manifest(&mut file, manifest)?;
    sync_directory(root)
}

fn write_replace_manifest(root: &Path, path: &Path, manifest: &CapsuleManifest) -> Result<()> {
    prepare_root(root)?;
    let temporary = root.join(format!(".capsule-{}.tmp", new_id("write")?));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    private_options(&mut options);
    let result = options
        .open(&temporary)
        .map_err(Error::from)
        .and_then(|mut file| write_manifest(&mut file, manifest))
        .and_then(|()| std::fs::rename(&temporary, path).map_err(Error::from))
        .and_then(|()| sync_directory(root));
    if result.is_err() {
        let _removed = std::fs::remove_file(&temporary);
    }
    result
}

fn write_manifest(file: &mut std::fs::File, manifest: &CapsuleManifest) -> Result<()> {
    validate_manifest(manifest)?;
    serde_json::to_writer_pretty(&mut *file, manifest)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn prepare_root(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root)?;
    #[cfg(unix)]
    std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
fn private_options(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn private_options(_options: &mut OpenOptions) {}

fn sync_directory(path: &Path) -> Result<()> {
    std::fs::File::open(path)?.sync_all()?;
    Ok(())
}

fn path_string(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "skill path is not UTF-8"))
}

#[cfg(test)]
mod tests;
