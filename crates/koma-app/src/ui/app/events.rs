//! Makepad event loop, widget action dispatch, and web runtime polling for `App`.

use std::path::Path;

use koma_app::domain::router::{route_key_with_bindings, OverlayKind, RouteDecision};
use koma_core::model::InputMode;
use makepad_app_terminal::widget::MpTermAction;
use makepad_widgets::makepad_platform::file_dialogs::FileDialogAction;
use makepad_widgets::*;

use crate::runtime::platform;
use crate::runtime::web_views::KomaApiEvent;
use crate::ui::overlay_list::{self, OverlayListWidgetRefExt};
use crate::ui::sheet_strip::{self, SheetStripAction};
use crate::ui::web_koma::{self, WebKomaAction};

use super::lifecycle;
use super::web_dialogs::WebModalRequest;
use super::{App, PromptMode};

fn sync_handled_key_down(
    handled_keys: &mut Vec<KeyCode>,
    key_code: KeyCode,
    is_repeat: bool,
    forwarded: bool,
) -> bool {
    // Only suppress repeats of a previously handled shortcut; macOS Cmd+key may omit KeyUp.
    let already_handled = is_repeat && handled_keys.contains(&key_code);
    if already_handled || !forwarded {
        if !handled_keys.contains(&key_code) {
            handled_keys.push(key_code);
        }
        return true;
    }
    if !is_repeat {
        handled_keys.retain(|code| *code != key_code);
    }
    false
}

impl App {
    fn resolve_web_modal(&mut self, cx: &mut Cx, accepted: bool) {
        let web_runtime = self
            .web_runtime
            .as_ref()
            .expect("Web runtime is initialized");
        let still_active = self.web_dialogs.resolve_active_modal(
            cx,
            &self.ui,
            web_runtime,
            self.state_dir.as_deref(),
            self.is_private,
            accepted,
        );
        if !still_active {
            self.apply_focus(cx);
        }
    }

    fn handle_dialog_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let close_dialog = self.ui.dialog(cx, ids!(close_sheet_dialog));
        if close_dialog.confirmed(actions) {
            if let Err(err) = self.close_current_sheet(cx) {
                self.show_toast(cx, err);
            }
        } else if close_dialog.cancelled(actions) || close_dialog.dismissed(actions) {
            self.apply_focus(cx);
        }

