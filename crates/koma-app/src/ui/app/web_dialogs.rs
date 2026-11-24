//! Web file dialogs, download save dialogs, modal prompts (JS/auth/permissions), and site permission storage.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use makepad_micro_serde::*;
use makepad_widgets::makepad_platform::file_dialogs::{FileDialog, FileDialogAction};
use makepad_widgets::*;

use super::App;

const SITE_PERMISSION_CAMERA: u32 = 1 << 0;
const SITE_PERMISSION_MICROPHONE: u32 = 1 << 1;
const SITE_PERMISSION_GEOLOCATION: u32 = 1 << 2;
const SITE_PERMISSION_DESKTOP_CAPTURE: u32 = 1 << 3;
const SITE_PERMISSION_OTHER: u32 = 1 << 4;

#[derive(Clone, Debug, Default, SerJson, DeJson)]
pub struct SitePermissionRecord {
    camera: Option<bool>,
    microphone: Option<bool>,
    geolocation: Option<bool>,
    desktop_capture: Option<bool>,
}

#[derive(Clone, Debug)]
pub(super) enum WebModalRequest {
    JsDialog {
        view_id: makepad_browser_view::ViewId,
        req: makepad_browser_view::JsDialogEvent,
    },
    Auth {
        view_id: makepad_browser_view::ViewId,
        req: makepad_browser_view::AuthRequestEvent,
    },
    Permission {
        view_id: makepad_browser_view::ViewId,
        req: makepad_browser_view::SitePermissionEvent,
    },
}

fn site_permission_labels(permissions: u32) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if (permissions & SITE_PERMISSION_CAMERA) != 0 {
        labels.push("Camera");
    }
    if (permissions & SITE_PERMISSION_MICROPHONE) != 0 {
        labels.push("Microphone");
    }
    if (permissions & SITE_PERMISSION_GEOLOCATION) != 0 {
        labels.push("Location");
    }
    if (permissions & SITE_PERMISSION_DESKTOP_CAPTURE) != 0 {
        labels.push("Screen sharing");
    }
    if labels.is_empty() || (permissions & SITE_PERMISSION_OTHER) != 0 {
        labels.push("Device features");
    }
    labels
}

impl App {
    fn next_dialog_live_id(&mut self) -> u64 {
        self.next_web_dialog_id += 1;
        self.next_web_dialog_id
    }

