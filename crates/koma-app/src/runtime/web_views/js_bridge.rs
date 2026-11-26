//! JavaScript bridge assets, IIFE expression builders, and JSON result parsers for `WebViewRuntime`.

use koma_core::model::KomaId;
use koma_ipc::protocol::{IpcPayload, IpcResponse, WebReadData, WebSnapshotData};
use makepad_micro_serde::*;

const WEB_DOM_JS: &str = include_str!("../../../assets/WebDOM.js");
const WEB_READ_JS: &str = include_str!("../../../assets/WebRead.js");
const WEB_NAVIGATION_JS: &str = include_str!("../../../assets/WebNavigation.js");
const KOMA_API_JS: &str = include_str!("../../../assets/KomaAPI.js");

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
pub fn build_koma_snapshot_expr(within: Option<&str>, full: bool) -> String {
    let scope = within.map(js_string).unwrap_or_else(|| "null".into());
    let interactive_only = if full { "false" } else { "true" };
    format!(
        "(() => {{\n{WEB_DOM_JS}\nreturn window.__komaWebDOM.komaSnapshot({scope}, undefined, {interactive_only});\n}})()"
    )
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

pub fn build_koma_press_expr(key: &str) -> String {
    let k = js_string(key);
    format!(
        "(() => {{\ntry {{\n  const parts = {k}.split('+').map(s => s.trim()).filter(Boolean);\n  if (!parts.length) return {{ ok: false, error: 'Key must not be empty' }};\n  const rawKey = parts.pop();\n  const mods = {{ ctrlKey: false, altKey: false, shiftKey: false, metaKey: false }};\n  for (const m of parts) {{\n    const low = m.toLowerCase();\n    if (low === 'ctrl' || low === 'control') mods.ctrlKey = true;\n    else if (low === 'alt' || low === 'opt' || low === 'option') mods.altKey = true;\n    else if (low === 'shift') mods.shiftKey = true;\n    else if (low === 'meta' || low === 'cmd' || low === 'super') mods.metaKey = true;\n    else return {{ ok: false, error: 'Unknown modifier: ' + m }};\n  }}\n  const keyMap = {{ enter: 'Enter', esc: 'Escape', escape: 'Escape', tab: 'Tab', space: ' ', backspace: 'Backspace', delete: 'Delete', home: 'Home', end: 'End', pageup: 'PageUp', pagedown: 'PageDown', arrowup: 'ArrowUp', arrowdown: 'ArrowDown', arrowleft: 'ArrowLeft', arrowright: 'ArrowRight', up: 'ArrowUp', down: 'ArrowDown', left: 'ArrowLeft', right: 'ArrowRight' }};\n  const key = keyMap[rawKey.toLowerCase()] ?? rawKey;\n  const el = document.activeElement || document.body || document.documentElement;\n  if (!el) return {{ ok: false, error: 'No active element' }};\n  const init = {{ key, bubbles: true, cancelable: true, ...mods }};\n  const notCancelled = el.dispatchEvent(new KeyboardEvent('keydown', init));\n  if (notCancelled && key === 'Enter' && !mods.ctrlKey && !mods.altKey && !mods.metaKey) {{\n    if (el.tagName === 'INPUT' && el.form) {{\n      el.form.requestSubmit ? el.form.requestSubmit() : el.form.submit();\n    }} else if (el.tagName === 'A' || el.tagName === 'BUTTON' || el.getAttribute('role') === 'button') {{\n      el.click();\n    }}\n  }}\n  el.dispatchEvent(new KeyboardEvent('keyup', init));\n  return {{ ok: true }};\n}} catch (e) {{\n  return {{ ok: false, error: String(e && e.message ? e.message : e) }};\n}}\n}})()"
    )
}

pub fn build_koma_scroll_expr(target: &str) -> String {
    let t = js_string(target.trim());
    format!(
        "(() => {{\nif (!window.__komaWebDOM) {{\n{WEB_DOM_JS}\n}}\ntry {{\n  const raw = {t};\n  const low = raw.toLowerCase();\n  if (!raw || low === 'down') {{ window.scrollBy(0, Math.round(window.innerHeight * 0.8)); return {{ ok: true }}; }}\n  if (low === 'up') {{ window.scrollBy(0, -Math.round(window.innerHeight * 0.8)); return {{ ok: true }}; }}\n  if (low === 'top') {{ window.scrollTo(0, 0); return {{ ok: true }}; }}\n  if (low === 'bottom') {{ window.scrollTo(0, document.documentElement.scrollHeight); return {{ ok: true }}; }}\n  if (/^[+-]?\\d+$/.test(raw)) {{ window.scrollBy(0, Number(raw)); return {{ ok: true }}; }}\n  const el = window.__komaWebDOM.komaResolveTarget(raw);\n  if (!el) return {{ ok: false, error: 'Scroll target not found: ' + raw }};\n  el.scrollIntoView({{ block: 'center', inline: 'center' }});\n  return {{ ok: true }};\n}} catch (e) {{\n  return {{ ok: false, error: String(e && e.message ? e.message : e) }};\n}}\n}})()"
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

pub(super) fn parse_web_nav_event(koma_id: &KomaId, payload: &str) -> Option<WebNavEvent> {
    let msg = WebNavMessage::deserialize_json(payload).ok()?;
    Some(WebNavEvent {
        koma_id: koma_id.clone(),
        action: msg.action,
        url: msg.url,
    })
}

pub(super) fn parse_koma_api_event(koma_id: &KomaId, payload: &str) -> Option<KomaApiEvent> {
    let msg = KomaApiMessage::deserialize_json(payload).ok()?;
    match msg.action.as_str() {
        "koma.focus" => msg.koma_id.map(|target| KomaApiEvent::FocusKoma {
            caller_koma_id: koma_id.clone(),
            target_koma_id: target.into(),
        }),
        "toast.show" => {
            msg.message
                .filter(|m| !m.trim().is_empty())
                .map(|message| KomaApiEvent::ShowToast {
                    caller_koma_id: koma_id.clone(),
                    message,
                    severity: msg.severity.unwrap_or_else(|| "info".into()),
                })
        }
        _ => None,
    }
}

pub(super) fn parse_snapshot_response(koma_id: KomaId, val: &str) -> IpcResponse {
    match SnapshotResult::deserialize_json(val) {
        Ok(res) if res.ok => IpcResponse::Ok(IpcPayload::WebSnapshot(WebSnapshotData {
            koma_id: koma_id.into_string(),
            snapshot: res.snapshot,
        })),
        Ok(res) => IpcResponse::err(res.error.unwrap_or_else(|| "Snapshot failed".into())),
        Err(e) => IpcResponse::err(format!("Snapshot parse failed: {e:?}")),
    }
}

pub(super) fn parse_read_response(koma_id: KomaId, val: &str) -> IpcResponse {
    match ReadResult::deserialize_json(val) {
        Ok(res) if res.ok => IpcResponse::Ok(IpcPayload::WebRead(WebReadData {
            koma_id: koma_id.into_string(),
            markdown: res.markdown,
            title: res.title,
            url: res.url,
        })),
        Ok(res) => IpcResponse::err(res.error.unwrap_or_else(|| "Read failed".into())),
        Err(e) => IpcResponse::err(format!("Read parse failed: {e:?}")),
    }
}

pub(super) fn parse_text_response(koma_id: KomaId, val: &str) -> IpcResponse {
    match TextResult::deserialize_json(val) {
        Ok(res) if res.ok => IpcResponse::Ok(IpcPayload::Text {
            koma_id: koma_id.into_string(),
            text: res.text.unwrap_or_default(),
        }),
        Ok(res) => IpcResponse::err(res.error.unwrap_or_else(|| "Text failed".into())),
        Err(e) => IpcResponse::err(format!("Text parse failed: {e:?}")),
    }
}

pub(super) fn parse_action_response(koma_id: KomaId, val: &str) -> IpcResponse {
    match ActionResult::deserialize_json(val) {
        Ok(res) if res.ok => IpcResponse::Ok(IpcPayload::KomaId {
            koma_id: koma_id.into_string(),
        }),
        Ok(res) => IpcResponse::err(res.error.unwrap_or_else(|| "Action failed".into())),
        Err(e) => IpcResponse::err(format!("Action parse failed: {e:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_expr_has_no_trailing_semicolon() {
        let expr = build_koma_snapshot_expr(None, false);
        let trimmed = expr.trim();
        assert!(!trimmed.ends_with(';'));
        assert!(trimmed.starts_with("(() => {"));
        assert!(trimmed.ends_with("})()"));
        assert!(trimmed.contains("komaSnapshot(null, undefined, true)"));

        let scoped = build_koma_snapshot_expr(Some("main"), true);
        assert!(scoped.contains("komaSnapshot(\"main\", undefined, false)"));

        for expr in [
            build_koma_read_expr(Some("@e1")),
            build_koma_text_expr(),
            build_koma_click_expr("#btn"),
            build_koma_fill_expr("input", "hello\nworld"),
            build_koma_press_expr("Enter"),
            build_koma_scroll_expr("down"),
            build_koma_api_inject_expr(),
            build_web_navigation_inject_expr(),
        ] {
            let t = expr.trim();
            assert!(!t.ends_with(';'));
            assert!(t.starts_with("(() => {"));
            assert!(t.ends_with("})()"));
        }
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
}
