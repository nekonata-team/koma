use std::fs::{self, File, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use koma_ipc::protocol::{IpcCommand, IpcRequest, WorkspaceCommand};
use koma_ipc::transport::send_request;
use koma_ipc::workspaces;

pub const DEFAULT_PROFILE: &str = "default";
const ACTIVATE_RETRY_ATTEMPTS: usize = 20;
const ACTIVATE_RETRY_DELAY: Duration = Duration::from_millis(50);

pub fn home() -> Option<String> {
    workspaces::home_dir().map(|p| p.to_string_lossy().into_owned())
}

pub fn root() -> Result<PathBuf, String> {
    workspaces::state_root()
}

fn profile_id(raw: &str) -> Result<String, String> {
    let id = raw.trim().to_ascii_lowercase();
    let reserved = matches!(id.as_str(), "private" | "con" | "prn" | "aux" | "nul")
        || matches!(
            id.as_bytes(),
            [b'c', b'o', b'm', b'0'..=b'9'] | [b'l', b'p', b't', b'0'..=b'9']
        );
    if (1..=32).contains(&id.len())
        && id.as_bytes()[0].is_ascii_alphanumeric()
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && !reserved
    {
        Ok(id)
    } else {
        Err("Use a Profile name of 1–32 letters, digits, or hyphens, starting with a letter or digit, and avoid reserved names".into())
    }
}

pub fn label(id: &str) -> &str {
    if id == DEFAULT_PROFILE {
        "Default"
    } else {
        id
    }
}

pub struct Profile {
    pub id: String,
    pub dir: PathBuf,
}

impl Profile {
    fn paths(root: &Path, raw: &str) -> Result<Self, String> {
        let id = profile_id(raw)?;
        let dir = workspaces::profile_dir(root, &id);
        Ok(Self { id, dir })
    }

    pub fn find(root: &Path, raw: &str) -> Result<Self, String> {
        let profile = Self::paths(root, raw)?;
        if profile.id != DEFAULT_PROFILE && !profile.dir.is_dir() {
            return Err(format!("Profile not found: {}", profile.id));
        }
        Ok(profile)
    }

    pub fn create(root: &Path, raw: &str) -> Result<Self, String> {
        let profile = Self::paths(root, raw)?;
        if profile.id == DEFAULT_PROFILE {
            return Err("Profile already exists: Default".into());
        }
        fs::create_dir_all(workspaces::profiles_dir(root)).map_err(|e| e.to_string())?;
        fs::create_dir(&profile.dir).map_err(|e| {
            if e.kind() == io::ErrorKind::AlreadyExists {
                format!("Profile already exists: {}", profile.id)
            } else {
                format!("Could not create Profile {}: {e}", profile.id)
            }
        })?;
        Ok(profile)
    }

    pub fn list(root: &Path) -> Result<Vec<String>, String> {
        let mut ids = vec![DEFAULT_PROFILE.into()];
        match fs::read_dir(workspaces::profiles_dir(root)) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry.map_err(|e| e.to_string())?;
                    if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                        let name = entry.file_name();
                        if let Some(id) = name.to_str() {
                            if id != DEFAULT_PROFILE && profile_id(id).as_deref() == Ok(id) {
                                ids.push(id.to_owned());
                            }
                        }
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
        ids[1..].sort_unstable();
        Ok(ids)
    }

    pub fn try_lock(&self) -> Result<Option<File>, String> {
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        try_lock(&workspaces::profile_lock_path(&self.dir))
    }

    pub fn activate(&self) -> Result<(), String> {
        let endpoint_path = workspaces::profile_endpoint_path(&self.dir);
        let request = IpcRequest::new(IpcCommand::Workspace(WorkspaceCommand::ProfileActivate {
            id: self.id.clone(),
        }));
        retry_until_ready(ACTIVATE_RETRY_ATTEMPTS, ACTIVATE_RETRY_DELAY, || {
            let socket = fs::read_to_string(&endpoint_path)
                .map_err(|e| format!("Profile {} has no ready IPC endpoint: {e}", self.id))?;
            let socket = socket.trim();
            if socket.is_empty() {
                return Err(format!("Profile {} has an empty IPC endpoint", self.id));
            }
            send_request(socket, &request)?.into_result()?;
            Ok(())
        })
    }
}

fn retry_until_ready<T, F>(attempts: usize, delay: Duration, mut op: F) -> Result<T, String>
where
    F: FnMut() -> Result<T, String>,
{
    let total = attempts.max(1);
    let mut last_err = String::new();
    for attempt in 0..total {
        match op() {
            Ok(val) => return Ok(val),
            Err(err) => {
                last_err = err;
                if attempt + 1 < total && !delay.is_zero() {
                    thread::sleep(delay);
                }
            }
        }
    }
    Err(last_err)
}

pub fn try_lock(path: &Path) -> Result<Option<File>, String> {
    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|e| format!("Could not open lock {}: {e}", path.display()))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => Err(format!("Could not lock {}: {e}", path.display())),
    }
}

pub fn process_socket(kind: &str) -> String {
    #[cfg(unix)]
    let file = format!("koma-{kind}-{}.sock", std::process::id());
    #[cfg(not(unix))]
    let file = format!("koma-{kind}-{}.port", std::process::id());
    std::env::temp_dir().join(file).display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_have_distinct_locations_and_one_owner_each() {
        let root = std::env::temp_dir().join(format!("koma-profile-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let default = Profile::find(&root, "Default").unwrap();
        let work = Profile::create(&root, "Work").unwrap();
        assert_eq!(default.dir, root.join("profiles/default"));
        assert_eq!(work.dir, root.join("profiles/work"));
        assert_eq!(Profile::find(&root, "work").unwrap().dir, work.dir);
        assert_eq!(Profile::list(&root).unwrap(), ["default", "work"]);
        assert!(Profile::create(&root, "WORK").is_err());
        assert!(Profile::create(&root, "../escape").is_err());
        for reserved in ["private", "CON", "aux", "CoM1", "lpt9"] {
            assert!(profile_id(reserved).is_err());
        }
        assert!(Profile::find(&root, "missing").is_err());

        let default_lock = default.try_lock().unwrap().unwrap();
        let work_lock = work.try_lock().unwrap().unwrap();
        assert!(default.try_lock().unwrap().is_none());
        assert!(work.try_lock().unwrap().is_none());
        drop(work_lock);
        assert!(work.try_lock().unwrap().is_some());
        drop(default_lock);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retry_until_ready_handles_transient_startup_races() {
        let mut calls = 0;
        let ok = retry_until_ready(4, Duration::ZERO, || {
            calls += 1;
            if calls < 3 {
                Err(format!("attempt {calls} failed"))
            } else {
                Ok("ready")
            }
        });
        assert_eq!(ok, Ok("ready"));
        assert_eq!(calls, 3);

        let mut fail_calls = 0;
        let err: Result<(), String> = retry_until_ready(3, Duration::ZERO, || {
            fail_calls += 1;
            Err(format!("err-{fail_calls}"))
        });
        assert_eq!(err, Err("err-3".into()));
        assert_eq!(fail_calls, 3);
    }
}