        let web_modal = self.ui.dialog(cx, ids!(web_modal_dialog));
        if web_modal.confirmed(actions) {
            self.resolve_web_modal(cx, true);
        } else if web_modal.cancelled(actions) || web_modal.dismissed(actions) {
            self.resolve_web_modal(cx, false);
        } else if self.web_dialogs.has_active_modal()
            && (self
                .ui
                .text_input(cx, ids!(web_modal_password))
                .returned(actions)
                .is_some()
                || (self.web_dialogs.active_modal_is_js_dialog()
                    && self
                        .ui
                        .text_input(cx, ids!(web_modal_input))
                        .returned(actions)
                        .is_some()))
        {
            web_modal.close(cx);
            self.resolve_web_modal(cx, true);
        }
    }

    fn handle_prompt_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if self.ui.button(cx, ids!(btn_prompt_cancel)).clicked(actions) {
            self.close_prompt(cx);
        }

        let prompt_input = self.ui.text_input(cx, ids!(prompt_input));
        if let Some(text) = prompt_input.changed(actions) {
            self.refresh_palette_candidates(cx, &text);
        }
        if let Some(row) = self
            .ui
            .overlay_list(cx, ids!(palette_candidates))
            .clicked(actions)
        {
            if row < self.palette_items.len() {
                self.palette_selected = row;
                let text = prompt_input.text();
                self.submit_prompt(cx, &text);
                return;
            }
        }
        if let Some((text, _)) = prompt_input.returned(actions) {
            self.submit_prompt(cx, &text);
        } else if prompt_input.escaped(actions) {
            self.close_prompt(cx);
        }
    }

    fn handle_terminal_action(
        &mut self,
        cx: &mut Cx,
        widget_uid: WidgetUid,
        action: MpTermAction,
    ) -> bool {
        match action {
            MpTermAction::Bell => {
                let source_id = self
                    .with_strip(cx, |strip, _| strip.koma_id_for_widget_uid(widget_uid))
                    .flatten()
                    .or_else(|| self.state.focused_koma().map(|b| b.id.clone()))
                    .unwrap_or_else(|| "terminal".into());
                let entry =
                    self.state
                        .push_notification(source_id, "Terminal Bell", "Attention requested");
                self.render_notifications_overlay(cx);
                self.update_status(cx);
                self.show_notification_toast(cx, &entry);
                false
            }
            MpTermAction::Notification { title, body } => {
                let source_id = self
                    .with_strip(cx, |strip, _| strip.koma_id_for_widget_uid(widget_uid))
                    .flatten()
                    .or_else(|| self.state.focused_koma().map(|b| b.id.clone()))
                    .unwrap_or_else(|| "terminal".into());
                let entry = self.state.push_notification(source_id, title, body);
                self.render_notifications_overlay(cx);
                self.update_status(cx);
                self.show_notification_toast(cx, &entry);
                false
            }
            action @ (MpTermAction::TitleChanged(_) | MpTermAction::PwdChanged(_)) => {
                let (label, cwd) = match action {
                    MpTermAction::TitleChanged(title) => (Some(title), None),
                    MpTermAction::PwdChanged(cwd) => {
                        let path = Path::new(&cwd);
                        if !path.is_absolute() || !path.is_dir() {
                            return false;
                        }
                        (None, Some(cwd))
                    }
                    _ => unreachable!(),
                };
                if let Some(id) = self
                    .with_strip(cx, |strip, _| strip.koma_id_for_widget_uid(widget_uid))
                    .flatten()
                {
                    return self.state.update_terminal_metadata(&id, label, cwd);
                }
                false
            }
            MpTermAction::Exited => {
                let exited_id = self
                    .with_strip(cx, |strip, _| strip.koma_id_for_widget_uid(widget_uid))
                    .flatten();
                if let Some(id) = exited_id {
                    let _ = self.close_koma_by_id(cx, &id, false);
                }
                false
            }
            _ => false,
        }
    }

    fn handle_sheet_strip_action(&mut self, cx: &mut Cx, action: SheetStripAction) {
        match action {
            SheetStripAction::FocusKoma {
                koma_id,
                enter_content_input,
            } => {
                let mut changed = self.state.focus_koma_by_id(&koma_id);
                if enter_content_input && self.state.mode != InputMode::ContentInput {
                    self.state.mode = InputMode::ContentInput;
                    changed = true;
                }
                if changed {
                    self.sync_after_mutation(cx, false);
                }
            }
            SheetStripAction::EnterOverviewTarget { sheet_idx, koma_id } => {
                let _ = self.state.switch_sheet(sheet_idx);
                if let Some(id) = koma_id {
                    let _ = self.state.focus_koma_by_id(&id);
                }
                self.set_mode(cx, InputMode::ContentInput, true);
            }
            SheetStripAction::ExitOverview => {
                self.set_mode(cx, InputMode::ContentInput, true);
            }
            SheetStripAction::MoveKoma {
                koma_id,
                target_sheet,
                target_index,
            } => {
                if self
                    .state
                    .move_koma_to_position(&koma_id, target_sheet, target_index)
                {
                    self.sync_after_mutation(cx, true);
                }
            }
            SheetStripAction::OverviewSettled => {
                if self.state.mode == InputMode::ContentInput && self.prompt_mode.is_none() {
                    self.pending_focus_sync = true;
                    self.try_flush_pending_focus(cx);
                }
            }
            SheetStripAction::None => {}
        }
    }

    fn handle_key_down(&mut self, cx: &mut Cx, event: &Event, ke: &KeyEvent) -> bool {
        self.suppress_text_input = false;
        if self.prompt_mode.is_some() && self.ui.text_input(cx, ids!(prompt_input)).is_composing() {
            let _ = cx.capture_actions(|cx| {
                self.ui.handle_event(cx, event, &mut Scope::empty());
            });
            return true;
        }
        if matches!(
            self.prompt_mode,
            Some(PromptMode::OpenKoma | PromptMode::SwitchProfile)
        ) && !ke.modifiers.control
            && !ke.modifiers.alt
            && !ke.modifiers.logo
            && !self.palette_items.is_empty()
        {
            let len = self.palette_items.len();
            match ke.key_code {
                KeyCode::ArrowDown => {
                    self.palette_selected = (self.palette_selected + 1) % len;
                    self.render_palette_candidates(cx);
                    self.ui.redraw(cx);
                    return true;
                }
                KeyCode::ArrowUp => {
                    self.palette_selected = (self.palette_selected + len - 1) % len;
                    self.render_palette_candidates(cx);
                    self.ui.redraw(cx);
                    return true;
                }
                _ => {}
            }
        }
        let decision = route_key_with_bindings(
            self.state.mode,
            self.current_overlay(),
            Self::map_key(ke),
            &self.config.keybindings,
        );
        if sync_handled_key_down(
            &mut self.handled_keys,
            ke.key_code,
            ke.is_repeat,
            matches!(decision, RouteDecision::Forward),
        ) {
            self.suppress_text_input = !ke.modifiers.control
                && !ke.modifiers.alt
                && !ke.modifiers.logo
                && !matches!(
                    ke.key_code,
                    KeyCode::Escape | KeyCode::ReturnKey | KeyCode::Tab
                );
            self.execute_route(cx, decision);
            return true;
        }
        false
    }

    fn pump_web_runtime(&mut self, cx: &mut Cx, event: &Event) {
        let (rt_changed, events) = {
            let mut rt = self.web_runtime().borrow_mut();
            (rt.pump_event(cx, event), rt.drain_events())
        };
        for popup_url in events.popups {
            let _ = self.create_web_koma(cx, popup_url, None);
        }
        for (view_id, url) in events.popup_views {
            let _ = self.create_web_koma_with_view(cx, view_id, url);
        }
        for external_url in events.external_urls {
            if let Err(err) = platform::open_external_app_url(&external_url) {
                self.show_toast(cx, err);
            }
        }
        for (view_id, req) in events.file_dialogs {
            let web_runtime = self
                .web_runtime
                .as_ref()
                .expect("Web runtime is initialized");
            self.web_dialogs
                .open_file_dialog(cx, web_runtime, view_id, req);
        }
        for (view_id, req) in events.download_requests {
            self.web_dialogs.open_download_dialog(cx, view_id, req);
        }
        for upd in events.download_updates {
            let toast_id = LiveId::from_str(&format!("download-{}", upd.download_id));
            let toast = if upd.state == 0 {
                let title = if upd.percent < 0 {
                    format!("Downloading {}...", upd.file_name)
                } else {
                    format!(
                        "Downloading {} ({}%)",
                        upd.file_name,
                        upd.percent.clamp(0, 100)
                    )
                };
                Toast::new(toast_id, &title).life(ToastLife::Indefinite)
            } else {
                let msg = match upd.state {
                    1 => format!("Saved {}", upd.file_name),
                    2 => format!("Download canceled: {}", upd.file_name),
                    _ => format!("Download failed: {}", upd.file_name),
                };
                Toast::new(toast_id, &msg)
            };
            self.ui.toaster(cx, ids!(toaster)).show(cx, toast);
        }
        for (view_id, req) in events.js_dialogs {
            self.web_dialogs.enqueue_modal(
                cx,
                &self.ui,
                WebModalRequest::JsDialog { view_id, req },
            );
        }
        for (view_id, req) in events.auth_requests {
            self.web_dialogs
                .enqueue_modal(cx, &self.ui, WebModalRequest::Auth { view_id, req });
        }
        for (view_id, req) in events.permission_requests {
            if let Some(granted) = self
                .web_dialogs
                .check_site_permission(&req.origin, req.permissions)
            {
                self.web_runtime().borrow_mut().resolve_permission_request(
                    cx,
                    view_id,
                    req.request_id,
                    granted,
                );
            } else {
                self.web_dialogs.enqueue_modal(
                    cx,
                    &self.ui,
                    WebModalRequest::Permission { view_id, req },
                );
            }
        }
        for koma_id in events.closed_komas {
            let _ = self.close_koma_by_id(cx, &koma_id, false);
        }
        let mut url_state_changed = false;
        for (koma_id, url) in events.url_changes {
            let label = koma_core::input::derive_url_label(&url);
            if self.state.update_koma_url(&koma_id, url, Some(label)) {
                url_state_changed = true;
            }
        }
        if url_state_changed {
            self.save_state(cx);
            self.sync_strip(cx, false);
            self.update_status(cx);
        }
        let mut newest = None;
        for notification in events.web_notifications {
            if self.state.koma_by_id(&notification.koma_id).is_some() {
                newest = Some(self.state.push_notification(
                    notification.koma_id,
                    notification.title,
                    notification.body,
                ));
            }
        }
        if let Some(newest) = newest {
            self.render_notifications_overlay(cx);
            self.update_status(cx);
            self.ui.redraw(cx);
            if self.window_focused
                && self
                    .last_web_toast
                    .is_none_or(|previous| previous.elapsed() >= std::time::Duration::from_secs(4))
            {
                self.last_web_toast = Some(std::time::Instant::now());
                self.show_notification_toast(cx, &newest);
            }
        }
        for ev in events.koma_api {
            match ev {
                KomaApiEvent::FocusKoma { target_koma_id, .. } => {
                    if self.state.focus_koma_by_id(&target_koma_id) {
                        self.state.mode = InputMode::ContentInput;
                        self.sync_after_mutation(cx, true);
                    }
                }
                KomaApiEvent::ShowToast { message, .. } => {
                    self.show_toast(cx, message);
                }
            }
        }
        for ev in events.web_nav {
            match ev.action.as_str() {
                "openKoma" => {
                    if let Some(url) = ev.url {
                        let _ = self.create_web_koma(cx, url, None);
                    }
                }
                "keepInDrawer" => {
                    if let Some(url) = ev.url {
                        let label = koma_core::input::derive_url_label(&url);
                        self.state.keep_koma_in_drawer(
                            koma_core::model::KomaKind::Web,
                            label,
                            Some(url),
                            None,
                            false,
                        );
                        self.sync_after_mutation(cx, false);
                    }
                }
                "removeKoma" => {
                    let _ = self.close_koma_by_id(cx, &ev.koma_id, false);
                }
                "focusNextKoma" => self.execute_route(cx, RouteDecision::FocusRelativeKoma(1)),
                "focusPreviousKoma" => self.execute_route(cx, RouteDecision::FocusRelativeKoma(-1)),
                "showOverview" => self.execute_route(cx, RouteDecision::ToggleOverview),
                "openPalette" | "showEssentials" => {
                    self.execute_route(cx, RouteDecision::OpenKomaPrompt)
                }
                "editCurrentPage" => self.execute_route(cx, RouteDecision::EditFocusedUrl),
                "copyURL" => {
                    if let Some(url) = ev.url {
                        cx.copy_to_clipboard(&url);
                        self.show_toast(cx, format!("Copied {url}"));
                    }
                }
                _ => {}
            }
        }
        if rt_changed {
            self.ui.widget(cx, ids!(sheet_strip)).redraw(cx);
        }
    }
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        self.init_runtime(cx);
    }

    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        for id in toast_actions(actions).filter_map(|a| match a {
            ToastAction::ActionPressed(id) => Some(*id),
            _ => None,
        }) {
            self.focus_toast_target(cx, id);
        }

        self.handle_dialog_actions(cx, actions);
        self.handle_prompt_actions(cx, actions);

        if let Some(row) = self.ui.overlay_list(cx, ids!(drawer_list)).clicked(actions) {
            if let Some(id) = self.state.drawer.get(row).map(|item| item.id.clone()) {
                if let Err(err) = self.place_drawer_item(cx, Some(&id)) {
                    self.show_toast(cx, err);
                }
            }
        }

        if let Some(row) = self
            .ui
            .overlay_list(cx, ids!(notifications_list))
            .clicked(actions)
        {
            if let Some(id) = self.state.notifications.get(row).map(|n| n.id.clone()) {
                match self.state.focus_notification_source(Some(&id)) {
                    Ok(_) => {
                        self.state.mode = InputMode::ContentInput;
                        self.sync_after_mutation(cx, true);
                    }
                    Err(err) => self.show_toast(cx, err),
                }
            }
        }

        if let Some(sheet_live_id) = self.ui.line_menu(cx, ids!(sheet_line_menu)).jumped(actions) {
            if let Some(idx) = self
                .state
                .sheets()
                .iter()
                .position(|sheet| LiveId::from_str(sheet.id.as_str()) == sheet_live_id)
            {
                if self.state.switch_sheet(idx) {
                    self.sync_after_mutation(cx, true);
                } else {
                    self.apply_focus(cx);
                }
            }
        }

        let mut terminal_metadata_changed = false;
        for action in actions {
            if let Some(file_action) = action.downcast_ref::<FileDialogAction>() {
                let web_runtime = self
                    .web_runtime
                    .as_ref()
                    .expect("Web runtime is initialized");
                self.web_dialogs
                    .handle_file_dialog_action(cx, web_runtime, file_action);
                continue;
            }
            if let Some(error) = action.downcast_ref::<lifecycle::ProfileOpenError>() {
                self.show_toast(cx, error.0.clone());
                continue;
            }
            #[cfg(target_os = "macos")]
            if let Some(status) =
                action.downcast_ref::<platform::default_browser::DefaultBrowserStatus>()
            {
                self.show_toast(cx, status.0.clone());
                continue;
            }
            let Some(widget_action) = action.as_widget_action() else {
                continue;
            };
            terminal_metadata_changed |= self.handle_terminal_action(
                cx,
                widget_action.widget_uid,
                widget_action.cast::<MpTermAction>(),
            );
            self.handle_sheet_strip_action(cx, widget_action.cast::<SheetStripAction>());
            match widget_action.cast::<WebKomaAction>() {
                WebKomaAction::Focused(koma_id) => {
                    let mut changed = self.state.focus_koma_by_id(&koma_id);
                    if self.state.mode != InputMode::ContentInput {
                        self.state.mode = InputMode::ContentInput;
                        changed = true;
                    }
                    if changed {
                        self.sync_strip(cx, false);
                        self.update_status(cx);
                    }
                }
                WebKomaAction::None => {}
            }
        }
        if terminal_metadata_changed {
            self.save_state(cx);
            self.sync_strip(cx, false);
            self.render_drawer_overlay(cx);
            self.update_status(cx);
        }
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        crate::makepad_widgets::script_mod(vm);
        makepad_app_terminal::widget::script_mod(vm);
        web_koma::script_mod(vm);
        sheet_strip::script_mod(vm);
        overlay_list::script_mod(vm);
        super::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        if let Event::Shutdown = event {
            self.shutdown_runtime(cx, event);
            return;
        }
        self.match_event(cx, event);
        self.init_runtime(cx);

        self.drain_ipc(cx);

        #[cfg(target_os = "macos")]
        if let Event::AppOpen(open) = event {
            self.open_external_urls(cx, open);
        }

        match event {
            Event::WindowGotFocus(id) => {
                if self.ui.window(cx, ids!(main_window)).window_id() == Some(*id) {
                    self.window_focused = true;
                }
            }
            Event::WindowLostFocus(id) => {
                self.handled_keys.clear();
                if self.ui.window(cx, ids!(main_window)).window_id() == Some(*id) {
                    self.window_focused = false;
                }
            }
            Event::KeyUp(key) => {
                self.suppress_text_input = false;
                if let Some(index) = self
                    .handled_keys
                    .iter()
                    .position(|code| *code == key.key_code)
                {
                    self.handled_keys.swap_remove(index);
                    return;
                }
            }
            _ => {}
        }

        let dialog_open = self.ui.dialog(cx, ids!(close_sheet_dialog)).is_open()
            || self.ui.dialog(cx, ids!(web_modal_dialog)).is_open();
        if matches!(event, Event::Pinch(_))
            && (dialog_open || self.current_overlay() != OverlayKind::None)
        {
            self.with_strip_mut(cx, |strip, cx| strip.end_content_pinch(cx));
            return;
        }

        if !dialog_open {
            if let Event::KeyDown(ke) = event {
                if self.handle_key_down(cx, event, ke) {
                    return;
                }
            }

            if let Event::TextInput(_) = event {
                if self.suppress_text_input {
                    self.suppress_text_input = false;
                    return;
                }
                let overlay = self.current_overlay();
                if overlay != OverlayKind::Prompt
                    && (overlay != OverlayKind::None || self.state.mode != InputMode::ContentInput)
                {
                    return;
                }
            }

            let overlay = self.current_overlay();
            if overlay != OverlayKind::Prompt
                && (overlay != OverlayKind::None || self.state.mode != InputMode::ContentInput)
                && matches!(event, Event::KeyUp(_))
            {
                return;
            }
        }

        let actions = cx.capture_actions(|cx| {
            self.ui.handle_event(cx, event, &mut Scope::empty());
        });

        if self.pending_focus_sync {
            self.try_flush_pending_focus(cx);
        }

        self.pump_web_runtime(cx, event);

        if !actions.is_empty() {
            self.handle_actions(cx, &actions);
        }

        self.sync_focus_from_ui(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handled_key_suppresses_repeats_but_not_fresh_press_after_missed_key_up() {
        let mut handled = Vec::new();
        assert!(sync_handled_key_down(
            &mut handled,
            KeyCode::KeyL,
            false,
            false
        ));
        assert!(sync_handled_key_down(
            &mut handled,
            KeyCode::KeyL,
            true,
            true
        ));
        assert!(!sync_handled_key_down(
            &mut handled,
            KeyCode::KeyL,
            false,
            true
        ));
        assert!(handled.is_empty());
    }
}
