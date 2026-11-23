//! Overlay rendering and prompt / palette / toast interactions for `App`.

use koma_app::domain::palette::{candidates, PaletteAction, PaletteItem, MAX_CANDIDATES};
use koma_core::input::{parse_open_koma_input_with_engine, resolve_web_url_with_engine};
use koma_core::model::{InputMode, KomaKind};
use makepad_widgets::*;

use crate::runtime::profiles::{self, Profile};

use super::{App, PromptMode};
use crate::ui::sheet_strip::SheetStrip;

impl App {
    pub(super) fn show_toast(&mut self, cx: &mut Cx, msg: impl Into<String>) {
        let text = msg.into();
        self.ui.label(cx, ids!(toast_label)).set_text(cx, &text);
        self.ui.view(cx, ids!(toast_overlay)).set_visible(cx, true);
        cx.stop_timer(self.toast_timer);
        self.toast_timer = cx.start_timeout(3.5);
        self.ui.redraw(cx);
    }

    pub(super) fn render_drawer_overlay(&self, cx: &mut Cx) {
        self.ui
            .view(cx, ids!(drawer_overlay))
            .set_visible(cx, self.state.drawer_open);
        if !self.state.drawer_open {
            return;
        }
        let body = if self.state.drawer.is_empty() {
            "(drawer is empty — press 'a' in Koma Mode to stow focused koma)".to_string()
        } else {
            self.state
                .drawer
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let prefix = if i == self.state.drawer_selected {
                        "▸ "
                    } else {
                        "  "
                    };
                    let detail = item
                        .initial_url
                        .as_deref()
                        .or(item.cwd.as_deref())
                        .unwrap_or("");
                    if detail.is_empty() {
                        format!(
                            "{prefix}[{}] {} ({})",
                            item.kind.as_str(),
                            item.label,
                            item.id
                        )
                    } else {
                        format!(
                            "{prefix}[{}] {} — {} ({})",
                            item.kind.as_str(),
                            item.label,
                            detail,
                            item.id
                        )
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        self.ui.label(cx, ids!(drawer_list)).set_text(cx, &body);
    }

    pub(super) fn render_notifications_overlay(&self, cx: &mut Cx) {
        self.ui
            .view(cx, ids!(notifications_overlay))
            .set_visible(cx, self.state.notifications_open);
        if !self.state.notifications_open {
            return;
        }
        let body = if self.state.notifications.is_empty() {
            "(no notifications)".to_string()
        } else {
            self.state
                .notifications
                .iter()
                .enumerate()
                .map(|(i, n)| {
                    let prefix = if i == self.state.notification_selected {
                        "▸ "
                    } else {
                        "  "
                    };
                    let mark = if n.unread { "*" } else { " " };
                    format!("{prefix}{mark} [{}] {}: {}", n.koma_id, n.title, n.body)
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        self.ui
            .label(cx, ids!(notifications_list))
            .set_text(cx, &body);
    }

    pub(super) fn toggle_drawer(&mut self, cx: &mut Cx) {
        self.state.drawer_open = !self.state.drawer_open;
        if self.state.drawer_open {
            self.state.notifications_open = false;
        }
        self.render_drawer_overlay(cx);
        self.render_notifications_overlay(cx);
        self.apply_focus(cx);
        self.update_status(cx);
        self.ui.redraw(cx);
    }

    pub(super) fn toggle_notifications(&mut self, cx: &mut Cx) {
        self.state.notifications_open = !self.state.notifications_open;
        if self.state.notifications_open {
            self.state.drawer_open = false;
        }
        self.render_drawer_overlay(cx);
        self.render_notifications_overlay(cx);
        self.apply_focus(cx);
        self.update_status(cx);
        self.ui.redraw(cx);
    }

    pub(super) fn refresh_palette_candidates(&mut self, cx: &mut Cx, query: &str) {
        self.palette_items = match self.prompt_mode {
            Some(PromptMode::OpenKoma) => candidates(query, &self.state, &self.config),
            Some(PromptMode::SwitchProfile) => {
                let needle = query.trim().to_ascii_lowercase();
                match profiles::root().and_then(|root| Profile::list(&root)) {
                    Ok(ids) => ids
                        .into_iter()
                        .filter(|id| id.contains(&needle))
                        .take(MAX_CANDIDATES)
                        .map(|id| PaletteItem {
                            label: profiles::label(&id).to_owned(),
                            action: PaletteAction::SwitchProfile(id),
                        })
                        .collect(),
                    Err(err) => {
                        self.show_toast(cx, err);
                        Vec::new()
                    }
                }
            }
            _ => Vec::new(),
        };
        self.palette_selected = 0;
        self.render_palette_candidates(cx);
    }

    pub(super) fn render_palette_candidates(&self, cx: &mut Cx) {
        let text = self
            .palette_items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let prefix = if i == self.palette_selected {
                    "▸ "
                } else {
                    "  "
                };
                format!("{prefix}{}", item.label)
            })
            .collect::<Vec<_>>()
            .join("\n");
        self.ui
            .label(cx, ids!(palette_candidates))
            .set_text(cx, &text);
    }

    pub(super) fn open_prompt(&mut self, cx: &mut Cx, mode: PromptMode) {
        let (label_text, initial_text) = match &mode {
            PromptMode::OpenKoma => ("Palette:", String::new()),
            PromptMode::NewProfile => ("New Profile:", String::new()),
            PromptMode::SwitchProfile => ("Switch Profile:", String::new()),
            PromptMode::SaveSheetPreset => (
                "Save Sheet Preset:",
                self.state.current_sheet().label.clone(),
            ),
            PromptMode::EditKomaUrl(koma_id) => {
                let current_url = self
                    .web_runtime()
                    .borrow()
                    .koma_url_title(koma_id)
                    .map(|(u, _)| u)
                    .filter(|u| !u.is_empty())
                    .or_else(|| {
                        self.state
                            .koma_by_id(koma_id)
                            .and_then(|b| b.initial_url.clone())
                    })
                    .unwrap_or_default();
                ("Edit URL:", current_url)
            }
            PromptMode::RenameKoma(koma_id) => {
                let current = self
                    .state
                    .koma_by_id(koma_id)
                    .map(|b| b.label.clone())
                    .unwrap_or_default();
                ("Rename Koma:", current)
            }
            PromptMode::RenameSheet(sheet_id) => {
                let current = self
                    .state
                    .sheets()
                    .iter()
                    .find(|d| &d.id == sheet_id)
                    .map(|d| d.label.clone())
                    .unwrap_or_default();
                ("Rename Sheet:", current)
            }
        };

        self.width_mode_open = false;
        self.state.drawer_open = false;
        self.state.notifications_open = false;
        self.render_drawer_overlay(cx);
        self.render_notifications_overlay(cx);
        self.prompt_mode = Some(mode);
        self.apply_focus(cx);

        self.ui
            .label(cx, ids!(prompt_label))
            .set_text(cx, label_text);
        self.ui
            .view(cx, ids!(palette_overlay))
            .set_visible(cx, true);

        let input = self.ui.text_input(cx, ids!(prompt_input));
        input.set_text(cx, &initial_text);
        self.refresh_palette_candidates(cx, &initial_text);
        self.pending_focus_sync = true;
        self.try_flush_pending_focus(cx);
        self.ui.redraw(cx);
        self.update_status(cx);
    }

    pub(super) fn try_flush_pending_focus(&mut self, cx: &mut Cx) {
        if self.prompt_mode.is_some() {
            let area = self.ui.widget(cx, ids!(prompt_input)).area();
            if !area.is_empty() && area.is_valid(cx) {
                let input = self.ui.text_input(cx, ids!(prompt_input));
                input.take_key_focus(cx);
                if !input.text().is_empty() {
                    if let Some(mut inner) = input.borrow_mut() {
                        inner.select_all(cx);
                    }
                }
                self.pending_focus_sync = false;
            }
        } else if self.state.mode == InputMode::ContentInput {
            let focused_id = self.state.focused_koma().map(|b| b.id.clone());
            let strip_widget = self.ui.widget(cx, ids!(sheet_strip));
            let ready = strip_widget
                .borrow_mut::<SheetStrip>()
                .is_none_or(|mut strip| strip.focus_koma(cx, focused_id.as_ref()));
            if ready {
                self.pending_focus_sync = false;
            }
        } else {
            self.pending_focus_sync = false;
        }
    }

    pub(super) fn open_edit_focused_url_prompt(&mut self, cx: &mut Cx) {
        let target_id = self
            .state
            .focused_koma()
            .filter(|b| b.kind == KomaKind::Web)
            .or_else(|| {
                self.state
                    .current_sheet()
                    .komas
                    .iter()
                    .find(|b| b.kind == KomaKind::Web)
            })
            .map(|b| b.id.clone());

        if let Some(koma_id) = target_id {
            self.open_prompt(cx, PromptMode::EditKomaUrl(koma_id));
        } else {
            self.open_prompt(cx, PromptMode::OpenKoma);
        }
    }

    pub(super) fn close_prompt(&mut self, cx: &mut Cx) {
        if self.prompt_mode.is_none() {
            return;
        }
        self.prompt_mode = None;
        self.palette_items.clear();
        self.ui
            .view(cx, ids!(palette_overlay))
            .set_visible(cx, false);
        self.state.mode = InputMode::ContentInput;
        self.apply_focus(cx);
        self.ui.redraw(cx);
        self.update_status(cx);
    }

    pub(super) fn submit_prompt(&mut self, cx: &mut Cx, raw: &str) {
        let Some(mode) = self.prompt_mode.take() else {
            return;
        };
        let selected = self
            .palette_items
            .get(self.palette_selected)
            .map(|item| item.action.clone());
        self.palette_items.clear();
        self.ui
            .view(cx, ids!(palette_overlay))
            .set_visible(cx, false);
        let home_dir = std::env::var("HOME").ok();

        match mode {
            PromptMode::OpenKoma => {
                let input = match selected.as_ref() {
                    Some(PaletteAction::FocusKoma(koma_id)) => {
                        self.state.focus_koma_by_id(koma_id);
                        self.state.mode = InputMode::ContentInput;
                        self.sync_after_mutation(cx, true);
                        return;
                    }
                    Some(PaletteAction::OpenInput(input)) => input.as_str(),
                    Some(PaletteAction::Prefill(prefix)) => {
                        self.prompt_mode = Some(PromptMode::OpenKoma);
                        self.ui
                            .view(cx, ids!(palette_overlay))
                            .set_visible(cx, true);
                        let input_ref = self.ui.text_input(cx, ids!(prompt_input));
                        input_ref.set_text(cx, prefix);
                        input_ref.set_key_focus(cx);
                        self.render_palette_candidates(cx);
                        return;
                    }
                    Some(PaletteAction::ApplyPreset(name)) => {
                        if let Err(err) = self.apply_sheet_preset_by_name(cx, name) {
                            self.show_toast(cx, err);
                        }
                        return;
                    }
                    Some(PaletteAction::Run(decision)) => {
                        self.state.mode = InputMode::ContentInput;
                        self.apply_focus(cx);
                        self.update_status(cx);
                        self.execute_route(cx, *decision);
                        return;
                    }
                    Some(PaletteAction::Open | PaletteAction::SwitchProfile(_)) | None => raw,
                };
                let expanded = match self.config.expand_alias(input) {
                    Ok(Some(v)) => v,
                    Ok(None) => input.to_string(),
                    Err(err) => {
                        self.show_toast(cx, err);
                        return;
                    }
                };
                match parse_open_koma_input_with_engine(
                    &expanded,
                    home_dir.as_deref(),
                    self.config.search_engine,
                ) {
                    Ok(koma) => {
                        self.create_koma(cx, koma, Some(input));
                        return;
                    }
                    Err(err) => self.show_toast(cx, err),
                }
            }
            PromptMode::NewProfile | PromptMode::SwitchProfile => {
                let id = match selected.as_ref() {
                    Some(PaletteAction::SwitchProfile(id)) => id,
                    _ => raw,
                };
                self.open_profile(
                    cx,
                    id.to_owned(),
                    matches!(mode, PromptMode::NewProfile),
                    |result| {
                        if let Err(err) = result {
                            Cx::post_action(super::lifecycle::ProfileOpenError(err));
                        }
                    },
                );
            }
            PromptMode::SaveSheetPreset => match self.state.save_current_sheet_as_preset(raw) {
                Ok(preset) => {
                    self.state.mode = InputMode::ContentInput;
                    self.save_state(cx);
                    self.show_toast(cx, format!("Saved preset: {}", preset.name));
                    self.apply_focus(cx);
                    self.update_status(cx);
                    return;
                }
                Err(err) => self.show_toast(cx, err),
            },
            PromptMode::EditKomaUrl(koma_id) => match resolve_web_url_with_engine(
                raw,
                home_dir.as_deref(),
                self.config.search_engine,
            ) {
                Ok((url, label)) => {
                    self.state
                        .update_koma_url(&koma_id, url.clone(), Some(label));
                    self.web_runtime().borrow_mut().navigate(cx, &koma_id, &url);
                    self.state.focus_koma_by_id(&koma_id);
                    self.state.mode = InputMode::ContentInput;
                    self.sync_after_mutation(cx, true);
                    return;
                }
                Err(err) => self.show_toast(cx, err),
            },
            PromptMode::RenameKoma(koma_id) => match self.state.rename_koma(&koma_id, raw) {
                Ok(_) => {
                    self.state.mode = InputMode::ContentInput;
                    self.sync_after_mutation(cx, false);
                    return;
                }
                Err(err) => self.show_toast(cx, err),
            },
            PromptMode::RenameSheet(sheet_id) => match self.state.rename_sheet(&sheet_id, raw) {
                Ok(_) => {
                    self.state.mode = InputMode::ContentInput;
                    self.sync_after_mutation(cx, false);
                    return;
                }
                Err(err) => self.show_toast(cx, err),
            },
        }

        self.state.mode = InputMode::ContentInput;
        self.apply_focus(cx);
        self.ui.redraw(cx);
        self.update_status(cx);
    }
}
