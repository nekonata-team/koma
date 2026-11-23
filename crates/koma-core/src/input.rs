//! Omnibox / Open Koma input parser (pure Rust, UI-free).

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchEngine {
    #[default]
    DuckDuckGo,
    Google,
    Brave,
}

impl SearchEngine {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "duckduckgo" => Ok(Self::DuckDuckGo),
            "google" => Ok(Self::Google),
            "brave" => Ok(Self::Brave),
            other => Err(format!(
                "Unknown search_engine '{other}' (expected: duckduckgo, google, brave)"
            )),
        }
    }

    pub fn search_url(self, encoded_query: &str) -> String {
        match self {
            Self::DuckDuckGo => format!("https://duckduckgo.com/?q={encoded_query}"),
            Self::Google => format!("https://www.google.com/search?q={encoded_query}"),
            Self::Brave => format!("https://search.brave.com/search?q={encoded_query}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KomaInput {
    Terminal {
        cwd: Option<String>,
        command: Option<String>,
    },
    Web {
        url: String,
        label: String,
    },
}

pub fn expand_tilde(path: &str, home_dir: Option<&str>) -> String {
    let Some(home) = home_dir.filter(|h| !h.is_empty()) else {
        return path.to_string();
    };
    let home = home.trim_end_matches('/');
    if path == "~" {
        home.to_string()
    } else if let Some(rest) = path.strip_prefix("~/") {
        format!("{home}/{rest}")
    } else {
        path.to_string()
    }
}

pub fn parse_open_koma_input(raw: &str, home_dir: Option<&str>) -> Result<KomaInput, String> {
    parse_open_koma_input_with_engine(raw, home_dir, SearchEngine::default())
}

pub fn parse_open_koma_input_with_engine(
    raw: &str,
    home_dir: Option<&str>,
    engine: SearchEngine,
) -> Result<KomaInput, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Empty input".into());
    }

    if let Some((cwd, command)) = parse_terminal_command(trimmed, home_dir) {
        return Ok(KomaInput::Terminal { cwd, command });
    }

    let (url, label) = resolve_web_url_with_engine(trimmed, home_dir, engine)?;
    Ok(KomaInput::Web { url, label })
}

pub fn resolve_web_url(raw: &str, home_dir: Option<&str>) -> Result<(String, String), String> {
    resolve_web_url_with_engine(raw, home_dir, SearchEngine::default())
}

pub fn resolve_web_url_with_engine(
    raw: &str,
    home_dir: Option<&str>,
    engine: SearchEngine,
) -> Result<(String, String), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Empty URL".into());
    }

    if has_explicit_scheme(trimmed) {
        let url = trimmed.to_string();
        let label = derive_url_label(&url);
        return Ok((url, label));
    }

    if trimmed == "~" || trimmed.starts_with("~/") || trimmed.starts_with('/') {
        let expanded = expand_tilde(trimmed, home_dir);
        let encoded_path = expanded.replace(' ', "%20");
        let url = format!("file://{encoded_path}");
        let label = derive_url_label(&url);
        return Ok((url, label));
    }

    if !trimmed.chars().any(char::is_whitespace) {
        if let Some((host, has_port)) = extract_host_and_port(trimmed) {
            if is_probable_host(&host, has_port) {
                let scheme = if is_local_host(&host) || has_port {
                    "http"
                } else {
                    "https"
                };
                let url = format!("{scheme}://{trimmed}");
                let label = derive_url_label(&url);
                return Ok((url, label));
            }
        }
    }

    let encoded = percent_encode_query(trimmed);
    let url = engine.search_url(&encoded);
    let label = truncate_label(trimmed, 24);
    Ok((url, label))
}

pub fn derive_url_label(url: &str) -> String {
    if let Some(path) = url.strip_prefix("file://") {
        let file_name = path.rsplit('/').find(|seg| !seg.is_empty()).unwrap_or(path);
        return truncate_label(file_name, 24);
    }

    let without_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let host_port = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(without_scheme);
    let host = host_port.strip_prefix("www.").unwrap_or(host_port);
    if host.is_empty() {
        truncate_label(url, 24)
    } else {
        truncate_label(host, 24)
    }
}

fn is_path_like(token: &str) -> bool {
    token == "~"
        || token.starts_with("~/")
        || token.starts_with('/')
        || token == "."
        || token == ".."
        || token.starts_with("./")
        || token.starts_with("../")
}

fn split_first_token(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    if s.is_empty() {
        return None;
    }
    if let Some(rest) = s.strip_prefix('"') {
        if let Some(end) = rest.find('"') {
            return Some((rest[..end].to_string(), rest[end + 1..].trim_start()));
        }
    }
    let idx = s.find(char::is_whitespace).unwrap_or(s.len());
    Some((s[..idx].to_string(), s[idx..].trim_start()))
}

fn parse_terminal_command(
    trimmed: &str,
    home_dir: Option<&str>,
) -> Option<(Option<String>, Option<String>)> {
    let rest = trimmed
        .strip_prefix(":terminal")
        .or_else(|| trimmed.strip_prefix(":term"))?;
    if rest.is_empty() {
        return Some((None, None));
    }
    if !rest.chars().next().is_some_and(char::is_whitespace) {
        return None;
    }
    let body = rest.trim();
    if body.is_empty() {
        return Some((None, None));
    }
    let (first, tail) = split_first_token(body)?;
    if is_path_like(&first) {
        let cwd = Some(expand_tilde(&first, home_dir));
        let command = (!tail.is_empty()).then(|| tail.to_string());
        Some((cwd, command))
    } else {
        Some((None, Some(body.to_string())))
    }
}

