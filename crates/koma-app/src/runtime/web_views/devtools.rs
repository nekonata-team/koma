//! DevTools HTTP discovery and frontend URL resolution for `WebViewRuntime`.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use koma_core::model::KomaId;
use makepad_micro_serde::*;

use super::WebViewRuntime;

impl WebViewRuntime {
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

fn parse_devtools_frontend_url(json: &str, target_url: Option<&str>, port: u16) -> Option<String> {
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
}
