use std::path::{Path, PathBuf};

pub const PROFILES_DIR: &str = "profiles";
pub const PRIVATE_ENDPOINTS: &str = "private-workspaces";
pub const ENDPOINT_FILE: &str = "endpoint";
pub const INSTANCE_LOCK_FILE: &str = "instance.lock";

fn non_empty_trimmed(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|s| !s.is_empty())
}

fn home_dir_from(
    home: Option<&str>,
    userprofile: Option<&str>,
    is_windows: bool,
) -> Option<PathBuf> {
    let candidates = if is_windows {
        [userprofile, home]
    } else {
        [home, None]
    };
    candidates
        .into_iter()
        .find_map(non_empty_trimmed)
        .map(PathBuf::from)
}

/// Resolves the current user's home directory (`HOME` on Unix; `USERPROFILE` then `HOME` on Windows).
pub fn home_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok();
    #[cfg(windows)]
    let userprofile = std::env::var("USERPROFILE").ok();
    #[cfg(not(windows))]
    let userprofile: Option<String> = None;
    home_dir_from(home.as_deref(), userprofile.as_deref(), cfg!(windows))
}

/// Pure config path resolution:
/// 1. `koma_config` (`$KOMA_CONFIG`)
/// 2. `xdg_config_home` (`$XDG_CONFIG_HOME/koma/config.toml`)
/// 3. `home` (`$HOME/.config/koma/config.toml`)
pub fn resolve_config_path(
    koma_config: Option<&str>,
    xdg_config_home: Option<&str>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(path) = non_empty_trimmed(koma_config) {
        return Some(PathBuf::from(path));
    }
    if let Some(xdg) = non_empty_trimmed(xdg_config_home) {
        return Some(Path::new(xdg).join("koma/config.toml"));
    }
    home.filter(|h| !h.as_os_str().is_empty())
        .map(|h| h.join(".config/koma/config.toml"))
}

/// `$KOMA_CONFIG`, else `$XDG_CONFIG_HOME/koma/config.toml`, else `$HOME/.config/koma/config.toml`.
pub fn config_path() -> Option<PathBuf> {
    let koma_config = std::env::var("KOMA_CONFIG").ok();
    let xdg_config_home = std::env::var("XDG_CONFIG_HOME").ok();
    let home = home_dir();
    resolve_config_path(
        koma_config.as_deref(),
        xdg_config_home.as_deref(),
        home.as_deref(),
    )
}

fn state_dir_from(koma_state_dir: Option<&str>, home: Option<&Path>) -> Option<PathBuf> {
    if let Some(dir) = non_empty_trimmed(koma_state_dir) {
        return Some(PathBuf::from(dir));
    }
    home.filter(|h| !h.as_os_str().is_empty())
        .map(|h| h.join(".local/state/koma"))
}

/// `$KOMA_STATE_DIR`, else `~/.local/state/koma`.
pub fn state_dir(home: Option<&str>) -> Option<PathBuf> {
    let env_dir = std::env::var("KOMA_STATE_DIR").ok();
    state_dir_from(env_dir.as_deref(), non_empty_trimmed(home).map(Path::new))
}

pub fn state_root() -> Result<PathBuf, String> {
    let env_dir = std::env::var("KOMA_STATE_DIR").ok();
    let home = home_dir();
    let dir = state_dir_from(env_dir.as_deref(), home.as_deref())
        .ok_or("Set HOME, USERPROFILE, or KOMA_STATE_DIR to use Workspaces")?;
    std::path::absolute(&dir).map_err(|e| format!("Could not resolve {}: {e}", dir.display()))
}

pub fn profiles_dir(root: &Path) -> PathBuf {
    root.join(PROFILES_DIR)
}

pub fn profile_dir(root: &Path, id: &str) -> PathBuf {
    profiles_dir(root).join(id)
}

pub fn profile_endpoint_path(profile_dir: &Path) -> PathBuf {
    profile_dir.join(ENDPOINT_FILE)
}

pub fn profile_lock_path(profile_dir: &Path) -> PathBuf {
    profile_dir.join(INSTANCE_LOCK_FILE)
}

pub fn private_endpoints_dir(root: &Path) -> PathBuf {
    root.join(PRIVATE_ENDPOINTS)
}

pub fn private_endpoint_path(root: &Path, id: &str) -> PathBuf {
    private_endpoints_dir(root).join(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_home_dir_per_platform_and_trims() {
        assert_eq!(
            home_dir_from(Some("  /Users/alice  "), Some("C:\\Users\\alice"), false),
            Some(PathBuf::from("/Users/alice"))
        );
        assert_eq!(
            home_dir_from(Some("   "), Some("C:\\Users\\alice"), false),
            None
        );
        assert_eq!(
            home_dir_from(Some("/home/alice"), Some("  C:\\Users\\alice "), true),
            Some(PathBuf::from("C:\\Users\\alice"))
        );
        assert_eq!(
            home_dir_from(Some("/home/alice"), Some("  "), true),
            Some(PathBuf::from("/home/alice"))
        );
        assert_eq!(home_dir_from(None, None, true), None);
    }

    #[test]
    fn resolves_config_path_with_precedence_and_trimming() {
        let home = Path::new("/Users/alice");
        assert_eq!(
            resolve_config_path(Some("  /tmp/custom.toml "), Some("/xdg"), Some(home)),
            Some(PathBuf::from("/tmp/custom.toml"))
        );
        assert_eq!(
            resolve_config_path(Some("   "), Some(" /xdg/cfg "), Some(home)),
            Some(PathBuf::from("/xdg/cfg/koma/config.toml"))
        );
        assert_eq!(
            resolve_config_path(None, Some("  "), Some(home)),
            Some(PathBuf::from("/Users/alice/.config/koma/config.toml"))
        );
        assert_eq!(resolve_config_path(None, None, None), None);
        assert_eq!(resolve_config_path(None, None, Some(Path::new(""))), None);
    }

    #[test]
    fn resolves_state_and_workspace_paths() {
        let home = Path::new("/Users/alice");
        assert_eq!(
            state_dir_from(Some(" /tmp/state "), Some(home)),
            Some(PathBuf::from("/tmp/state"))
        );
        assert_eq!(
            state_dir_from(Some("  "), Some(home)),
            Some(PathBuf::from("/Users/alice/.local/state/koma"))
        );
        assert_eq!(state_dir_from(None, None), None);

        let root = Path::new("/state");
        assert_eq!(profiles_dir(root), PathBuf::from("/state/profiles"));
        let pdir = profile_dir(root, "work");
        assert_eq!(pdir, PathBuf::from("/state/profiles/work"));
        assert_eq!(
            profile_endpoint_path(&pdir),
            PathBuf::from("/state/profiles/work/endpoint")
        );
        assert_eq!(
            profile_lock_path(&pdir),
            PathBuf::from("/state/profiles/work/instance.lock")
        );
        assert_eq!(
            private_endpoints_dir(root),
            PathBuf::from("/state/private-workspaces")
        );
        assert_eq!(
            private_endpoint_path(root, "private-123"),
            PathBuf::from("/state/private-workspaces/private-123")
        );
    }
}
