use super::{step_index, Koma, KomaContent, KomaId, WorkspaceState};

impl WorkspaceState {
    pub fn stow_focused_koma_to_drawer(&mut self) -> Result<Koma, String> {
        let mut koma = self
            .remove_focused_koma()
            .ok_or_else(|| "No focused koma to stow".to_string())?;
        koma.sheet_id = None;
        self.drawer.push(koma.clone());
        self.drawer_selected = self.drawer.len() - 1;
        Ok(koma)
    }

    pub fn keep_koma_in_drawer(&mut self, label: String, spec: KomaContent) -> Koma {
        let id = self.allocate_koma_id();
        let item = Koma::new(id, None, self.profile_id.clone(), label, spec);
        self.drawer.push(item.clone());
        self.drawer_selected = self.drawer.len() - 1;
        item
    }

    pub fn place_drawer_item(&mut self, id: Option<&KomaId>) -> Result<Koma, String> {
        let idx = self.resolve_drawer_index(id)?;
        let mut koma = self.drawer.remove(idx);
        self.clamp_drawer_selection();
        let sheet = self.current_sheet_mut();
        let insert_idx = sheet
            .focused_index()
            .map(|i| (i + 1).min(sheet.komas.len()))
            .unwrap_or(0);
        koma.sheet_id = Some(sheet.id.clone());
        sheet.komas.insert(insert_idx, koma.clone());
        sheet.focused_koma_id = Some(koma.id.clone());
        self.drawer_open = false;
        Ok(koma)
    }

    pub fn discard_drawer_item(&mut self, id: Option<&KomaId>) -> Result<Koma, String> {
        let idx = self.resolve_drawer_index(id)?;
        let item = self.drawer.remove(idx);
        self.clamp_drawer_selection();
        self.discarded_drawer.push(item.clone());
        Ok(item)
    }

    pub fn restore_discarded_drawer_item(&mut self) -> Result<Koma, String> {
        let item = self
            .discarded_drawer
            .pop()
            .ok_or_else(|| "No discarded drawer item to restore".to_string())?;
        self.drawer.push(item.clone());
        self.drawer_selected = self.drawer.len() - 1;
        Ok(item)
    }

    pub fn select_drawer_relative(&mut self, delta: isize) {
        self.drawer_selected = step_index(self.drawer_selected, self.drawer.len(), delta);
    }

    fn resolve_drawer_index(&self, id: Option<&KomaId>) -> Result<usize, String> {
        if self.drawer.is_empty() {
            return Err("Drawer is empty".into());
        }
        match id {
            Some(target) => self
                .drawer
                .iter()
                .position(|b| &b.id == target)
                .ok_or_else(|| format!("Drawer item not found: {target}")),
            None => Ok(self.drawer_selected.min(self.drawer.len() - 1)),
        }
    }

    fn clamp_drawer_selection(&mut self) {
        if self.drawer.is_empty() {
            self.drawer_selected = 0;
        } else {
            self.drawer_selected = self.drawer_selected.min(self.drawer.len() - 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawer_lifecycle_stows_keeps_places_discards_and_restores() {
        let mut state = WorkspaceState::new();
        let term =
            state.insert_koma_after_focused("Shell".into(), KomaContent::terminal(None, None));
        state.insert_koma_after_focused("Web".into(), KomaContent::web("https://example.com"));
        state.focus_koma_by_id(&term.id);
        state.add_sheet(false);
        state.switch_sheet(0);

        let stowed = state.stow_focused_koma_to_drawer().unwrap();
        assert_eq!(stowed.id, term.id);
        assert_eq!(stowed.sheet_id, None);
        assert_eq!(state.drawer.len(), 1);
        assert_eq!(state.sheets[0].komas.len(), 1);

        let kept = state.keep_koma_in_drawer("Docs".into(), KomaContent::web("https://docs.rs"));
        assert_eq!(kept.sheet_id, None);
        assert_eq!(state.drawer.len(), 2);

        let discarded = state.discard_drawer_item(Some(&kept.id)).unwrap();
        assert_eq!(discarded.id, kept.id);
        assert_eq!(state.drawer.len(), 1);
        assert_eq!(state.discarded_drawer.len(), 1);

        let restored = state.restore_discarded_drawer_item().unwrap();
        assert_eq!(restored.id, kept.id);
        assert_eq!(state.drawer.len(), 2);

        state.switch_sheet(1);
        let placed = state.place_drawer_item(Some(&stowed.id)).unwrap();
        assert_eq!(placed.id, stowed.id);
        assert_eq!(placed.sheet_id.as_ref(), Some(&state.current_sheet().id));
        assert_eq!(state.focused_koma().unwrap().id, stowed.id);
        assert_eq!(state.drawer.len(), 1);
    }
}
