//! User settings loaded from `~/.config/koma/config.toml` (UI- and Makepad-independent).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use koma_core::input::SearchEngine;
use koma_core::model::{DEFAULT_WIDTH_PRESETS, KOMA_WIDTH_STEP};

use crate::domain::router::{parse_action, parse_key_stroke, Keybindings};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CenterFocused {
    #[default]
    Never,
    Always,
    OnOverflow,
}

impl CenterFocused {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "never" => Ok(Self::Never),
            "always" => Ok(Self::Always),
            "on-overflow" | "on_overflow" => Ok(Self::OnOverflow),
            other => Err(format!(
                "Unknown layout.center_focused '{other}' (expected: never, always, on-overflow)"
            )),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalConfig {
    pub session_prefixes: Vec<String>,
    pub session_list_command: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetPresetConfig {
    pub name: String,
    pub komas: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// Fractions of the strip's usable width cycled by the width-preset key.
    pub width_presets: Vec<f64>,
    /// Pixels added or removed by the narrow/widen keys.
    pub width_step: f64,
    /// Horizontal gap in logical pixels between adjacent komas.
    pub koma_gap: f64,
    pub center_focused: CenterFocused,
    pub aliases: BTreeMap<String, String>,
    pub sheet_presets: Vec<SheetPresetConfig>,
    pub search_engine: SearchEngine,
    pub terminal: TerminalConfig,
    pub keybindings: Keybindings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            width_presets: DEFAULT_WIDTH_PRESETS.to_vec(),
            width_step: KOMA_WIDTH_STEP,
            koma_gap: 10.0,
            center_focused: CenterFocused::default(),
            aliases: BTreeMap::new(),
            sheet_presets: Vec::new(),
            search_engine: SearchEngine::default(),
            terminal: TerminalConfig::default(),
            keybindings: Keybindings::default(),
        }
    }
}

