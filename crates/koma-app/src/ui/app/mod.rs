//! Top-level Makepad application integrating Sheets, KeyboardRouter, SheetStrip, and IPC.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;

use koma_app::config::Config;
use koma_app::domain::palette::PaletteItem;
use koma_app::domain::router::OverlayKind;
use koma_app::domain::sheet_presets::available_sheet_presets;
use koma_app::persistence;
use koma_core::input::{derive_url_label, KomaInput};
use koma_core::model::{InputMode, KomaId, KomaKind, KomaMeta, SheetId, WorkspaceState};
use makepad_widgets::*;

use crate::runtime::ipc_server::IpcServer;
use crate::runtime::web_views::WebViewRuntime;
use crate::ui::sheet_strip::SheetStrip;

#[cfg(target_os = "macos")]
mod default_browser;
mod events;
#[cfg(target_os = "macos")]
mod external_urls;
mod ipc_dispatch;
mod lifecycle;
mod overlays;
mod routing;
mod web_dialogs;

use web_dialogs::{SitePermissionRecord, WebModalRequest};

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
                            draw_bg.color: theme.color_surface_container_high
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
                                    draw_text.color: theme.color_primary
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
                                draw_text.color: theme.color_on_surface_variant
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
                            draw_bg.color: theme.color_surface_container_high
                            draw_bg.border_radius: 8.0

                            drawer_title := Label{
                                width: Fill
                                text: "Drawer (Enter: place, d/x: discard, u: restore, Tab/Esc: close)"
                                draw_text.color: theme.color_primary
                            }

                            drawer_list := Label{
                                width: Fill
                                text: "(empty)"
                                draw_text.color: theme.color_on_surface
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
                            draw_bg.color: theme.color_surface_container_high
                            draw_bg.border_radius: 8.0

                            notifications_title := Label{
                                width: Fill
                                text: "Notifications (Enter: jump to koma, r: toggle read, m/Esc: close)"
                                draw_text.color: theme.color_primary
                            }

                            notifications_list := Label{
                                width: Fill
                                text: "(no notifications)"
                                draw_text.color: theme.color_on_surface
                            }
                        }
                    }

                    close_sheet_dialog := DangerDialog{
                        size: Sm
                        content +: {
                            body +: {
                                close_sheet_dialog_body := Label{
                                    width: Fill
                                    text: ""
                                }
                            }
                        }
                    }

                    web_modal_dialog := Dialog{
                        size: Sm
                        content +: {
                            body +: {
                                spacing: 8
                                web_modal_origin := Label{
                                    width: Fill
                                    text: ""
                                    draw_text.color: #x8b95b5
                                }
                                web_modal_message := Label{
                                    width: Fill
                                    text: ""
                                }
                                web_modal_input := TextInput{
                                    visible: false
                                    width: Fill
                                    empty_text: ""
                                }
                                web_modal_password := TextInput{
                                    visible: false
                                    width: Fill
                                    is_password: true
                                    empty_text: "Password"
                                }
                            }
                        }
                    }

                    toaster := Toaster{place: BottomEnd}
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
    handled_keys: Vec<KeyCode>,
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
    window_focused: bool,
    #[rust]
    last_web_toast: Option<std::time::Instant>,
    #[rust]
    next_web_dialog_id: u64,
    #[rust]
    pending_web_file_dialogs: HashMap<u64, (makepad_browser_view::ViewId, u64)>,
    #[rust]
    pending_web_folder_dialog: Option<(makepad_browser_view::ViewId, u64)>,
    #[rust]
    pending_web_downloads: HashMap<u64, (makepad_browser_view::ViewId, u32)>,
    #[rust]
    active_web_modal: Option<WebModalRequest>,
    #[rust]
    web_modal_queue: VecDeque<WebModalRequest>,
    #[rust]
    site_permissions: HashMap<String, SitePermissionRecord>,
}

impl App {
    fn web_runtime(&self) -> &Rc<RefCell<WebViewRuntime>> {
        self.web_runtime
            .as_ref()
            .expect("Web runtime is initialized")
    }

    pub(super) fn with_strip<R>(
        &self,
        cx: &mut Cx,
        f: impl FnOnce(&SheetStrip, &mut Cx) -> R,
    ) -> Option<R> {
        self.ui
            .widget(cx, ids!(sheet_strip))
            .borrow::<SheetStrip>()
            .map(|strip| f(&strip, cx))
    }

