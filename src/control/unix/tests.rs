use super::{bind_socket, remove_socket};
use crate::error::{Error, ErrorKind, Result};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::test(flavor = "current_thread")]
async fn socket_is_private_and_live_service_is_not_replaced() -> Result<()> {
    let path = temporary("socket")?.with_extension("sock");
    let listener = bind_socket(path.clone()).await?;
    let mode = std::fs::metadata(&path)?.permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    assert!(bind_socket(path.clone()).await.is_err());
    drop(listener);
    remove_socket(path).await?;
    Ok(())
}

fn temporary(name: &str) -> Result<PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| Error::new(ErrorKind::Io, error.to_string()))?
        .as_nanos();
    Ok(std::env::temp_dir().join(format!(
        "spewer-control-{name}-{}-{nanos}",
        std::process::id()
    )))
}
