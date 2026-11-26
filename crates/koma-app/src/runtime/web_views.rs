//! CEF `BrowserViews` runtime controller shared across Sheets, Komas, and IPC requests.

use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use koma_core::model::KomaId;
use koma_ipc::protocol::{IpcPayload, IpcResponse, WebScriptTargetData};
use makepad_browser_view::{
    AuthRequestEvent, BrowserViews, DownloadRequestEvent, DownloadUpdateEvent, FileDialogEvent,
    JsDialogEvent, RingShown, SitePermissionEvent, ViewId, ViewOp,
};
use makepad_widgets::*;

use super::web_notifications::{NotificationBridge, WebNotification};

mod devtools;
mod js_bridge;

pub use js_bridge::{
    build_koma_api_inject_expr, build_koma_click_expr, build_koma_fill_expr, build_koma_press_expr,
    build_koma_read_expr, build_koma_scroll_expr, build_koma_snapshot_expr, build_koma_text_expr,
    build_web_navigation_inject_expr, KomaApiEvent, WebNavEvent,
};
use js_bridge::{
    parse_action_response, parse_koma_api_event, parse_read_response, parse_snapshot_response,
    parse_text_response, parse_web_nav_event,
};

const SCRIPT_TARGET_WAIT: Duration = Duration::from_secs(8);

enum PendingEval {
    Raw {
        koma_id: KomaId,
        reply: mpsc::SyncSender<IpcResponse>,
    },
    Snapshot {
        koma_id: KomaId,
        reply: mpsc::SyncSender<IpcResponse>,
    },
    Read {
        koma_id: KomaId,
        reply: mpsc::SyncSender<IpcResponse>,
    },
    Text {
        koma_id: KomaId,
        reply: mpsc::SyncSender<IpcResponse>,
    },
    Action {
        koma_id: KomaId,
        reply: mpsc::SyncSender<IpcResponse>,
    },
}

pub fn should_record_url_change(last: Option<&str>, next: &str) -> bool {
    !next.is_empty() && next != "about:blank" && last != Some(next)
}

struct PendingScriptTarget {
    koma_id: KomaId,
    started: Instant,
    reply: mpsc::SyncSender<IpcResponse>,
}

#[derive(Default)]
struct KomaViewState {
    size: Option<(Vec2d, f64)>,
    hidden: Option<bool>,
    koma_api_enabled: bool,
    koma_api_injected_url: Option<String>,
    web_nav_injected_epoch: Option<u64>,
    last_observed_url: Option<String>,
}

#[derive(Default)]
struct PendingEvents {
    popups: Vec<String>,
    popup_views: Vec<(ViewId, String)>,
    external_urls: Vec<String>,
    file_dialogs: Vec<(ViewId, FileDialogEvent)>,
    download_requests: Vec<(ViewId, DownloadRequestEvent)>,
    download_updates: Vec<DownloadUpdateEvent>,
    js_dialogs: Vec<(ViewId, JsDialogEvent)>,
    auth_requests: Vec<(ViewId, AuthRequestEvent)>,
    permission_requests: Vec<(ViewId, SitePermissionEvent)>,
    closed_komas: Vec<KomaId>,
    koma_api: Vec<KomaApiEvent>,
    web_nav: Vec<WebNavEvent>,
    web_notifications: Vec<WebNotification>,
    url_changes: Vec<(KomaId, String)>,
}

pub struct WebViewRuntime {
    views: BrowserViews,
    koma_to_view: HashMap<KomaId, ViewId>,
    view_to_koma: HashMap<ViewId, KomaId>,
    koma_states: HashMap<KomaId, KomaViewState>,
    focused_koma: Option<KomaId>,
    pending_evals: HashMap<u64, PendingEval>,
    pending_script_targets: Vec<PendingScriptTarget>,
    pending: PendingEvents,
    notification_bridges: HashMap<ViewId, NotificationBridge>,
    default_page_zoom: f64,
    last_size: Option<(Vec2d, f64)>,
    last_event: Option<(u64, bool)>,
}