    pub(super) fn with_strip_mut<R>(
        &self,
        cx: &mut Cx,
        f: impl FnOnce(&mut SheetStrip, &mut Cx) -> R,
    ) -> Option<R> {
        self.ui
            .widget(cx, ids!(sheet_strip))
            .borrow_mut::<SheetStrip>()
            .map(|mut strip| f(&mut strip, cx))
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
        self.with_strip_mut(cx, |strip, cx| {
            strip.sync_sheet_view(
                cx,
                &self.state,
                &self.sock_path,
                &self.path_env,
                reveal_focused,
            );
        });
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
        if self.current_overlay() != OverlayKind::None || self.state.mode != InputMode::ContentInput
        {
            if self
                .with_strip_mut(cx, |strip, cx| strip.blur_all(cx))
                .is_none()
            {
                return;
            }
            if self.prompt_mode.is_none() {
                cx.set_key_focus(Area::Empty);
            }
            return;
        }
        let focused_id = self.state.focused_koma().map(|b| b.id.clone());
        let focused =
            self.with_strip_mut(cx, |strip, cx| strip.focus_koma(cx, focused_id.as_ref()));
        if focused == Some(false) {
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
        self.with_strip_mut(cx, |strip, cx| strip.remove_koma(cx, &item.id, item.kind));
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
        self.create_terminal_koma_after(cx, cwd, None)
    }

    pub(super) fn create_terminal_koma_after(
        &mut self,
        cx: &mut Cx,
        cwd: Option<String>,
        caller: Option<&KomaId>,
    ) -> KomaMeta {
        self.create_koma_after(cx, KomaInput::Terminal { cwd, command: None }, None, caller)
    }

    pub(super) fn create_web_koma(
        &mut self,
        cx: &mut Cx,
        url: String,
        label: Option<String>,
    ) -> KomaMeta {
        self.create_web_koma_after(cx, url, label, None)
    }

    pub(super) fn create_web_koma_with_view(
        &mut self,
        cx: &mut Cx,
        view_id: makepad_browser_view::ViewId,
        url: String,
    ) -> KomaMeta {
        let display_url = if url.is_empty() {
            "about:blank".to_string()
        } else {
            url
        };
        let label = derive_url_label(&display_url);
        let (koma, focused_active) = self.state.insert_koma_after(
            KomaKind::Web,
            label,
            Some(display_url.clone()),
            None,
            None,
            None,
        );
        self.web_runtime()
            .borrow_mut()
            .bind_popup_view(cx, &koma.id, view_id, &display_url);
        if focused_active {
            self.state.mode = InputMode::ContentInput;
        }
        self.sync_after_mutation(cx, focused_active);
        if !focused_active {
            self.show_created_koma_toast(cx, &koma);
        }
        koma
    }

    pub(super) fn create_web_koma_after(
        &mut self,
        cx: &mut Cx,
        url: String,
        label: Option<String>,
        caller: Option<&KomaId>,
    ) -> KomaMeta {
        let label = label.unwrap_or_else(|| derive_url_label(&url));
        self.create_koma_after(cx, KomaInput::Web { url, label }, None, caller)
    }

    fn create_koma(&mut self, cx: &mut Cx, input: KomaInput, recent: Option<&str>) -> KomaMeta {
        self.create_koma_after(cx, input, recent, None)
    }

    fn create_koma_after(
        &mut self,
        cx: &mut Cx,
        input: KomaInput,
        recent: Option<&str>,
        caller: Option<&KomaId>,
    ) -> KomaMeta {
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
        let (koma, focused_active) = self
            .state
            .insert_koma_after(kind, label, url, cwd, command, caller);
        if focused_active {
            self.state.mode = InputMode::ContentInput;
        }
        if let Some(input) = recent {
            self.state.record_recent(input, koma.label.as_str());
        }
        self.sync_after_mutation(cx, focused_active);
        if !focused_active {
            self.show_created_koma_toast(cx, &koma);
        }
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
        self.with_strip_mut(cx, |strip, cx| {
            for koma in &removed.komas {
                strip.remove_koma(cx, &koma.id, koma.kind);
            }
        });
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
        self.with_strip_mut(cx, |strip, cx| {
            strip.close_koma(cx, &removed.id, removed.kind);
        });
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
            .with_strip(cx, |strip, cx| strip.focused_widget_koma_id(cx))
            .flatten();
        if let Some(id) = next_id {
            if self.state.focus_koma_by_id(&id) {
                self.save_state(cx);
                self.sync_strip(cx, false);
                self.update_status(cx);
            }
        }
    }
}