    pub(super) fn open_web_file_dialog(
        &mut self,
        cx: &mut Cx,
        view_id: makepad_browser_view::ViewId,
        req: makepad_browser_view::FileDialogEvent,
    ) {
        if req.mode == 2 {
            if let Some((prev_view, prev_req)) = self
                .pending_web_folder_dialog
                .replace((view_id, req.request_id))
            {
                self.web_runtime()
                    .borrow_mut()
                    .resolve_file_dialog(cx, prev_view, prev_req, None);
            }
            let mut dialog = FileDialog::new();
            if !req.title.is_empty() {
                dialog = dialog.set_title(req.title);
            }
            if !req.default_path.is_empty() {
                dialog = dialog.set_location(PathBuf::from(req.default_path));
            }
            cx.open_select_folder_dialog(dialog);
            return;
        }
        let dialog_id = self.next_dialog_live_id();
        self.pending_web_file_dialogs
            .insert(dialog_id, (view_id, req.request_id));
        let mut dialog = FileDialog::new().set_id(LiveId(dialog_id));
        if !req.title.is_empty() {
            dialog = dialog.set_title(req.title);
        }
        if !req.default_path.is_empty() {
            let path = PathBuf::from(&req.default_path);
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                dialog = dialog.set_filename(name.to_string());
            }
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                dialog = dialog.set_location(parent.to_path_buf());
            }
        }
        let exts: Vec<String> = req
            .accept_filters
            .iter()
            .filter_map(|f| f.strip_prefix('.').map(|s| s.to_string()))
            .filter(|s| !s.is_empty())
            .collect();
        if !exts.is_empty() {
            dialog = dialog.add_filter("Allowed Files".to_string(), exts);
        }
        if req.mode == 3 {
            cx.open_save_file_dialog(dialog);
        } else {
            if req.mode == 1 {
                dialog = dialog.set_multiple(true);
            }
            cx.open_select_file_dialog(dialog);
        }
    }

    pub(super) fn open_web_download_dialog(
        &mut self,
        cx: &mut Cx,
        view_id: makepad_browser_view::ViewId,
        req: makepad_browser_view::DownloadRequestEvent,
    ) {
        let dialog_id = self.next_dialog_live_id();
        self.pending_web_downloads
            .insert(dialog_id, (view_id, req.download_id));
        let mut dialog = FileDialog::new()
            .set_id(LiveId(dialog_id))
            .set_filename(req.suggested_name);
        if let Some(dir) = std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Downloads"))
            .filter(|p| p.is_dir())
        {
            dialog = dialog.set_location(dir);
        }
        cx.open_save_file_dialog(dialog);
    }

    pub(super) fn handle_web_file_dialog_action(&mut self, cx: &mut Cx, action: &FileDialogAction) {
        match action {
            FileDialogAction::FileSelected { id, paths } => {
                if let Some((view_id, request_id)) = self.pending_web_file_dialogs.remove(&id.0) {
                    let strs = paths
                        .iter()
                        .map(|p| p.to_string_lossy().into_owned())
                        .collect();
                    self.web_runtime().borrow_mut().resolve_file_dialog(
                        cx,
                        view_id,
                        request_id,
                        Some(strs),
                    );
                }
            }
            FileDialogAction::FileCancelled { id } => {
                if let Some((view_id, request_id)) = self.pending_web_file_dialogs.remove(&id.0) {
                    self.web_runtime()
                        .borrow_mut()
                        .resolve_file_dialog(cx, view_id, request_id, None);
                }
            }
            FileDialogAction::SaveFileSelected { id, path } => {
                let path_str = path.to_string_lossy().into_owned();
                if let Some((view_id, request_id)) = self.pending_web_file_dialogs.remove(&id.0) {
                    self.web_runtime().borrow_mut().resolve_file_dialog(
                        cx,
                        view_id,
                        request_id,
                        Some(vec![path_str]),
                    );
                } else if let Some((view_id, download_id)) =
                    self.pending_web_downloads.remove(&id.0)
                {
                    self.web_runtime().borrow_mut().resolve_download(
                        cx,
                        view_id,
                        download_id,
                        Some(path_str),
                    );
                }
            }
            FileDialogAction::SaveFileCancelled { id } => {
                if let Some((view_id, request_id)) = self.pending_web_file_dialogs.remove(&id.0) {
                    self.web_runtime()
                        .borrow_mut()
                        .resolve_file_dialog(cx, view_id, request_id, None);
                } else if let Some((view_id, download_id)) =
                    self.pending_web_downloads.remove(&id.0)
                {
                    self.web_runtime().borrow_mut().resolve_download(
                        cx,
                        view_id,
                        download_id,
                        None,
                    );
                }
            }
            FileDialogAction::FolderSelected(path) => {
                if let Some((view_id, request_id)) = self.pending_web_folder_dialog.take() {
                    let path_str = path.to_string_lossy().into_owned();
                    self.web_runtime().borrow_mut().resolve_file_dialog(
                        cx,
                        view_id,
                        request_id,
                        Some(vec![path_str]),
                    );
                }
            }
            FileDialogAction::FolderCancelled => {
                if let Some((view_id, request_id)) = self.pending_web_folder_dialog.take() {
                    self.web_runtime()
                        .borrow_mut()
                        .resolve_file_dialog(cx, view_id, request_id, None);
                }
            }
            FileDialogAction::FileLoaded { .. } | FileDialogAction::None => {}
        }
    }

    pub(super) fn enqueue_web_modal(&mut self, cx: &mut Cx, request: WebModalRequest) {
        self.web_modal_queue.push_back(request);
        if self.active_web_modal.is_none() {
            self.present_next_web_modal(cx);
        }
    }

    fn present_next_web_modal(&mut self, cx: &mut Cx) {
        let Some(request) = self.web_modal_queue.pop_front() else {
            self.apply_focus(cx);
            return;
        };
        let input = self.ui.text_input(cx, ids!(web_modal_input));
        let password = self.ui.text_input(cx, ids!(web_modal_password));
        match &request {
            WebModalRequest::JsDialog { req, .. } => {
                let origin = if req.origin_url.is_empty() {
                    "Page".to_string()
                } else {
                    koma_core::input::derive_url_label(&req.origin_url)
                };
                let message = if req.kind == 3 && req.message.is_empty() {
                    "Changes you made may not be saved.".to_string()
                } else {
                    req.message.clone()
                };
                self.ui
                    .label(cx, ids!(web_modal_origin))
                    .set_text(cx, &origin);
                self.ui
                    .label(cx, ids!(web_modal_message))
                    .set_text(cx, &message);
                let is_prompt = req.kind == 2;
                input.set_visible(cx, is_prompt);
                password.set_visible(cx, false);
                if is_prompt {
                    input.set_text(cx, &req.default_prompt);
                } else {
                    input.set_text(cx, "");
                }
                password.set_text(cx, "");
                let (title, confirm, cancel) = match req.kind {
                    0 => ("Alert", "OK", ""),
                    1 => ("Confirm", "OK", "Cancel"),
                    2 => ("Prompt", "OK", "Cancel"),
                    _ => ("Leave page?", "Leave", "Stay"),
                };
                self.active_web_modal = Some(request);
                self.ui
                    .dialog(cx, ids!(web_modal_dialog))
                    .ask(cx, title, confirm, cancel);
                if is_prompt {
                    input.set_key_focus(cx);
                }
            }
            WebModalRequest::Auth { req, .. } => {
                let target = if req.is_proxy {
                    format!("Proxy {}:{}", req.host, req.port)
                } else {
                    format!("{}:{}", req.host, req.port)
                };
                let origin = if req.realm.is_empty() {
                    format!("{target} ({})", req.scheme)
                } else {
                    format!("{target} — {}", req.realm)
                };
                self.ui
                    .label(cx, ids!(web_modal_origin))
                    .set_text(cx, &origin);
                self.ui
                    .label(cx, ids!(web_modal_message))
                    .set_text(cx, "Enter your username and password.");
                input.set_visible(cx, true);
                input.set_text(cx, "");
                password.set_visible(cx, true);
                password.set_text(cx, "");
                self.active_web_modal = Some(request);
                self.ui.dialog(cx, ids!(web_modal_dialog)).ask(
                    cx,
                    &format!("Sign in to {target}"),
                    "Sign In",
                    "Cancel",
                );
                input.set_key_focus(cx);
            }
            WebModalRequest::Permission { req, .. } => {
                let origin = if req.origin.is_empty() {
                    "This site".to_string()
                } else {
                    req.origin.clone()
                };
                let labels = site_permission_labels(req.permissions);
                self.ui
                    .label(cx, ids!(web_modal_origin))
                    .set_text(cx, &origin);
                self.ui
                    .label(cx, ids!(web_modal_message))
                    .set_text(cx, &format!("Allow access to {}?", labels.join(", ")));
                input.set_visible(cx, false);
                input.set_text(cx, "");
                password.set_visible(cx, false);
                password.set_text(cx, "");
                self.active_web_modal = Some(request);
                self.ui.dialog(cx, ids!(web_modal_dialog)).ask(
                    cx,
                    "Permission request",
                    "Allow",
                    "Deny",
                );
            }
        }
    }

    pub(super) fn resolve_active_web_modal(&mut self, cx: &mut Cx, accepted: bool) {
        let Some(request) = self.active_web_modal.take() else {
            return;
        };
        let input_text = self.ui.text_input(cx, ids!(web_modal_input)).text();
        let password_text = self.ui.text_input(cx, ids!(web_modal_password)).text();
        match request {
            WebModalRequest::JsDialog { view_id, req } => {
                let accept = if req.kind == 0 { true } else { accepted };
                let user_input = if accept && req.kind == 2 {
                    input_text
                } else {
                    String::new()
                };
                self.web_runtime().borrow_mut().resolve_js_dialog(
                    cx,
                    view_id,
                    req.request_id,
                    accept,
                    user_input,
                );
            }
            WebModalRequest::Auth { view_id, req } => {
                let username = accepted.then_some(input_text);
                self.web_runtime().borrow_mut().resolve_auth_request(
                    cx,
                    view_id,
                    req.request_id,
                    username,
                    password_text,
                );
            }
            WebModalRequest::Permission { view_id, req } => {
                self.record_site_permission(&req.origin, req.permissions, accepted);
                self.web_runtime().borrow_mut().resolve_permission_request(
                    cx,
                    view_id,
                    req.request_id,
                    accepted,
                );
            }
        }
        self.present_next_web_modal(cx);
    }

    pub(super) fn load_site_permissions(&mut self, state_dir: &Path) {
        let path = state_dir.join("site_permissions.json");
        let Ok(raw) = std::fs::read_to_string(path) else {
            return;
        };
        if let Ok(entries) = HashMap::<String, SitePermissionRecord>::deserialize_json(&raw) {
            self.site_permissions = entries;
        }
    }

    fn save_site_permissions(&self) {
        if self.is_private {
            return;
        }
        let Some(dir) = self.state_dir.as_ref() else {
            return;
        };
        let path = dir.join("site_permissions.json");
        let tmp = dir.join("site_permissions.json.tmp");
        let json = self.site_permissions.serialize_json();
        let _ = std::fs::write(&tmp, json).and_then(|()| std::fs::rename(&tmp, &path));
    }

    pub(super) fn check_site_permission(&self, origin: &str, permissions: u32) -> Option<bool> {
        if origin.is_empty() || (permissions & SITE_PERMISSION_OTHER) != 0 {
            return None;
        }
        let rec = self.site_permissions.get(origin)?;
        let checks = [
            (SITE_PERMISSION_CAMERA, rec.camera),
            (SITE_PERMISSION_MICROPHONE, rec.microphone),
            (SITE_PERMISSION_GEOLOCATION, rec.geolocation),
            (SITE_PERMISSION_DESKTOP_CAPTURE, rec.desktop_capture),
        ];
        let mut any_requested = false;
        let mut all_allowed = true;
        for (bit, saved) in checks {
            if (permissions & bit) != 0 {
                any_requested = true;
                match saved {
                    Some(false) => return Some(false),
                    Some(true) => {}
                    None => all_allowed = false,
                }
            }
        }
        (any_requested && all_allowed).then_some(true)
    }

    fn record_site_permission(&mut self, origin: &str, permissions: u32, granted: bool) {
        if origin.is_empty() || (permissions & SITE_PERMISSION_OTHER) != 0 {
            return;
        }
        let rec = self.site_permissions.entry(origin.to_string()).or_default();
        if (permissions & SITE_PERMISSION_CAMERA) != 0 {
            rec.camera = Some(granted);
        }
        if (permissions & SITE_PERMISSION_MICROPHONE) != 0 {
            rec.microphone = Some(granted);
        }
        if (permissions & SITE_PERMISSION_GEOLOCATION) != 0 {
            rec.geolocation = Some(granted);
        }
        if (permissions & SITE_PERMISSION_DESKTOP_CAPTURE) != 0 {
            rec.desktop_capture = Some(granted);
        }
        self.save_site_permissions();
    }
}
