use makepad_widgets::*;

use super::App;

impl App {
    pub(super) fn open_external_urls(&mut self, cx: &mut Cx, event: &AppOpenEvent) {
        let mut urls = external_urls(event).peekable();
        if urls.peek().is_some() {
            self.ui.window(cx, ids!(main_window)).show(cx);
            for url in urls {
                self.create_web_koma(cx, url.into(), None);
            }
        }
    }
}

fn external_urls(event: &AppOpenEvent) -> impl Iterator<Item = &str> {
    event
        .items
        .iter()
        .filter(|_| event.source == AppOpenSource::System)
        .filter_map(AppOpenItem::url)
        .filter(|url| {
            url.split_once("://").is_some_and(|(scheme, rest)| {
                (scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
                    && !rest.starts_with(['/', '?', '#'])
                    && !rest.is_empty()
                    && !url.chars().any(|c| c.is_control() || c.is_whitespace())
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_only_system_http_urls_without_rewriting_them() {
        let mut event = AppOpenEvent {
            items: [
                "http://example.test/a",
                "HTTPS://example.test/a%20b?q=x#section",
                "https:///missing-host",
                "example.html",
                "file:///tmp/page.html",
                "javascript://example.test/",
                "mailto:user@example.test",
            ]
            .map(|value| AppOpenItem::parse(value, None))
            .into(),
            source: AppOpenSource::System,
        };
        assert_eq!(
            external_urls(&event).collect::<Vec<_>>(),
            [
                "http://example.test/a",
                "HTTPS://example.test/a%20b?q=x#section"
            ]
        );
        event.source = AppOpenSource::Launch;
        assert!(external_urls(&event).next().is_none());
    }
}
