//! DevTools frontend URL resolution for `WebViewRuntime`.

use koma_core::model::KomaId;

use super::WebViewRuntime;

impl WebViewRuntime {
    pub fn resolve_devtools_url(&self, koma_id: &KomaId) -> Result<String, String> {
        let port = self
            .views
            .devtools_port()
            .ok_or_else(|| "DevTools unavailable: browser host not ready".to_string())?;
        let target = self
            .view_id(koma_id)
            .and_then(|id| self.views.state(id))
            .and_then(|state| state.devtools_target.as_ref());
        match target {
            Some(Ok(target_id)) if !target_id.is_empty() => {
                Ok(devtools_frontend_url(port, target_id))
            }
            Some(Err(err)) => Err(err.clone()),
            _ => Ok(format!("http://127.0.0.1:{port}/")),
        }
    }
}

fn devtools_frontend_url(port: u16, target_id: &str) -> String {
    format!(
        "devtools://devtools/bundled/inspector.html?ws=127.0.0.1:{port}/devtools/page/{target_id}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devtools_frontend_url_builds_inspector_ws_url() {
        assert_eq!(
            devtools_frontend_url(52981, "BF60A1449D2DA8687695C48D1E7076F0"),
            "devtools://devtools/bundled/inspector.html?ws=127.0.0.1:52981/devtools/page/BF60A1449D2DA8687695C48D1E7076F0"
        );
    }
}
