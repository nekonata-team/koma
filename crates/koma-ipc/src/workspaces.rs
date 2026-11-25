use std::path::{Path, PathBuf};

pub const PRIVATE_ENDPOINTS: &str = "private-workspaces";

/// `$KOMA_STATE_DIR`, else `~/.local/state/koma`.
pub fn state_dir(home: Option<&str>) -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("KOMA_STATE_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    home.map(|h| Path::new(h).join(".local/state/koma"))
}

pub fn state_root() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"));
    let dir = state_dir(home.as_deref().ok())
        .ok_or("Set HOME, USERPROFILE, or KOMA_STATE_DIR to use Workspaces")?;
    std::path::absolute(&dir).map_err(|e| format!("Could not resolve {}: {e}", dir.display()))
}
