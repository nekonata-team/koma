//! Profile state saved as JSON under `~/.local/state/koma/` (UI-independent).

use std::collections::HashMap;
use std::path::Path;

use makepad_micro_serde::*;

use koma_core::model::{
    Koma, KomaContent, KomaId, PersonalSheetPreset, RecentEntry, SheetId, SheetState,
    WorkspaceState, MAX_KOMA_WIDTH, MIN_KOMA_WIDTH,
};
pub use koma_ipc::workspaces::state_dir;

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
enum SavedKomaContent {
    Terminal {
        /// Absent until a path is given or reported by the shell (OSC 7).
        cwd: Option<String>,
        command: Option<String>,
    },
    Web {
        initial_url: String,
        koma_api_enabled: bool,
    },
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
struct SavedKoma {
    id: String,
    label: String,
    content: SavedKomaContent,
    width: f64,
    ratio: Option<f64>,
}

impl SavedKoma {
    fn from_koma(b: &Koma) -> Self {
        let content = match &b.content {
            KomaContent::Terminal { cwd, command } => SavedKomaContent::Terminal {
                cwd: cwd.clone(),
                command: command.clone(),
            },
            KomaContent::Web {
                initial_url,
                koma_api_enabled,
            } => SavedKomaContent::Web {
                initial_url: initial_url.clone(),
                koma_api_enabled: *koma_api_enabled,
            },
        };
        Self {
            id: b.id.to_string(),
            label: b.label.clone(),
            content,
            width: b.width,
            ratio: b.ratio,
        }
    }

