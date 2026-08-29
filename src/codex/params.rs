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
        "serviceName": "spewer"
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

fn sandbox_name(request: &TaskRequest) -> &str {
    if request.permissions.filesystem == "read-only" {
        "read-only"
    } else {
        "workspace-write"
    }
}

fn sandbox_policy(request: &TaskRequest) -> Value {
    if request.permissions.filesystem == "read-only" {
        return json!({"type":"readOnly"});
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
    format!(
        "Objective:\n{}\n\nAcceptance criteria:\n{}\n\nProjected files:\n{}\n\nConstraints:\n{}\n\nWork only inside the supplied repository. Run focused verification when possible. Finish with a concise summary and the verification you ran.",
        request.objective, acceptance, files, notes
    )
}
