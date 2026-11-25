//! Scrollable single-line list for overlays, backed by `PortalList` and `ListItemOne`.

use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.OverlayListBase = #(OverlayList::register_widget(vm))
    mod.widgets.OverlayList = set_type_default() do mod.widgets.OverlayListBase{
        width: Fill
        height: 260
        empty_text: "(empty)"
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
                text: ""
                draw_text.color: theme.color_on_surface_variant
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverlayListItem {
    pub primary: String,
    pub trailing: String,
}

#[derive(Clone, Debug, Default)]
pub enum OverlayListAction {
    #[default]
    None,
    Clicked(usize),
}

#[derive(Script, ScriptHook, Widget)]
pub struct OverlayList {
    #[deref]
    view: View,
    #[live]
    empty_text: String,
    #[rust]
    items: Vec<OverlayListItem>,
    #[rust]
    selected: usize,
    #[rust]
    reveal_selected: bool,
}

fn next_first_id(selected: usize, first_id: usize, visible_items: usize) -> Option<usize> {
    let full_rows = visible_items.saturating_sub(1).max(1);
    if selected < first_id {
        Some(selected)
    } else if selected >= first_id.saturating_add(full_rows) {
        Some(selected.saturating_sub(full_rows - 1))
    } else {
        None
    }
}

impl Widget for OverlayList {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        let Event::Actions(actions) = event else {
            return;
        };
        let list = self.view.portal_list(cx, ids!(list));
        for (row, item) in list.items_with_actions(actions) {
            if row < self.items.len() && item.as_list_item().clicked(actions) {
                cx.widget_action(self.widget_uid(), OverlayListAction::Clicked(row));
            }
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        while let Some(step) = self.view.draw_walk(cx, scope, walk).step() {
            let portal = step.as_portal_list();
            let Some(mut list) = portal.borrow_mut() else {
                continue;
            };
            if self.items.is_empty() {
                list.set_item_range(cx, 0, 1);
                while let Some(row) = list.next_visible_item(cx) {
                    if row > 0 {
                        continue;
                    }
                    let item = list.item(cx, row, id!(Empty));
                    item.set_text(cx, &self.empty_text);
                    item.draw_all(cx, &mut Scope::empty());
                }
                continue;
            }
            list.set_item_range(cx, 0, self.items.len());
            if self.reveal_selected {
                self.reveal_selected = false;
                if let Some(next_first) =
                    next_first_id(self.selected, list.first_id(), list.visible_items())
                {
                    list.set_first_id_and_scroll(next_first, 0.0);
                }
            }
            while let Some(row) = list.next_visible_item(cx) {
                let Some(entry) = self.items.get(row) else {
                    continue;
                };
                let item = list.item(cx, row, id!(Row));
                let li = item.as_list_item();
                item.set_text(cx, &entry.primary);
                li.trailing().set_text(cx, &entry.trailing);
                li.set_selected(cx, row == self.selected);
                item.draw_all(cx, &mut Scope::empty());
            }
        }
        DrawStep::done()
    }
}

impl OverlayListRef {
    pub fn set_items(&self, cx: &mut Cx, items: Vec<OverlayListItem>, selected: usize) {
        let Some(mut inner) = self.borrow_mut() else {
            return;
        };
        if inner.items != items || inner.selected != selected {
            inner.items = items;
            inner.selected = selected;
            inner.reveal_selected = true;
            inner.view.redraw(cx);
        }
    }

    pub fn clicked(&self, actions: &Actions) -> Option<usize> {
        actions
            .find_widget_action(self.widget_uid())
            .and_then(|action| match action.cast::<OverlayListAction>() {
                OverlayListAction::Clicked(row) => Some(row),
                OverlayListAction::None => None,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::next_first_id;

    #[test]
    fn reveals_selected_row_only_when_outside_visible_window() {
        assert_eq!(next_first_id(0, 0, 8), None);
        assert_eq!(next_first_id(6, 0, 8), None);
        assert_eq!(next_first_id(7, 0, 8), Some(1));
        assert_eq!(next_first_id(9, 2, 8), Some(3));
        assert_eq!(next_first_id(0, 3, 8), Some(0));
        assert_eq!(next_first_id(2, 0, 0), Some(2));
    }
}
