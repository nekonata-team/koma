//! Read-only import of Ghostty settings supported by Koma's terminal.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CellHeight {
    Pixels(i32),
    Percent(i32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct TerminalSettings {
    pub font_family: Vec<String>,
    pub font_family_bold: Vec<String>,
    pub font_size: Option<f64>,
    pub bold_weight: Option<f32>,
    pub cell_height: Option<CellHeight>,
    pub foreground: Option<[u8; 3]>,
    pub background: Option<[u8; 3]>,
    pub cursor_color: Option<[u8; 3]>,
    pub palette: BTreeMap<u8, [u8; 3]>,
    pub background_opacity: Option<f32>,
    pub copy_on_select: bool,
    pub ligatures: bool,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            font_family: Vec::new(),
            font_family_bold: Vec::new(),
            font_size: None,
            bold_weight: None,
            cell_height: None,
            foreground: None,
            background: None,
            cursor_color: None,
            palette: BTreeMap::new(),
            background_opacity: None,
            copy_on_select: false,
            ligatures: true,
        }
    }
}

pub fn load(home: Option<&Path>, dark: bool) -> (TerminalSettings, Vec<String>) {
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| home.map(|dir| dir.join(".config")));
    let mac = home
        .filter(|_| cfg!(target_os = "macos"))
        .map(|dir| dir.join("Library/Application Support/com.mitchellh.ghostty"));
    let xdg = xdg.map(|dir| dir.join("ghostty"));
    let dirs: Vec<_> = xdg.into_iter().chain(mac).collect();
    let mut themes: Vec<_> = dirs.iter().map(|dir| dir.join("themes")).collect();
    if let Some(resources) = std::env::var_os("GHOSTTY_RESOURCES_DIR").filter(|dir| !dir.is_empty())
    {
        themes.push(PathBuf::from(resources).join("themes"));
    }
    if cfg!(target_os = "macos") {
        if let Some(home) = home {
            themes.push(home.join("Applications/Ghostty.app/Contents/Resources/ghostty/themes"));
        }
        themes.push(PathBuf::from(
            "/Applications/Ghostty.app/Contents/Resources/ghostty/themes",
        ));
    } else if cfg!(target_os = "linux") {
        let data_dirs = std::env::var_os("XDG_DATA_DIRS")
            .filter(|dirs| !dirs.is_empty())
            .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
        themes.extend(std::env::split_paths(&data_dirs).map(|dir| dir.join("ghostty/themes")));
    }
    let explicit = std::env::var_os("KOMA_GHOSTTY_CONFIG").filter(|path| !path.is_empty());
    load_from(&dirs, &themes, dark, explicit.as_deref().map(Path::new))
}

struct Setting {
    path: PathBuf,
    line: usize,
    key: String,
    value: String,
}

fn load_from(
    dirs: &[PathBuf],
    themes: &[PathBuf],
    dark: bool,
    explicit: Option<&Path>,
) -> (TerminalSettings, Vec<String>) {
    let mut entries = Vec::new();
    let mut errors = Vec::new();
    let mut includes = VecDeque::new();
    if let Some(path) = explicit {
        read_settings(
            path,
            false,
            &[],
            Some(&mut includes),
            &mut entries,
            &mut errors,
        );
    } else {
        for dir in dirs {
            for name in ["config", "config.ghostty"] {
                read_settings(
                    &dir.join(name),
                    true,
                    &[],
                    Some(&mut includes),
                    &mut entries,
                    &mut errors,
                );
            }
        }
    }
    // Newly discovered includes go after earlier declarations, even across files.
    for _ in 0..256 {
        let Some((path, optional, parents)) = includes.pop_front() else {
            break;
        };
        read_settings(
            &path,
            optional,
            &parents,
            Some(&mut includes),
            &mut entries,
            &mut errors,
        );
    }
    if !includes.is_empty() {
        errors.push("Ghostty config-file limit (256) exceeded".into());
    }
    let mut settings = TerminalSettings::default();
    if let Some(entry) = entries.iter().rev().find(|entry| entry.key == "theme") {
        let name = theme_name(&entry.value, dark);
        if !name.is_empty() {
            let path = Path::new(name);
            let path = if path.is_absolute() {
                Some(path.to_owned())
            } else if path.file_name() == Some(path.as_os_str()) {
                themes
                    .iter()
                    .map(|dir| dir.join(path))
                    .find(|path| path.is_file())
            } else {
                None
            };
            if let Some(path) = path {
                let mut theme = Vec::new();
                read_settings(&path, false, &[], None, &mut theme, &mut errors);
                apply(&mut settings, &theme, &mut errors);
            } else {
                errors.push(format!(
                    "{}:{}: Ghostty theme {name:?} not found",
                    entry.path.display(),
                    entry.line
                ));
            }
        }
    }
    apply(&mut settings, &entries, &mut errors);
    (settings, errors)
}

