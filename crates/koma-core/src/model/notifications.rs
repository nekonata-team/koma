use super::{step_index, KomaId, NotificationEntry, WorkspaceState, MAX_NOTIFICATIONS};

impl WorkspaceState {
    pub fn push_notification(
        &mut self,
        koma_id: KomaId,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> NotificationEntry {
        let id = format!("notif-{}", self.next_notification_seq);
        self.next_notification_seq += 1;
        let entry = NotificationEntry {
            id,
            koma_id,
            title: title.into(),
            body: body.into(),
            unread: true,
        };
        self.notifications.push(entry.clone());
        if self.notifications.len() > MAX_NOTIFICATIONS {
            let overflow = self.notifications.len() - MAX_NOTIFICATIONS;
            self.notifications.drain(0..overflow);
        }
        self.notification_selected = self.notifications.len().saturating_sub(1);
        entry
    }

    pub fn mark_notification_read(&mut self, id: &str, unread: bool) -> Result<(), String> {
        let entry = self
            .notifications
            .iter_mut()
            .find(|n| n.id == id)
            .ok_or_else(|| format!("Notification not found: {id}"))?;
        entry.unread = unread;
        Ok(())
    }

    pub fn focus_notification_source(&mut self, id: Option<&str>) -> Result<KomaId, String> {
        if self.notifications.is_empty() {
            return Err("No notifications".into());
        }
        let idx = match id {
            Some(target) => self
                .notifications
                .iter()
                .position(|n| n.id == target)
                .ok_or_else(|| format!("Notification not found: {target}"))?,
            None => self.notification_selected.min(self.notifications.len() - 1),
        };
        let koma_id = self.notifications[idx].koma_id.clone();
        self.notifications[idx].unread = false;
        if !self.focus_koma_by_id(&koma_id) {
            return Err(format!("Source koma no longer exists: {koma_id}"));
        }
        self.notifications_open = false;
        Ok(koma_id)
    }

    pub fn select_notification_relative(&mut self, delta: isize) {
        self.notification_selected =
            step_index(self.notification_selected, self.notifications.len(), delta);
    }

    pub fn unread_notification_count(&self) -> usize {
        self.notifications.iter().filter(|n| n.unread).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifications_track_unread_and_focus_source_across_sheets() {
        let mut state = WorkspaceState::poc_default();
        let n1 = state.push_notification("sheet2-term".into(), "Build", "Done");
        assert_eq!(state.unread_notification_count(), 1);

        state.mark_notification_read(&n1.id, false).unwrap();
        assert_eq!(state.unread_notification_count(), 0);
        state.mark_notification_read(&n1.id, true).unwrap();
        assert_eq!(state.unread_notification_count(), 1);

        let focused = state.focus_notification_source(Some(&n1.id)).unwrap();
        assert_eq!(focused.as_str(), "sheet2-term");
        assert_eq!(state.active_sheet, 1);
        assert_eq!(state.focused_koma().unwrap().id, focused);
        assert_eq!(state.unread_notification_count(), 0);
    }
}