fn require_non_empty_str(
    table: &toml::map::Map<String, toml::Value>,
    ctx: &str,
    key: &str,
) -> Result<String, String> {
    let val = table
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("{ctx}.{key} must be a non-empty string"))?;
    Ok(val.to_string())
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, String> {
        let table: toml::Table = text.parse().map_err(|e| format!("Invalid TOML: {e}"))?;
        let mut config = Self::default();
        if let Some(layout) = table.get("layout") {
            let layout = layout.as_table().ok_or("[layout] must be a table")?;
            if let Some(value) = layout.get("width_presets") {
                let list = value
                    .as_array()
                    .ok_or("layout.width_presets must be an array")?;
                let presets: Vec<f64> = list
                    .iter()
                    .map(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
                    .collect::<Option<_>>()
                    .ok_or("layout.width_presets must contain numbers")?;
                if presets.is_empty() || presets.iter().any(|p| !(*p > 0.0 && *p <= 1.0)) {
                    return Err("layout.width_presets needs values in (0, 1]".into());
                }
                config.width_presets = presets;
            }
            if let Some(value) = layout.get("width_step") {
                let step = value
                    .as_float()
                    .or_else(|| value.as_integer().map(|i| i as f64))
                    .filter(|s| *s > 0.0)
                    .ok_or("layout.width_step must be a positive number")?;
                config.width_step = step;
            }
            if let Some(value) = layout.get("gap") {
                let gap = value
                    .as_float()
                    .or_else(|| value.as_integer().map(|i| i as f64))
                    .filter(|g| g.is_finite() && *g >= 0.0)
                    .ok_or("layout.gap must be a non-negative number")?;
                config.koma_gap = gap;
            }
            if let Some(value) = layout.get("center_focused") {
                let raw = value
                    .as_str()
                    .ok_or("layout.center_focused must be a string")?;
                config.center_focused = CenterFocused::parse(raw)?;
            }
        }
        if let Some(aliases) = table.get("aliases") {
            let tbl = aliases.as_table().ok_or("[aliases] must be a table")?;
            for (key, value) in tbl {
                let name = key.trim();
                if name.is_empty() || name.chars().any(char::is_whitespace) {
                    return Err(format!("aliases.{key} must not be empty or contain spaces"));
                }
                if name == "terminal" || name == "term" {
                    return Err(format!("aliases.{name} is reserved"));
                }
                let target = value
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| format!("aliases.{name} must be a non-empty string"))?;
                config.aliases.insert(name.to_string(), target.to_string());
            }
        }
        if let Some(presets) = table.get("sheet_presets") {
            let list = presets
                .as_array()
                .ok_or("[[sheet_presets]] must be an array of tables")?;
            for (i, item) in list.iter().enumerate() {
                let tbl = item
                    .as_table()
                    .ok_or_else(|| format!("sheet_presets[{i}] must be a table"))?;
                let name = require_non_empty_str(tbl, &format!("sheet_presets[{i}]"), "name")?;
                let komas_val = tbl
                    .get("komas")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| format!("sheet_presets[{i}].komas must be an array"))?;
                let mut komas = Vec::with_capacity(komas_val.len());
                for (j, b) in komas_val.iter().enumerate() {
                    let s = b
                        .as_str()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| {
                            format!("sheet_presets[{i}].komas[{j}] must be a non-empty string")
                        })?;
                    komas.push(s.to_string());
                }
                if komas.is_empty() {
                    return Err(format!("sheet_presets[{i}].komas must not be empty"));
                }
                config.sheet_presets.push(SheetPresetConfig { name, komas });
            }
        }
        if let Some(web) = table.get("web") {
            let web = web.as_table().ok_or("[web] must be a table")?;
            if let Some(value) = web.get("search_engine") {
                let raw = value.as_str().ok_or("web.search_engine must be a string")?;
                config.search_engine = SearchEngine::parse(raw)?;
            }
        }
        if let Some(terminal) = table.get("terminal") {
            let terminal = terminal.as_table().ok_or("[terminal] must be a table")?;
            if let Some(value) = terminal.get("session_prefixes") {
                let list = value
                    .as_array()
                    .ok_or("terminal.session_prefixes must be an array")?;
                for (i, item) in list.iter().enumerate() {
                    let prefix =
                        item.as_str()
                            .filter(|s| !s.trim().is_empty())
                            .ok_or_else(|| {
                                format!("terminal.session_prefixes[{i}] must be a non-empty string")
                            })?;
                    config.terminal.session_prefixes.push(prefix.to_string());
                }
            }
            if let Some(value) = terminal.get("session_list_command") {
                let cmd = value
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .ok_or("terminal.session_list_command must be a non-empty string")?;
                config.terminal.session_list_command = Some(cmd.to_string());
            }
        }
        if let Some(kb) = table.get("keybindings") {
            let tbl = kb.as_table().ok_or("[keybindings] must be a table")?;
            for (scope, value) in tbl {
                let target = match scope.as_str() {
                    "global" => &mut config.keybindings.global,
                    "koma" => &mut config.keybindings.koma,
                    "overview" => &mut config.keybindings.overview,
                    other => return Err(format!("Unknown [keybindings.{other}] scope")),
                };
                let scope_tbl = value
                    .as_table()
                    .ok_or_else(|| format!("[keybindings.{scope}] must be a table"))?;
                for (key_spec, action_val) in scope_tbl {
                    let stroke = parse_key_stroke(key_spec)?;
                    let action_str = action_val.as_str().ok_or_else(|| {
                        format!("keybindings.{scope}.{key_spec} must be a string")
                    })?;
                    let action = parse_action(action_str)?;
                    target.insert(stroke, action);
                }
            }
        }
        Ok(config)
    }

    /// Expands `:<alias> [arg]` against `[aliases]`, following chained aliases.
    /// Returns `Ok(None)` when `raw` does not start with a configured alias.
    pub fn expand_alias(&self, raw: &str) -> Result<Option<String>, String> {
        let mut current = raw.trim().to_string();
        let mut seen: Vec<String> = Vec::new();
        for _ in 0..16 {
            let Some(rest) = current.strip_prefix(':') else {
                break;
            };
            let mut parts = rest.splitn(2, char::is_whitespace);
            let key = parts.next().unwrap_or("");
            let Some(template) = self.aliases.get(key) else {
                break;
            };
            if seen.iter().any(|s| s == key) {
                return Err(format!("Alias cycle detected at :{key}"));
            }
            seen.push(key.to_string());
            let arg = parts.next().map(str::trim).unwrap_or("");
            current = if template.contains("{}") {
                if arg.is_empty() {
                    return Err(format!("Missing argument for :{key}"));
                }
                template.replace("{}", arg)
            } else if !arg.is_empty() {
                return Err(format!("Alias :{key} does not accept arguments"));
            } else {
                template.clone()
            };
        }
        if seen.is_empty() {
            Ok(None)
        } else {
            Ok(Some(current))
        }
    }

    /// Reads the config file; a missing file yields defaults.
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }
}

