//! Workspace layout restoration.

use std::collections::HashMap;

use super::{KomaMeta, SheetId, SheetState, WorkspaceState};

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
}

#[cfg(test)]
mod tests {
    use super::super::KomaKind;
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
        let mut state = WorkspaceState::new();
        state.add_sheet(true);
        let mut sheets = state.sheets;
        sheets[1].focused_koma_id = Some("missing".into());
        let state = WorkspaceState::restore_layout("profile-a".into(), sheets, 1, vec![]).unwrap();
        assert_eq!(state.active_sheet_index(), 1);
        assert_eq!(state.current_sheet().focused_koma_id, None);
    }

    #[test]
    fn restore_layout_rejects_mismatched_koma_sheet_id() {
        let mut state = WorkspaceState::new();
        state.insert_koma_after_focused(KomaKind::Terminal, "Shell".into(), None, None);
        state.add_sheet(false);
        let base = state.sheets;

        let mut sheets = base.clone();
        sheets[0].komas[0].sheet_id = Some(sheets[1].id.clone());
        assert!(WorkspaceState::restore_layout("profile-a".into(), sheets, 0, vec![]).is_err());

        let mut sheets = base.clone();
        sheets[0].komas[0].sheet_id = None;
        assert!(WorkspaceState::restore_layout("profile-a".into(), sheets, 0, vec![]).is_err());

        let mut drawer_koma = base[0].komas[0].clone();
        drawer_koma.id = "drawer-1".into();
        // drawer_koma still has sheet_id = Some("sheet-1"), which is invalid in Drawer
        assert!(
            WorkspaceState::restore_layout("profile-a".into(), base, 0, vec![drawer_koma]).is_err()
        );
    }
}
