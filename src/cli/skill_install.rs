//! Conflict-safe installation of the bundled frontier delegation skill.

use crate::error::{Error, ErrorKind, Result};
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

const SKILL: &[u8] = include_bytes!("../../integrations/codex/spewer-delegation/SKILL.md");

#[derive(Debug, Serialize)]
pub(super) struct SkillInstallReport {
    path: PathBuf,
    created: bool,
    digest: String,
}

pub(super) fn install() -> Result<SkillInstallReport> {
    let root = if let Some(root) = std::env::var_os("CODEX_HOME") {
        PathBuf::from(root)
    } else {
        let home = std::env::var_os("HOME").ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                "HOME or CODEX_HOME is required to install the frontier skill",
            )
        })?;
        PathBuf::from(home).join(".codex")
    };
    install_at(&root)
}

fn install_at(codex_root: &Path) -> Result<SkillInstallReport> {
    let directory = codex_root.join("skills/spewer-delegation");
    let path = directory.join("SKILL.md");
    let digest = crate::util::sha256(SKILL)?;
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            if std::fs::read(&path)? != SKILL {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    format!(
                        "frontier skill differs at {}; review or remove it before installation",
                        path.display()
                    ),
                ));
            }
            return Ok(SkillInstallReport {
                path,
                created: false,
                digest,
            });
        }
        Ok(_metadata) => {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "frontier skill path is not a regular file",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    std::fs::create_dir_all(&directory)?;
    set_private_directory(&directory)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    private_file_options(&mut options);
    let mut file = options.open(&path)?;
    file.write_all(SKILL)?;
    file.sync_all()?;
    std::fs::File::open(&directory)?.sync_all()?;
    Ok(SkillInstallReport {
        path,
        created: true,
        digest,
    })
}

#[cfg(unix)]
fn set_private_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_directory(_path: &Path) -> Result<()> {
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
mod tests {
    use super::install_at;
    use crate::error::Result;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn identical_skill_is_reused_and_changed_skill_is_rejected() -> Result<()> {
        let root = temporary()?;
        let first = install_at(&root)?;
        assert!(first.created);
        let repeated = install_at(&root)?;
        assert!(!repeated.created);
        std::fs::write(&first.path, "changed")?;
        assert!(install_at(&root).is_err());
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    fn temporary() -> Result<PathBuf> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| {
                crate::error::Error::new(crate::error::ErrorKind::Io, error.to_string())
            })?
            .as_nanos();
        Ok(std::env::temp_dir().join(format!("spewer-skill-{}-{nanos}", std::process::id())))
    }
}
