use super::{LocalConfig, write_new, write_replace};
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::DEFAULT_MODEL;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn config_is_private_read_only_and_replacement_is_guarded() -> Result<()> {
    let root = temporary("private")?;
    std::fs::create_dir_all(&root)?;
    let path = root.join(".spewer/config.json");
    let config = LocalConfig::defaults(root.clone())?;
    write_new(&path, &config)?;
    let loaded = LocalConfig::load_from(&path)?;
    let request = loaded.infer_question("What is two plus two?", None)?;
    assert_eq!(request.engine.model, DEFAULT_MODEL);
    assert_eq!(request.permissions.filesystem, "read-only");
    assert!(
        matches!(write_new(&path, &config), Err(error) if error.kind() == ErrorKind::InvalidInput)
    );
    let digest = crate::util::sha256(&std::fs::read(&path)?)?;
    let mut replacement = config.clone();
    replacement.budgets.tokens = 200_000;
    write_replace(&path, &replacement, &digest)?;
    assert_eq!(LocalConfig::load_from(&path)?.budgets.tokens, 200_000);
    assert!(matches!(
        write_replace(&path, &config, &digest),
        Err(error) if error.kind() == ErrorKind::InvalidInput
    ));
    assert_private(&path)?;
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(unix)]
fn assert_private(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let file_mode = std::fs::metadata(path)?.permissions().mode() & 0o777;
    let directory = path
        .parent()
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "config parent missing"))?;
    let directory_mode = std::fs::metadata(directory)?.permissions().mode() & 0o777;
    if file_mode != 0o600 || directory_mode != 0o700 {
        return Err(Error::new(ErrorKind::Io, "configuration is not private"));
    }
    Ok(())
}

#[cfg(not(unix))]
fn assert_private(_path: &Path) -> Result<()> {
    Ok(())
}

fn temporary(name: &str) -> Result<PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| Error::new(ErrorKind::Io, error.to_string()))?
        .as_nanos();
    Ok(std::env::temp_dir().join(format!(
        "spewer-config-{name}-{}-{nanos}",
        std::process::id()
    )))
}