/// `$KOMA_CONFIG`, else `~/.config/koma/config.toml`.
pub fn config_path(home: Option<&str>) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("KOMA_CONFIG").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(path));
    }
    home.map(|h| Path::new(h).join(".config/koma/config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_layout_and_falls_back_to_defaults() {
        assert_eq!(Config::parse("").unwrap(), Config::default());
        let config = Config::parse(
            "[layout]\nwidth_presets = [0.5, 1]\nwidth_step = 40\ngap = 16\ncenter_focused = \"on-overflow\"",
        )
        .unwrap();
        assert_eq!(config.width_presets, vec![0.5, 1.0]);
        assert_eq!(config.width_step, 40.0);
        assert_eq!(config.koma_gap, 16.0);
        assert_eq!(config.center_focused, CenterFocused::OnOverflow);
        assert!(Config::parse("[layout]\ncenter_focused = \"maybe\"").is_err());
    }

    #[test]
    fn rejects_invalid_values() {
        assert!(Config::parse("[layout]\nwidth_presets = []").is_err());
        assert!(Config::parse("[layout]\nwidth_presets = [1.5]").is_err());
        assert!(Config::parse("[layout]\nwidth_step = -1").is_err());
        assert!(Config::parse("[layout]\ngap = -2").is_err());
        assert!(Config::parse("not toml =").is_err());
    }

    #[test]
    fn missing_file_yields_defaults() {
        let path = std::env::temp_dir().join("koma-missing-config.toml");
        let _ = std::fs::remove_file(&path);
        assert_eq!(Config::load(&path).unwrap(), Config::default());
    }

    #[test]
    fn parses_aliases_terminal_and_sheet_presets() {
        let toml = r#"
[aliases]
repo = ":terminal ~/projects"
zmx = ":terminal ~ zmx attach {}"

[terminal]
session_prefixes = ["zmx attach "]
session_list_command = "zmx list --short"

[[sheet_presets]]
name = "Dev"
komas = [":terminal", "https://example.com"]
"#;
        let cfg = Config::parse(toml).unwrap();
        assert_eq!(
            cfg.aliases.get("repo").map(String::as_str),
            Some(":terminal ~/projects")
        );
        assert_eq!(
            cfg.expand_alias(":repo").unwrap().as_deref(),
            Some(":terminal ~/projects")
        );
        assert_eq!(
            cfg.expand_alias(":zmx dev").unwrap().as_deref(),
            Some(":terminal ~ zmx attach dev")
        );
        assert!(cfg.expand_alias(":zmx").is_err());
        assert_eq!(cfg.expand_alias(":terminal").unwrap(), None);
        assert_eq!(cfg.terminal.session_prefixes, vec!["zmx attach "]);
        assert_eq!(
            cfg.terminal.session_list_command.as_deref(),
            Some("zmx list --short")
        );
        assert_eq!(
            cfg.sheet_presets,
            vec![SheetPresetConfig {
                name: "Dev".into(),
                komas: vec![":terminal".into(), "https://example.com".into()]
            }]
        );

        assert!(Config::parse("[aliases]\nterminal = \"x\"").is_err());
        assert!(Config::parse("[aliases]\n\"\" = \"x\"").is_err());
        assert!(Config::parse("[[sheet_presets]]\nname = \"Dev\"\nkomas = []").is_err());
        assert!(Config::parse("[[sheet_presets]]\nname = \"Dev\"\nkomas = [123]").is_err());
    }

    #[test]
    fn parses_web_search_engine() {
        let cfg = Config::parse("[web]\nsearch_engine = \"brave\"").unwrap();
        assert_eq!(cfg.search_engine, SearchEngine::Brave);
        assert!(Config::parse("[web]\nsearch_engine = \"yahoo\"").is_err());
    }

    #[test]
    fn expands_chained_aliases_and_rejects_cycles() {
        let cfg = Config::parse(
            r#"
[aliases]
zmx = ":terminal ~ zmx attach {}"
x = ":zmx {}"
work = ":zmx work"
a = ":b"
b = ":a"
"#,
        )
        .unwrap();
        assert_eq!(
            cfg.expand_alias(":x dev").unwrap().as_deref(),
            Some(":terminal ~ zmx attach dev")
        );
        assert_eq!(
            cfg.expand_alias(":work").unwrap().as_deref(),
            Some(":terminal ~ zmx attach work")
        );
        assert!(cfg.expand_alias(":a").is_err());
    }

    #[test]
    fn parses_keybinding_overrides_and_rejects_invalid_entries() {
        let cfg = Config::parse(
            r#"
[keybindings.global]
"alt+left" = "focus_prev_koma"
"opt+right" = "focus_next_koma"

[keybindings.koma]
"y" = "duplicate_koma"
"x" = "none"
"#,
        )
        .unwrap();
        assert_eq!(cfg.keybindings.global.len(), 2);
        assert_eq!(cfg.keybindings.koma.len(), 2);

        assert!(Config::parse("[keybindings.invalid]\n\"a\" = \"toggle_overview\"").is_err());
        assert!(Config::parse("[keybindings.koma]\n\"ctrl++\" = \"toggle_overview\"").is_err());
        assert!(Config::parse("[keybindings.koma]\n\"y\" = \"unknown_action\"").is_err());
    }
}
