//! Top-level Makepad application integrating Sheets, KeyboardRouter, SheetStrip, and IPC.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use koma_app::config::Config;
use koma_app::domain::palette::PaletteItem;
use koma_app::domain::router::{
    route_key_with_bindings, Key, KeyStroke, OverlayKind, RouteDecision,
};
use koma_app::domain::sheet_presets::available_sheet_presets;
use koma_app::persistence;
use koma_core::input::{derive_url_label, KomaInput};
use koma_core::model::{InputMode, KomaId, KomaKind, KomaMeta, SheetId, WorkspaceState};
use makepad_app_terminal::widget::MpTermAction;
use makepad_widgets::*;

use crate::runtime::ipc_server::IpcServer;
use crate::runtime::web_views::{KomaApiEvent, WebViewRuntime};
use crate::ui::sheet_strip::{self, SheetStrip, SheetStripAction};
use crate::ui::web_koma::{self, WebKomaAction};

mod ipc_dispatch;
mod lifecycle;
mod overlays;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    startup() do #(App::script_component(vm)){
        ui: Root{
            main_window := Window{
                window.inner_size: vec2(1280, 760)
                window.title: "Koma [Sheet 1] — CONTENT INPUT"
                body +: {
                    flow: Overlay
                    padding: 0
                    spacing: 0

                    sheet_strip := SheetStrip{
                        width: Fill
                        height: Fill
                    }

                    palette_overlay := View{
                        visible: false
                        new_batch: true
                        width: Fill
                        height: Fill
                        align: Align{x: 0.5 y: 0.12}
                        padding: Inset{top: 24 left: 16 right: 16 bottom: 16}

                        prompt_bar := RoundedView{
                            width: 580
                            height: Fit
                            flow: Down
                            padding: Inset{left: 14 right: 14 top: 10 bottom: 10}
                            spacing: 8
                            draw_bg.color: #x1a1e2e
                            draw_bg.border_radius: 8.0

                            View{
                                width: Fill
                                height: Fit
                                flow: Right
                                align: Align{x: 0.0 y: 0.5}
                                spacing: 8

                                prompt_label := Label{
                                    width: Fit
                                    text: "Palette:"
                                    draw_text.color: #x7aa2f7
                                }

                                prompt_input := TextInputFlat{
                                    width: Fill
                                    height: Fit
                                    empty_text: "URL, search, :terminal [path], or command"
                                }

                                btn_prompt_cancel := ButtonFlat{
                                    text: "Esc"
                                }
                            }

                            palette_candidates := Label{
                                width: Fill
                                text: ""
                                draw_text.color: #x94a3b8
                            }
                        }
                    }

                    drawer_overlay := View{
                        visible: false
                        new_batch: true
                        width: Fill
                        height: Fill
                        align: Align{x: 0.5 y: 0.14}
                        padding: Inset{top: 24 left: 16 right: 16 bottom: 16}

                        drawer_card := RoundedView{
                            width: 620
                            height: Fit
                            flow: Down
                            padding: Inset{left: 14 right: 14 top: 10 bottom: 10}
                            spacing: 8
                            draw_bg.color: #x181d2c
                            draw_bg.border_radius: 8.0

                            drawer_title := Label{
                                width: Fill
                                text: "Drawer (Enter: place, d/x: discard, u: restore, Tab/Esc: close)"
                                draw_text.color: #x7aa2f7
                            }

                            drawer_list := Label{
                                width: Fill
                                text: "(empty)"
                                draw_text.color: #xc0caf5
                            }
                        }
                    }

                    notifications_overlay := View{
                        visible: false
                        new_batch: true
                        width: Fill
                        height: Fill
                        align: Align{x: 0.5 y: 0.14}
                        padding: Inset{top: 24 left: 16 right: 16 bottom: 16}

                        notifications_card := RoundedView{
                            width: 620
                            height: Fit
                            flow: Down
                            padding: Inset{left: 14 right: 14 top: 10 bottom: 10}
                            spacing: 8
                            draw_bg.color: #x181d2c
                            draw_bg.border_radius: 8.0

                            notifications_title := Label{
                                width: Fill
                                text: "Notifications (Enter: jump to koma, r: toggle read, m/Esc: close)"
                                draw_text.color: #x7aa2f7
                            }

                            notifications_list := Label{
                                width: Fill
                                text: "(no notifications)"
                                draw_text.color: #xc0caf5
                            }
                        }
                    }

                    toast_overlay := View{
                        visible: false
                        new_batch: true
                        width: Fill
                        height: Fill
                        align: Align{x: 1.0 y: 1.0}
                        padding: Inset{right: 16 bottom: 16 left: 16 top: 16}

                        toast_card := RoundedView{
                            width: Fit
                            height: Fit
                            padding: Inset{left: 14 right: 14 top: 8 bottom: 8}
                            draw_bg.color: #x3b1d2e
                            draw_bg.border_radius: 6.0

                            toast_label := Label{
                                width: Fit
                                text: ""
                                draw_text.color: #xf7768e
                            }
                        }
                    }

                    close_sheet_dialog := DangerDialog{
                        size: Sm
                        body +: {
                            close_sheet_dialog_body := Label{
                                width: Fill
                                text: ""
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
enum PromptMode {
    OpenKoma,
    SaveSheetPreset,
    EditKomaUrl(KomaId),
    RenameKoma(KomaId),
    RenameSheet(SheetId),
    NewProfile,
    SwitchProfile,
}

fn query_external_sessions(cmd: &str) -> Result<Vec<String>, String> {
    #[cfg(windows)]
    let output = std::process::Command::new("cmd").args(["/C", cmd]).output();
    #[cfg(not(windows))]
    let output = std::process::Command::new("sh").args(["-c", cmd]).output();
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

fn derive_cwd_label(cwd: Option<&str>) -> String {
    let Some(path) = cwd.filter(|s| !s.trim().is_empty()) else {
        return "Shell".into();
    };
    let tail = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("/");
    format!("Shell ({tail})")
}

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,
    #[rust(WorkspaceState::default())]
    state: WorkspaceState,
    #[rust]
    web_runtime: Option<Rc<RefCell<WebViewRuntime>>>,
    #[rust]
    ipc: Option<IpcServer>,
    #[rust]
    prompt_mode: Option<PromptMode>,
    #[rust]
    width_mode_open: bool,
    #[rust]
    suppress_text_input: bool,
    #[rust]
    pending_focus_sync: bool,
    #[rust]
    sock_path: String,
    #[rust]
    path_env: String,
    #[rust]
    config: Config,
    #[rust]
    state_dir: Option<PathBuf>,
    #[rust]
    profile_lock: Option<std::fs::File>,
    #[rust]
    is_private: bool,
    #[rust]
    private_cef_dir: Option<PathBuf>,
    #[rust]
    palette_items: Vec<PaletteItem>,
    #[rust]
    palette_selected: usize,
    #[rust]
    toast_timer: Timer,
}

impl App {
    fn web_runtime(&self) -> &Rc<RefCell<WebViewRuntime>> {
        self.web_runtime
            .as_ref()
            .expect("Web runtime is initialized")
    }

    pub(super) fn save_state(&mut self, cx: &mut Cx) {
        if self.is_private {
            return;
        }
        let Some(dir) = self.state_dir.clone() else {
            return;
        };
        if let Err(err) = persistence::save(&dir, &self.state) {
            self.show_toast(cx, err);
        }
    }

    fn sync_strip(&mut self, cx: &mut Cx, reveal_focused: bool) {
        if let Some(mut strip) = self
            .ui
            .widget(cx, ids!(sheet_strip))
            .borrow_mut::<SheetStrip>()
        {
            strip.sync_sheet_view(
                cx,
                &self.state,
                &self.sock_path,
                &self.path_env,
                reveal_focused,
            );
        }
    }

    fn current_overlay(&self) -> OverlayKind {
        if self.prompt_mode.is_some() {
            OverlayKind::Prompt
        } else if self.state.drawer_open {
            OverlayKind::Drawer
        } else if self.state.notifications_open {
            OverlayKind::Notifications
        } else if self.width_mode_open {
            OverlayKind::Width
        } else {
            OverlayKind::None
        }
    }

    pub(super) fn update_status(&self, cx: &mut Cx) {
        let sheet = self.state.current_sheet();
        let mode_label = match self.current_overlay() {
            OverlayKind::Prompt => "PALETTE",
            OverlayKind::Drawer => "DRAWER",
            OverlayKind::Notifications => "NOTIFICATIONS",
            OverlayKind::Width => "WIDTH",
            OverlayKind::None => match self.state.mode {
                InputMode::KomaMode => "KOMA MODE",
                InputMode::Overview => "OVERVIEW",
                InputMode::ContentInput => "CONTENT INPUT",
            },
        };
        let unread = self.state.unread_notification_count();
        let badge = if unread > 0 {
            format!(" ({unread} unread)")
        } else {
            String::new()
        };
        let profile = if self.is_private {
            "Private"
        } else {
            crate::runtime::profiles::label(&self.state.profile_id)
        };
        let title = format!("Koma [{profile} / {}] — {mode_label}{badge}", sheet.label);
        self.ui.window(cx, ids!(main_window)).set_title(cx, &title);
    }

    pub(super) fn apply_focus(&mut self, cx: &mut Cx) {
        let strip_widget = self.ui.widget(cx, ids!(sheet_strip));
        let Some(mut strip) = strip_widget.borrow_mut::<SheetStrip>() else {
            return;
        };
        if self.current_overlay() != OverlayKind::None || self.state.mode != InputMode::ContentInput
        {
            strip.blur_all(cx);
            if self.prompt_mode.is_none() {
                cx.set_key_focus(Area::Empty);
            }
            return;
        }
        let focused_id = self.state.focused_koma().map(|b| b.id.clone());
        if !strip.focus_koma(cx, focused_id.as_ref()) {
            self.pending_focus_sync = true;
            cx.new_next_frame();
        }
    }

    pub(super) fn sync_after_mutation(&mut self, cx: &mut Cx, reveal_focused: bool) {
        self.save_state(cx);
        self.sync_strip(cx, reveal_focused);
        self.render_drawer_overlay(cx);
        self.render_notifications_overlay(cx);
        self.apply_focus(cx);
        self.update_status(cx);
    }

    fn stow_focused_to_drawer(&mut self, cx: &mut Cx) {
        match self.state.stow_focused_koma_to_drawer() {
            Ok(koma) => {
                self.show_toast(cx, format!("Stowed {} to Drawer", koma.label));
                self.sync_after_mutation(cx, true);
            }
            Err(err) => self.show_toast(cx, err),
        }
    }

    pub(super) fn place_drawer_item(
        &mut self,
        cx: &mut Cx,
        id: Option<&KomaId>,
    ) -> Result<KomaMeta, String> {
        let koma = self.state.place_drawer_item(id)?;
        self.state.mode = InputMode::ContentInput;
        self.sync_after_mutation(cx, true);
        Ok(koma)
    }

    pub(super) fn discard_drawer_item(
        &mut self,
        cx: &mut Cx,
        id: Option<&KomaId>,
    ) -> Result<KomaMeta, String> {
        let item = self.state.discard_drawer_item(id)?;
        if let Some(mut strip) = self
            .ui
            .widget(cx, ids!(sheet_strip))
            .borrow_mut::<SheetStrip>()
        {
            strip.remove_koma(cx, &item.id, item.kind);
        }
        self.sync_after_mutation(cx, false);
        Ok(item)
    }

    pub(super) fn restore_discarded_drawer_item(
        &mut self,
        cx: &mut Cx,
    ) -> Result<KomaMeta, String> {
        let item = self.state.restore_discarded_drawer_item()?;
        if item.kind == KomaKind::Web && item.koma_api_enabled {
            self.web_runtime()
                .borrow_mut()
                .set_koma_api_enabled(cx, &item.id, true);
        }
        self.sync_after_mutation(cx, false);
        Ok(item)
    }

    pub(super) fn switch_sheet(&mut self, cx: &mut Cx, idx: usize) {
        if !self.state.switch_sheet(idx) {
            return;
        }
        self.sync_after_mutation(cx, true);
    }

    fn switch_sheet_relative(&mut self, cx: &mut Cx, delta: isize, wrap: bool) {
        if !self.state.switch_sheet_relative(delta, wrap) {
            return;
        }
        self.sync_after_mutation(cx, true);
    }

    fn add_sheet(&mut self, cx: &mut Cx) {
        self.state.add_sheet(false);
        self.sync_after_mutation(cx, true);
    }

    pub(super) fn create_terminal_koma(&mut self, cx: &mut Cx, cwd: Option<String>) -> KomaMeta {
        self.create_koma(cx, KomaInput::Terminal { cwd, command: None }, None)
    }

    pub(super) fn create_web_koma(
        &mut self,
        cx: &mut Cx,
        url: String,
        label: Option<String>,
    ) -> KomaMeta {
        let label = label.unwrap_or_else(|| derive_url_label(&url));
        self.create_koma(cx, KomaInput::Web { url, label }, None)
    }

    fn create_koma(&mut self, cx: &mut Cx, input: KomaInput, recent: Option<&str>) -> KomaMeta {
        let (kind, label, url, cwd, command) = match input {
            KomaInput::Terminal { cwd, command } => {
                let label = if cwd.is_some() {
                    derive_cwd_label(cwd.as_deref())
                } else if let Some(cmd) = &command {
                    cmd.split_whitespace()
                        .last()
                        .map(str::to_string)
                        .unwrap_or_else(|| "Shell".into())
                } else {
                    derive_cwd_label(None)
                };
                (KomaKind::Terminal, label, None, cwd, command)
            }
            KomaInput::Web { url, label } => (KomaKind::Web, label, Some(url), None, None),
        };
        let koma = self
            .state
            .insert_koma_with_command(kind, label, url, cwd, command);
        self.state.mode = InputMode::ContentInput;
        if let Some(input) = recent {
            self.state.record_recent(input, koma.label.as_str());
        }
        self.sync_after_mutation(cx, true);
        koma
    }

    pub(super) fn spawn_private_workspace(&mut self, cx: &mut Cx) -> Result<(), String> {
        let exe =
            std::env::current_exe().map_err(|e| format!("Failed to locate executable: {e}"))?;
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("--private")
            .env("KOMA_PRIVATE", "1")
            .env_remove("KOMA_SOCKET")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        cmd.spawn()
            .map_err(|e| format!("Failed to launch Private Workspace: {e}"))?;
        self.show_toast(cx, "Opened Private Workspace");
        Ok(())
    }

    pub(super) fn request_close_current_sheet(&mut self, cx: &mut Cx) -> Result<(), String> {
        if self.state.sheets().len() <= 1 {
            return Err("Cannot close the last sheet".into());
        }
        let sheet = self.state.current_sheet();
        if sheet.komas.is_empty() {
            return self.close_current_sheet(cx);
        }
        let count = sheet.komas.len();
        let noun = if count == 1 { "koma" } else { "komas" };
        let title = format!("Close \"{}\"?", sheet.label);
        let body = format!("This sheet has {count} open {noun}. Closing it will terminate them.");
        self.ui
            .label(cx, ids!(close_sheet_dialog_body))
            .set_text(cx, &body);
        self.ui
            .dialog(cx, ids!(close_sheet_dialog))
            .ask(cx, &title, "Close Sheet", "Cancel");
        Ok(())
    }

    pub(super) fn close_current_sheet(&mut self, cx: &mut Cx) -> Result<(), String> {
        let removed = self.state.remove_current_sheet()?;
        if let Some(mut strip) = self
            .ui
            .widget(cx, ids!(sheet_strip))
            .borrow_mut::<SheetStrip>()
        {
            for koma in &removed.komas {
                strip.remove_koma(cx, &koma.id, koma.kind);
            }
        }
        self.sync_after_mutation(cx, true);
        Ok(())
    }

    pub(super) fn apply_sheet_preset_by_name(
        &mut self,
        cx: &mut Cx,
        name: &str,
    ) -> Result<usize, String> {
        let (name, komas) =
            available_sheet_presets(&self.state.personal_presets, &self.config.sheet_presets)
                .find(|p| p.name.eq_ignore_ascii_case(name))
                .map(|p| (p.name.to_owned(), p.komas.to_vec()))
                .ok_or_else(|| format!("Unknown sheet preset: {name}"))?;
        let home_dir = std::env::var("HOME").ok();
        let (idx, _) = self
            .state
            .apply_sheet_preset(&name, &komas, home_dir.as_deref(), true)?;
        self.state.mode = InputMode::ContentInput;
        self.sync_after_mutation(cx, true);
        Ok(idx)
    }

    pub(super) fn open_devtools_for_focused_web(
        &mut self,
        cx: &mut Cx,
    ) -> Result<KomaMeta, String> {
        let target = self
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
            .cloned()
            .ok_or_else(|| "No Web Koma in current sheet".to_string())?;
        let url = self
            .web_runtime()
            .borrow()
            .resolve_devtools_url(&target.id)?;
        let koma = self.create_web_koma(cx, url, Some(format!("DevTools: {}", target.label)));
        Ok(koma)
    }

    fn finish_remove_koma(&mut self, cx: &mut Cx, removed: koma_core::model::KomaMeta) {
        if let Some(mut strip) = self
            .ui
            .widget(cx, ids!(sheet_strip))
            .borrow_mut::<SheetStrip>()
        {
            strip.close_koma(cx, &removed.id, removed.kind);
        }
        self.sync_after_mutation(cx, true);
    }

    fn close_focused_koma(&mut self, cx: &mut Cx, prefer_left: bool) {
        let Some(id) = self.state.focused_koma().map(|koma| koma.id.clone()) else {
            self.show_toast(cx, "No koma to close");
            return;
        };
        let _ = self.close_koma_by_id(cx, &id, prefer_left);
    }

    fn close_koma_by_id(
        &mut self,
        cx: &mut Cx,
        id: &KomaId,
        prefer_left: bool,
    ) -> Result<(), String> {
        let removed = self
            .state
            .close_koma(id, prefer_left)
            .ok_or_else(|| format!("Koma not found: {id}"))?;
        self.finish_remove_koma(cx, removed);
        Ok(())
    }

    fn reopen_closed_koma(&mut self, cx: &mut Cx) {
        let Some(koma) = self.state.reopen_closed_koma() else {
            self.show_toast(cx, "No closed Koma to reopen");
            return;
        };
        if koma.kind == KomaKind::Web && koma.koma_api_enabled {
            self.web_runtime()
                .borrow_mut()
                .set_koma_api_enabled(cx, &koma.id, true);
        }
        self.sync_after_mutation(cx, true);
        self.show_toast(cx, format!("Reopened {}", koma.label));
    }

    fn focused_web_id(&self) -> Option<KomaId> {
        self.state
            .focused_koma()
            .filter(|b| b.kind == KomaKind::Web)
            .map(|b| b.id.clone())
    }

    fn sync_focus_from_ui(&mut self, cx: &mut Cx) {
        if self.pending_focus_sync
            || self.prompt_mode.is_some()
            || self.state.mode != InputMode::ContentInput
        {
            return;
        }
        let next_id = self
            .ui
            .widget(cx, ids!(sheet_strip))
            .borrow::<SheetStrip>()
            .and_then(|strip| strip.focused_widget_koma_id(cx));
        if let Some(id) = next_id {
            if self.state.focus_koma_by_id(&id) {
                self.save_state(cx);
                self.sync_strip(cx, false);
                self.update_status(cx);
            }
        }
    }

    fn map_key(ke: &KeyEvent) -> KeyStroke {
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

    fn set_mode(&mut self, cx: &mut Cx, next: InputMode, reveal: bool) {
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
                    .ui
                    .widget(cx, ids!(sheet_strip))
                    .borrow::<SheetStrip>()
                    .map_or(1244.0, |s| s.usable_width());
                if self.state.resize_focused_koma(
                    step as f64 * self.config.width_step,
                    usable,
                    self.config.koma_gap,
                ) {
                    self.sync_after_mutation(cx, true);
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
                if let Some(mut strip) = self
                    .ui
                    .widget(cx, ids!(sheet_strip))
                    .borrow_mut::<SheetStrip>()
                {
                    strip.center_focused_koma(cx);
                }
            }
            RouteDecision::OverviewZoom(delta) => {
                if let Some(mut strip) = self
                    .ui
                    .widget(cx, ids!(sheet_strip))
                    .borrow_mut::<SheetStrip>()
                {
                    strip.adjust_overview_zoom(cx, delta);
                }
            }
            RouteDecision::OverviewZoomReset => {
                if let Some(mut strip) = self
                    .ui
                    .widget(cx, ids!(sheet_strip))
                    .borrow_mut::<SheetStrip>()
                {
                    strip.reset_overview_zoom(cx);
                }
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
            RouteDecision::DuplicateFocusedKoma => {
                let live_url = self.state.focused_koma().and_then(|b| {
                    (b.kind == KomaKind::Web).then(|| {
                        self.web_runtime()
                            .borrow()
                            .koma_url_title(&b.id)
                            .map(|(u, _)| u)
                    })?
                });
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
                        match query_external_sessions(cmd) {
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
                    Ok(_) => {
                        self.state.mode = InputMode::ContentInput;
                        self.sync_after_mutation(cx, true);
                    }
                    Err(err) => self.show_toast(cx, err),
                }
            }
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
            RouteDecision::Forward | RouteDecision::Consume => {}
        }
    }
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        self.init_runtime(cx);
    }

    fn handle_timer(&mut self, cx: &mut Cx, event: &TimerEvent) {
        if self
            .toast_timer
            .is_event(&Event::Timer(event.clone()))
            .is_some()
        {
            self.ui.view(cx, ids!(toast_overlay)).set_visible(cx, false);
            self.ui.redraw(cx);
        }
    }

    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let close_dialog = self.ui.dialog(cx, ids!(close_sheet_dialog));
        if close_dialog.confirmed(actions) {
            if let Err(err) = self.close_current_sheet(cx) {
                self.show_toast(cx, err);
            }
        } else if close_dialog.cancelled(actions) || close_dialog.dismissed(actions) {
            self.apply_focus(cx);
        }

        if self.ui.button(cx, ids!(btn_prompt_cancel)).clicked(actions) {
            self.close_prompt(cx);
        }

        let prompt_input = self.ui.text_input(cx, ids!(prompt_input));
        if let Some(text) = prompt_input.changed(actions) {
            self.refresh_palette_candidates(cx, &text);
        }
        if let Some((text, _)) = prompt_input.returned(actions) {
            self.submit_prompt(cx, &text);
        } else if prompt_input.escaped(actions) {
            self.close_prompt(cx);
        }

        let mut terminal_metadata_changed = false;
        for action in actions {
            if let Some(error) = action.downcast_ref::<lifecycle::ProfileOpenError>() {
                self.show_toast(cx, error.0.clone());
                continue;
            }
            let Some(widget_action) = action.as_widget_action() else {
                continue;
            };
            match widget_action.cast::<MpTermAction>() {
                MpTermAction::Bell => {
                    let source_id = self
                        .ui
                        .widget(cx, ids!(sheet_strip))
                        .borrow::<SheetStrip>()
                        .and_then(|strip| strip.koma_id_for_widget_uid(widget_action.widget_uid))
                        .or_else(|| self.state.focused_koma().map(|b| b.id.clone()))
                        .unwrap_or_else(|| "terminal".into());
                    self.state
                        .push_notification(source_id, "Terminal Bell", "Attention requested");
                    self.render_notifications_overlay(cx);
                    self.update_status(cx);
                    self.show_toast(cx, "Terminal Bell");
                }
                MpTermAction::Notification { title, body } => {
                    let source_id = self
                        .ui
                        .widget(cx, ids!(sheet_strip))
                        .borrow::<SheetStrip>()
                        .and_then(|strip| strip.koma_id_for_widget_uid(widget_action.widget_uid))
                        .or_else(|| self.state.focused_koma().map(|b| b.id.clone()))
                        .unwrap_or_else(|| "terminal".into());
                    let summary = if title.is_empty() {
                        body.clone()
                    } else if body.is_empty() {
                        title.clone()
                    } else {
                        format!("{title}: {body}")
                    };
                    self.state.push_notification(source_id, title, body);
                    self.render_notifications_overlay(cx);
                    self.update_status(cx);
                    self.show_toast(cx, summary);
                }
                action @ (MpTermAction::TitleChanged(_) | MpTermAction::PwdChanged(_)) => {
                    let (label, cwd) = match action {
                        MpTermAction::TitleChanged(title) => (Some(title), None),
                        MpTermAction::PwdChanged(cwd) => {
                            let path = Path::new(&cwd);
                            if !path.is_absolute() || !path.is_dir() {
                                continue;
                            }
                            (None, Some(cwd))
                        }
                        _ => unreachable!(),
                    };
                    if let Some(id) = self
                        .ui
                        .widget(cx, ids!(sheet_strip))
                        .borrow::<SheetStrip>()
                        .and_then(|strip| strip.koma_id_for_widget_uid(widget_action.widget_uid))
                    {
                        terminal_metadata_changed |=
                            self.state.update_terminal_metadata(&id, label, cwd);
                    }
                }
                MpTermAction::Exited => {
                    if let Some(removed) = self
                        .ui
                        .widget(cx, ids!(sheet_strip))
                        .borrow::<SheetStrip>()
                        .and_then(|strip| strip.koma_id_for_widget_uid(widget_action.widget_uid))
                        .and_then(|id| self.state.remove_koma_by_id(&id))
                    {
                        self.finish_remove_koma(cx, removed);
                    }
                }
                _ => {}
            }
            match widget_action.cast::<SheetStripAction>() {
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
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        if let Event::Shutdown = event {
            self.shutdown_runtime(cx, event);
            return;
        }
        self.match_event(cx, event);
        self.init_runtime(cx);

        self.drain_ipc(cx);

        let dialog_open = self.ui.dialog(cx, ids!(close_sheet_dialog)).is_open();

        if !dialog_open {
            if let Event::KeyDown(ke) = event {
                self.suppress_text_input = false;
                if self.prompt_mode.is_some()
                    && self.ui.text_input(cx, ids!(prompt_input)).is_composing()
                {
                    let _ = cx.capture_actions(|cx| {
                        self.ui.handle_event(cx, event, &mut Scope::empty());
                    });
                    return;
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
                            return;
                        }
                        KeyCode::ArrowUp => {
                            self.palette_selected = (self.palette_selected + len - 1) % len;
                            self.render_palette_candidates(cx);
                            self.ui.redraw(cx);
                            return;
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
                if !matches!(decision, RouteDecision::Forward) {
                    self.suppress_text_input = !ke.modifiers.control
                        && !ke.modifiers.alt
                        && !ke.modifiers.logo
                        && !matches!(
                            ke.key_code,
                            KeyCode::Escape | KeyCode::ReturnKey | KeyCode::Tab
                        );
                    self.execute_route(cx, decision);
                    return;
                }
            }

            if let Event::KeyUp(_) = event {
                self.suppress_text_input = false;
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

        let rt_changed = self.web_runtime().borrow_mut().pump_event(cx, event);
        let popups = self.web_runtime().borrow_mut().take_popup_urls();
        for popup_url in popups {
            let _ = self.create_web_koma(cx, popup_url, None);
        }
        let url_changes = self.web_runtime().borrow_mut().take_url_changes();
        let mut url_state_changed = false;
        for (koma_id, url) in url_changes {
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
        let koma_api_events = self.web_runtime().borrow_mut().take_koma_api_events();
        for ev in koma_api_events {
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
        let web_nav_events = self.web_runtime().borrow_mut().take_web_nav_events();
        for ev in web_nav_events {
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

        if !actions.is_empty() {
            self.handle_actions(cx, &actions);
        }

        self.sync_focus_from_ui(cx);
    }
}