fn has_explicit_scheme(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    [
        "http://",
        "https://",
        "file://",
        "about:",
        "data:",
        "devtools://",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
}

fn extract_host_and_port(s: &str) -> Option<(String, bool)> {
    let authority = s.split(['/', '?', '#']).next()?;
    if authority.is_empty() {
        return None;
    }

    if let Some(rest) = authority.strip_prefix('[') {
        let (addr, tail) = rest.split_once(']')?;
        if tail.is_empty() {
            return Some((addr.to_string(), false));
        }
        let port = tail.strip_prefix(':')?;
        if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) {
            return Some((addr.to_string(), true));
        }
        return None;
    }

    if let Some((host, port)) = authority.rsplit_once(':') {
        if !host.is_empty() && !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) {
            return Some((host.to_string(), true));
        }
        return None;
    }

    Some((authority.to_string(), false))
}

fn is_probable_host(host: &str, has_port: bool) -> bool {
    if host.eq_ignore_ascii_case("localhost") || is_ipv4(host) || host.contains(':') {
        return true;
    }
    if host.starts_with('.') || host.ends_with('.') || host.contains("..") {
        return false;
    }
    let Some((_, tld)) = host.rsplit_once('.') else {
        return has_port;
    };
    let valid_chars = host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    valid_chars && tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic())
}

fn is_local_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host == "127.0.0.1"
        || host == "0.0.0.0"
        || host == "::1"
        || host.ends_with(".local")
}

fn is_ipv4(host: &str) -> bool {
    let mut parts = host.split('.');
    let count = (0..4)
        .filter(|_| parts.next().and_then(|p| p.parse::<u8>().ok()).is_some())
        .count();
    count == 4 && parts.next().is_none()
}

fn percent_encode_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push(hex_digit(byte >> 4));
                out.push(hex_digit(byte & 0x0f));
            }
        }
    }
    out
}

fn hex_digit(v: u8) -> char {
    match v {
        0..=9 => (b'0' + v) as char,
        _ => (b'a' + (v - 10)) as char,
    }
}

fn truncate_label(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        s.to_string()
    } else {
        let prefix: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{prefix}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_terminal_commands_with_and_without_cwd() {
        assert_eq!(
            parse_open_koma_input(":terminal", Some("/Users/test")).unwrap(),
            KomaInput::Terminal {
                cwd: None,
                command: None,
            }
        );
        assert_eq!(
            parse_open_koma_input("  :terminal ~/projects  ", Some("/Users/test")).unwrap(),
            KomaInput::Terminal {
                cwd: Some("/Users/test/projects".into()),
                command: None,
            }
        );
        assert_eq!(
            parse_open_koma_input(":term /tmp", Some("/Users/test")).unwrap(),
            KomaInput::Terminal {
                cwd: Some("/tmp".into()),
                command: None,
            }
        );
        assert_eq!(
            parse_open_koma_input(":terminal ~/projects zmx attach dev", Some("/Users/test"))
                .unwrap(),
            KomaInput::Terminal {
                cwd: Some("/Users/test/projects".into()),
                command: Some("zmx attach dev".into()),
            }
        );
        assert_eq!(
            parse_open_koma_input(":terminal zmx attach dev", Some("/Users/test")).unwrap(),
            KomaInput::Terminal {
                cwd: None,
                command: Some("zmx attach dev".into()),
            }
        );
    }

    #[test]
    fn parses_web_urls_hosts_paths_and_search_queries() {
        assert_eq!(
            parse_open_koma_input("https://makepad.nl/docs", None).unwrap(),
            KomaInput::Web {
                url: "https://makepad.nl/docs".into(),
                label: "makepad.nl".into(),
            }
        );
        assert_eq!(
            parse_open_koma_input("example.com", None).unwrap(),
            KomaInput::Web {
                url: "https://example.com".into(),
                label: "example.com".into(),
            }
        );
        assert_eq!(
            parse_open_koma_input("localhost:3000/app", None).unwrap(),
            KomaInput::Web {
                url: "http://localhost:3000/app".into(),
                label: "localhost:3000".into(),
            }
        );
        assert_eq!(
            parse_open_koma_input("/tmp/a.html", None).unwrap(),
            KomaInput::Web {
                url: "file:///tmp/a.html".into(),
                label: "a.html".into(),
            }
        );
        assert_eq!(
            parse_open_koma_input("rust makepad", None).unwrap(),
            KomaInput::Web {
                url: "https://duckduckgo.com/?q=rust+makepad".into(),
                label: "rust makepad".into(),
            }
        );
    }

    #[test]
    fn rejects_empty_input() {
        assert!(parse_open_koma_input("   ", None).is_err());
        assert!(resolve_web_url("", None).is_err());
    }

    #[test]
    fn resolves_configured_search_engines() {
        assert_eq!(
            resolve_web_url_with_engine("rust makepad", None, SearchEngine::Google).unwrap(),
            (
                "https://www.google.com/search?q=rust+makepad".into(),
                "rust makepad".into()
            )
        );
        assert_eq!(
            resolve_web_url_with_engine("rust makepad", None, SearchEngine::Brave).unwrap(),
            (
                "https://search.brave.com/search?q=rust+makepad".into(),
                "rust makepad".into()
            )
        );
        assert_eq!(SearchEngine::parse("Google").unwrap(), SearchEngine::Google);
        assert!(SearchEngine::parse("yahoo").is_err());
    }
}