    fn into_koma(self, sheet_id: Option<SheetId>, profile_id: String) -> Result<Koma, String> {
        if !self.width.is_finite() || self.width <= 0.0 {
            return Err(format!("Invalid Koma width: {}", self.width));
        }
        if let Some(r) = self.ratio {
            if !(r > 0.0 && r <= 1.0) {
                return Err(format!("Invalid Koma ratio: {r}"));
            }
        }
        let content = match self.content {
            SavedKomaContent::Terminal { cwd, command } => KomaContent::Terminal { cwd, command },
            SavedKomaContent::Web {
                initial_url,
                koma_api_enabled,
            } => KomaContent::Web {
                initial_url,
                koma_api_enabled,
            },
        };
        let mut koma = Koma::new(self.id.into(), sheet_id, profile_id, self.label, content);
        koma.width = self.width.clamp(MIN_KOMA_WIDTH, MAX_KOMA_WIDTH);
        koma.ratio = self.ratio;
        Ok(koma)
    }
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
struct SavedSheet {
    id: String,
    label: String,
    focused_koma_id: Option<String>,
    komas: Vec<SavedKoma>,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
struct SavedRecent {
    input: String,
    label: String,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
struct SavedPreset {
    name: String,
    komas: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
struct SavedState {
    version: usize,
    profile_id: String,
    active_sheet: usize,
    next_koma_seq: usize,
    next_sheet_seq: usize,
    sheets: Vec<SavedSheet>,
    drawer: Option<Vec<SavedKoma>>,
    recents: Option<Vec<SavedRecent>>,
    personal_presets: Option<Vec<SavedPreset>>,
}

const VERSION: usize = 2;

pub fn to_json(state: &WorkspaceState) -> String {
    SavedState {
        version: VERSION,
        profile_id: state.profile_id.clone(),
        active_sheet: state.active_sheet_index(),
        next_koma_seq: state.next_koma_seq,
        next_sheet_seq: state.next_sheet_seq,
        sheets: state
            .sheets()
            .iter()
            .map(|d| SavedSheet {
                id: d.id.to_string(),
                label: d.label.clone(),
                focused_koma_id: d.focused_koma_id.as_ref().map(ToString::to_string),
                komas: d.komas.iter().map(SavedKoma::from_koma).collect(),
            })
            .collect(),
        drawer: (!state.drawer.is_empty())
            .then(|| state.drawer.iter().map(SavedKoma::from_koma).collect()),
        recents: (!state.recents.is_empty()).then(|| {
            state
                .recents
                .iter()
                .map(|r| SavedRecent {
                    input: r.input.clone(),
                    label: r.label.clone(),
                })
                .collect()
        }),
        personal_presets: (!state.personal_presets.is_empty()).then(|| {
            state
                .personal_presets
                .iter()
                .map(|p| SavedPreset {
                    name: p.name.clone(),
                    komas: p.komas.clone(),
                })
                .collect()
        }),
    }
    .serialize_json_pretty()
}

enum ParseStateError {
    UnsupportedVersion(usize),
    Corrupt(String),
}

impl ParseStateError {
    fn into_message(self) -> String {
        match self {
            Self::UnsupportedVersion(version) => format!("Unsupported state version {version}"),
            Self::Corrupt(msg) => msg,
        }
    }
}

fn parse_state(text: &str) -> Result<WorkspaceState, ParseStateError> {
    let saved = SavedState::deserialize_json(text)
        .map_err(|e| ParseStateError::Corrupt(format!("Invalid state JSON: {e:?}")))?;
    if saved.version != VERSION {
        return Err(ParseStateError::UnsupportedVersion(saved.version));
    }
    let mut sheets = Vec::new();
    for d in saved.sheets {
        let sheet_id = SheetId::from(d.id);
        let mut komas = Vec::new();
        for b in d.komas {
            komas.push(
                b.into_koma(Some(sheet_id.clone()), saved.profile_id.clone())
                    .map_err(ParseStateError::Corrupt)?,
            );
        }
        sheets.push(SheetState {
            id: sheet_id,
            label: d.label,
            focused_koma_id: d.focused_koma_id.map(KomaId::from),
            komas,
        });
    }
    let mut drawer = Vec::new();
    for b in saved.drawer.unwrap_or_default() {
        drawer.push(
            b.into_koma(None, saved.profile_id.clone())
                .map_err(ParseStateError::Corrupt)?,
        );
    }
    let recents = saved
        .recents
        .unwrap_or_default()
        .into_iter()
        .map(|r| RecentEntry {
            input: r.input,
            label: r.label,
        })
        .collect();
    let personal_presets = saved
        .personal_presets
        .unwrap_or_default()
        .into_iter()
        .map(|p| PersonalSheetPreset {
            name: p.name,
            komas: p.komas,
        })
        .collect();
    let mut state =
        WorkspaceState::restore_layout(saved.profile_id, sheets, saved.active_sheet, drawer)
            .map_err(ParseStateError::Corrupt)?;
    state.next_koma_seq = saved.next_koma_seq;
    state.next_sheet_seq = saved.next_sheet_seq;
    state.recents = recents;
    state.personal_presets = personal_presets;
    Ok(state)
}

pub fn from_json(text: &str) -> Result<WorkspaceState, String> {
    parse_state(text).map_err(ParseStateError::into_message)
}

/// Loads `state.json`; a missing file yields `None`. A corrupt file is moved to
/// `state.json.corrupt-<timestamp>` and reported as `Err` so the caller can start fresh.
/// A valid file with an unsupported schema version is left untouched and reported as `Err`.
pub fn load(dir: &Path) -> Result<Option<WorkspaceState>, String> {
    let path = dir.join("state.json");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    match parse_state(&text) {
        Ok(state) => Ok(Some(state)),
        Err(err @ ParseStateError::UnsupportedVersion(_)) => Err(err.into_message()),
        Err(ParseStateError::Corrupt(err)) => {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let backup = dir.join(format!("state.json.corrupt-{stamp}"));
            let moved = std::fs::rename(&path, &backup)
                .map(|_| format!("moved to {}", backup.display()))
                .unwrap_or_else(|e| format!("could not be moved: {e}"));
            Err(format!("{err}; {moved}"))
        }
    }
}

/// Writes `state.json` atomically (temp file + rename).
pub fn save(dir: &Path, state: &WorkspaceState) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let tmp = dir.join("state.json.tmp");
    std::fs::write(&tmp, to_json(state)).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, dir.join("state.json")).map_err(|e| format!("save state: {e}"))
}

#[derive(Clone, Debug, Default, PartialEq, Eq, SerJson, DeJson)]
pub struct SitePermissionRecord {
    pub camera: Option<bool>,
    pub microphone: Option<bool>,
    pub geolocation: Option<bool>,
    pub desktop_capture: Option<bool>,
}

/// Loads `site_permissions.json`; a missing file yields an empty map.
pub fn load_site_permissions(dir: &Path) -> Result<HashMap<String, SitePermissionRecord>, String> {
    let path = dir.join("site_permissions.json");
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    HashMap::<String, SitePermissionRecord>::deserialize_json(&raw)
        .map_err(|e| format!("Invalid site permissions JSON: {e:?}"))
}

/// Writes `site_permissions.json` atomically (temp file + rename).
pub fn save_site_permissions(
    dir: &Path,
    permissions: &HashMap<String, SitePermissionRecord>,
) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join("site_permissions.json");
    let tmp = dir.join("site_permissions.json.tmp");
    std::fs::write(&tmp, permissions.serialize_json())
        .map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("save site permissions: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("koma-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn save_and_load_round_trip_keeps_layout() {
        let dir = temp_dir("roundtrip");
        let empty = WorkspaceState::default();
        assert_eq!(empty.sheets().len(), 1);
        assert!(empty.current_sheet().komas.is_empty());
        save(&dir, &empty).unwrap();
        let saved = std::fs::read_to_string(dir.join("state.json")).unwrap();
        assert!(saved.lines().count() > 1);
        assert_eq!(load(&dir).unwrap().unwrap(), empty);

        let mut state = WorkspaceState::new();
        let web = state
            .insert_koma_after_focused("Local".into(), KomaContent::web("file:///tmp/index.html"));
        state.set_koma_api_enabled(&web.id, true);
        assert!(state.koma_by_id(&web.id).unwrap().koma_api_enabled());
        state.cycle_width_preset(&[0.5]);
        state.add_sheet(true);
        state.keep_koma_in_drawer("Docs".into(), KomaContent::web("https://docs.rs"));
        state.record_recent("https://docs.rs", "Docs");
        state.save_current_sheet_as_preset("Coding").unwrap();
        save(&dir, &state).unwrap();
        let loaded = load(&dir).unwrap().unwrap();
        assert_eq!(loaded, state);
        assert_eq!(loaded.drawer[0].sheet_id, None);
        assert_eq!(
            loaded.sheets()[0].komas[0].sheet_id.as_ref(),
            Some(&loaded.sheets()[0].id)
        );

        // Unique IDs that do not follow the current allocation format are still valid.
        let mut custom = WorkspaceState::new();
        let mut koma = custom
            .insert_koma_after_focused("Custom".into(), KomaContent::web("https://example.com"));
        koma.id = "custom-web".into();
        koma.sheet_id = Some("custom-sheet".into());
        let custom = WorkspaceState::restore_layout(
            "default".into(),
            vec![SheetState {
                id: "custom-sheet".into(),
                label: "Custom".into(),
                focused_koma_id: Some("custom-web".into()),
                komas: vec![koma],
            }],
            0,
            vec![],
        )
        .unwrap();
        save(&dir, &custom).unwrap();
        let loaded = load(&dir).unwrap().unwrap();
        assert!(loaded.koma_by_id(&"custom-web".into()).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_state_is_none_and_corrupt_state_is_quarantined() {
        let dir = temp_dir("corrupt");
        assert_eq!(load(&dir).unwrap(), None);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("state.json"), "{ nope").unwrap();
        assert!(load(&dir).is_err());
        assert!(!dir.join("state.json").exists());
        assert_eq!(load(&dir).unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unsupported_version_is_not_quarantined() {
        let dir = temp_dir("unsupported-version");
        std::fs::create_dir_all(&dir).unwrap();
        let mut saved = SavedState::deserialize_json(&to_json(&WorkspaceState::new())).unwrap();
        saved.version = VERSION + 1;
        let json = saved.serialize_json_pretty();
        let path = dir.join("state.json");
        std::fs::write(&path, &json).unwrap();

        let err = load(&dir).unwrap_err();
        assert_eq!(err, format!("Unsupported state version {}", VERSION + 1));
        assert!(path.exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), json);
        let entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(entries, vec!["state.json"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_duplicate_sheet_ids() {
        let mut state = WorkspaceState::new();
        state.add_sheet(false);
        let mut saved = SavedState::deserialize_json(&to_json(&state)).unwrap();
        saved.sheets[1].id = saved.sheets[0].id.clone();
        assert_eq!(
            from_json(&saved.serialize_json()).unwrap_err(),
            format!(
                "Duplicate Sheet ID {} at Sheet positions 1 and 2",
                saved.sheets[0].id
            )
        );
    }

    #[test]
    fn rejects_duplicate_koma_ids_across_sheets_and_drawer() {
        let mut state = WorkspaceState::new();
        state.insert_koma_after_focused("Shell".into(), KomaContent::terminal(None, None));
        state.add_sheet(true);
        let mut saved = SavedState::deserialize_json(&to_json(&state)).unwrap();
        let koma = saved.sheets[0].komas[0].clone();
        let first_sheet = saved.sheets[0].id.clone();
        let second_sheet = saved.sheets[1].id.clone();

        saved.drawer = Some(vec![koma.clone()]);
        assert_eq!(
            from_json(&saved.serialize_json()).unwrap_err(),
            format!(
                "Duplicate Koma ID {}: Sheet {first_sheet}, item 1 and Drawer, item 1",
                koma.id
            )
        );

        saved.drawer = None;
        saved.sheets[1].komas[0] = koma.clone();
        assert_eq!(
            from_json(&saved.serialize_json()).unwrap_err(),
            format!("Duplicate Koma ID {}: Sheet {first_sheet}, item 1 and Sheet {second_sheet}, item 1", koma.id)
        );
    }

    #[test]
    fn rejects_invalid_active_sheet() {
        let mut saved = SavedState::deserialize_json(&to_json(&WorkspaceState::new())).unwrap();
        saved.active_sheet = saved.sheets.len();
        assert_eq!(
            from_json(&saved.serialize_json()).unwrap_err(),
            "State has no valid active Sheet"
        );
    }

    #[test]
    fn site_permissions_save_and_load_round_trip() {
        let dir = temp_dir("site-permissions-roundtrip");
        assert!(load_site_permissions(&dir).unwrap().is_empty());

        let mut perms = HashMap::new();
        perms.insert(
            "https://meet.example.com".to_string(),
            SitePermissionRecord {
                camera: Some(true),
                microphone: Some(false),
                geolocation: None,
                desktop_capture: Some(true),
            },
        );
        save_site_permissions(&dir, &perms).unwrap();
        assert_eq!(load_site_permissions(&dir).unwrap(), perms);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_site_permissions_json_returns_err() {
        let dir = temp_dir("site-permissions-invalid");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("site_permissions.json"), "{ invalid").unwrap();
        assert!(load_site_permissions(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
