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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeAppearance {
    #[default]
    Dark,
    Light,
    System,
}

impl ThemeAppearance {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "dark" => Ok(Self::Dark),
            "light" => Ok(Self::Light),
            "system" => Ok(Self::System),
            other => Err(format!(
                "Unknown theme.appearance '{other}' (expected: dark, light, system)"
            )),
        }
    }

    pub fn is_dark(self, host_dark: bool) -> bool {
        match self {
            Self::Dark => true,
            Self::Light => false,
            Self::System => host_dark,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeConfig {
    pub appearance: ThemeAppearance,
    pub accent: Option<[f32; 4]>,
}

impl ThemeConfig {
    /// Emits `mod.theme.color_primary` for Makepad's `set_app_theme_tokens`.
    pub fn app_theme_tokens_script(&self) -> Option<String> {
        let [r, g, b, a] = self.accent?;
        Some(format!(
            "mod.theme.color_primary = #{:02x}{:02x}{:02x}{:02x}",
            (r * 255.0).round() as u8,
            (g * 255.0).round() as u8,
            (b * 255.0).round() as u8,
            (a * 255.0).round() as u8,
        ))
    }
}

fn parse_hex_color(raw: &str, key: &str) -> Result<[f32; 4], String> {
    let hex = raw.trim().strip_prefix('#').unwrap_or(raw.trim());
    if !matches!(hex.len(), 6 | 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!(
            "theme.{key} must be a 6- or 8-digit hex color (e.g. #3b82f6)"
        ));
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0) as f32 / 255.0;
    let a = if hex.len() == 8 { byte(6) } else { 1.0 };
    Ok([byte(0), byte(2), byte(4), a])
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalConfig {
    pub shell: Option<String>,
    pub session_prefixes: Vec<String>,
    pub session_list_command: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScaleConfig {
    pub web: f64,
    pub terminal: f64,
}

impl Default for ScaleConfig {
    fn default() -> Self {
        Self {
            web: 1.0,
            terminal: 1.0,
        }
    }
}

fn parse_scale(value: &toml::Value, key: &str) -> Result<f64, String> {
    value
        .as_float()
        .or_else(|| value.as_integer().map(|i| i as f64))
        .filter(|s| s.is_finite() && (0.5..=2.0).contains(s))
        .ok_or_else(|| format!("scale.{key} must be a number in [0.5, 2.0]"))
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
    pub scale: ScaleConfig,
    pub theme: ThemeConfig,
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
            scale: ScaleConfig::default(),
            theme: ThemeConfig::default(),
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

/// Deep-merges `overlay` into `base`: nested tables are merged recursively,
/// while all other values in `overlay` replace the corresponding entry in `base`.
pub fn merge_tables(base: &mut toml::Table, overlay: toml::Table) {
    for (key, value) in overlay {
        match (base.get_mut(&key), value) {
            (Some(toml::Value::Table(base_tbl)), toml::Value::Table(overlay_tbl)) => {
                merge_tables(base_tbl, overlay_tbl);
            }
            (_, val) => {
                base.insert(key, val);
            }
        }
    }
}

fn parse_number(value: &toml::Value) -> Option<f64> {
    value
        .as_float()
        .or_else(|| value.as_integer().map(|i| i as f64))
}

impl Config {
    pub fn parse_table(text: &str) -> Result<toml::Table, String> {
        text.parse().map_err(|e| format!("Invalid TOML: {e}"))
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let table = Self::parse_table(text)?;
        Self::from_table(&table)
    }

    pub fn from_table(table: &toml::Table) -> Result<Self, String> {
        let mut config = Self::default();
        for (section, value) in table {
            match section.as_str() {
                "layout" => {
                    let layout = value.as_table().ok_or("[layout] must be a table")?;
                    for (key, val) in layout {
                        match key.as_str() {
                            "width_presets" => {
                                let list = val
                                    .as_array()
                                    .ok_or("layout.width_presets must be an array")?;
                                let presets: Vec<f64> = list
                                    .iter()
                                    .map(parse_number)
                                    .collect::<Option<_>>()
                                    .ok_or("layout.width_presets must contain numbers")?;
                                if presets.is_empty()
                                    || presets
                                        .iter()
                                        .any(|p| !(p.is_finite() && *p > 0.0 && *p <= 1.0))
                                {
                                    return Err(
                                        "layout.width_presets needs values in (0, 1]".into()
                                    );
                                }
                                config.width_presets = presets;
                            }
                            "width_step" => {
                                let step = parse_number(val)
                                    .filter(|s| s.is_finite() && *s > 0.0)
                                    .ok_or("layout.width_step must be a positive number")?;
                                config.width_step = step;
                            }
                            "gap" => {
                                let gap = parse_number(val)
                                    .filter(|g| g.is_finite() && *g >= 0.0)
                                    .ok_or("layout.gap must be a non-negative number")?;
                                config.koma_gap = gap;
                            }
                            "center_focused" => {
                                let raw = val
                                    .as_str()
                                    .ok_or("layout.center_focused must be a string")?;
                                config.center_focused = CenterFocused::parse(raw)?;
                            }
                            other => return Err(format!("Unknown layout.{other} key")),
                        }
                    }
                }
                "aliases" => {
                    let tbl = value.as_table().ok_or("[aliases] must be a table")?;
                    for (key, val) in tbl {
                        let name = key.trim();
                        if name.is_empty() || name.chars().any(char::is_whitespace) {
                            return Err(format!(
                                "aliases.{key} must not be empty or contain spaces"
                            ));
                        }
                        if name == "terminal" || name == "term" {
                            return Err(format!("aliases.{name} is reserved"));
                        }
                        let target = val
                            .as_str()
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .ok_or_else(|| format!("aliases.{name} must be a non-empty string"))?;
                        config.aliases.insert(name.to_string(), target.to_string());
                    }
                }
                "sheet_presets" => {
                    let list = value
                        .as_array()
                        .ok_or("[[sheet_presets]] must be an array of tables")?;
                    for (i, item) in list.iter().enumerate() {
                        let tbl = item
                            .as_table()
                            .ok_or_else(|| format!("sheet_presets[{i}] must be a table"))?;
                        for key in tbl.keys() {
                            if !matches!(key.as_str(), "name" | "komas") {
                                return Err(format!("Unknown sheet_presets[{i}].{key} key"));
                            }
                        }
                        let name =
                            require_non_empty_str(tbl, &format!("sheet_presets[{i}]"), "name")?;
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
                                    format!(
                                        "sheet_presets[{i}].komas[{j}] must be a non-empty string"
                                    )
                                })?;
                            komas.push(s.to_string());
                        }
                        if komas.is_empty() {
                            return Err(format!("sheet_presets[{i}].komas must not be empty"));
                        }
                        config.sheet_presets.push(SheetPresetConfig { name, komas });
                    }
                }
                "web" => {
                    let web = value.as_table().ok_or("[web] must be a table")?;
                    for (key, val) in web {
                        match key.as_str() {
                            "search_engine" => {
                                let raw =
                                    val.as_str().ok_or("web.search_engine must be a string")?;
                                config.search_engine = SearchEngine::parse(raw)?;
                            }
                            other => return Err(format!("Unknown web.{other} key")),
                        }
                    }
                }
                "terminal" => {
                    let terminal = value.as_table().ok_or("[terminal] must be a table")?;
                    for (key, val) in terminal {
                        match key.as_str() {
                            "shell" => {
                                let shell = val
                                    .as_str()
                                    .map(str::trim)
                                    .filter(|s| !s.is_empty())
                                    .ok_or("terminal.shell must be a non-empty string")?;
                                config.terminal.shell = Some(shell.to_string());
                            }
                            "session_prefixes" => {
                                let list = val
                                    .as_array()
                                    .ok_or("terminal.session_prefixes must be an array")?;
                                for (i, item) in list.iter().enumerate() {
                                    let prefix = item
                                        .as_str()
                                        .filter(|s| !s.trim().is_empty())
                                        .ok_or_else(|| {
                                            format!(
                                                "terminal.session_prefixes[{i}] must be a non-empty string"
                                            )
                                        })?;
                                    config.terminal.session_prefixes.push(prefix.to_string());
                                }
                            }
                            "session_list_command" => {
                                let cmd = val
                                    .as_str()
                                    .map(str::trim)
                                    .filter(|s| !s.is_empty())
                                    .ok_or(
                                        "terminal.session_list_command must be a non-empty string",
                                    )?;
                                config.terminal.session_list_command = Some(cmd.to_string());
                            }
                            other => return Err(format!("Unknown terminal.{other} key")),
                        }
                    }
                }
                "scale" => {
                    let tbl = value.as_table().ok_or("[scale] must be a table")?;
                    for (key, val) in tbl {
                        match key.as_str() {
                            "web" => config.scale.web = parse_scale(val, key)?,
                            "terminal" => config.scale.terminal = parse_scale(val, key)?,
                            other => return Err(format!("Unknown scale.{other} key")),
                        }
                    }
                }
                "theme" => {
                    let tbl = value.as_table().ok_or("[theme] must be a table")?;
                    for (key, val) in tbl {
                        match key.as_str() {
                            "appearance" => {
                                let raw = val
                                    .as_str()
                                    .ok_or_else(|| format!("theme.{key} must be a string"))?;
                                config.theme.appearance = ThemeAppearance::parse(raw)?;
                            }
                            "accent" => {
                                let raw = val
                                    .as_str()
                                    .ok_or_else(|| format!("theme.{key} must be a string"))?;
                                config.theme.accent = Some(parse_hex_color(raw, key)?);
                            }
                            other => return Err(format!("Unknown theme.{other} key")),
                        }
                    }
                }
                "keybindings" => {
                    let tbl = value.as_table().ok_or("[keybindings] must be a table")?;
                    for (scope, val) in tbl {
                        let target = match scope.as_str() {
                            "global" => &mut config.keybindings.global,
                            "koma" => &mut config.keybindings.koma,
                            "overview" => &mut config.keybindings.overview,
                            other => return Err(format!("Unknown [keybindings.{other}] scope")),
                        };
                        let scope_tbl = val
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
                other => return Err(format!("Unknown config key '{other}'")),
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

/// `$KOMA_CONFIG`, else `$XDG_CONFIG_HOME/koma/config.toml`, else `~/.config/koma/config.toml`.
pub fn config_path(home: Option<&str>) -> Option<PathBuf> {
    match home {
        Some(h) => {
            let koma_config = std::env::var("KOMA_CONFIG").ok();
            let xdg_config_home = std::env::var("XDG_CONFIG_HOME").ok();
            koma_ipc::workspaces::resolve_config_path(
                koma_config.as_deref(),
                xdg_config_home.as_deref(),
                Some(Path::new(h)),
            )
        }
        None => koma_ipc::workspaces::config_path(),
    }
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
shell = "pwsh"
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
        assert_eq!(cfg.terminal.shell.as_deref(), Some("pwsh"));
        assert_eq!(cfg.terminal.session_prefixes, vec!["zmx attach "]);
        assert!(Config::parse("[terminal]\nshell = \"   \"").is_err());
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

    #[test]
    fn parses_theme_appearance_and_accent() {
        let cfg = Config::parse(
            r##"
[theme]
appearance = "system"
accent = "#112233cc"
"##,
        )
        .unwrap();
        assert_eq!(cfg.theme.appearance, ThemeAppearance::System);
        assert!(cfg.theme.appearance.is_dark(true));
        assert!(!cfg.theme.appearance.is_dark(false));
        assert_eq!(
            cfg.theme.app_theme_tokens_script().as_deref(),
            Some("mod.theme.color_primary = #112233cc")
        );

        assert!(Config::parse("[theme]\nappearance = \"sepia\"").is_err());
        assert!(Config::parse("[theme]\naccent = \"red\"").is_err());
        assert!(Config::parse("[theme]\nunknown_key = \"#ffffff\"").is_err());
    }

    #[test]
    fn parses_scale_and_rejects_invalid_values() {
        assert_eq!(
            Config::default().scale,
            ScaleConfig {
                web: 1.0,
                terminal: 1.0
            }
        );

        let cfg = Config::parse("[scale]\nweb = 0.9\nterminal = 2").unwrap();
        assert_eq!(
            cfg.scale,
            ScaleConfig {
                web: 0.9,
                terminal: 2.0
            }
        );

        let min_cfg = Config::parse("[scale]\nweb = 0.5\nterminal = 0.5").unwrap();
        assert_eq!(
            min_cfg.scale,
            ScaleConfig {
                web: 0.5,
                terminal: 0.5
            }
        );

        assert!(Config::parse("[scale]\nweb = 0.49").is_err());
        assert!(Config::parse("[scale]\nterminal = 2.01").is_err());
        assert!(Config::parse("[scale]\nweb = \"0.9\"").is_err());
        assert!(Config::parse("[scale]\nunknown = 1.0").is_err());
        assert!(Config::parse("scale = 1.0").is_err());
    }

    #[test]
    fn rejects_unknown_keys_and_non_finite_floats() {
        assert!(Config::parse("unknown_section = 1").is_err());
        assert!(Config::parse("[unknown]\na = 1").is_err());
        assert!(Config::parse("[layout]\nunknown = 1").is_err());
        assert!(Config::parse("[web]\nunknown = \"x\"").is_err());
        assert!(Config::parse("[terminal]\nunknown = \"x\"").is_err());
        assert!(Config::parse(
            "[[sheet_presets]]\nname = \"Dev\"\nkomas = [\":terminal\"]\nextra = true"
        )
        .is_err());
        assert!(Config::parse("[theme]\nunknown = 123").is_err());

        assert!(Config::parse("[layout]\nwidth_step = inf").is_err());
        assert!(Config::parse("[layout]\nwidth_step = nan").is_err());
        assert!(Config::parse("[layout]\ngap = inf").is_err());
        assert!(Config::parse("[layout]\nwidth_presets = [nan]").is_err());
        assert!(Config::parse("[scale]\nweb = inf").is_err());
    }

    #[test]
    fn merges_raw_tables_before_typed_validation() {
        let mut base = Config::parse_table(
            r#"
[layout]
width_step = 40
gap = 12

[terminal]
shell = "zsh"
"#,
        )
        .unwrap();
        let overlay = Config::parse_table(
            r#"
[layout]
gap = 20

[web]
search_engine = "brave"
"#,
        )
        .unwrap();

        merge_tables(&mut base, overlay);
        let cfg = Config::from_table(&base).unwrap();
        assert_eq!(cfg.width_step, 40.0);
        assert_eq!(cfg.koma_gap, 20.0);
        assert_eq!(cfg.terminal.shell.as_deref(), Some("zsh"));
        assert_eq!(cfg.search_engine, SearchEngine::Brave);
    }
}
