//! CEF `BrowserViews` runtime controller shared across Sheets, Komas, and IPC requests.

use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use koma_core::model::KomaId;
use koma_ipc::protocol::{
    IpcPayload, IpcResponse, WebReadData, WebScriptTargetData, WebSnapshotData,
};
use makepad_browser_view::{BrowserViews, RingShown, ViewId, ViewOp};
use makepad_micro_serde::*;
use makepad_widgets::*;

const WEB_DOM_JS: &str = include_str!("../../assets/WebDOM.js");
const WEB_READ_JS: &str = include_str!("../../assets/WebRead.js");
const WEB_NAVIGATION_JS: &str = include_str!("../../assets/WebNavigation.js");
const KOMA_API_JS: &str = include_str!("../../assets/KomaAPI.js");
const SCRIPT_TARGET_WAIT: Duration = Duration::from_secs(8);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebNavEvent {
    pub koma_id: KomaId,
    pub action: String,
    pub url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KomaApiEvent {
    FocusKoma {
        caller_koma_id: KomaId,
        target_koma_id: KomaId,
    },
    ShowToast {
        caller_koma_id: KomaId,
        message: String,
        severity: String,
    },
}

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

#[derive(DeJson)]
struct SnapshotResult {
    ok: bool,
    snapshot: Option<String>,
    error: Option<String>,
}

#[derive(DeJson)]
struct ReadResult {
    ok: bool,
    markdown: Option<String>,
    title: Option<String>,
    url: Option<String>,
    error: Option<String>,
}

#[derive(DeJson)]
struct TextResult {
    ok: bool,
    text: Option<String>,
    error: Option<String>,
}

#[derive(DeJson)]
struct ActionResult {
    ok: bool,
    error: Option<String>,
}

#[derive(DeJson)]
struct KomaApiMessage {
    action: String,
    #[rename(komaId)]
    koma_id: Option<String>,
    message: Option<String>,
    severity: Option<String>,
}

#[derive(DeJson)]
struct WebNavMessage {
    action: String,
    url: Option<String>,
}

fn js_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Builds a semicolon-free IIFE wrapping `WebDOM.js` for `cef.evaluate_javascript`.
pub fn build_koma_snapshot_expr() -> String {
    format!("(() => {{\n{WEB_DOM_JS}\nreturn window.__komaWebDOM.komaSnapshot();\n}})()")
}

pub fn build_koma_read_expr(within: Option<&str>) -> String {
    let scope = within.map(js_string).unwrap_or_else(|| "null".into());
    format!(
        "(() => {{\nif (!window.__komaWebDOM) {{\n{WEB_DOM_JS}\n}}\nif (!globalThis.komaWebRead) {{\n{WEB_READ_JS}\n}}\ntry {{\n  const r = globalThis.komaWebRead.read({scope});\n  return {{ ok: true, markdown: r.markdown, title: r.title, url: r.url }};\n}} catch (e) {{\n  return {{ ok: false, error: String(e && e.message ? e.message : e) }};\n}}\n}})()"
    )
}

pub fn build_koma_text_expr() -> String {
    format!(
        "(() => {{\nif (!globalThis.komaWebRead) {{\n{WEB_READ_JS}\n}}\ntry {{\n  return {{ ok: true, text: globalThis.komaWebRead.text() }};\n}} catch (e) {{\n  return {{ ok: false, error: String(e && e.message ? e.message : e) }};\n}}\n}})()"
    )
}

pub fn build_koma_click_expr(target: &str) -> String {
    let t = js_string(target);
    format!(
        "(() => {{\nif (!window.__komaWebDOM) {{\n{WEB_DOM_JS}\n}}\ntry {{\n  const el = window.__komaWebDOM.komaResolveTarget({t});\n  if (!el) return {{ ok: false, error: 'Target not found: ' + {t} }};\n  el.scrollIntoView({{ block: 'center', inline: 'center' }});\n  el.click();\n  return {{ ok: true }};\n}} catch (e) {{\n  return {{ ok: false, error: String(e && e.message ? e.message : e) }};\n}}\n}})()"
    )
}

pub fn build_koma_fill_expr(target: &str, text: &str) -> String {
    let t = js_string(target);
    let v = js_string(text);
    format!(
        "(() => {{\nif (!window.__komaWebDOM) {{\n{WEB_DOM_JS}\n}}\ntry {{\n  const el = window.__komaWebDOM.komaResolveTarget({t});\n  if (!el) return {{ ok: false, error: 'Target not found: ' + {t} }};\n  el.focus();\n  window.__komaWebDOM.komaSetValue(el, {v});\n  window.__komaWebDOM.komaDispatchInput(el);\n  return {{ ok: true }};\n}} catch (e) {{\n  return {{ ok: false, error: String(e && e.message ? e.message : e) }};\n}}\n}})()"
    )
}

pub fn build_koma_api_inject_expr() -> String {
    format!(
        "(() => {{\nconst komaAPITransport = {{ postMessage(msg) {{ console.debug('__KOMA_API__:' + JSON.stringify(msg)); return Promise.resolve(); }} }};\n{KOMA_API_JS}\nreturn true;\n}})()"
    )
}

pub fn build_web_navigation_inject_expr() -> String {
    format!(
        "(() => {{\nglobalThis.__komaWebNavTransport = {{ postMessage(msg) {{ console.debug('__KOMA_WEB_NAV__:' + JSON.stringify(msg)); }} }};\n{WEB_NAVIGATION_JS}\nwindow.__komaWebNavigation?.configure({{ enabled: true, alphabet: 'asdfghjkl', reduceMotion: false, ignoredHosts: [], paused: false }});\nreturn true;\n}})()"
    )
}

pub fn should_record_url_change(last: Option<&str>, next: &str) -> bool {
    !next.is_empty() && next != "about:blank" && last != Some(next)
}

struct PendingScriptTarget {
    koma_id: KomaId,
    started: Instant,
    reply: mpsc::SyncSender<IpcResponse>,
}

pub struct WebViewRuntime {
    views: BrowserViews,
    koma_to_view: HashMap<KomaId, ViewId>,
    view_to_koma: HashMap<ViewId, KomaId>,
    koma_sizes: HashMap<KomaId, (Vec2d, f64)>,
    koma_hidden: HashMap<KomaId, bool>,
    focused_koma: Option<KomaId>,
    pending_evals: HashMap<u64, PendingEval>,
    pending_script_targets: Vec<PendingScriptTarget>,
    pending_popups: Vec<String>,
    koma_api_enabled: HashMap<KomaId, bool>,
    koma_api_injected_url: HashMap<KomaId, String>,
    pending_koma_api: Vec<KomaApiEvent>,
    web_nav_injected_url: HashMap<KomaId, String>,
    pending_web_nav: Vec<WebNavEvent>,
    last_observed_url: HashMap<KomaId, String>,
    pending_url_changes: Vec<(KomaId, String)>,
    last_size: Option<(Vec2d, f64)>,
    last_event: Option<(u64, bool)>,
}

impl WebViewRuntime {
    pub fn new(profile_dir: std::path::PathBuf) -> Self {
        Self {
            views: BrowserViews::new_with_profile_dir(profile_dir),
            koma_to_view: HashMap::new(),
            view_to_koma: HashMap::new(),
            koma_sizes: HashMap::new(),
            koma_hidden: HashMap::new(),
            focused_koma: None,
            pending_evals: HashMap::new(),
            pending_script_targets: Vec::new(),
            pending_popups: Vec::new(),
            koma_api_enabled: HashMap::new(),
            koma_api_injected_url: HashMap::new(),
            pending_koma_api: Vec::new(),
            web_nav_injected_url: HashMap::new(),
            pending_web_nav: Vec::new(),
            last_observed_url: HashMap::new(),
            pending_url_changes: Vec::new(),
            last_size: None,
            last_event: None,
        }
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
        let id = self.views.open(url, true, 0xff10_1521)?;
        self.koma_to_view.insert(koma_id.clone(), id);
        self.view_to_koma.insert(id, koma_id.clone());
        self.koma_sizes.insert(koma_id.clone(), (size, dpi));
        self.koma_hidden.insert(koma_id.clone(), !visible);
        self.last_observed_url
            .insert(koma_id.clone(), url.to_string());
        self.last_size = Some((size, dpi));
        self.views.set_size(id, size, dpi);
        self.views.send(id, ViewOp::Hidden { on: !visible });
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
        self.koma_sizes.remove(koma_id);
        self.koma_hidden.remove(koma_id);
        self.koma_api_enabled.remove(koma_id);
        self.koma_api_injected_url.remove(koma_id);
        self.web_nav_injected_url.remove(koma_id);
        self.last_observed_url.remove(koma_id);
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
        if self.koma_hidden.get(koma_id).copied() == Some(hidden) {
            return;
        }
        self.koma_hidden.insert(koma_id.clone(), hidden);
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
        if let Some((prev_size, prev_dpi)) = self.koma_sizes.get(koma_id).copied() {
            if (prev_size.x - size.x).abs() < 0.5
                && (prev_size.y - size.y).abs() < 0.5
                && (prev_dpi - dpi).abs() < 0.001
            {
                return;
            }
        }
        self.koma_sizes.insert(koma_id.clone(), (size, dpi));
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
        std::mem::take(&mut self.pending_popups)
    }

    pub fn take_koma_api_events(&mut self) -> Vec<KomaApiEvent> {
        std::mem::take(&mut self.pending_koma_api)
    }

    pub fn take_web_nav_events(&mut self) -> Vec<WebNavEvent> {
        std::mem::take(&mut self.pending_web_nav)
    }

    pub fn take_url_changes(&mut self) -> Vec<(KomaId, String)> {
        std::mem::take(&mut self.pending_url_changes)
    }

    pub fn set_koma_api_enabled(&mut self, cx: &mut Cx, koma_id: &KomaId, enabled: bool) {
        self.koma_api_enabled.insert(koma_id.clone(), enabled);
        if !enabled {
            self.koma_api_injected_url.remove(koma_id);
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
        reply: mpsc::SyncSender<IpcResponse>,
    ) {
        let expr = build_koma_snapshot_expr();
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
                if let Some((loading, url)) =
                    self.views.state(id).map(|s| (s.loading, s.url.clone()))
                {
                    if !loading && !url.is_empty() {
                        if should_record_url_change(
                            self.last_observed_url.get(koma_id).map(String::as_str),
                            &url,
                        ) {
                            self.last_observed_url.insert(koma_id.clone(), url.clone());
                            self.pending_url_changes
                                .push((koma_id.clone(), url.clone()));
                            changed = true;
                        }
                        let nav_already = self.web_nav_injected_url.get(koma_id) == Some(&url);
                        if !nav_already {
                            self.web_nav_injected_url
                                .insert(koma_id.clone(), url.clone());
                            let expr = build_web_navigation_inject_expr();
                            self.views.evaluate(id, &expr);
                            self.views.service(cx);
                        }
                        if self.koma_api_enabled.get(koma_id).copied().unwrap_or(false)
                            && is_local_html_url(&url)
                        {
                            let already = self.koma_api_injected_url.get(koma_id) == Some(&url);
                            if !already {
                                self.koma_api_injected_url.insert(koma_id.clone(), url);
                                let expr = build_koma_api_inject_expr();
                                self.views.evaluate(id, &expr);
                                self.views.service(cx);
                            }
                        }
                    }
                }
            }
            for line in self.views.take_console(id) {
                if let Some(payload) = line.message.strip_prefix("__KOMA_WEB_NAV__:") {
                    if let Some(koma_id) = &koma_id_opt {
                        if let Ok(msg) = WebNavMessage::deserialize_json(payload) {
                            self.pending_web_nav.push(WebNavEvent {
                                koma_id: koma_id.clone(),
                                action: msg.action,
                                url: msg.url,
                            });
                            changed = true;
                        }
                    }
                    continue;
                }
                if let Some(payload) = line.message.strip_prefix("__KOMA_API__:") {
                    if let Some(koma_id) = &koma_id_opt {
                        let allowed = self.koma_api_enabled.get(koma_id).copied().unwrap_or(false)
                            && self
                                .views
                                .state(id)
                                .is_some_and(|s| is_local_html_url(&s.url));
                        if allowed {
                            if let Ok(msg) = KomaApiMessage::deserialize_json(payload) {
                                match msg.action.as_str() {
                                    "koma.focus" => {
                                        if let Some(target) = msg.koma_id {
                                            self.pending_koma_api.push(KomaApiEvent::FocusKoma {
                                                caller_koma_id: koma_id.clone(),
                                                target_koma_id: target.into(),
                                            });
                                            changed = true;
                                        }
                                    }
                                    "toast.show" => {
                                        if let Some(message) = msg.message {
                                            if !message.trim().is_empty() {
                                                self.pending_koma_api.push(
                                                    KomaApiEvent::ShowToast {
                                                        caller_koma_id: koma_id.clone(),
                                                        message,
                                                        severity: msg
                                                            .severity
                                                            .unwrap_or_else(|| "info".into()),
                                                    },
                                                );
                                                changed = true;
                                            }
                                        }
                                    }
                                    _ => {}
                                }
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
                        let resp = match SnapshotResult::deserialize_json(&val) {
                            Ok(res) if res.ok => {
                                IpcResponse::Ok(IpcPayload::WebSnapshot(WebSnapshotData {
                                    koma_id: koma_id.into_string(),
                                    snapshot: res.snapshot,
                                }))
                            }
                            Ok(res) => IpcResponse::err(
                                res.error.unwrap_or_else(|| "Snapshot failed".into()),
                            ),
                            Err(e) => IpcResponse::err(format!("Snapshot parse failed: {e:?}")),
                        };
                        let _ = reply.send(resp);
                    }
                    (Some(PendingEval::Read { koma_id, reply }), Ok(val)) => {
                        let resp = match ReadResult::deserialize_json(&val) {
                            Ok(res) if res.ok => {
                                IpcResponse::Ok(IpcPayload::WebRead(WebReadData {
                                    koma_id: koma_id.into_string(),
                                    markdown: res.markdown,
                                    title: res.title,
                                    url: res.url,
                                }))
                            }
                            Ok(res) => {
                                IpcResponse::err(res.error.unwrap_or_else(|| "Read failed".into()))
                            }
                            Err(e) => IpcResponse::err(format!("Read parse failed: {e:?}")),
                        };
                        let _ = reply.send(resp);
                    }
                    (Some(PendingEval::Text { koma_id, reply }), Ok(val)) => {
                        let resp = match TextResult::deserialize_json(&val) {
                            Ok(res) if res.ok => IpcResponse::Ok(IpcPayload::Text {
                                koma_id: koma_id.into_string(),
                                text: res.text.unwrap_or_default(),
                            }),
                            Ok(res) => {
                                IpcResponse::err(res.error.unwrap_or_else(|| "Text failed".into()))
                            }
                            Err(e) => IpcResponse::err(format!("Text parse failed: {e:?}")),
                        };
                        let _ = reply.send(resp);
                    }
                    (Some(PendingEval::Action { koma_id, reply }), Ok(val)) => {
                        let resp = match ActionResult::deserialize_json(&val) {
                            Ok(res) if res.ok => IpcResponse::Ok(IpcPayload::KomaId {
                                koma_id: koma_id.into_string(),
                            }),
                            Ok(res) => IpcResponse::err(
                                res.error.unwrap_or_else(|| "Action failed".into()),
                            ),
                            Err(e) => IpcResponse::err(format!("Action parse failed: {e:?}")),
                        };
                        let _ = reply.send(resp);
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
                self.pending_popups.extend(popups);
                changed = true;
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
        self.koma_sizes.clear();
        self.koma_hidden.clear();
        self.pending_evals.clear();
        for pending in self.pending_script_targets.drain(..) {
            let _ = pending
                .reply
                .send(IpcResponse::err("Browser host is shutting down"));
        }
        self.pending_popups.clear();
        self.koma_api_enabled.clear();
        self.koma_api_injected_url.clear();
        self.pending_koma_api.clear();
        self.web_nav_injected_url.clear();
        self.pending_web_nav.clear();
    }

    pub fn koma_ime_caret(&self, koma_id: &KomaId) -> Option<(i32, i32, i32, i32)> {
        let id = self.koma_to_view.get(koma_id).copied()?;
        self.views.state(id)?.ime_caret
    }

    pub fn resolve_devtools_url(&self, koma_id: &KomaId) -> Result<String, String> {
        let port = self
            .views
            .devtools_port()
            .ok_or_else(|| "DevTools unavailable: browser host not ready".to_string())?;
        let target_url = self.koma_url_title(koma_id).map(|(u, _)| u);
        if let Some(json) = fetch_devtools_json(port) {
            if let Some(url) = parse_devtools_frontend_url(&json, target_url.as_deref(), port) {
                return Ok(url);
            }
        }
        Ok(format!("http://127.0.0.1:{port}/"))
    }
}

fn fetch_devtools_json(port: u16) -> Option<String> {
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;

    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let mut stream = None;
    for attempt in 0..10 {
        match TcpStream::connect_timeout(&addr, Duration::from_millis(200)) {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(_) if attempt + 1 < 10 => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return None,
        }
    }
    let mut stream = stream?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
    let req =
        format!("GET /json/list HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).ok()?;

    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    let mut header_end = None;
    let mut content_length = None;
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&buf[..n]);
                if header_end.is_none() {
                    if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        let end = pos + 4;
                        header_end = Some(end);
                        let headers = String::from_utf8_lossy(&raw[..pos]);
                        content_length = headers.lines().find_map(|line| {
                            let (k, v) = line.split_once(':')?;
                            k.trim()
                                .eq_ignore_ascii_case("content-length")
                                .then(|| v.trim().parse::<usize>().ok())?
                        });
                    }
                }
                if let (Some(end), Some(len)) = (header_end, content_length) {
                    if raw.len() >= end + len {
                        break;
                    }
                }
            }
            Err(_) => break,
        }
    }
    let end = header_end?;
    let body = match content_length {
        Some(len) => raw.get(end..end + len)?,
        None => &raw[end..],
    };
    Some(String::from_utf8_lossy(body).trim().to_string())
}

pub fn parse_devtools_frontend_url(
    json: &str,
    target_url: Option<&str>,
    port: u16,
) -> Option<String> {
    let val = JsonValue::deserialize_json(json).ok()?;
    let JsonValue::Array(items) = val else {
        return None;
    };
    let normalize = |raw: &str| -> String {
        if raw.starts_with('/') {
            format!("http://127.0.0.1:{port}{raw}")
        } else {
            raw.to_string()
        }
    };
    let mut first_page: Option<String> = None;
    for item in items {
        let is_page = item
            .key("type")
            .and_then(|v| v.string())
            .is_some_and(|t| t == "page");
        if !is_page {
            continue;
        }
        let resolved = if let Some(ws_url) = item
            .key("webSocketDebuggerUrl")
            .and_then(|v| v.string())
            .and_then(|u| u.strip_prefix("ws://").or_else(|| u.strip_prefix("wss://")))
        {
            format!("devtools://devtools/bundled/inspector.html?ws={ws_url}")
        } else if let Some(frontend) = item.key("devtoolsFrontendUrl").and_then(|v| v.string()) {
            normalize(frontend)
        } else {
            continue;
        };
        let url_match = target_url.filter(|u| !u.is_empty()).is_some_and(|wanted| {
            item.key("url")
                .and_then(|v| v.string())
                .is_some_and(|u| u == wanted)
        });
        if url_match {
            return Some(resolved);
        }
        if first_page.is_none() {
            first_page = Some(resolved);
        }
    }
    first_page
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
    fn parse_devtools_frontend_url_matches_target_and_normalizes_relative_path() {
        let json = r#"[
            {"type":"service_worker","url":"https://a.test/sw.js","devtoolsFrontendUrl":"/devtools/inspector.html?ws=127.0.0.1:9222/sw"},
            {"type":"page","url":"https://one.test/","devtoolsFrontendUrl":"/devtools/inspector.html?ws=127.0.0.1:9222/page/1"},
            {"type":"page","url":"https://two.test/","devtoolsFrontendUrl":"/devtools/inspector.html?ws=127.0.0.1:9222/page/2"}
        ]"#;
        assert_eq!(
            parse_devtools_frontend_url(json, Some("https://two.test/"), 9222).as_deref(),
            Some("http://127.0.0.1:9222/devtools/inspector.html?ws=127.0.0.1:9222/page/2")
        );
        assert_eq!(
            parse_devtools_frontend_url(json, Some("https://missing.test/"), 9222).as_deref(),
            Some("http://127.0.0.1:9222/devtools/inspector.html?ws=127.0.0.1:9222/page/1")
        );

        let cef_json = r#"[ {
   "description": "",
   "devtoolsFrontendUrl": "https://chrome-devtools-frontend.appspot.com/serve_rev/@46876a0c2df3492ed67a8846950ba6d37170b58c/inspector.html?ws=127.0.0.1:52981/devtools/page/BF60A1449D2DA8687695C48D1E7076F0",
   "id": "BF60A1449D2DA8687695C48D1E7076F0",
   "title": "",
   "type": "page",
   "url": "data:text/html,\u003Ch1>hello\u003C/h1>",
   "webSocketDebuggerUrl": "ws://127.0.0.1:52981/devtools/page/BF60A1449D2DA8687695C48D1E7076F0"
} ]"#;
        assert_eq!(
            parse_devtools_frontend_url(cef_json, Some("data:text/html,<h1>hello</h1>"), 52981)
                .as_deref(),
            Some("devtools://devtools/bundled/inspector.html?ws=127.0.0.1:52981/devtools/page/BF60A1449D2DA8687695C48D1E7076F0")
        );
    }

    #[test]
    fn snapshot_expr_has_no_trailing_semicolon() {
        let expr = build_koma_snapshot_expr();
        let trimmed = expr.trim();
        assert!(!trimmed.ends_with(';'));
        assert!(trimmed.starts_with("(() => {"));
        assert!(trimmed.ends_with("})()"));
        assert!(trimmed.contains("komaSnapshot()"));

        for expr in [
            build_koma_read_expr(Some("@e1")),
            build_koma_text_expr(),
            build_koma_click_expr("#btn"),
            build_koma_fill_expr("input", "hello\nworld"),
            build_koma_api_inject_expr(),
        ] {
            let t = expr.trim();
            assert!(!t.ends_with(';'));
            assert!(t.starts_with("(() => {"));
            assert!(t.ends_with("})()"));
        }
        assert!(is_local_html_url("file:///tmp/report.html"));
        assert!(is_local_html_url("FILE:///tmp/index.HTM?x=1"));
        assert!(!is_local_html_url("https://example.com/a.html"));
        assert!(!is_local_html_url("file:///tmp/data.json"));
    }

    #[test]
    fn snapshot_result_parses_ok_and_error() {
        let ok_json = r#"{"ok":true,"snapshot":"@e1 [link] \"Products\"\n  @e2 [button] \"Get Started Now!\""}"#;
        let ok = SnapshotResult::deserialize_json(ok_json).unwrap();
        assert!(ok.ok);
        assert!(ok.snapshot.unwrap().contains("@e1"));
        assert!(ok.error.is_none());

        let err_json = r#"{"ok":false,"error":"Scope not found"}"#;
        let err = SnapshotResult::deserialize_json(err_json).unwrap();
        assert!(!err.ok);
        assert_eq!(err.error.as_deref(), Some("Scope not found"));
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
