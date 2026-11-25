//! Scrollable list of Notification entries backed by `PortalList` and `ListItem`.

use koma_core::model::NotificationEntry;
use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.NotificationsListBase = #(NotificationsList::register_widget(vm))
    mod.widgets.NotificationsList = set_type_default() do mod.widgets.NotificationsListBase{
        width: Fill
        height: 260
        list := PortalList{
            width: Fill
            height: Fill
            Row := ListItemOne{
                divider: ListItemDivider.Inset
                trailing: Label{
                    text: ""
                    draw_text.color: theme.color_on_surface_variant
                }
            }
            Empty := Label{
                width: Fill
                padding: Inset{top: 8 bottom: 8 left: 4 right: 4}
                text: "(no notifications)"
                draw_text.color: theme.color_on_surface_variant
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub enum NotificationsListAction {
    #[default]
    None,
    Clicked(usize),
}

#[derive(Script, ScriptHook, Widget)]
pub struct NotificationsList {
    #[deref]
    view: View,
    #[rust]
    entries: Vec<NotificationEntry>,
    #[rust]
    selected: usize,
}

impl Widget for NotificationsList {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        let Event::Actions(actions) = event else {
            return;
        };
        let list = self.view.portal_list(cx, ids!(list));
        for (row, item) in list.items_with_actions(actions) {
            if row < self.entries.len() && item.as_list_item().clicked(actions) {
                cx.widget_action(self.widget_uid(), NotificationsListAction::Clicked(row));
            }
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        while let Some(step) = self.view.draw_walk(cx, scope, walk).step() {
            let portal = step.as_portal_list();
            let Some(mut list) = portal.borrow_mut() else {
                continue;
            };
            if self.entries.is_empty() {
                list.set_item_range(cx, 0, 1);
                while let Some(row) = list.next_visible_item(cx) {
                    if row > 0 {
                        continue;
                    }
                    let item = list.item(cx, row, id!(Empty));
                    item.draw_all(cx, &mut Scope::empty());
                }
                continue;
            }
            list.set_item_range(cx, 0, self.entries.len());
            while let Some(row) = list.next_visible_item(cx) {
                let Some(entry) = self.entries.get(row) else {
                    continue;
                };
                let item = list.item(cx, row, id!(Row));
                let li = item.as_list_item();
                let mark = if entry.unread { "● " } else { "" };
                let primary = match (entry.title.is_empty(), entry.body.is_empty()) {
                    (false, false) => format!("{mark}{} — {}", entry.title, entry.body),
                    (false, true) => format!("{mark}{}", entry.title),
                    (true, false) => format!("{mark}{}", entry.body),
                    (true, true) => format!("{mark}[{}]", entry.koma_id),
                };
                item.set_text(cx, &primary);
                li.trailing().set_text(cx, entry.koma_id.as_str());
                li.set_selected(cx, row == self.selected);
                item.draw_all(cx, &mut Scope::empty());
            }
        }
        DrawStep::done()
    }
}

impl NotificationsListRef {
    pub fn set_entries(&self, cx: &mut Cx, entries: Vec<NotificationEntry>, selected: usize) {
        let Some(mut inner) = self.borrow_mut() else {
            return;
        };
        inner.entries = entries;
        inner.selected = selected;
        inner.view.redraw(cx);
    }

    pub fn clicked(&self, actions: &Actions) -> Option<usize> {
        actions
            .find_widget_action(self.widget_uid())
            .and_then(|action| match action.cast::<NotificationsListAction>() {
                NotificationsListAction::Clicked(row) => Some(row),
                NotificationsListAction::None => None,
            })
    }
}
