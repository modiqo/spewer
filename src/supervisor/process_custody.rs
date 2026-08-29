//! Conservative cleanup for App Server processes left by a hard service crash.

use crate::error::{Error, ErrorKind, Result};
use crate::store::UncertainDispatch;

#[cfg(unix)]
use nix::errno::Errno;
#[cfg(unix)]
use nix::sys::signal::{Signal, killpg};
#[cfg(unix)]
use nix::unistd::Pid;
#[cfg(unix)]
use std::path::Path;
#[cfg(unix)]
use std::time::Duration;

pub(super) async fn reap(dispatch: &UncertainDispatch) -> Result<String> {
    let Some(process_group) = dispatch.process_group else {
        return Ok("no registered App Server process".to_owned());
    };
    #[cfg(unix)]
    {
        let signature = dispatch.process_signature.clone().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                "registered process has no executable signature",
            )
        })?;
        tokio::task::spawn_blocking(move || reap_unix(process_group, &signature)).await?
    }
    #[cfg(not(unix))]
    {
        let _ignored = process_group;
        Err(Error::new(
            ErrorKind::InvalidInput,
            "automatic orphan cleanup is unavailable on this platform",
        ))
    }
}

#[cfg(unix)]
fn reap_unix(process_group: u32, signature: &str) -> Result<String> {
    let pid = i32::try_from(process_group)
        .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?;
    let pid = Pid::from_raw(pid);
    if !alive(pid)? {
        return Ok("registered App Server process had already exited".to_owned());
    }
    verify_signature(process_group, signature)?;
    signal(pid, Signal::SIGTERM)?;
    for _attempt in 0..20 {
        if !alive(pid)? {
            return Ok("registered App Server process group was terminated".to_owned());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    signal(pid, Signal::SIGKILL)?;
    Ok("registered App Server process group required forced termination".to_owned())
}

#[cfg(unix)]
fn verify_signature(process_group: u32, signature: &str) -> Result<()> {
    let output = std::process::Command::new("ps")
        .args(["-p", &process_group.to_string(), "-o", "command="])
        .output()?;
    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::Io,
            "could not inspect registered App Server process",
        ));
    }
    let command = String::from_utf8_lossy(&output.stdout);
    let basename = Path::new(signature)
        .file_name()
        .and_then(|value| value.to_str())
        .map_or(signature, |value| value);
    if command.contains(signature) || (!basename.is_empty() && command.contains(basename)) {
        return Ok(());
    }
    Err(Error::new(
        ErrorKind::InvalidInput,
        "registered process identity no longer matches the App Server executable",
    ))
}

#[cfg(unix)]
fn alive(process_group: Pid) -> Result<bool> {
    match killpg(process_group, None) {
        Ok(()) | Err(Errno::EPERM) => Ok(true),
        Err(Errno::ESRCH) => Ok(false),
        Err(error) => Err(Error::new(ErrorKind::Io, error.to_string())),
    }
}

#[cfg(unix)]
fn signal(process_group: Pid, signal: Signal) -> Result<()> {
    match killpg(process_group, signal) {
        Ok(()) | Err(Errno::ESRCH) => Ok(()),
        Err(error) => Err(Error::new(ErrorKind::Io, error.to_string())),
    }
}
