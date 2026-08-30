use crate::error::{Error, ErrorKind, Result};
use crate::protocol::TaskRequest;
use crate::workspace::Workspace;
use serde_json::{Value, json};

pub(crate) fn thread(request: &TaskRequest, workspace: &Workspace) -> Value {
    json!({
        "model": request.engine.model,
        "cwd": workspace.path,
        "approvalPolicy": "never",
        "sandbox": sandbox_name(request),
        "serviceName": "spewer",
        "config": {
            "features": {
                "default_mode_request_user_input": true
            }
        }
    })
}

pub(crate) fn turn(request: &TaskRequest, workspace: &Workspace, thread_id: &str) -> Result<Value> {
    let mut parameters = json!({
        "threadId": thread_id,
        "input": [{"type":"text", "text": task_prompt(request)}],
        "cwd": workspace.path,
        "approvalPolicy": "never",
        "sandboxPolicy": sandbox_policy(request),
        "model": request.engine.model
    });
    if let Some(effort) = &request.engine.effort {
        let object = parameters.as_object_mut().ok_or_else(|| {
            Error::new(
                ErrorKind::EngineProtocol,
                "turn parameters are not an object",
            )
        })?;
        object.insert("effort".to_owned(), Value::String(effort.clone()));
    }
    Ok(parameters)
}

pub(crate) fn sandbox_name(request: &TaskRequest) -> &str {
    match request.permissions.filesystem.as_str() {
        "read-only" => "read-only",
        "danger-full-access" => "danger-full-access",
        _ => "workspace-write",
    }
}

fn sandbox_policy(request: &TaskRequest) -> Value {
    if request.permissions.filesystem == "read-only" {
        return json!({"type":"readOnly"});
    }
    if request.permissions.filesystem == "danger-full-access" {
        return json!({"type":"dangerFullAccess"});
    }
    json!({
        "type":"workspaceWrite",
        "writableRoots": [],
        "networkAccess": request.permissions.network == "allow"
    })
}

fn task_prompt(request: &TaskRequest) -> String {
    let acceptance = request
        .acceptance
        .iter()
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n");
    let files = request.context.files.join(", ");
    let notes = request.context.notes.join("\n");
    let skill = match request
        .capsule
        .as_ref()
        .and_then(|capsule| capsule.binding.as_ref())
        .and_then(|binding| {
            binding
                .evidence
                .skill
                .as_ref()
                .zip(binding.instructions.as_deref())
        }) {
        Some((skill, instructions)) => format!(
            "\n\nSkill activation:\nThe parent selected this specialized capsule and explicitly invoked the bound skill '{}'. Enter that skill for this task.\n\nBound skill instructions:\n{instructions}",
            skill.name
        ),
        None => String::new(),
    };
    format!(
        "Objective:\n{}\n\nAcceptance criteria:\n{}\n\nProjected files:\n{}\n\nConstraints:\n{}{}\n\nWork only inside the supplied repository. Run focused verification when possible. Finish with a concise summary and the verification you ran.",
        request.objective, acceptance, files, notes, skill
    )
}

#[cfg(test)]
mod tests {
    use super::{sandbox_name, sandbox_policy, task_prompt, thread};
    use crate::capsule::{
        CapsuleBindingSnapshot, CapsuleEvidence, CapsuleKind, CapsuleRequest, SkillAdvertisement,
    };
    use crate::protocol::TaskRequest;
    use crate::workspace::Workspace;
    use serde_json::json;
    use std::path::PathBuf;

    #[test]
    fn app_server_default_mode_can_request_parent_input() -> Result<(), serde_json::Error> {
        let request: TaskRequest =
            serde_json::from_str(include_str!("../../tests/fixtures/task-request.json"))?;
        let workspace = Workspace {
            source_repository: PathBuf::from("/source"),
            path: PathBuf::from("/workspace"),
            base_revision: "revision".to_owned(),
            artifacts_directory: PathBuf::from("/artifacts"),
        };
        assert_eq!(
            thread(&request, &workspace)
                .pointer("/config/features/default_mode_request_user_input"),
            Some(&json!(true))
        );
        Ok(())
    }

    #[test]
    fn maps_explicit_dangerous_authority_to_codex() -> Result<(), serde_json::Error> {
        let mut request: TaskRequest =
            serde_json::from_str(include_str!("../../tests/fixtures/task-request.json"))?;
        request.permissions.filesystem = "danger-full-access".to_owned();
        request.permissions.network = "allow".to_owned();
        assert_eq!(sandbox_name(&request), "danger-full-access");
        assert_eq!(sandbox_policy(&request), json!({"type":"dangerFullAccess"}));
        Ok(())
    }

    #[test]
    fn specialized_capsule_explicitly_activates_its_bound_skill() -> Result<(), serde_json::Error> {
        let mut request: TaskRequest =
            serde_json::from_str(include_str!("../../tests/fixtures/task-request.json"))?;
        request.capsule = Some(CapsuleRequest {
            id: "play-codex".to_owned(),
            revision: "a".repeat(64),
            binding: Some(CapsuleBindingSnapshot {
                evidence: CapsuleEvidence {
                    id: "play-codex".to_owned(),
                    revision: "a".repeat(64),
                    kind: CapsuleKind::Specialized,
                    skill: Some(SkillAdvertisement {
                        name: "play".to_owned(),
                        description: "Run saved procedures".to_owned(),
                        revision: "revision".to_owned(),
                        digest: "b".repeat(64),
                    }),
                },
                instructions: Some("Run the typed Play runtime.".to_owned()),
            }),
        });
        let prompt = task_prompt(&request);
        assert!(prompt.contains("explicitly invoked the bound skill 'play'"));
        assert!(prompt.contains("Run the typed Play runtime."));
        Ok(())
    }
}