impl WebViewRuntime {
    pub fn new(profile_dir: std::path::PathBuf) -> Self {
        Self {
            views: BrowserViews::new_with_profile_dir(profile_dir),
            koma_to_view: HashMap::new(),
            view_to_koma: HashMap::new(),
            koma_states: HashMap::new(),
            focused_koma: None,
            pending_evals: HashMap::new(),
            pending_script_targets: Vec::new(),
            pending: PendingEvents::default(),
            notification_bridges: HashMap::new(),
            default_page_zoom: 1.0,
            last_size: None,
            last_event: None,
        }
    }

    pub fn set_default_page_zoom(&mut self, factor: f64) {
        self.default_page_zoom = factor;
    }

    pub fn view_count(&self) -> usize {
        self.koma_to_view.len()
    }

    pub fn open_extra_koma(
        &mut self,
        cx: &mut Cx,
        koma_id: &KomaId,
        url: &str,
    ) -> Result<usize, String> {
        let (size, dpi) = self.last_size.unwrap_or((dvec2(640.0, 680.0), 2.0));
        self.ensure_koma(cx, koma_id, url, false, size, dpi)
            .ok_or_else(|| format!("Failed to open extra koma: {koma_id}"))?;
        Ok(self.koma_to_view.len())
    }

    pub fn commit_text(&mut self, cx: &mut Cx, koma_id: &KomaId, text: &str) -> Result<(), String> {
        let id = self
            .koma_to_view
            .get(koma_id)
            .copied()
            .ok_or_else(|| format!("Web view not open: {koma_id}"))?;
        self.views.send(id, ViewOp::Focus { on: true });
        self.views.send(
            id,
            ViewOp::Text {
                text: text.to_string(),
            },
        );
        self.views.service(cx);
        Ok(())
    }

    pub fn views_mut(&mut self) -> &mut BrowserViews {
        &mut self.views
    }

    pub fn view_id(&self, koma_id: &KomaId) -> Option<ViewId> {
        self.koma_to_view.get(koma_id).copied()
    }

    pub fn focused_view_id(&self) -> Option<ViewId> {
        self.focused_koma
            .as_ref()
            .and_then(|id| self.koma_to_view.get(id).copied())
    }

    pub fn ensure_koma(
        &mut self,
        cx: &mut Cx,
        koma_id: &KomaId,
        url: &str,
        visible: bool,
        size: Vec2d,
        dpi: f64,
    ) -> Option<ViewId> {
        if let Some(id) = self.koma_to_view.get(koma_id).copied() {
            return Some(id);
        }
        let bridge = NotificationBridge::new()
            .map_err(|err| {
                eprintln!("Could not initialize Web notifications: {err}");
            })
            .ok();
        let mut document_script = String::new();
        if let Some(bridge) = bridge.as_ref() {
            document_script.push_str(&bridge.script());
            document_script.push('\n');
        }
        document_script.push_str(&build_web_navigation_inject_expr());
        document_script.push(';');
        let id =
            self.views
                .open_with_document_script(url, true, 0xff10_1521, Some(document_script))?;
        if let Some(bridge) = bridge {
            self.notification_bridges.insert(id, bridge);
        }
        self.koma_to_view.insert(koma_id.clone(), id);
        self.view_to_koma.insert(id, koma_id.clone());
        let state = self.koma_states.entry(koma_id.clone()).or_default();
        state.size = Some((size, dpi));
        state.hidden = Some(!visible);
        state.last_observed_url = Some(url.to_string());
        self.last_size = Some((size, dpi));
        self.views.set_size(id, size, dpi);
        self.views.send(id, ViewOp::Hidden { on: !visible });
        if self.default_page_zoom != 1.0 {
            self.views.send(
                id,
                ViewOp::PageZoom {
                    factor: self.default_page_zoom,
                },
            );
        }
        self.views.service(cx);
        Some(id)
    }