fn theme_name(value: &str, dark: bool) -> &str {
    let (mut light, mut night) = (None, None);
    for item in value.split(',') {
        match item.trim().split_once(':') {
            Some(("light", name)) => light = Some(name.trim()),
            Some(("dark", name)) => night = Some(name.trim()),
            _ => return value,
        }
    }
    match (light, night) {
        (Some(light), Some(night)) => {
            if dark {
                night
            } else {
                light
            }
        }
        _ => value,
    }
}

type Includes = VecDeque<(PathBuf, bool, Vec<PathBuf>)>;

fn read_settings(
    path: &Path,
    optional: bool,
    parents: &[PathBuf],
    mut includes: Option<&mut Includes>,
    out: &mut Vec<Setting>,
    errors: &mut Vec<String>,
) {
    let identity = match path.canonicalize() {
        Ok(path) => path,
        Err(e) if optional && e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            errors.push(format!("{}: {e}", path.display()));
            return;
        }
    };
    if parents.contains(&identity) {
        errors.push(format!("{}: Ghostty config-file cycle", path.display()));
        return;
    }
    // Do not accidentally block on FIFOs or device files referenced by config-file.
    if !identity.is_file() {
        errors.push(format!("{}: not a regular file", path.display()));
        return;
    }
    let contents = match std::fs::read_to_string(&identity) {
        Ok(contents) => contents,
        Err(e) => {
            errors.push(format!("{}: {e}", path.display()));
            return;
        }
    };
    let parent = path.parent().unwrap_or(Path::new(""));
    for (line, text) in contents.lines().enumerate() {
        let text = text.trim();
        if text.starts_with('#') {
            continue;
        }
        let Some((key, raw)) = text.split_once('=') else {
            continue;
        };
        let (key, raw) = (key.trim(), raw.trim());
        let value = unquote(raw);
        if key == "config-file" {
            if let Some(includes) = includes.as_deref_mut() {
                if value.is_empty() {
                    includes.clear();
                } else {
                    let (name, optional) = raw
                        .strip_prefix('?')
                        .map_or((value, false), |name| (unquote(name), true));
                    let mut parents = parents.to_vec();
                    parents.push(identity.clone());
                    includes.push_back((parent.join(name), optional, parents));
                }
            }
        } else {
            out.push(Setting {
                path: path.to_owned(),
                line: line + 1,
                key: key.into(),
                value: value.into(),
            });
        }
    }
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value)
}

