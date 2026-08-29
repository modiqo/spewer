//! Owner-private defaults used to infer one-off question tasks.

use crate::error::{Error, ErrorKind, Result};
use crate::protocol::{
    Budgets, CallbackRequest, DEFAULT_MODEL, EngineRequest, PROTOCOL_VERSION, Permissions,
    TaskContext, TaskRequest, WorkspaceRequest,
};
use crate::util::new_id;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Current local configuration format.
pub const CONFIG_VERSION: u32 = 1;

/// Persisted defaults for `spewer ask`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LocalConfig {
    /// Configuration format version.
    pub version: u32,
    /// Absolute Git workspace used unless `ask --workspace` overrides it.
    pub default_workspace: String,
    /// App Server engine and model requested for questions.
    pub engine: EngineRequest,
    /// Authority granted to inferred question tasks.
    pub permissions: Permissions,
    /// Hard limits applied to inferred question tasks.
    pub budgets: Budgets,
}

impl LocalConfig {
    /// Builds safe question defaults for one workspace.
    pub fn defaults(workspace: PathBuf) -> Result<Self> {
        let workspace = absolute_workspace(workspace)?;
        let config = Self {
            version: CONFIG_VERSION,
            default_workspace: path_string(&workspace)?,
            engine: EngineRequest {
                kind: "codex-app-server".to_owned(),
                model: DEFAULT_MODEL.to_owned(),
                effort: None,
            },
            permissions: Permissions {
                filesystem: "read-only".to_owned(),
                network: "deny".to_owned(),
                commands: "engine-policy".to_owned(),
                command_allowlist: Vec::new(),
                environment_allowlist: Vec::new(),
                writable_paths: Vec::new(),
            },
            budgets: Budgets {
                wall_seconds: 180,
                tokens: 100_000,
                tool_calls: 20,
                retries: 0,
                cost_usd: 1.0,
            },
        };
        config.validate()?;
        Ok(config)
    }

    /// Loads and validates the configured question defaults.
    pub fn load() -> Result<Self> {
        Self::load_from(&config_path()?)
    }

    /// Converts a question into the complete public task protocol.
    pub fn infer_question(
        &self,
        question: &str,
        workspace: Option<PathBuf>,
    ) -> Result<TaskRequest> {
        self.validate()?;
        let question = question.trim();
        if question.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "ask requires a question",
            ));
        }
        let workspace = match workspace {
            Some(path) => absolute_workspace(path)?,
            None => absolute_workspace(PathBuf::from(&self.default_workspace))?,
        };
        let task_id = new_id("tsk")?;
        let request = TaskRequest {
            protocol_version: PROTOCOL_VERSION.to_owned(),
            task_id: Some(task_id),
            idempotency_key: new_id("ask")?,
            objective: question.to_owned(),
            acceptance: vec!["Answer the question directly and accurately.".to_owned()],
            workspace: WorkspaceRequest {
                path: path_string(&workspace)?,
                base_revision: None,
            },
            context: TaskContext {
                files: Vec::new(),
                notes: vec!["Inferred by spewer ask; do not modify workspace files.".to_owned()],
            },
            capsule: None,
            permissions: self.permissions.clone(),
            budgets: self.budgets.clone(),
            engine: self.engine.clone(),
            callback: CallbackRequest {
                mode: "wait".to_owned(),
                consumer_id: Some("spewer-ask".to_owned()),
            },
            private_continuation: None,
        };
        request.validate()?;
        Ok(request)
    }

    fn load_from(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Error::new(
                    ErrorKind::InvalidInput,
                    format!(
                        "configuration not found at {}; run 'spewer init'",
                        path.display()
                    ),
                )
            } else {
                error.into()
            }
        })?;
        let config: Self = serde_json::from_slice(&bytes)?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if self.version != CONFIG_VERSION {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "unsupported local configuration version",
            ));
        }
        if self.permissions.filesystem != "read-only" || !self.permissions.writable_paths.is_empty()
        {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "ask configuration must remain read-only",
            ));
        }
        if !Path::new(&self.default_workspace).is_absolute() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "default_workspace must be absolute",
            ));
        }
        let probe = self.invariant_probe();
        probe.validate()?;
        Ok(())
    }

    fn invariant_probe(&self) -> TaskRequest {
        TaskRequest {
            protocol_version: PROTOCOL_VERSION.to_owned(),
            task_id: None,
            idempotency_key: "config-validation".to_owned(),
            objective: "config validation".to_owned(),
            acceptance: Vec::new(),
            workspace: WorkspaceRequest {
                path: self.default_workspace.clone(),
                base_revision: None,
            },
            context: TaskContext::default(),
            capsule: None,
            permissions: self.permissions.clone(),
            budgets: self.budgets.clone(),
            engine: self.engine.clone(),
            callback: CallbackRequest {
                mode: "wait".to_owned(),
                consumer_id: Some("spewer-ask".to_owned()),
            },
            private_continuation: None,
        }
    }
}