    pub fn close_koma(&mut self, cx: &mut Cx, koma_id: &KomaId) {
        self.pending_script_targets.retain(|pending| {
            if &pending.koma_id != koma_id {
                return true;
            }
            let _ = pending
                .reply
                .send(IpcResponse::err(format!("Web Koma closed: {koma_id}")));
            false
        });
        let Some(id) = self.koma_to_view.remove(koma_id) else {
            return;
        };
        self.view_to_koma.remove(&id);
        self.notification_bridges.remove(&id);
        self.koma_states.remove(koma_id);
        if self.focused_koma.as_ref() == Some(koma_id) {
            self.focused_koma = None;
        }
        self.views.close(id);
        self.views.service(cx);
    }

    fn script_target(&self, koma_id: &KomaId) -> Option<IpcResponse> {
        let state = self.views.state(self.view_id(koma_id)?)?;
        if let Some(error) = &state.error {
            return Some(IpcResponse::err(error));
        }
        Some(match state.devtools_target.as_ref()? {
            Ok(target_id) => IpcResponse::Ok(IpcPayload::WebScriptTarget(WebScriptTargetData {
                koma_id: koma_id.to_string(),
                port: self.views.devtools_port()?,
                target_id: target_id.clone(),
            })),
            Err(err) => IpcResponse::err(err),
        })
    }

    pub fn script_target_for_ipc(
        &mut self,
        cx: &mut Cx,
        koma_id: KomaId,
        reply: mpsc::SyncSender<IpcResponse>,
    ) {
        if let Some(response) = self.script_target(&koma_id) {
            let _ = reply.send(response);
        } else {
            cx.start_timeout(SCRIPT_TARGET_WAIT.as_secs_f64());
            self.pending_script_targets.push(PendingScriptTarget {
                koma_id,
                started: Instant::now(),
                reply,
            });
        }
    }

    pub fn set_koma_hidden(&mut self, cx: &mut Cx, koma_id: &KomaId, hidden: bool) {
        let Some(id) = self.koma_to_view.get(koma_id).copied() else {
            return;
        };
        let state = self.koma_states.entry(koma_id.clone()).or_default();
        if state.hidden == Some(hidden) {
            return;
        }
        state.hidden = Some(hidden);
        if hidden {
            self.views.send(id, ViewOp::Focus { on: false });
        }
        self.views.send(id, ViewOp::Hidden { on: hidden });
        self.views.service(cx);
    }

    pub fn set_koma_size(&mut self, cx: &mut Cx, koma_id: &KomaId, size: Vec2d, dpi: f64) {
        self.last_size = Some((size, dpi));
        let Some(id) = self.koma_to_view.get(koma_id).copied() else {
            return;
        };
        let state = self.koma_states.entry(koma_id.clone()).or_default();
        if let Some((prev_size, prev_dpi)) = state.size {
            if (prev_size.x - size.x).abs() < 0.5
                && (prev_size.y - size.y).abs() < 0.5
                && (prev_dpi - dpi).abs() < 0.001
            {
                return;
            }
        }
        state.size = Some((size, dpi));
        self.views.set_size(id, size, dpi);
        self.views.service(cx);
    }

    pub fn focus_koma(&mut self, cx: &mut Cx, koma_id: Option<&KomaId>) {
        if self.focused_koma.as_ref() == koma_id {
            if let Some(id) = koma_id.and_then(|b| self.koma_to_view.get(b).copied()) {
                self.views.send(id, ViewOp::Focus { on: true });
                self.views.service(cx);
            }
            return;
        }
        if let Some(prev_id) = self.focused_view_id() {
            self.views.send(prev_id, ViewOp::Focus { on: false });
        }
        self.focused_koma = koma_id.cloned();
        if let Some(next_id) = self.focused_view_id() {
            self.views.send(next_id, ViewOp::Focus { on: true });
        }
        self.views.service(cx);
    }

    pub fn navigate(&mut self, cx: &mut Cx, koma_id: &KomaId, url: &str) -> bool {
        let Some(id) = self.koma_to_view.get(koma_id).copied() else {
            return false;
        };
        self.views.send(
            id,
            ViewOp::Navigate {
                url: url.to_string(),
            },
        );
        self.views.service(cx);
        true
    }

