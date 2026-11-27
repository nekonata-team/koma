//! Keyboard mapping and `RouteDecision` execution for `App`.

use koma_app::domain::router::{Key, KeyStroke, RouteDecision};
use koma_core::model::{InputMode, KomaKind};
use makepad_widgets::*;

#[cfg(target_os = "macos")]
use super::default_browser;
use super::{App, PromptMode};

fn query_external_sessions(cmd: &str, shell: Option<&str>) -> Result<Vec<String>, String> {
    #[cfg(windows)]
    let output = {
        let _ = shell;
        std::process::Command::new("cmd").args(["/C", cmd]).output()
    };
    #[cfg(not(windows))]
    let output = {
        let shell = shell
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .or_else(|| std::env::var("SHELL").ok().filter(|s| !s.is_empty()))
            .unwrap_or_else(|| "/bin/zsh".to_owned());
        std::process::Command::new(&shell)
            .args(["-l", "-c", cmd])
            .output()
    };
    let out = output.map_err(|e| format!("Failed to run session_list_command: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "session_list_command failed ({}): {}",
            out.status,
            stderr.trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

impl App {
    pub(super) fn map_key(ke: &KeyEvent) -> KeyStroke {
        let key = match ke.key_code {
            KeyCode::Comma => Key::Comma,
            KeyCode::Space => Key::Space,
            KeyCode::Tab => Key::Tab,
            KeyCode::Escape => Key::Escape,
            KeyCode::ReturnKey => Key::Return,
            KeyCode::Minus | KeyCode::NumpadSubtract => Key::Minus,
            KeyCode::Equals => Key::Equals,
            KeyCode::NumpadAdd => Key::Plus,
            KeyCode::Semicolon => Key::Semicolon,
            KeyCode::LBracket => Key::BracketLeft,
            KeyCode::RBracket => Key::BracketRight,
            KeyCode::ArrowLeft => Key::Left,
            KeyCode::ArrowRight => Key::Right,
            KeyCode::ArrowUp => Key::Up,
            KeyCode::ArrowDown => Key::Down,
            KeyCode::Key0 => Key::Digit(0),
            KeyCode::Key1 => Key::Digit(1),
            KeyCode::Key2 => Key::Digit(2),
            KeyCode::Key3 => Key::Digit(3),
            KeyCode::Key4 => Key::Digit(4),
            KeyCode::Key5 => Key::Digit(5),
            KeyCode::Key6 => Key::Digit(6),
            KeyCode::Key7 => Key::Digit(7),
            KeyCode::Key8 => Key::Digit(8),
            KeyCode::Key9 => Key::Digit(9),
            KeyCode::KeyA => Key::A,
            KeyCode::KeyB => Key::B,
            KeyCode::KeyC => Key::C,
            KeyCode::KeyD => Key::D,
            KeyCode::KeyE => Key::E,
            KeyCode::KeyF => Key::F,
            KeyCode::KeyG => Key::G,
            KeyCode::KeyH => Key::H,
            KeyCode::KeyI => Key::I,
            KeyCode::KeyJ => Key::J,
            KeyCode::KeyK => Key::K,
            KeyCode::KeyL => Key::L,
            KeyCode::KeyM => Key::M,
            KeyCode::KeyN => Key::N,
            KeyCode::KeyO => Key::O,
            KeyCode::KeyP => Key::P,
            KeyCode::KeyQ => Key::Q,
            KeyCode::KeyR => Key::R,
            KeyCode::KeyS => Key::S,
            KeyCode::KeyT => Key::T,
            KeyCode::KeyU => Key::U,
            KeyCode::KeyV => Key::V,
            KeyCode::KeyW => Key::W,
            KeyCode::KeyX => Key::X,
            KeyCode::KeyY => Key::Y,
            KeyCode::KeyZ => Key::Z,
            _ => Key::Other,
        };
        KeyStroke {
            ctrl: ke.modifiers.control,
            alt: ke.modifiers.alt,
            logo: ke.modifiers.logo,
            shift: ke.modifiers.shift,
            key,
        }
    }

    pub(super) fn set_mode(&mut self, cx: &mut Cx, next: InputMode, reveal: bool) {
        self.state.mode = next;
        self.sync_after_mutation(cx, reveal);
    }

    pub(super) fn toggle_overview(&mut self, cx: &mut Cx) {
        let next = if self.state.mode == InputMode::Overview {
            InputMode::ContentInput
        } else {
            InputMode::Overview
        };
        self.set_mode(cx, next, true);
    }

    fn duplicate_focused_koma(&mut self, cx: &mut Cx) {
        let live_url = self.state.focused_koma().and_then(|b| {
            (b.kind == KomaKind::Web).then(|| {
                self.web_runtime()
                    .borrow()
                    .koma_url_title(&b.id)
                    .map(|(u, _)| u)
            })?
        });
        let had_terminal_command = self
            .state
            .focused_koma()
            .is_some_and(|b| b.kind == KomaKind::Terminal && b.command.is_some());
        let needs_sessions = self.state.focused_koma().is_some_and(|b| {
            b.kind == KomaKind::Terminal
                && b.command.as_deref().is_some_and(|cmd| {
                    self.config
                        .terminal
                        .session_prefixes
                        .iter()
                        .any(|p| !p.is_empty() && cmd.starts_with(p.as_str()))
                })
        });
        let extra_sessions = if needs_sessions {
            if let Some(cmd) = self.config.terminal.session_list_command.as_deref() {
                match query_external_sessions(cmd, self.config.terminal.shell.as_deref()) {
                    Ok(names) => names,
                    Err(err) => {
                        self.show_toast(cx, err);
                        return;
                    }
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        match self.state.duplicate_focused_koma(
            &self.config.terminal.session_prefixes,
            &extra_sessions,
            live_url,
        ) {
            Ok(duplicated) => {
                self.state.mode = InputMode::ContentInput;
                self.sync_after_mutation(cx, true);
                if had_terminal_command && duplicated.command.is_none() {
                    let msg = if needs_sessions {
                        "Opened plain shell (could not derive next session name)"
                    } else {
                        "Opened plain shell (command did not match terminal.session_prefixes)"
                    };
                    self.show_toast(cx, msg);
                }
            }
            Err(err) => self.show_toast(cx, err),
        }
    }

    pub(super) fn execute_route(&mut self, cx: &mut Cx, decision: RouteDecision) {
        match decision {
            RouteDecision::ToggleMode => {
                self.width_mode_open = false;
                let next = match self.state.mode {
                    InputMode::ContentInput => InputMode::KomaMode,
                    InputMode::KomaMode | InputMode::Overview => InputMode::ContentInput,
                };
                self.set_mode(cx, next, false);
            }
            RouteDecision::EnterContentInput | RouteDecision::ExitOverview => {
                self.width_mode_open = false;
                self.set_mode(cx, InputMode::ContentInput, true);
            }
            RouteDecision::ToggleOverview => {
                self.width_mode_open = false;
                self.toggle_overview(cx);
            }
            RouteDecision::ClosePrompt => self.close_prompt(cx),
            RouteDecision::FocusRelativeKoma(delta) => {
                if self.state.focus_relative_koma(delta) {
                    self.sync_after_mutation(cx, true);
                }
            }
            RouteDecision::MoveFocusedKoma(delta) => {
                if self.state.move_focused_koma(delta) {
                    self.sync_after_mutation(cx, true);
                }
            }
            RouteDecision::MoveFocusedKomaToSheet(delta) => {
                if self.state.move_focused_koma_to_sheet(delta) {
                    self.sync_after_mutation(cx, true);
                }
            }
            RouteDecision::ResizeFocusedKoma(step) => {
                let usable = self
                    .with_strip(cx, |s, _| s.usable_width())
                    .unwrap_or(1244.0);
                if self.state.resize_focused_koma(
                    step as f64 * self.config.width_step,
                    usable,
                    self.config.koma_gap,
                ) {
                    self.sync_after_mutation(cx, true);
                }
            }
            RouteDecision::ContentZoom(step) => {
                if let Some(id) = self.state.focused_koma().map(|k| k.id.clone()) {
                    self.with_strip_mut(cx, |strip, cx| strip.adjust_content_zoom(cx, &id, step));
                }
            }
            RouteDecision::CycleWidthPreset => {
                if self.state.cycle_width_preset(&self.config.width_presets) {
                    self.sync_after_mutation(cx, true);
                }
            }
            RouteDecision::ApplyWidthPreset(idx) => {
                self.width_mode_open = false;
                self.state.mode = InputMode::ContentInput;
                if self
                    .state
                    .apply_width_preset(&self.config.width_presets, idx)
                {
                    self.sync_after_mutation(cx, true);
                } else {
                    self.show_toast(cx, format!("No width preset #{}", idx + 1));
                    self.apply_focus(cx);
                    self.update_status(cx);
                }
            }
            RouteDecision::EnterWidthMode => {
                self.width_mode_open = true;
                self.update_status(cx);
            }
            RouteDecision::ExitWidthMode => {
                self.width_mode_open = false;
                self.update_status(cx);
            }
            RouteDecision::ToggleMaximizeFocusedKoma => {
                if self.width_mode_open {
                    self.width_mode_open = false;
                    self.state.mode = InputMode::ContentInput;
                }
                if self.state.toggle_maximize_focused_koma() {
                    self.sync_after_mutation(cx, true);
                }
            }
            RouteDecision::CenterFocusedKoma => {
                if self.width_mode_open {
                    self.width_mode_open = false;
                    self.state.mode = InputMode::ContentInput;
                    self.apply_focus(cx);
                    self.update_status(cx);
                }
                self.with_strip_mut(cx, |strip, cx| strip.center_focused_koma(cx));
            }
            RouteDecision::OverviewZoom(delta) => {
                self.with_strip_mut(cx, |strip, cx| strip.adjust_overview_zoom(cx, delta));
            }
            RouteDecision::OverviewZoomReset => {
                self.with_strip_mut(cx, |strip, cx| strip.reset_overview_zoom(cx));
            }
            RouteDecision::SwitchSheet(idx) => self.switch_sheet(cx, idx),
            RouteDecision::SwitchSheetRelative(delta, wrap) => {
                self.switch_sheet_relative(cx, delta, wrap)
            }
            RouteDecision::AddSheet => self.add_sheet(cx),
            RouteDecision::CloseCurrentSheet => {
                if let Err(err) = self.request_close_current_sheet(cx) {
                    self.show_toast(cx, err);
                }
            }
            RouteDecision::OpenKomaPrompt => self.open_prompt(cx, PromptMode::OpenKoma),
            RouteDecision::NewProfile => self.open_prompt(cx, PromptMode::NewProfile),
            RouteDecision::SwitchProfile => self.open_prompt(cx, PromptMode::SwitchProfile),
            RouteDecision::OpenSaveSheetPresetPrompt => {
                self.open_prompt(cx, PromptMode::SaveSheetPreset)
            }
            RouteDecision::OpenPrivateWorkspace => {
                if let Err(err) = self.spawn_private_workspace(cx) {
                    self.show_toast(cx, err);
                }
            }
            RouteDecision::OpenDevTools => {
                if let Err(err) = self.open_devtools_for_focused_web(cx) {
                    self.show_toast(cx, err);
                }
            }
            RouteDecision::EditFocusedUrl => self.open_edit_focused_url_prompt(cx),
            RouteDecision::RenameFocusedKoma => {
                if let Some(koma) = self.state.focused_koma() {
                    let id = koma.id.clone();
                    self.open_prompt(cx, PromptMode::RenameKoma(id));
                } else {
                    self.show_toast(cx, "No koma to rename");
                }
            }
            RouteDecision::RenameCurrentSheet => {
                let id = self.state.current_sheet().id.clone();
                self.open_prompt(cx, PromptMode::RenameSheet(id));
            }
            RouteDecision::CreateTerminal => {
                let _ = self.create_terminal_koma(cx, None);
            }
            RouteDecision::DuplicateFocusedKoma => self.duplicate_focused_koma(cx),
            RouteDecision::CloseFocusedKoma => self.close_focused_koma(cx, false),
            RouteDecision::CloseFocusedKomaFocusLeft => self.close_focused_koma(cx, true),
            RouteDecision::ReopenClosedKoma => self.reopen_closed_koma(cx),
            RouteDecision::ReloadFocusedWeb => {
                if let Some(id) = self.focused_web_id() {
                    self.web_runtime().borrow_mut().reload(cx, &id);
                }
            }
            RouteDecision::ReloadConfig => match self.reload_configs(cx) {
                Ok(()) => self.show_toast(cx, "Reloaded config"),
                Err(err) => self.show_toast(cx, err),
            },
            RouteDecision::HistoryBackFocusedWeb => {
                if let Some(id) = self.focused_web_id() {
                    self.web_runtime().borrow_mut().go_back(cx, &id);
                }
            }
            RouteDecision::HistoryForwardFocusedWeb => {
                if let Some(id) = self.focused_web_id() {
                    self.web_runtime().borrow_mut().go_forward(cx, &id);
                }
            }
            RouteDecision::ToggleDrawer => self.toggle_drawer(cx),
            RouteDecision::StowFocusedToDrawer => self.stow_focused_to_drawer(cx),
            RouteDecision::ToggleNotifications => self.toggle_notifications(cx),
            RouteDecision::DrawerSelect(delta) => {
                self.state.select_drawer_relative(delta);
                self.render_drawer_overlay(cx);
                self.ui.redraw(cx);
            }
            RouteDecision::DrawerPlace => {
                if let Err(err) = self.place_drawer_item(cx, None) {
                    self.show_toast(cx, err);
                }
            }
            RouteDecision::DrawerDiscard => match self.discard_drawer_item(cx, None) {
                Ok(item) => self.show_toast(
                    cx,
                    format!("Discarded {} (press 'u' to restore)", item.label),
                ),
                Err(err) => self.show_toast(cx, err),
            },
            RouteDecision::DrawerRestore => match self.restore_discarded_drawer_item(cx) {
                Ok(item) => self.show_toast(cx, format!("Restored {} to Drawer", item.label)),
                Err(err) => self.show_toast(cx, err),
            },
            RouteDecision::NotificationSelect(delta) => {
                self.state.select_notification_relative(delta);
                self.render_notifications_overlay(cx);
                self.ui.redraw(cx);
            }
            RouteDecision::NotificationFocusSource => {
                match self.state.focus_notification_source(None) {
                    Ok(_) => {
                        self.state.mode = InputMode::ContentInput;
                        self.sync_after_mutation(cx, true);
                    }
                    Err(err) => self.show_toast(cx, err),
                }
            }
            RouteDecision::NotificationToggleRead => {
                if let Some(n) = self
                    .state
                    .notifications
                    .get(self.state.notification_selected)
                    .cloned()
                {
                    let _ = self.state.mark_notification_read(&n.id, !n.unread);
                    self.render_notifications_overlay(cx);
                    self.update_status(cx);
                    self.ui.redraw(cx);
                }
            }
            #[cfg(target_os = "macos")]
            RouteDecision::ShowDefaultBrowser => default_browser::show(),
            #[cfg(target_os = "macos")]
            RouteDecision::SetDefaultBrowser => default_browser::set(),
            RouteDecision::Forward | RouteDecision::Consume => {}
        }
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::query_external_sessions;

    #[test]
    fn query_external_sessions_runs_via_login_shell_and_parses_lines() {
        let out =
            query_external_sessions("printf ' alpha \\n\\nbeta\\n'", Some("/bin/sh")).unwrap();
        assert_eq!(out, vec!["alpha", "beta"]);
    }

    #[test]
    fn query_external_sessions_reports_nonzero_exit() {
        let err = query_external_sessions("echo boom >&2; exit 2", Some("/bin/sh")).unwrap_err();
        assert!(err.contains("session_list_command failed"));
        assert!(err.contains("boom"));
    }
}
