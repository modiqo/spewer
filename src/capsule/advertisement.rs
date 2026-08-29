use super::{CapsuleAdvertisement, CapsuleKind, CapsuleManifest, SkillAdvertisement};
use crate::error::Result;
use crate::util::sha256;

pub(super) fn advertisement(manifest: &CapsuleManifest) -> Result<CapsuleAdvertisement> {
    let skill = manifest.skill.as_ref().map(|binding| SkillAdvertisement {
        name: binding.name.clone(),
        description: binding.description.clone(),
        revision: binding.revision.clone(),
        digest: binding.digest.clone(),
    });
    let kind = if skill.is_some() {
        CapsuleKind::Specialized
    } else {
        CapsuleKind::Generic
    };
    let (network, tools) = runtime_capabilities(&manifest.engine.kind);
    let revision = sha256(&serde_json::to_vec(&(
        &manifest.id,
        kind,
        &manifest.description,
        &manifest.engine,
        network,
        &tools,
        &skill,
    ))?)?;
    Ok(CapsuleAdvertisement {
        id: manifest.id.clone(),
        revision,
        kind,
        description: manifest.description.clone(),
        engine: manifest.engine.clone(),
        network,
        tools,
        skill,
    })
}

fn runtime_capabilities(engine: &str) -> (bool, Vec<String>) {
    match engine {
        "codex-app-server" => (true, ["commands", "filesystem"].map(str::to_owned).into()),
        _ => (false, Vec::new()),
    }
}