    pub fn reload(&mut self, cx: &mut Cx, koma_id: &KomaId) -> bool {
        let Some(id) = self.koma_to_view.get(koma_id).copied() else {
            return false;
        };
        self.views.send(id, ViewOp::Reload);
        self.views.service(cx);
        true
    }

    pub fn go_back(&mut self, cx: &mut Cx, koma_id: &KomaId) -> bool {
        let Some(id) = self.koma_to_view.get(koma_id).copied() else {
            return false;
        };
        self.views.send(id, ViewOp::Back);
        self.views.service(cx);
        true
    }

    pub fn go_forward(&mut self, cx: &mut Cx, koma_id: &KomaId) -> bool {
        let Some(id) = self.koma_to_view.get(koma_id).copied() else {
            return false;
        };
        self.views.send(id, ViewOp::Forward);
        self.views.service(cx);
        true
    }

    pub fn koma_frame(&self, koma_id: &KomaId) -> Option<RingShown> {
        self.koma_to_view
            .get(koma_id)
            .copied()
            .and_then(|id| self.views.frame(id))
    }

    pub fn koma_url_title(&self, koma_id: &KomaId) -> Option<(String, String)> {
        let id = self.koma_to_view.get(koma_id).copied()?;
        let state = self.views.state(id)?;
        Some((state.url.clone(), state.title.clone()))
    }

