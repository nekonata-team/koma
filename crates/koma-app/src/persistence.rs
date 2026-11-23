//! Profile state saved as JSON under `~/.local/state/koma/` (UI-independent).

use std::path::{Path, PathBuf};

use makepad_micro_serde::*;

use koma_core::model::{
    KomaId, KomaKind, KomaMeta, PersonalSheetPreset, RecentEntry, SheetId, SheetState,
    WorkspaceState, MAX_KOMA_WIDTH, MIN_KOMA_WIDTH,
};

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
struct SavedKoma {
    id: String,
    kind: String,
    label: String,
    initial_url: Option<String>,
    cwd: Option<String>,
    command: Option<String>,
    width: f64,
    ratio: Option<f64>,
    koma_api_enabled: Option<bool>,
}

impl SavedKoma {
    fn from_meta(b: &KomaMeta) -> Self {
        Self {
            id: b.id.to_string(),
            kind: b.kind.as_str().into(),
            label: b.label.clone(),
            initial_url: b.initial_url.clone(),
            cwd: b.cwd.clone(),
            command: b.command.clone(),
            width: b.width,
            ratio: b.ratio,
            koma_api_enabled: b.koma_api_enabled.then_some(true),
        }
    }

    fn into_meta(self, sheet_id: Option<SheetId>, profile_id: String) -> Result<KomaMeta, String> {
        let kind = match self.kind.as_str() {
            "terminal" => KomaKind::Terminal,
            "web" => KomaKind::Web,
            other => return Err(format!("Unknown Koma kind: {other}")),
        };
        if !self.width.is_finite() || self.width <= 0.0 {
            return Err(format!("Invalid Koma width: {}", self.width));
        }
        if let Some(r) = self.ratio {
            if !(r > 0.0 && r <= 1.0) {
                return Err(format!("Invalid Koma ratio: {r}"));
            }
        }
        Ok(KomaMeta {
            id: self.id.into(),
            sheet_id,
            profile_id,
            kind,
            label: self.label,
            initial_url: self.initial_url,
            cwd: self.cwd,
            command: self.command,
            width: self.width.clamp(MIN_KOMA_WIDTH, MAX_KOMA_WIDTH),
            ratio: self.ratio,
            restore: None,
            koma_api_enabled: self.koma_api_enabled.unwrap_or(false),
        })
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

const VERSION: usize = 1;

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
                komas: d.komas.iter().map(SavedKoma::from_meta).collect(),
            })
            .collect(),
        drawer: (!state.drawer.is_empty())
            .then(|| state.drawer.iter().map(SavedKoma::from_meta).collect()),
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

pub fn from_json(text: &str) -> Result<WorkspaceState, String> {
    let saved =
        SavedState::deserialize_json(text).map_err(|e| format!("Invalid state JSON: {e:?}"))?;
    if saved.version != VERSION {
        return Err(format!("Unsupported state version {}", saved.version));
    }
    let mut sheets = Vec::new();
    for d in saved.sheets {
        let sheet_id = SheetId::from(d.id);
        let mut komas = Vec::new();
        for b in d.komas {
            komas.push(b.into_meta(Some(sheet_id.clone()), saved.profile_id.clone())?);
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
        drawer.push(b.into_meta(None, saved.profile_id.clone())?);
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
        WorkspaceState::restore_layout(saved.profile_id, sheets, saved.active_sheet, drawer)?;
    state.next_koma_seq = saved.next_koma_seq;
    state.next_sheet_seq = saved.next_sheet_seq;
    state.recents = recents;
    state.personal_presets = personal_presets;
    Ok(state)
}

/// `$KOMA_STATE_DIR`, else `~/.local/state/koma`.
pub fn state_dir(home: Option<&str>) -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("KOMA_STATE_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    home.map(|h| Path::new(h).join(".local/state/koma"))
}

/// Loads `state.json`; a missing file yields `None`. A corrupt file is moved to
/// `state.json.corrupt-<timestamp>` and reported as `Err` so the caller can start fresh.
pub fn load(dir: &Path) -> Result<Option<WorkspaceState>, String> {
    let path = dir.join("state.json");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    match from_json(&text) {
        Ok(state) => Ok(Some(state)),
        Err(err) => {
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let web = state.insert_koma_after_focused(
            KomaKind::Web,
            "Local".into(),
            Some("file:///tmp/index.html".into()),
            None,
        );
        state.set_koma_api_enabled(&web.id, true);
        assert!(state.koma_by_id(&web.id).unwrap().koma_api_enabled);
        state.cycle_width_preset(&[0.5]);
        state.add_sheet(true);
        state.keep_koma_in_drawer(
            KomaKind::Web,
            "Docs".into(),
            Some("https://docs.rs".into()),
            None,
            false,
        );
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
        save(&dir, &WorkspaceState::poc_default()).unwrap();
        let loaded = load(&dir).unwrap().unwrap();
        assert!(loaded.koma_by_id(&"sheet1-web".into()).is_some());
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
    fn rejects_duplicate_sheet_ids() {
        let mut saved =
            SavedState::deserialize_json(&to_json(&WorkspaceState::poc_default())).unwrap();
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
        let mut saved =
            SavedState::deserialize_json(&to_json(&WorkspaceState::poc_default())).unwrap();
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
}
