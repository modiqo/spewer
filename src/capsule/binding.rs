//! Acceptance-time capsule resolution and immutable instruction snapshots.

use super::selection::validate_digest;
use super::{
    CapsuleBindingSnapshot, CapsuleEvidence, CapsuleRequest, SkillBinding, advertisement,
    load_manifest, manifest_path, validate_identifier,
};
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::TaskRequest;
use crate::util::sha256;
use std::path::Path;

pub(super) fn select_at(root: &Path, capsule_id: &str) -> Result<CapsuleRequest> {
    validate_identifier(capsule_id)?;
    let manifest = load_manifest(&manifest_path(root, capsule_id)?)?;
    let advertised = advertisement(&manifest)?;
    Ok(CapsuleRequest {
        id: advertised.id,
        revision: advertised.revision,
        binding: None,
    })
}

pub(super) fn resolve_external_at(root: &Path, request: &mut TaskRequest) -> Result<()> {
    if let Some(capsule) = &mut request.capsule {
        capsule.binding = None;
    }
    ensure_bound_at(root, request)
}

pub(super) fn ensure_bound_at(root: &Path, request: &mut TaskRequest) -> Result<()> {
    let Some(selection) = &request.capsule else {
        return Ok(());
    };
    selection.validate()?;
    if selection.binding.is_some() {
        return Ok(());
    }
    let path = manifest_path(root, &selection.id)?;
    let manifest = load_manifest(&path)?;
    let advertised = advertisement(&manifest)?;
    if advertised.revision != selection.revision {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!(
                "capsule {} changed; refresh capabilities before submitting",
                selection.id
            ),
        ));
    }
    if manifest.engine.kind != request.engine.kind || manifest.engine.model != request.engine.model
    {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "task engine and model do not match the selected capsule",
        ));
    }
    let instructions = manifest.skill.as_ref().map(load_instructions).transpose()?;
    let evidence = CapsuleEvidence {
        id: advertised.id,
        revision: advertised.revision,
        kind: advertised.kind,
        skill: advertised.skill,
    };
    let selection = request
        .capsule
        .as_mut()
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "capsule selection disappeared"))?;
    selection.binding = Some(CapsuleBindingSnapshot {
        evidence,
        instructions,
    });
    Ok(())
}

fn load_instructions(binding: &SkillBinding) -> Result<String> {
    validate_digest("skill digest", &binding.digest)?;
    let path = Path::new(&binding.source);
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "bound skill source is not a regular file",
        ));
    }
    let bytes = std::fs::read(path)?;
    if sha256(&bytes)? != binding.digest {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "bound skill changed; bind it again before submitting",
        ));
    }
    String::from_utf8(bytes)
        .map_err(|_| Error::new(ErrorKind::InvalidInput, "bound skill is not valid UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::{ensure_bound_at, resolve_external_at, select_at};
    use crate::capsule::{bind_skill_at, ensure_default_at, unbind_skill_at};
    use crate::config::LocalConfig;
    use crate::error::{Error, ErrorKind, Result};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn selected_specialization_is_snapshotted_and_stale_edits_fail() -> Result<()> {
        let root = temporary()?;
        let capsules = root.join("capsules");
        let workspace = root.join("workspace");
        let skill = root.join("SKILL.md");
        std::fs::create_dir_all(&workspace)?;
        std::fs::write(
            &skill,
            "---\nname: reviewer\ndescription: Review changes\nversion: 1\n---\nCheck every change.\n",
        )?;
        let _default = ensure_default_at(&capsules)?;
        let _bound = bind_skill_at(&capsules, "default", &skill)?;
        let selection = select_at(&capsules, "default")?;
        let mut request = LocalConfig::defaults(workspace)?.infer_question("Review it", None)?;
        request.capsule = Some(selection.clone());
        ensure_bound_at(&capsules, &mut request)?;
        let snapshot = request
            .capsule
            .as_ref()
            .and_then(|capsule| capsule.binding.as_ref())
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "binding missing"))?;
        assert!(
            snapshot
                .instructions
                .as_deref()
                .is_some_and(|text| text.contains("Check every change."))
        );
        if let Some(instructions) = request
            .capsule
            .as_mut()
            .and_then(|capsule| capsule.binding.as_mut())
            .and_then(|binding| binding.instructions.as_mut())
        {
            *instructions = "Injected caller instructions.".to_owned();
        }
        resolve_external_at(&capsules, &mut request)?;
        assert!(
            request
                .capsule
                .as_ref()
                .and_then(|capsule| capsule.binding.as_ref())
                .and_then(|binding| binding.instructions.as_deref())
                .is_some_and(|text| text.contains("Check every change."))
        );

        let _unbound = unbind_skill_at(&capsules, "default")?;
        let mut stale_revision =
            LocalConfig::defaults(root.join("workspace"))?.infer_question("Review it", None)?;
        stale_revision.capsule = Some(selection.clone());
        assert!(ensure_bound_at(&capsules, &mut stale_revision).is_err());

        let _rebound = bind_skill_at(&capsules, "default", &skill)?;
        let current = select_at(&capsules, "default")?;
        std::fs::write(&skill, "changed")?;
        let mut stale =
            LocalConfig::defaults(root.join("workspace"))?.infer_question("Review it", None)?;
        stale.capsule = Some(current);
        assert!(ensure_bound_at(&capsules, &mut stale).is_err());
        ensure_bound_at(&capsules, &mut request)?;
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    fn temporary() -> Result<PathBuf> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| Error::new(ErrorKind::Io, error.to_string()))?
            .as_nanos();
        Ok(std::env::temp_dir().join(format!("spewer-binding-{}-{nanos}", std::process::id())))
    }
}
