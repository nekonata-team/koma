use koma_core::model::KomaId;
use makepad_micro_serde::*;

const MARK: &str = "__KOMA_WEB_NOTIFICATION__:";
const SCRIPT: &str = include_str!("../../assets/WebNotifications.js");

pub(super) struct NotificationBridge {
    prefix: String,
}

#[derive(DeJson)]
struct Message {
    origin: String,
    title: String,
    body: String,
}

pub(crate) struct WebNotification {
    pub koma_id: KomaId,
    pub title: String,
    pub body: String,
}

impl NotificationBridge {
    pub(super) fn new() -> Result<Self, getrandom::Error> {
        let mut bytes = [0; 16];
        getrandom::fill(&mut bytes)?;
        let key = u128::from_ne_bytes(bytes);
        Ok(Self {
            prefix: format!("{MARK}{key:032x}:"),
        })
    }

    pub(super) fn script(&self) -> String {
        // Only this view's document-start closure has the key. A console log from an
        // embedded frame must not be able to impersonate the top-level page.
        format!("{SCRIPT}(\"{}\");", self.prefix)
    }

    pub(super) fn receive(
        &self,
        koma_id: &KomaId,
        url: &str,
        line: &str,
    ) -> Option<WebNotification> {
        let payload = line.strip_prefix(&self.prefix)?;
        if payload.len() > 4_096 {
            return None;
        }
        let message = Message::deserialize_json(payload).ok()?;
        let (scheme, rest) = url.split_once("://")?;
        if !matches!(scheme, "https" | "http") {
            return None;
        }
        let authority = rest.split(['/', '?', '#']).next()?;
        let host = authority
            .rsplit('@')
            .next()
            .filter(|host| !host.is_empty())?;
        if message.origin != format!("{scheme}://{host}") {
            return None;
        }
        let title = clean(&message.title, 120);
        let body = clean(&message.body, 360);
        Some(WebNotification {
            koma_id: koma_id.clone(),
            title: if title.is_empty() {
                host.into()
            } else {
                format!("{host} — {title}")
            },
            body,
        })
    }
}

fn clean(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|c| {
            !c.is_control() && !matches!(c, '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
        .take(limit)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_the_views_key_and_current_top_level_origin() {
        let bridge = NotificationBridge::new().unwrap();
        let other = NotificationBridge::new().unwrap();
        let id = KomaId::from("koma-1");
        let payload =
            r#"{"origin":"https://example.test:8443","title":"Build\u000a done","body":"OK"}"#;
        let line = format!("{}{payload}", bridge.prefix);
        let url = "https://user@example.test:8443/next";
        assert!(other.receive(&id, url, &line).is_none());
        assert!(bridge
            .receive(&id, "https://different.test/", &line)
            .is_none());
        assert!(bridge
            .receive(&id, "file:///tmp/example.html", &line)
            .is_none());
        let notification = bridge.receive(&id, url, &line).unwrap();
        assert_eq!(notification.koma_id, id);
        assert_eq!(notification.title, "example.test:8443 — Build done");
        assert_eq!(notification.body, "OK");
    }
}
