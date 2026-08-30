use crate::error::{Error, ErrorKind, Result};
use crate::protocol::TaskRequest;
use std::path::{Path, PathBuf};

const MAX_PROJECTED_BYTES: u64 = 1_048_576;

pub(super) async fn build(request: &TaskRequest, workspace: &Path) -> Result<String> {
    let request = request.clone();
    let workspace = workspace.to_owned();
    tokio::task::spawn_blocking(move || build_blocking(&request, &workspace)).await?
}

fn build_blocking(request: &TaskRequest, workspace: &Path) -> Result<String> {
    let acceptance = list_or_none(&request.acceptance);
    let notes = list_or_none(&request.context.notes);
    let files = projected_files(request, workspace)?;
    let skill = request
        .capsule
        .as_ref()
        .and_then(|capsule| capsule.binding.as_ref())
        .and_then(|binding| {
            binding
                .evidence
                .skill
                .as_ref()
                .zip(binding.instructions.as_deref())
        })
        .map_or_else(
            || "(none)".to_owned(),
            |(skill, instructions)| {
                format!(
                    "The parent selected this specialized capsule and explicitly invoked the bound skill '{}'. Apply it to this task.\n\n{instructions}",
                    skill.name
                )
            },
        );
    let authority = if request.permissions.network == "allow" {
        "You may use the read-only web_search tool for current public information. Cite useful source URLs in the final answer. You cannot fetch arbitrary URLs, run commands, or modify files."
    } else {
        "You have no tools and cannot modify files. Use only the supplied task and projected context."
    };
    Ok(format!(
        "You are a bounded local inference worker. {authority} Return a direct answer without claiming that you ran commands or changed the workspace.\n\nObjective:\n{}\n\nAcceptance criteria:\n{}\n\nConstraints:\n{}\n\nBound skill instructions:\n{}\n\nProjected files:\n{}\n\n/no_think",
        request.objective, acceptance, notes, skill, files,
    ))
}

fn projected_files(request: &TaskRequest, workspace: &Path) -> Result<String> {
    if request.context.files.is_empty() {
        return Ok("(none)".to_owned());
    }
    let root = std::fs::canonicalize(workspace)?;
    let mut total = 0_u64;
    let mut projected = Vec::new();
    for relative in &request.context.files {
        let candidate = PathBuf::from(&root).join(relative);
        let path = std::fs::canonicalize(candidate)?;
        if !path.starts_with(&root) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("projected file escapes the worktree: {relative}"),
            ));
        }
        let metadata = std::fs::metadata(&path)?;
        if !metadata.is_file() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("projected context is not a file: {relative}"),
            ));
        }
        total = total
            .checked_add(metadata.len())
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "projected context overflow"))?;
        if total > MAX_PROJECTED_BYTES {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "projected file context exceeds 1 MiB",
            ));
        }
        let content = std::fs::read_to_string(path).map_err(|error| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("projected file must be UTF-8 ({relative}): {error}"),
            )
        })?;
        projected.push(format!("--- {relative} ---\n{content}"));
    }
    Ok(projected.join("\n\n"))
}

fn list_or_none(items: &[String]) -> String {
    if items.is_empty() {
        return "(none)".to_owned();
    }
    items
        .iter()
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n")
}
