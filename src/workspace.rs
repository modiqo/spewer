//! Isolated Git worktrees and immutable diff artifacts.

use crate::error::{Error, ErrorKind, Result};
use crate::protocol::{Artifact, TaskRequest};
use crate::util::{data_root, sha256};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// A task-scoped isolated worktree.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Workspace {
    /// Canonical source repository.
    pub source_repository: PathBuf,
    /// Isolated worktree path passed to the engine.
    pub path: PathBuf,
    /// Immutable starting commit.
    pub base_revision: String,
    /// Content-addressed artifact directory.
    pub artifacts_directory: PathBuf,
}

/// Diff and path-boundary evidence captured after a run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceEvidence {
    /// SHA-256 hash of the binary Git diff.
    pub diff_hash: String,
    /// Changed paths relative to the worktree.
    pub changed_paths: Vec<String>,
    /// Immutable diff artifact.
    pub artifact: Artifact,
}

impl Workspace {
    /// Creates a detached worktree below Spewer's data directory.
    pub async fn prepare(request: &TaskRequest, task_id: &str) -> Result<Self> {
        let requested = PathBuf::from(&request.workspace.path);
        let source_repository = blocking_path(move || std::fs::canonicalize(requested)).await?;
        reject_broad_root(&source_repository)?;
        let top_level = git_text(&source_repository, &["rev-parse", "--show-toplevel"]).await?;
        let git_root = blocking_path(move || std::fs::canonicalize(top_level.trim())).await?;
        if source_repository != git_root {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "workspace.path must name the repository root",
            ));
        }
        let requested_revision = match &request.workspace.base_revision {
            Some(revision) => revision.as_str(),
            None => "HEAD",
        };
        let revision_arg = format!("{requested_revision}^{{commit}}");
        let base_revision = git_text(&source_repository, &["rev-parse", &revision_arg])
            .await?
            .trim()
            .to_owned();
        let data_root = data_root()?;
        let worktrees = data_root.join("workspaces");
        let artifacts_directory = data_root.join("artifacts");
        let directories = vec![worktrees.clone(), artifacts_directory.clone()];
        blocking_unit(move || {
            for directory in directories {
                std::fs::create_dir_all(directory)?;
            }
            Ok(())
        })
        .await?;
        let path = worktrees.join(task_id);
        if path.exists() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("worktree already exists at {}", path.display()),
            ));
        }
        git_status(
            &source_repository,
            &[
                "worktree",
                "add",
                "--detach",
                path_text(&path)?,
                &base_revision,
            ],
        )
        .await?;
        Ok(Self {
            source_repository,
            path,
            base_revision,
            artifacts_directory,
        })
    }

    /// Reopens a retained isolated worktree after checkpoint validation.
    pub async fn restore(
        request: &TaskRequest,
        projection: &crate::reducer::Projection,
    ) -> Result<Self> {
        let requested = PathBuf::from(&request.workspace.path);
        let source_repository = blocking_path(move || std::fs::canonicalize(requested)).await?;
        reject_broad_root(&source_repository)?;
        let projected = PathBuf::from(&projection.workspace.path);
        let path = blocking_path(move || std::fs::canonicalize(projected)).await?;
        let artifacts_directory = data_root()?.join("artifacts");
        let create = artifacts_directory.clone();
        blocking_unit(move || {
            std::fs::create_dir_all(create)?;
            Ok(())
        })
        .await?;
        Ok(Self {
            source_repository,
            path,
            base_revision: projection.workspace.base_revision.clone(),
            artifacts_directory,
        })
    }

    /// Captures a binary diff and rejects changes outside declared paths.
    pub async fn capture(&self, allowed_paths: &[String]) -> Result<WorkspaceEvidence> {
        git_status(&self.path, &["add", "--intent-to-add", "--all"]).await?;
        let names = git_bytes(&self.path, &["diff", "--name-only", "-z", "HEAD"]).await?;
        let changed_paths = parse_paths(&names)?;
        validate_changed_paths(&changed_paths, allowed_paths)?;
        let diff = git_bytes(&self.path, &["diff", "--binary", "--no-ext-diff", "HEAD"]).await?;
        let diff_hash = sha256(&diff)?;
        let artifact_path = self.artifacts_directory.join(format!("{diff_hash}.diff"));
        let bytes = diff;
        blocking_unit(move || {
            std::fs::write(artifact_path, bytes)?;
            Ok(())
        })
        .await?;
        Ok(WorkspaceEvidence {
            artifact: Artifact {
                kind: "git-diff".to_owned(),
                uri: format!("artifact://sha256/{diff_hash}"),
                sha256: diff_hash.clone(),
            },
            diff_hash,
            changed_paths,
        })
    }
}

fn reject_broad_root(path: &Path) -> Result<()> {
    if path.parent().is_none() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "filesystem root cannot be a workspace",
        ));
    }
    if let Some(home) = std::env::var_os("HOME")
        && path == Path::new(&home)
    {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "home directory cannot be a workspace",
        ));
    }
    Ok(())
}

fn validate_changed_paths(changed: &[String], allowed: &[String]) -> Result<()> {
    if allowed.is_empty() {
        return Ok(());
    }
    for changed_path in changed {
        let path = Path::new(changed_path);
        let permitted = allowed
            .iter()
            .any(|allowed_path| path.starts_with(allowed_path));
        if !permitted {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("worker changed disallowed path {changed_path}"),
            ));
        }
    }
    Ok(())
}

fn parse_paths(bytes: &[u8]) -> Result<Vec<String>> {
    let mut paths = Vec::new();
    for raw in bytes.split(|byte| *byte == 0).filter(|raw| !raw.is_empty()) {
        let path = std::str::from_utf8(raw)
            .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?;
        paths.push(path.to_owned());
    }
    Ok(paths)
}

async fn git_text(directory: &Path, arguments: &[&str]) -> Result<String> {
    let bytes = git_bytes(directory, arguments).await?;
    String::from_utf8(bytes).map_err(|error| Error::new(ErrorKind::Io, error.to_string()))
}

async fn git_bytes(directory: &Path, arguments: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .output()
        .await?;
    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::Io,
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    Ok(output.stdout)
}

async fn git_status(directory: &Path, arguments: &[&str]) -> Result<()> {
    let _output = git_bytes(directory, arguments).await?;
    Ok(())
}

async fn blocking_path<F>(operation: F) -> Result<PathBuf>
where
    F: FnOnce() -> std::io::Result<PathBuf> + Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await?
        .map_err(Error::from)
}

async fn blocking_unit<F>(operation: F) -> Result<()>
where
    F: FnOnce() -> std::io::Result<()> + Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await?
        .map_err(Error::from)
}

fn path_text(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "path is not valid UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::validate_changed_paths;

    #[test]
    fn rejects_changes_outside_allowed_paths() {
        let changed = vec!["src/ok.rs".to_owned(), "secrets.txt".to_owned()];
        let allowed = vec!["src".to_owned()];
        assert!(validate_changed_paths(&changed, &allowed).is_err());
    }
}
