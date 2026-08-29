//! Codex executable discovery shared by direct and detached runs.

use std::path::PathBuf;

pub(super) fn default_program() -> PathBuf {
    if let Some(path) = executable_on_path("codex") {
        return path;
    }
    if let Some(directory) = std::env::var_os("CODEX_INSTALL_DIR") {
        let candidate = PathBuf::from(directory).join("codex");
        if candidate.is_file() {
            return candidate;
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let candidate = PathBuf::from(home).join(".local/bin/codex");
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from("codex")
}

fn executable_on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|directory| directory.join(name))
            .find(|candidate| candidate.is_file())
    })
}
