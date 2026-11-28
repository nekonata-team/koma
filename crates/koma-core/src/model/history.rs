//! Closed Koma reopen stack and recently opened input history for `WorkspaceState`.

use super::{Koma, KomaContent, KomaId, RecentEntry, WorkspaceState, MAX_RECENTS};

const MAX_CLOSED_KOMAS: usize = 20;

impl WorkspaceState {
    pub fn close_koma(&mut self, id: &KomaId, prefer_left: bool) -> Option<Koma> {
        let (index, koma) = self.take_koma(id, prefer_left)?;
        if self.closed_komas.len() == MAX_CLOSED_KOMAS {
            self.closed_komas.remove(0);
        }
        self.closed_komas.push((index, koma.clone()));
        Some(koma)
    }

    pub fn reopen_closed_koma(&mut self) -> Option<Koma> {
        let (original_index, mut koma) = self.closed_komas.pop()?;
        let origin = self
            .sheets
            .iter()
            .position(|sheet| Some(&sheet.id) == koma.sheet_id.as_ref());
        self.active_sheet = origin.unwrap_or(self.active_sheet);
        let sheet = self.current_sheet_mut();
        let index = if origin.is_some() {
            original_index.min(sheet.komas.len())
        } else {
            sheet.focused_index().map_or(0, |index| index + 1)
        };
        koma.sheet_id = Some(sheet.id.clone());
        if let KomaContent::Terminal { command, .. } = &mut koma.content {
            *command = None;
        }
        sheet.komas.insert(index, koma.clone());
        sheet.focused_koma_id = Some(koma.id.clone());
        Some(koma)
    }

    pub fn record_recent(&mut self, input: impl Into<String>, label: impl Into<String>) {
        if self.is_private {
            return;
        }
        let input = input.into();
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return;
        }
        let trimmed_str = trimmed.to_string();
        let label_str = label.into().trim().to_string();
        let label_str = if label_str.is_empty() {
            trimmed_str.clone()
        } else {
            label_str
        };
        self.recents.retain(|r| r.input != trimmed_str);
        self.recents.insert(
            0,
            RecentEntry {
                input: trimmed_str,
                label: label_str,
            },
        );
        self.recents.truncate(MAX_RECENTS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reopens_explicitly_closed_komas_in_reverse_order_and_restores_focus() {
        let mut state = WorkspaceState::new();
        let terminal = state
            .insert_koma_after_focused("Shell".into(), KomaContent::terminal(None, None))
            .id;
        let web = state
            .insert_koma_after_focused("Web".into(), KomaContent::web("https://example.com"))
            .id;
        state.add_sheet(true);
        let other_sheet_koma = state.focused_koma().unwrap().id.clone();
        state.switch_sheet(0);

        let original = state.current_sheet().komas.clone();
        let term = state.koma_by_id_mut(&terminal).unwrap();
        term.content = KomaContent::terminal(Some("/work".into()), Some("example-command".into()));
        state.close_koma(&web, false).unwrap();
        state.close_koma(&terminal, false).unwrap();
        state.remove_koma_by_id(&other_sheet_koma).unwrap();
        state.switch_sheet(1);

        let restored = state.reopen_closed_koma().unwrap();
        assert_eq!(restored.id, terminal);
        assert_eq!(restored.cwd(), Some("/work"));
        assert_eq!(restored.command(), None);
        assert_eq!(state.active_sheet_index(), 0);
        assert_eq!(state.focused_koma(), Some(&restored));

        assert_eq!(state.reopen_closed_koma().as_ref(), Some(&original[1]));
        assert_eq!(state.current_sheet().komas, [restored, original[1].clone()]);
        assert_eq!(state.focused_koma().map(|koma| &koma.id), Some(&web));
        assert!(state.reopen_closed_koma().is_none());
    }

    #[test]
    fn reopens_next_to_focus_when_the_original_sheet_was_removed() {
        let mut state = WorkspaceState::new();
        let id = state
            .insert_koma_after_focused("Shell".into(), KomaContent::terminal(None, None))
            .id;
        state.add_sheet(true);
        state.switch_sheet(0);
        state.close_koma(&id, false).unwrap();
        state.remove_current_sheet().unwrap();
        let target = state.current_sheet().id.clone();
        let focused_id = state.focused_koma().unwrap().id.clone();
        let restored = state.reopen_closed_koma().unwrap();
        assert_eq!(restored.sheet_id, Some(target));
        assert_eq!(state.current_sheet().komas[0].id, focused_id);
        assert_eq!(state.current_sheet().komas[1].id, id);
        assert_eq!(state.focused_koma(), Some(&restored));
    }

    #[test]
    fn allocation_skips_the_id_of_a_closed_koma() {
        let mut state = WorkspaceState::new();
        let seq = state.next_koma_seq;
        let closed =
            state.insert_koma_after_focused("Closed".into(), KomaContent::terminal(None, None));
        state.close_koma(&closed.id, false).unwrap();

        state.next_koma_seq = seq;
        let created =
            state.insert_koma_after_focused("New".into(), KomaContent::terminal(None, None));
        assert_ne!(created.id, closed.id);
        assert_eq!(state.reopen_closed_koma().unwrap().id, closed.id);
    }

    #[test]
    fn recents_dedup_cap_and_skip_when_private() {
        let mut state = WorkspaceState::new();
        for i in 0..(MAX_RECENTS + 3) {
            state.record_recent(format!("https://ex{i}.com"), format!("Ex {i}"));
        }
        assert_eq!(state.recents.len(), MAX_RECENTS);
        assert_eq!(
            state.recents[0].input,
            format!("https://ex{}.com", MAX_RECENTS + 2)
        );

        state.record_recent("https://ex5.com", "Ex 5 Updated");
        assert_eq!(state.recents.len(), MAX_RECENTS);
        assert_eq!(state.recents[0].input, "https://ex5.com");
        assert_eq!(state.recents[0].label, "Ex 5 Updated");

        let mut priv_state = WorkspaceState::new();
        priv_state.is_private = true;
        priv_state.record_recent("https://secret.example", "Secret");
        assert!(priv_state.recents.is_empty());
    }
}