fn rgb(value: &str) -> Option<[u8; 3]> {
    let value = value.strip_prefix('#').unwrap_or(value);
    if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(value, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

fn assign<T>(slot: &mut Option<T>, value: &str, parse: impl FnOnce(&str) -> Option<T>) -> bool {
    if value.is_empty() {
        *slot = None;
    } else if let Some(value) = parse(value) {
        *slot = Some(value);
    } else {
        return false;
    }
    true
}

fn apply(settings: &mut TerminalSettings, entries: &[Setting], errors: &mut Vec<String>) {
    for entry in entries {
        let value = entry.value.as_str();
        let valid = match entry.key.as_str() {
            "font-family" | "font-family-bold" => {
                let families = if entry.key == "font-family" {
                    &mut settings.font_family
                } else {
                    &mut settings.font_family_bold
                };
                if value.is_empty() {
                    families.clear();
                } else {
                    families.push(value.to_owned());
                }
                true
            }
            "font-size" => assign(&mut settings.font_size, value, |v| {
                v.parse().ok().filter(|n| (1.0..=256.0).contains(n))
            }),
            "font-variation-bold" => {
                if let Some(("wght", weight)) = value
                    .split_once('=')
                    .map(|(axis, weight)| (axis.trim(), weight.trim()))
                {
                    !weight.is_empty()
                        && assign(&mut settings.bold_weight, weight, |v| {
                            v.parse().ok().filter(|n| (1.0..=1000.0).contains(n))
                        })
                } else if value.is_empty() {
                    settings.bold_weight = None;
                    true
                } else {
                    true
                } // Other axes are not implemented.
            }
            "adjust-cell-height" => assign(&mut settings.cell_height, value, |v| {
                if let Some(percent) = v.strip_suffix('%') {
                    percent
                        .parse()
                        .ok()
                        .filter(|n| (-99..=1000).contains(n))
                        .map(CellHeight::Percent)
                } else {
                    v.parse()
                        .ok()
                        .filter(|n| (-1000..=1000).contains(n))
                        .map(CellHeight::Pixels)
                }
            }),
            "foreground" => assign(&mut settings.foreground, value, rgb),
            "background" => assign(&mut settings.background, value, rgb),
            "cursor-color" => assign(&mut settings.cursor_color, value, rgb),
            "background-opacity" => assign(&mut settings.background_opacity, value, |v| {
                v.parse().ok().filter(|n| (0.0..=1.0).contains(n))
            }),
            "copy-on-select" => match value.trim().to_ascii_lowercase().as_str() {
                "true" | "clipboard" => {
                    settings.copy_on_select = true;
                    true
                }
                "false" => {
                    settings.copy_on_select = false;
                    true
                }
                _ => false,
            },
            "font-feature" => {
                if value.trim().is_empty() {
                    settings.ligatures = true;
                } else {
                    for feat in value
                        .split([',', ' '])
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                    {
                        match feat {
                            "-calt" | "-liga" => settings.ligatures = false,
                            "+calt" | "calt" | "+liga" | "liga" => settings.ligatures = true,
                            _ => {}
                        }
                    }
                }
                true
            }
            "palette" => {
                if value.is_empty() {
                    settings.palette.clear();
                    true
                } else if let Some((index, color)) =
                    value.split_once('=').and_then(|(index, color)| {
                        Some((index.trim().parse().ok()?, rgb(color.trim())?))
                    })
                {
                    settings.palette.insert(index, color);
                    true
                } else {
                    false
                }
            }
            _ => continue,
        };
        if !valid {
            errors.push(format!(
                "{}:{}: invalid Ghostty {}",
                entry.path.display(),
                entry.line,
                entry.key
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "koma-ghostty-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&dir).unwrap();
            Self(dir)
        }

        fn write(&self, relative: &str, contents: &str) {
            let file = self.0.join(relative);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, contents).unwrap();
        }

        fn load(&self, dark: bool) -> (TerminalSettings, Vec<String>) {
            load_from(
                &[self.0.clone(), self.0.join("mac")],
                &[self.0.join("themes")],
                dark,
                None,
            )
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn resolves_includes_themes_and_overrides() {
        let files = Fixture::new();
        files.write("config", "background-opacity = 0.75\nfont-size = 11");
        files.write(
            "config.ghostty",
            r#"
config-file = "sub/font options"
config-file = sibling
config-file = ?missing
font-family = "First Face"
font-family = Second Face
font-family-bold = Ignored
font-size = 12
palette = 1=#010203
palette = 200=aabbcc
theme = light:Day,dark:Night
"#,
        );
        files.write(
            "sub/font options",
            "config-file = next\nfont-size = 13\nfont-variation-bold = wght=650\nfont-family = Third Face\nfont-family-bold = \"\"\nfont-family-bold = Bold Face",
        );
        files.write("sibling", "font-size = 20");
        files.write("sub/next", "font-size = 14.5\nadjust-cell-height = -10%\n");
        files.write("themes/Day", "background = ffffff\nfont-size = 10");
        files.write("themes/Night", "background = 123456\nforeground = cccccc\ncursor-color = eeeeee\npalette = 1=ffffff\npalette = 2=112233");
        files.write("mac/config.ghostty", "foreground = 001122");

        let (settings, errors) = files.load(true);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            settings,
            TerminalSettings {
                font_family: vec![
                    "First Face".into(),
                    "Second Face".into(),
                    "Third Face".into()
                ],
                font_family_bold: vec!["Bold Face".into()],
                font_size: Some(14.5),
                bold_weight: Some(650.0),
                cell_height: Some(CellHeight::Percent(-10)),
                foreground: Some([0, 17, 34]),
                background: Some([18, 52, 86]),
                cursor_color: Some([238; 3]),
                palette: BTreeMap::from([
                    (1, [1, 2, 3]),
                    (2, [17, 34, 51]),
                    (200, [170, 187, 204])
                ]),
                background_opacity: Some(0.75),
                copy_on_select: false,
                ligatures: true,
            }
        );
        assert_eq!(files.load(false).0.background, Some([255; 3]));
    }

    #[test]
    fn skips_bad_settings_and_cyclic_includes_without_losing_valid_values() {
        let files = Fixture::new();
        assert_eq!(files.load(true), (TerminalSettings::default(), vec![]));
        files.write(
            "config.ghostty",
            r#"
font-size = 17
font-size = nan
adjust-cell-height = +2
font-variation-bold = wght=700
font-variation-bold = ""
palette = 1=abcdef
palette =
palette = 255=234567
palette = 256=000000
config-file = config.ghostty
"#,
        );
        let (settings, errors) = files.load(true);
        assert_eq!(settings.font_size, Some(17.0));
        assert_eq!(settings.cell_height, Some(CellHeight::Pixels(2)));
        assert_eq!(settings.bold_weight, None);
        assert_eq!(settings.palette, BTreeMap::from([(255, [35, 69, 103])]));
        assert_eq!(errors.len(), 3, "{errors:?}");
    }

    #[test]
    fn parses_copy_on_select() {
        let files = Fixture::new();
        files.write("config.ghostty", "copy-on-select = clipboard\n");
        let (settings, errors) = files.load(true);
        assert!(errors.is_empty());
        assert!(settings.copy_on_select);

        files.write("config.ghostty", "copy-on-select = false\n");
        let (settings, errors) = files.load(true);
        assert!(errors.is_empty());
        assert!(!settings.copy_on_select);
    }

    #[test]
    fn parses_font_feature_ligatures() {
        let files = Fixture::new();
        files.write("config.ghostty", "font-feature = -calt\n");
        let (settings, errors) = files.load(true);
        assert!(errors.is_empty());
        assert!(!settings.ligatures);

        files.write(
            "config.ghostty",
            "font-feature = -calt\nfont-feature = +calt\n",
        );
        let (settings, errors) = files.load(true);
        assert!(errors.is_empty());
        assert!(settings.ligatures);
    }
}