/// Returns the current configuration digest when a regular file exists.
pub fn existing_digest() -> Result<Option<String>> {
    let path = config_path()?;
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_file() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!(
                "configuration path is not a regular file: {}",
                path.display()
            ),
        ));
    }
    Ok(Some(crate::util::sha256(&std::fs::read(path)?)?))
}

/// Creates defaults or replaces the exact configuration digest approved by the caller.
pub fn initialize(
    workspace: Option<PathBuf>,
    expected_existing: Option<String>,
) -> Result<PathBuf> {
    let workspace = match workspace {
        Some(path) => path,
        None => std::env::current_dir()?,
    };
    let config = LocalConfig::defaults(workspace)?;
    let path = config_path()?;
    match expected_existing {
        Some(digest) => write_replace(&path, &config, &digest)?,
        None => write_new(&path, &config)?,
    }
    Ok(path)
}

/// Returns `SPEWER_CONFIG` or `~/.spewer/config.json`.
pub fn config_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("SPEWER_CONFIG") {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME")
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "HOME or SPEWER_CONFIG is required"))?;
    Ok(PathBuf::from(home).join(".spewer/config.json"))
}

fn write_new(path: &Path, config: &LocalConfig) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "configuration path has no parent"))?;
    std::fs::create_dir_all(parent)?;
    make_private_directory(parent)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    private_file_options(&mut options);
    let mut file = options.open(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            Error::new(
                ErrorKind::InvalidInput,
                format!(
                    "configuration already exists at {}; edit it or remove it explicitly",
                    path.display()
                ),
            )
        } else {
            error.into()
        }
    })?;
    serde_json::to_writer_pretty(&mut file, config)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn write_replace(path: &Path, config: &LocalConfig, expected: &str) -> Result<()> {
    ensure_digest(path, expected)?;
    let parent = path
        .parent()
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "configuration path has no parent"))?;
    make_private_directory(parent)?;
    let name = path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "configuration name is not UTF-8"))?;
    let temporary = parent.join(format!(".{name}.{}.tmp", new_id("cfg")?));
    let result = write_temporary(&temporary, config)
        .and_then(|()| ensure_digest(path, expected))
        .and_then(|()| std::fs::rename(&temporary, path).map_err(Error::from))
        .and_then(|()| sync_directory(parent));
    if result.is_err() {
        let _removed = std::fs::remove_file(&temporary);
    }
    result
}

fn write_temporary(path: &Path, config: &LocalConfig) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    private_file_options(&mut options);
    let mut file = options.open(path)?;
    serde_json::to_writer_pretty(&mut file, config)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn ensure_digest(path: &Path, expected: &str) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "configuration changed before replacement",
        ));
    }
    let observed = crate::util::sha256(&std::fs::read(path)?)?;
    if observed != expected {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "configuration changed after confirmation; retry init --overwrite",
        ));
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<()> {
    std::fs::File::open(path)?.sync_all()?;
    Ok(())
}

fn absolute_workspace(path: PathBuf) -> Result<PathBuf> {
    Ok(std::fs::canonicalize(path)?)
}

fn path_string(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "workspace path is not UTF-8"))
}

#[cfg(unix)]
fn make_private_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn make_private_directory(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn private_file_options(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn private_file_options(_options: &mut OpenOptions) {}

#[cfg(test)]
mod tests;
