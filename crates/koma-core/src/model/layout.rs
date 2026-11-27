//! Workspace layout restoration and test fixture construction.

use std::collections::HashMap;

use super::{
    InputMode, KomaKind, KomaMeta, SheetId, SheetState, WorkspaceState, DEFAULT_KOMA_WIDTH,
};

impl WorkspaceState {
    /// Restore Sheets and Drawer while initializing transient state as for a new Workspace.
    pub fn restore_layout(
        profile_id: String,
        mut sheets: Vec<SheetState>,
        active_sheet_index: usize,
        drawer: Vec<KomaMeta>,
    ) -> Result<Self, String> {
        if active_sheet_index >= sheets.len() {
            return Err("State has no valid active Sheet".into());
        }

        let mut sheet_positions = HashMap::new();
        for (index, sheet) in sheets.iter().enumerate() {
            let position = index + 1;
            if let Some(first) = sheet_positions.insert(&sheet.id, position) {
                return Err(format!(
                    "Duplicate Sheet ID {} at Sheet positions {first} and {position}",
                    sheet.id
                ));
            }
        }

        let location = |sheet: Option<&SheetId>, position| match sheet {
            Some(id) => format!("Sheet {id}, item {position}"),
            None => format!("Drawer, item {position}"),
        };
        let mut koma_locations = HashMap::new();
        let groups = sheets
            .iter()
            .map(|sheet| (Some(&sheet.id), sheet.komas.as_slice()))
            .chain(std::iter::once((None, drawer.as_slice())));
        for (sheet_id, komas) in groups {
            for (index, koma) in komas.iter().enumerate() {
                let position = index + 1;
                if koma.sheet_id.as_ref() != sheet_id {
                    return Err(format!(
                        "Koma {} at {} has mismatched sheet_id {:?} (expected {:?})",
                        koma.id,
                        location(sheet_id, position),
                        koma.sheet_id,
                        sheet_id
                    ));
                }
                if let Some((first_sheet, first_position)) =
                    koma_locations.insert(&koma.id, (sheet_id, position))
                {
                    return Err(format!(
                        "Duplicate Koma ID {}: {} and {}",
                        koma.id,
                        location(first_sheet, first_position),
                        location(sheet_id, position)
                    ));
                }
            }
        }

        for sheet in &mut sheets {
            sheet.focused_koma_id = sheet
                .focused_koma_id
                .take()
                .filter(|id| sheet.komas.iter().any(|koma| &koma.id == id));
        }
        Ok(Self {
            profile_id,
            sheets,
            active_sheet: active_sheet_index,
            drawer,
            ..Self::new()
        })
    }

    /// Two-Sheet fixture used by unit tests.
    pub fn poc_default() -> Self {
        let profile_id = "profile-a".to_string();
        Self {
            profile_id: profile_id.clone(),
            is_private: false,
            mode: InputMode::ContentInput,
            active_sheet: 0,
            next_koma_seq: 1,
            next_sheet_seq: 3,
            drawer: Vec::new(),
            discarded_drawer: Vec::new(),
            closed_komas: Vec::new(),
            drawer_open: false,
            drawer_selected: 0,
            notifications: Vec::new(),
            next_notification_seq: 1,
            notifications_open: false,
            notification_selected: 0,
            recents: Vec::new(),
            personal_presets: Vec::new(),
            sheets: vec![
                SheetState {
                    id: "sheet-1".into(),
                    label: "Sheet 1".into(),
                    focused_koma_id: Some("sheet1-term".into()),
                    komas: vec![
                        KomaMeta {
                            id: "sheet1-term".into(),
                            sheet_id: Some("sheet-1".into()),
                            profile_id: profile_id.clone(),
                            kind: KomaKind::Terminal,
                            label: "Shell 1".into(),
                            initial_url: None,
                            cwd: None,
                            command: None,
                            width: DEFAULT_KOMA_WIDTH,
                            ratio: None,
                            restore: None,
                            koma_api_enabled: false,
                        },
                        KomaMeta {
                            id: "sheet1-web".into(),
                            sheet_id: Some("sheet-1".into()),
                            profile_id: profile_id.clone(),
                            kind: KomaKind::Web,
                            label: "Makepad".into(),
                            initial_url: Some("https://makepad.nl".into()),
                            cwd: None,
                            command: None,
                            width: DEFAULT_KOMA_WIDTH,
                            ratio: None,
                            restore: None,
                            koma_api_enabled: false,
                        },
                    ],
                },
                SheetState {
                    id: "sheet-2".into(),
                    label: "Sheet 2".into(),
                    focused_koma_id: Some("sheet2-term".into()),
                    komas: vec![
                        KomaMeta {
                            id: "sheet2-term".into(),
                            sheet_id: Some("sheet-2".into()),
                            profile_id: profile_id.clone(),
                            kind: KomaKind::Terminal,
                            label: "Shell 2".into(),
                            initial_url: None,
                            cwd: None,
                            command: None,
                            width: DEFAULT_KOMA_WIDTH,
                            ratio: None,
                            restore: None,
                            koma_api_enabled: false,
                        },
                        KomaMeta {
                            id: "sheet2-web".into(),
                            sheet_id: Some("sheet-2".into()),
                            profile_id,
                            kind: KomaKind::Web,
                            label: "Example".into(),
                            initial_url: Some("https://example.com".into()),
                            cwd: None,
                            command: None,
                            width: DEFAULT_KOMA_WIDTH,
                            ratio: None,
                            restore: None,
                            koma_api_enabled: false,
                        },
                    ],
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_layout_requires_a_valid_active_sheet() {
        assert!(WorkspaceState::restore_layout("profile-a".into(), Vec::new(), 0, vec![]).is_err());
        let state = WorkspaceState::new();
        assert!(
            WorkspaceState::restore_layout("profile-a".into(), state.sheets, 1, vec![]).is_err()
        );
    }

    #[test]
    fn restore_layout_drops_a_stale_koma_focus() {
        let mut sheets = WorkspaceState::poc_default().sheets;
        sheets[1].focused_koma_id = Some("missing".into());
        let state = WorkspaceState::restore_layout("profile-a".into(), sheets, 1, vec![]).unwrap();
        assert_eq!(state.active_sheet_index(), 1);
        assert_eq!(state.current_sheet().focused_koma_id, None);
    }

    #[test]
    fn restore_layout_rejects_mismatched_koma_sheet_id() {
        let mut sheets = WorkspaceState::poc_default().sheets;
        sheets[0].komas[0].sheet_id = Some("sheet-2".into());
        assert!(WorkspaceState::restore_layout("profile-a".into(), sheets, 0, vec![]).is_err());

        let mut sheets = WorkspaceState::poc_default().sheets;
        sheets[0].komas[0].sheet_id = None;
        assert!(WorkspaceState::restore_layout("profile-a".into(), sheets, 0, vec![]).is_err());

        let sheets = WorkspaceState::poc_default().sheets;
        let mut drawer_koma = sheets[0].komas[0].clone();
        drawer_koma.id = "drawer-1".into();
        // drawer_koma still has sheet_id = Some("sheet-1"), which is invalid in Drawer
        assert!(
            WorkspaceState::restore_layout("profile-a".into(), sheets, 0, vec![drawer_koma])
                .is_err()
        );
    }
}