    pub fn take_popup_urls(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending.popups)
    }

    pub fn take_popup_views(&mut self) -> Vec<(ViewId, String)> {
        std::mem::take(&mut self.pending.popup_views)
    }

    pub fn take_external_urls(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending.external_urls)
    }

    pub fn take_closed_komas(&mut self) -> Vec<KomaId> {
        std::mem::take(&mut self.pending.closed_komas)
    }

    pub fn take_file_dialogs(&mut self) -> Vec<(ViewId, FileDialogEvent)> {
        std::mem::take(&mut self.pending.file_dialogs)
    }

    pub fn resolve_file_dialog(
        &mut self,
        cx: &mut Cx,
        view_id: ViewId,
        request_id: u64,
        paths: Option<Vec<String>>,
    ) {
        self.views
            .send(view_id, ViewOp::FileDialogResult { request_id, paths });
        self.views.service(cx);
    }

    pub fn take_download_requests(&mut self) -> Vec<(ViewId, DownloadRequestEvent)> {
        std::mem::take(&mut self.pending.download_requests)
    }

    pub fn take_download_updates(&mut self) -> Vec<DownloadUpdateEvent> {
        std::mem::take(&mut self.pending.download_updates)
    }

    pub fn resolve_download(
        &mut self,
        cx: &mut Cx,
        view_id: ViewId,
        download_id: u32,
        save_path: Option<String>,
    ) {
        self.views.send(
            view_id,
            ViewOp::DownloadResult {
                download_id,
                save_path,
            },
        );
        self.views.service(cx);
    }

    pub fn take_js_dialogs(&mut self) -> Vec<(ViewId, JsDialogEvent)> {
        std::mem::take(&mut self.pending.js_dialogs)
    }

    pub fn resolve_js_dialog(
        &mut self,
        cx: &mut Cx,
        view_id: ViewId,
        request_id: u64,
        accept: bool,
        user_input: String,
    ) {
        self.views.send(
            view_id,
            ViewOp::JsDialogResult {
                request_id,
                accept,
                user_input,
            },
        );
        self.views.service(cx);
    }

    pub fn take_auth_requests(&mut self) -> Vec<(ViewId, AuthRequestEvent)> {
        std::mem::take(&mut self.pending.auth_requests)
    }

    pub fn resolve_auth_request(
        &mut self,
        cx: &mut Cx,
        view_id: ViewId,
        request_id: u64,
        username: Option<String>,
        password: String,
    ) {
        self.views.send(
            view_id,
            ViewOp::AuthResult {
                request_id,
                username,
                password,
            },
        );
        self.views.service(cx);
    }

    pub fn take_permission_requests(&mut self) -> Vec<(ViewId, SitePermissionEvent)> {
        std::mem::take(&mut self.pending.permission_requests)
    }

    pub fn resolve_permission_request(
        &mut self,
        cx: &mut Cx,
        view_id: ViewId,
        request_id: u64,
        granted: bool,
    ) {
        self.views.send(
            view_id,
            ViewOp::PermissionResult {
                request_id,
                granted,
            },
        );
        self.views.service(cx);
    }

    pub fn bind_popup_view(&mut self, cx: &mut Cx, koma_id: &KomaId, view_id: ViewId, url: &str) {
        self.koma_to_view.insert(koma_id.clone(), view_id);
        self.view_to_koma.insert(view_id, koma_id.clone());
        let (size, dpi) = self.last_size.unwrap_or((dvec2(640.0, 680.0), 2.0));
        let state = self.koma_states.entry(koma_id.clone()).or_default();
        state.size = Some((size, dpi));
        state.hidden = Some(false);
        state.last_observed_url = Some(url.to_string());
        self.views.set_size(view_id, size, dpi);
        self.views.send(view_id, ViewOp::Hidden { on: false });
        if self.default_page_zoom != 1.0 {
            self.views.send(
                view_id,
                ViewOp::PageZoom {
                    factor: self.default_page_zoom,
                },
            );
        }
        self.views.service(cx);
    }

    pub fn take_koma_api_events(&mut self) -> Vec<KomaApiEvent> {
        std::mem::take(&mut self.pending.koma_api)
    }

    pub fn take_web_nav_events(&mut self) -> Vec<WebNavEvent> {
        std::mem::take(&mut self.pending.web_nav)
    }

    pub fn take_web_notifications(&mut self) -> Vec<WebNotification> {
        std::mem::take(&mut self.pending.web_notifications)
    }

    pub fn take_url_changes(&mut self) -> Vec<(KomaId, String)> {
        std::mem::take(&mut self.pending.url_changes)
    }

    pub fn set_koma_api_enabled(&mut self, cx: &mut Cx, koma_id: &KomaId, enabled: bool) {
        let state = self.koma_states.entry(koma_id.clone()).or_default();
        state.koma_api_enabled = enabled;
        if !enabled {
            state.koma_api_injected_url = None;
            if let Some(id) = self.koma_to_view.get(koma_id).copied() {
                self.views.send(id, ViewOp::Reload);
                self.views.service(cx);
            }
        }
    }

    fn start_ipc_eval(
        &mut self,
        cx: &mut Cx,
        koma_id: &KomaId,
        expr: &str,
        make_pending: impl FnOnce(KomaId, mpsc::SyncSender<IpcResponse>) -> PendingEval,
        reply: mpsc::SyncSender<IpcResponse>,
    ) {
        let Some(id) = self.koma_to_view.get(koma_id).copied() else {
            let _ = reply.send(IpcResponse::err(format!("Web view not open: {koma_id}")));
            return;
        };
        let eval_id = self.views.evaluate(id, expr);
        self.pending_evals
            .insert(eval_id, make_pending(koma_id.clone(), reply));
        self.views.service(cx);
    }

    pub fn eval_for_ipc(
        &mut self,
        cx: &mut Cx,
        koma_id: &KomaId,
        expr: &str,
        reply: mpsc::SyncSender<IpcResponse>,
    ) {
        let clean = expr.trim().trim_end_matches(';');
        self.start_ipc_eval(
            cx,
            koma_id,
            clean,
            |b, r| PendingEval::Raw {
                koma_id: b,
                reply: r,
            },
            reply,
        );
    }

    pub fn snapshot_for_ipc(
        &mut self,
        cx: &mut Cx,
        koma_id: &KomaId,
        within: Option<&str>,
        full: bool,
        reply: mpsc::SyncSender<IpcResponse>,
    ) {
        let expr = build_koma_snapshot_expr(within, full);
        self.start_ipc_eval(
            cx,
            koma_id,
            &expr,
            |b, r| PendingEval::Snapshot {
                koma_id: b,
                reply: r,
            },
            reply,
        );
    }

    pub fn read_for_ipc(
        &mut self,
        cx: &mut Cx,
        koma_id: &KomaId,
        within: Option<&str>,
        reply: mpsc::SyncSender<IpcResponse>,
    ) {
        let expr = build_koma_read_expr(within);
        self.start_ipc_eval(
            cx,
            koma_id,
            &expr,
            |b, r| PendingEval::Read {
                koma_id: b,
                reply: r,
            },
            reply,
        );
    }

    pub fn text_for_ipc(
        &mut self,
        cx: &mut Cx,
        koma_id: &KomaId,
        reply: mpsc::SyncSender<IpcResponse>,
    ) {
        let expr = build_koma_text_expr();
        self.start_ipc_eval(
            cx,
            koma_id,
            &expr,
            |b, r| PendingEval::Text {
                koma_id: b,
                reply: r,
            },
            reply,
        );
    }

    pub fn click_for_ipc(
        &mut self,
        cx: &mut Cx,
        koma_id: &KomaId,
        target: &str,
        reply: mpsc::SyncSender<IpcResponse>,
    ) {
        let expr = build_koma_click_expr(target);
        self.start_ipc_eval(
            cx,
            koma_id,
            &expr,
            |b, r| PendingEval::Action {
                koma_id: b,
                reply: r,
            },
            reply,
        );
    }

    pub fn fill_for_ipc(
        &mut self,
        cx: &mut Cx,
        koma_id: &KomaId,
        target: &str,
        text: &str,
        reply: mpsc::SyncSender<IpcResponse>,
    ) {
        let expr = build_koma_fill_expr(target, text);
        self.start_ipc_eval(
            cx,
            koma_id,
            &expr,
            |b, r| PendingEval::Action {
                koma_id: b,
                reply: r,
            },
            reply,
        );
    }

    pub fn press_for_ipc(
        &mut self,
        cx: &mut Cx,
        koma_id: &KomaId,
        key: &str,
        reply: mpsc::SyncSender<IpcResponse>,
    ) {
        let expr = build_koma_press_expr(key);
        self.start_ipc_eval(
            cx,
            koma_id,
            &expr,
            |b, r| PendingEval::Action {
                koma_id: b,
                reply: r,
            },
            reply,
        );
    }

    pub fn scroll_for_ipc(
        &mut self,
        cx: &mut Cx,
        koma_id: &KomaId,
        target: &str,
        reply: mpsc::SyncSender<IpcResponse>,
    ) {
        let expr = build_koma_scroll_expr(target);
        self.start_ipc_eval(
            cx,
            koma_id,
            &expr,
            |b, r| PendingEval::Action {
                koma_id: b,
                reply: r,
            },
            reply,
        );
    }

    /// Pumps `BrowserViews` at most once per `cx.event_id()` even when multiple widgets share this runtime.
    pub fn pump_event(&mut self, cx: &mut Cx, event: &Event) -> bool {
        let event_id = cx.event_id();
        if let Some((last_id, last_changed)) = self.last_event {
            if last_id == event_id {
                return last_changed;
            }
        }

        let mut changed = self.views.handle_event(cx, event);
        let ids: Vec<ViewId> = self.view_to_koma.keys().copied().collect();

        for id in ids {
            let koma_id_opt = self.view_to_koma.get(&id).cloned();
            if let Some(koma_id) = &koma_id_opt {
                if let Some((loading, url, epoch)) = self
                    .views
                    .state(id)
                    .map(|s| (s.loading, s.url.clone(), s.navigation_epoch))
                {
                    if !loading && !url.is_empty() {
                        let state = self.koma_states.entry(koma_id.clone()).or_default();
                        if should_record_url_change(state.last_observed_url.as_deref(), &url) {
                            state.last_observed_url = Some(url.clone());
                            self.pending
                                .url_changes
                                .push((koma_id.clone(), url.clone()));
                            changed = true;
                        }
                        if state.web_nav_injected_epoch != Some(epoch) {
                            state.web_nav_injected_epoch = Some(epoch);
                            let expr = build_web_navigation_inject_expr();
                            self.views.evaluate(id, &expr);
                            self.views.service(cx);
                        }
                        if state.koma_api_enabled
                            && is_local_html_url(&url)
                            && state.koma_api_injected_url.as_deref() != Some(url.as_str())
                        {
                            state.koma_api_injected_url = Some(url);
                            let expr = build_koma_api_inject_expr();
                            self.views.evaluate(id, &expr);
                            self.views.service(cx);
                        }
                    }
                }
            }
            let console = self.views.take_console(id);
            let notification_source = self
                .notification_bridges
                .get(&id)
                .zip(koma_id_opt.as_ref())
                .zip(self.views.state(id));
            for line in console {
                if let Some(notification) =
                    notification_source.and_then(|((bridge, koma), state)| {
                        bridge.receive(koma, &state.url, &line.message)
                    })
                {
                    self.pending.web_notifications.push(notification);
                    continue;
                }
                if let Some(payload) = line.message.strip_prefix("__KOMA_WEB_NAV__:") {
                    if let Some(ev) = koma_id_opt
                        .as_ref()
                        .and_then(|koma_id| parse_web_nav_event(koma_id, payload))
                    {
                        self.pending.web_nav.push(ev);
                        changed = true;
                    }
                    continue;
                }
                if let Some(payload) = line.message.strip_prefix("__KOMA_API__:") {
                    if let Some(koma_id) = &koma_id_opt {
                        let allowed = self
                            .koma_states
                            .get(koma_id)
                            .is_some_and(|s| s.koma_api_enabled)
                            && self
                                .views
                                .state(id)
                                .is_some_and(|s| is_local_html_url(&s.url));
                        if allowed {
                            if let Some(ev) = parse_koma_api_event(koma_id, payload) {
                                self.pending.koma_api.push(ev);
                                changed = true;
                            }
                        }
                    }
                    continue;
                }
            }
            for (eval_id, result) in self.views.take_evaluations(id) {
                match (self.pending_evals.remove(&eval_id), result) {
                    (Some(PendingEval::Raw { koma_id, reply }), Ok(val)) => {
                        let _ = reply.send(IpcResponse::Ok(IpcPayload::WebEval {
                            koma_id: koma_id.into_string(),
                            value: val,
                        }));
                    }
                    (Some(PendingEval::Snapshot { koma_id, reply }), Ok(val)) => {
                        let _ = reply.send(parse_snapshot_response(koma_id, &val));
                    }
                    (Some(PendingEval::Read { koma_id, reply }), Ok(val)) => {
                        let _ = reply.send(parse_read_response(koma_id, &val));
                    }
                    (Some(PendingEval::Text { koma_id, reply }), Ok(val)) => {
                        let _ = reply.send(parse_text_response(koma_id, &val));
                    }
                    (Some(PendingEval::Action { koma_id, reply }), Ok(val)) => {
                        let _ = reply.send(parse_action_response(koma_id, &val));
                    }
                    (
                        Some(
                            PendingEval::Raw { reply, .. }
                            | PendingEval::Snapshot { reply, .. }
                            | PendingEval::Read { reply, .. }
                            | PendingEval::Text { reply, .. }
                            | PendingEval::Action { reply, .. },
                        ),
                        Err(err),
                    ) => {
                        let _ = reply.send(IpcResponse::err(err));
                    }
                    (_, Ok(_)) => {}
                    (_, Err(err)) => {
                        eprintln!("[koma:web] eval {eval_id}: {err}");
                    }
                }
            }
            let popups = self.views.take_popups(id);
            if !popups.is_empty() {
                self.pending.popups.extend(popups);
                changed = true;
            }
            let popup_views = self.views.take_popup_views(id);
            if !popup_views.is_empty() {
                self.pending.popup_views.extend(popup_views);
                changed = true;
            }
            let external_urls = self.views.take_external_urls(id);
            if !external_urls.is_empty() {
                self.pending.external_urls.extend(external_urls);
                changed = true;
            }
            let file_dialogs = self.views.take_file_dialogs(id);
            if !file_dialogs.is_empty() {
                self.pending
                    .file_dialogs
                    .extend(file_dialogs.into_iter().map(|req| (id, req)));
                changed = true;
            }
            let download_requests = self.views.take_download_requests(id);
            if !download_requests.is_empty() {
                self.pending
                    .download_requests
                    .extend(download_requests.into_iter().map(|req| (id, req)));
                changed = true;
            }
            let download_updates = self.views.take_download_updates(id);
            if !download_updates.is_empty() {
                self.pending.download_updates.extend(download_updates);
                changed = true;
            }
            let js_dialogs = self.views.take_js_dialogs(id);
            if !js_dialogs.is_empty() {
                self.pending
                    .js_dialogs
                    .extend(js_dialogs.into_iter().map(|req| (id, req)));
                changed = true;
            }
            let auth_requests = self.views.take_auth_requests(id);
            if !auth_requests.is_empty() {
                self.pending
                    .auth_requests
                    .extend(auth_requests.into_iter().map(|req| (id, req)));
                changed = true;
            }
            let permission_requests = self.views.take_permission_requests(id);
            if !permission_requests.is_empty() {
                self.pending
                    .permission_requests
                    .extend(permission_requests.into_iter().map(|req| (id, req)));
                changed = true;
            }
            if self.views.take_closed(id) {
                if let Some(koma_id) = self.view_to_koma.get(&id).cloned() {
                    self.pending.closed_komas.push(koma_id);
                    changed = true;
                }
            }
        }

        if !self.pending_script_targets.is_empty() {
            let mut requests = std::mem::take(&mut self.pending_script_targets);
            requests.retain(|pending| {
                let response = self.script_target(&pending.koma_id).or_else(|| {
                    (pending.started.elapsed() >= SCRIPT_TARGET_WAIT).then(|| {
                        IpcResponse::err(format!("DevTools target not ready: {}", pending.koma_id))
                    })
                });
                let Some(response) = response else {
                    return true;
                };
                let _ = pending.reply.send(response);
                false
            });
            self.pending_script_targets = requests;
        }
        self.last_event = Some((event_id, changed));
        changed
    }

    pub fn shutdown(&mut self, cx: &mut Cx) {
        self.views.shutdown(cx);
        self.koma_to_view.clear();
        self.view_to_koma.clear();
        self.koma_states.clear();
        self.pending_evals.clear();
        for pending in self.pending_script_targets.drain(..) {
            let _ = pending
                .reply
                .send(IpcResponse::err("Browser host is shutting down"));
        }
        self.pending = PendingEvents::default();
    }

    pub fn koma_ime_caret(&self, koma_id: &KomaId) -> Option<(i32, i32, i32, i32)> {
        let id = self.koma_to_view.get(koma_id).copied()?;
        self.views.state(id)?.ime_caret
    }
}

pub fn is_local_html_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    let path_part = lower.split(['?', '#']).next().unwrap_or("");
    lower.starts_with("file://") && (path_part.ends_with(".html") || path_part.ends_with(".htm"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_local_html_url_matches_file_html_only() {
        assert!(is_local_html_url("file:///tmp/report.html"));
        assert!(is_local_html_url("FILE:///tmp/index.HTM?x=1"));
        assert!(!is_local_html_url("https://example.com/a.html"));
        assert!(!is_local_html_url("file:///tmp/data.json"));
    }

    #[test]
    fn url_change_ignores_blank_and_unchanged_urls() {
        assert!(!should_record_url_change(None, ""));
        assert!(!should_record_url_change(None, "about:blank"));
        assert!(!should_record_url_change(
            Some("https://example.com/"),
            "https://example.com/"
        ));
        assert!(should_record_url_change(
            Some("https://example.com/"),
            "https://example.com/next"
        ));
    }
}
