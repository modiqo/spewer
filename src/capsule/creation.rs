use super::{
    CapsuleManifest, DEFAULT_CAPSULE, MANIFEST_VERSION, default_manifest, load_manifest,
    manifest_path, validate_identifier, validate_manifest, write_new_manifest,
};
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::EngineRequest;
use std::path::Path;

pub(super) fn ensure_default_at(root: &Path) -> Result<CapsuleManifest> {
    let path = manifest_path(root, DEFAULT_CAPSULE)?;
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_file() => load_manifest(&path),
        Ok(_metadata) => Err(Error::new(
            ErrorKind::InvalidInput,
            "default capsule path is not a regular file",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let manifest = default_manifest();
            write_new_manifest(root, &path, &manifest)?;
            load_manifest(&path)
        }
        Err(error) => Err(error.into()),
    }
}

pub(super) fn create_at(
    root: &Path,
    capsule_id: &str,
    description: String,
    engine: EngineRequest,
) -> Result<CapsuleManifest> {
    validate_identifier(capsule_id)?;
    let path = manifest_path(root, capsule_id)?;
    match std::fs::symlink_metadata(&path) {
        Ok(_) => {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("capsule {capsule_id} already exists"),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let manifest = CapsuleManifest {
        version: MANIFEST_VERSION,
        id: capsule_id.to_owned(),
        description,
        engine,
        skill: None,
    };
    validate_manifest(&manifest)?;
    write_new_manifest(root, &path, &manifest)?;
    let persisted = load_manifest(&path)?;
    if persisted != manifest {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("capsule {capsule_id} was created concurrently"),
        ));
    }
    Ok(persisted)
}
