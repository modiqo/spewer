use super::{
    CapsuleKind, bind_skill_at, catalog_at, create_at, ensure_default_at, unbind_skill_at,
};
use crate::error::Result;
use crate::protocol::{DEFAULT_MODEL, EngineRequest};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn default_capsule_specializes_and_returns_to_generic() -> Result<()> {
    let root = temporary("lifecycle")?;
    let skill = root.join("example/SKILL.md");
    let skill_parent = skill.parent().ok_or_else(|| {
        crate::error::Error::new(
            crate::error::ErrorKind::InvalidInput,
            "test path has no parent",
        )
    })?;
    std::fs::create_dir_all(skill_parent)?;
    std::fs::write(
        &skill,
        "---\nname: arithmetic\ndescription: Solve bounded arithmetic\nversion: 2\n---\nDo arithmetic.\n",
    )?;

    let manifest = ensure_default_at(&root.join("capsules"))?;
    assert_eq!(manifest.engine.model, DEFAULT_MODEL);
    let generic = catalog_at(&root.join("capsules"))?;
    let generic_capsule = generic.capsules.first().ok_or_else(|| {
        crate::error::Error::new(crate::error::ErrorKind::InvalidInput, "catalog is empty")
    })?;
    assert_eq!(generic_capsule.kind, CapsuleKind::Generic);
    assert!(generic_capsule.network);
    assert_eq!(generic_capsule.tools, ["commands", "filesystem"]);

    let bound = bind_skill_at(&root.join("capsules"), "default", &skill)?;
    assert_eq!(
        bound.skill.as_ref().map(|skill| skill.name.as_str()),
        Some("arithmetic")
    );
    let specialized = catalog_at(&root.join("capsules"))?;
    let specialized_capsule = specialized.capsules.first().ok_or_else(|| {
        crate::error::Error::new(crate::error::ErrorKind::InvalidInput, "catalog is empty")
    })?;
    assert_eq!(specialized_capsule.kind, CapsuleKind::Specialized);
    assert_ne!(generic.revision, specialized.revision);
    assert_eq!(
        specialized_capsule
            .skill
            .as_ref()
            .map(|skill| skill.revision.as_str()),
        Some("2")
    );

    let _manifest = unbind_skill_at(&root.join("capsules"), "default")?;
    let restored = catalog_at(&root.join("capsules"))?;
    assert_eq!(
        restored.capsules.first().map(|capsule| capsule.kind),
        Some(CapsuleKind::Generic)
    );
    assert_eq!(restored.revision, generic.revision);
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn missing_catalog_has_a_stable_implicit_default() -> Result<()> {
    let root = temporary("implicit")?;
    let first = catalog_at(&root)?;
    let second = catalog_at(&root)?;
    assert_eq!(first, second);
    assert_eq!(first.capsules.len(), 1);
    assert_eq!(
        first.capsules.first().map(|capsule| capsule.kind),
        Some(CapsuleKind::Generic)
    );
    Ok(())
}

#[test]
fn additional_capsule_is_created_once() -> Result<()> {
    let root = temporary("create")?;
    let engine = EngineRequest {
        kind: "ollama".to_owned(),
        model: "qwen3:30b-a3b".to_owned(),
        effort: None,
    };
    let manifest = create_at(
        &root,
        "qwen3-local",
        "Local Qwen3".to_owned(),
        engine.clone(),
    )?;
    assert_eq!(manifest.engine, engine);
    let catalog = catalog_at(&root)?;
    let qwen = catalog
        .capsules
        .iter()
        .find(|capsule| capsule.id == "qwen3-local")
        .ok_or_else(|| {
            crate::error::Error::new(
                crate::error::ErrorKind::InvalidInput,
                "Qwen capsule is missing",
            )
        })?;
    assert!(!qwen.network);
    assert!(qwen.tools.is_empty());
    assert!(create_at(&root, "qwen3-local", "Duplicate".to_owned(), engine).is_err());
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn persisted_catalog_is_owner_private() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let root = temporary("permissions")?.join("capsules");
    let _manifest = ensure_default_at(&root)?;
    assert_eq!(
        std::fs::metadata(&root)?.permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(root.join("default.json"))?
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let parent = root.parent().ok_or_else(|| {
        crate::error::Error::new(
            crate::error::ErrorKind::InvalidInput,
            "test root has no parent",
        )
    })?;
    std::fs::remove_dir_all(parent)?;
    Ok(())
}

fn temporary(name: &str) -> Result<PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| crate::error::Error::new(crate::error::ErrorKind::Io, error.to_string()))?
        .as_nanos();
    Ok(std::env::temp_dir().join(format!(
        "spewer-capsule-{name}-{}-{nanos}",
        std::process::id()
    )))
}
