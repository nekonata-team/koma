//! Pure keyboard routing decisions for Content Input, Koma Mode, and Prompt input.

use std::collections::HashMap;

use koma_core::model::InputMode;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Key {
    Comma,
    Space,
    Tab,
    Escape,
    Return,
    Minus,
    Equals,
    Plus,
    Semicolon,
    BracketLeft,
    BracketRight,
    Left,
    Right,
    Up,
    Down,
    Digit(u8),
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    #[default]
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayKind {
    None,
    Prompt,
    Drawer,
    Notifications,
    Width,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct KeyStroke {
    pub ctrl: bool,
    pub alt: bool,
    pub logo: bool,
    pub shift: bool,
    pub key: Key,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteDecision {
    Forward,
    Consume,
    ToggleMode,
    EnterContentInput,
    ToggleOverview,
    ExitOverview,
    ClosePrompt,
    FocusRelativeKoma(isize),
    MoveFocusedKoma(isize),
    MoveFocusedKomaToSheet(isize),
    ResizeFocusedKoma(isize),
    ContentZoom(i8),
    CycleWidthPreset,
    ApplyWidthPreset(usize),
    EnterWidthMode,
    ExitWidthMode,
    ToggleMaximizeFocusedKoma,
    CenterFocusedKoma,
    OverviewZoom(isize),
    OverviewZoomReset,
    SwitchSheet(usize),
    SwitchSheetRelative(isize, bool),
    AddSheet,
    CloseCurrentSheet,
    OpenKomaPrompt,
    OpenSaveSheetPresetPrompt,
    OpenPrivateWorkspace,
    NewProfile,
    SwitchProfile,
    OpenDevTools,
    EditFocusedUrl,
    RenameFocusedKoma,
    RenameCurrentSheet,
    CreateTerminal,
    DuplicateFocusedKoma,
    CloseFocusedKoma,
    CloseFocusedKomaFocusLeft,
    ReopenClosedKoma,
    ReloadFocusedWeb,
    ReloadConfig,
    HistoryBackFocusedWeb,
    HistoryForwardFocusedWeb,
    ToggleDrawer,
    StowFocusedToDrawer,
    ToggleNotifications,
    DrawerSelect(isize),
    DrawerPlace,
    DrawerDiscard,
    DrawerRestore,
    NotificationSelect(isize),
    NotificationFocusSource,
    NotificationToggleRead,
}

/// User-configured keybinding overrides (`None` unbinds a default shortcut).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Keybindings {
    pub global: HashMap<KeyStroke, Option<RouteDecision>>,
    pub koma: HashMap<KeyStroke, Option<RouteDecision>>,
    pub overview: HashMap<KeyStroke, Option<RouteDecision>>,
}

pub fn parse_key_stroke(spec: &str) -> Result<KeyStroke, String> {
    parse_key_stroke_with_primary(spec, PRIMARY_IS_LOGO)
}

pub fn parse_key_stroke_with_primary(
    spec: &str,
    primary_is_logo: bool,
) -> Result<KeyStroke, String> {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return Err("Keybinding cannot be empty".into());
    }
    let parts: Vec<&str> = trimmed.split('+').collect();
    if parts.iter().any(|p| p.trim().is_empty()) {
        return Err(format!("Invalid keybinding syntax: {spec}"));
    }

    let mut stroke = KeyStroke::default();
    for modifier in &parts[..parts.len() - 1] {
        match modifier.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "control" => stroke.ctrl = true,
            "alt" | "opt" | "option" => stroke.alt = true,
            "super" | "cmd" | "logo" | "win" => stroke.logo = true,
            "shift" => stroke.shift = true,
            "mod" | "primary" => {
                if primary_is_logo {
                    stroke.logo = true;
                } else {
                    stroke.ctrl = true;
                }
            }
            other => return Err(format!("Unknown modifier '{other}' in '{spec}'")),
        }
    }

    let raw_key = parts[parts.len() - 1].trim();
    if raw_key.len() == 1 && raw_key.as_bytes()[0].is_ascii_uppercase() {
        stroke.shift = true;
    }
    stroke.key = match raw_key.to_ascii_lowercase().as_str() {
        "comma" | "," => Key::Comma,
        "space" => Key::Space,
        "tab" => Key::Tab,
        "esc" | "escape" => Key::Escape,
        "enter" | "return" => Key::Return,
        "minus" | "-" => Key::Minus,
        "equal" | "equals" | "=" => Key::Equals,
        "plus" => Key::Plus,
        "semicolon" | ";" => Key::Semicolon,
        "bracket_left" | "[" => Key::BracketLeft,
        "bracket_right" | "]" => Key::BracketRight,
        "left" => Key::Left,
        "right" => Key::Right,
        "up" => Key::Up,
        "down" => Key::Down,
        "0" => Key::Digit(0),
        "1" => Key::Digit(1),
        "2" => Key::Digit(2),
        "3" => Key::Digit(3),
        "4" => Key::Digit(4),
        "5" => Key::Digit(5),
        "6" => Key::Digit(6),
        "7" => Key::Digit(7),
        "8" => Key::Digit(8),
        "9" => Key::Digit(9),
        "a" => Key::A,
        "b" => Key::B,
        "c" => Key::C,
        "d" => Key::D,
        "e" => Key::E,
        "f" => Key::F,
        "g" => Key::G,
        "h" => Key::H,
        "i" => Key::I,
        "j" => Key::J,
        "k" => Key::K,
        "l" => Key::L,
        "m" => Key::M,
        "n" => Key::N,
        "o" => Key::O,
        "p" => Key::P,
        "q" => Key::Q,
        "r" => Key::R,
        "s" => Key::S,
        "t" => Key::T,
        "u" => Key::U,
        "v" => Key::V,
        "w" => Key::W,
        "x" => Key::X,
        "y" => Key::Y,
        "z" => Key::Z,
        other => return Err(format!("Unknown key '{other}' in '{spec}'")),
    };
    Ok(stroke)
}

pub fn parse_action(name: &str) -> Result<Option<RouteDecision>, String> {
    let decision = match name.trim() {
        "none" => return Ok(None),
        "toggle_mode" => RouteDecision::ToggleMode,
        "enter_content_input" => RouteDecision::EnterContentInput,
        "toggle_overview" => RouteDecision::ToggleOverview,
        "exit_overview" => RouteDecision::ExitOverview,
        "focus_prev_koma" => RouteDecision::FocusRelativeKoma(-1),
        "focus_next_koma" => RouteDecision::FocusRelativeKoma(1),
        "move_prev_koma" => RouteDecision::MoveFocusedKoma(-1),
        "move_next_koma" => RouteDecision::MoveFocusedKoma(1),
        "prev_sheet" => RouteDecision::SwitchSheetRelative(-1, false),
        "next_sheet" => RouteDecision::SwitchSheetRelative(1, false),
        "prev_sheet_wrap" => RouteDecision::SwitchSheetRelative(-1, true),
        "next_sheet_wrap" => RouteDecision::SwitchSheetRelative(1, true),
        "move_prev_sheet" => RouteDecision::MoveFocusedKomaToSheet(-1),
        "move_next_sheet" => RouteDecision::MoveFocusedKomaToSheet(1),
        "sheet_1" => RouteDecision::SwitchSheet(0),
        "sheet_2" => RouteDecision::SwitchSheet(1),
        "sheet_3" => RouteDecision::SwitchSheet(2),
        "sheet_4" => RouteDecision::SwitchSheet(3),
        "sheet_5" => RouteDecision::SwitchSheet(4),
        "sheet_6" => RouteDecision::SwitchSheet(5),
        "sheet_7" => RouteDecision::SwitchSheet(6),
        "sheet_8" => RouteDecision::SwitchSheet(7),
        "sheet_9" => RouteDecision::SwitchSheet(8),
        "narrow_koma" => RouteDecision::ResizeFocusedKoma(-1),
        "widen_koma" => RouteDecision::ResizeFocusedKoma(1),
        "cycle_width" => RouteDecision::CycleWidthPreset,
        "enter_width_mode" => RouteDecision::EnterWidthMode,
        "toggle_maximize" => RouteDecision::ToggleMaximizeFocusedKoma,
        "center_koma" => RouteDecision::CenterFocusedKoma,
        "overview_zoom_in" => RouteDecision::OverviewZoom(1),
        "overview_zoom_out" => RouteDecision::OverviewZoom(-1),
        "overview_zoom_reset" => RouteDecision::OverviewZoomReset,
        "new_sheet" => RouteDecision::AddSheet,
        "close_sheet" => RouteDecision::CloseCurrentSheet,
        "open_palette" => RouteDecision::OpenKomaPrompt,
        "save_sheet_preset" => RouteDecision::OpenSaveSheetPresetPrompt,
        "new_private_workspace" => RouteDecision::OpenPrivateWorkspace,
        "open_devtools" => RouteDecision::OpenDevTools,
        "edit_url" => RouteDecision::EditFocusedUrl,
        "rename_koma" => RouteDecision::RenameFocusedKoma,
        "rename_sheet" => RouteDecision::RenameCurrentSheet,
        "new_terminal" => RouteDecision::CreateTerminal,
        "duplicate_koma" => RouteDecision::DuplicateFocusedKoma,
        "close_koma" => RouteDecision::CloseFocusedKoma,
        "close_koma_focus_left" => RouteDecision::CloseFocusedKomaFocusLeft,
        "reopen_koma" => RouteDecision::ReopenClosedKoma,
        "reload_web" => RouteDecision::ReloadFocusedWeb,
        "reload_config" => RouteDecision::ReloadConfig,
        "web_back" => RouteDecision::HistoryBackFocusedWeb,
        "web_forward" => RouteDecision::HistoryForwardFocusedWeb,
        "toggle_drawer" => RouteDecision::ToggleDrawer,
        "stow_drawer" => RouteDecision::StowFocusedToDrawer,
        "restore_drawer" => RouteDecision::DrawerRestore,
        "toggle_notifications" => RouteDecision::ToggleNotifications,
        other => return Err(format!("Unknown keybinding action '{other}'")),
    };
    Ok(Some(decision))
}

/// Decides how a key press is handled without UI side effects.
pub fn route_key(mode: InputMode, prompt_open: bool, stroke: KeyStroke) -> RouteDecision {
    let overlay = if prompt_open {
        OverlayKind::Prompt
    } else {
        OverlayKind::None
    };
    route_key_with_overlay(mode, overlay, stroke)
}

pub fn route_key_with_overlay(
    mode: InputMode,
    overlay: OverlayKind,
    stroke: KeyStroke,
) -> RouteDecision {
    route_key_with_bindings(mode, overlay, stroke, &Keybindings::default())
}

pub fn route_key_with_bindings(
    mode: InputMode,
    overlay: OverlayKind,
    stroke: KeyStroke,
    bindings: &Keybindings,
) -> RouteDecision {
    match overlay {
        OverlayKind::Prompt => {
            return match stroke.key {
                Key::Escape => RouteDecision::ClosePrompt,
                _ => RouteDecision::Forward,
            };
        }
        OverlayKind::Drawer => return route_drawer_overlay(stroke),
        OverlayKind::Notifications => return route_notifications_overlay(stroke),
        OverlayKind::Width => return route_width_mode(stroke),
        OverlayKind::None => {}
    }

    match bindings.global.get(&stroke) {
        Some(Some(decision)) => return *decision,
        Some(None) => {}
        None => {
            if let Some(global) = route_global(stroke).filter(|action| {
                mode != InputMode::Overview || !matches!(action, RouteDecision::ContentZoom(_))
            }) {
                return global;
            }
        }
    }

    match mode {
        InputMode::ContentInput => RouteDecision::Forward,
        InputMode::KomaMode => match bindings.koma.get(&stroke) {
            Some(Some(decision)) => *decision,
            Some(None) => RouteDecision::Consume,
            None => route_koma_mode(stroke),
        },
        InputMode::Overview => match bindings.overview.get(&stroke) {
            Some(Some(decision)) => *decision,
            Some(None) => RouteDecision::Consume,
            None => route_overview(stroke),
        },
    }
}

fn route_drawer_overlay(stroke: KeyStroke) -> RouteDecision {
    match stroke.key {
        Key::Escape | Key::Tab => RouteDecision::ToggleDrawer,
        Key::J | Key::Down => RouteDecision::DrawerSelect(1),
        Key::K | Key::Up => RouteDecision::DrawerSelect(-1),
        Key::Return => RouteDecision::DrawerPlace,
        Key::D | Key::X => RouteDecision::DrawerDiscard,
        Key::U => RouteDecision::DrawerRestore,
        _ => RouteDecision::Consume,
    }
}

fn route_notifications_overlay(stroke: KeyStroke) -> RouteDecision {
    match stroke.key {
        Key::Escape | Key::M => RouteDecision::ToggleNotifications,
        Key::J | Key::Down => RouteDecision::NotificationSelect(1),
        Key::K | Key::Up => RouteDecision::NotificationSelect(-1),
        Key::Return => RouteDecision::NotificationFocusSource,
        Key::R => RouteDecision::NotificationToggleRead,
        _ => RouteDecision::Consume,
    }
}

fn route_width_mode(stroke: KeyStroke) -> RouteDecision {
    match stroke.key {
        Key::Digit(n @ 1..=9) => RouteDecision::ApplyWidthPreset((n - 1) as usize),
        Key::W | Key::R => RouteDecision::CycleWidthPreset,
        Key::Minus if stroke.shift => RouteDecision::ResizeFocusedKoma(1),
        Key::Semicolon if stroke.shift => RouteDecision::ResizeFocusedKoma(1),
        Key::Equals | Key::Plus => RouteDecision::ResizeFocusedKoma(1),
        Key::Minus => RouteDecision::ResizeFocusedKoma(-1),
        Key::F => RouteDecision::ToggleMaximizeFocusedKoma,
        Key::C => RouteDecision::CenterFocusedKoma,
        Key::Escape => RouteDecision::ExitWidthMode,
        Key::Return | Key::I => RouteDecision::EnterContentInput,
        _ => RouteDecision::Consume,
    }
}

fn route_koma_or_sheet_nav(stroke: KeyStroke) -> Option<RouteDecision> {
    match stroke.key {
        Key::H | Key::Left => Some(if stroke.shift {
            RouteDecision::MoveFocusedKoma(-1)
        } else {
            RouteDecision::FocusRelativeKoma(-1)
        }),
        Key::L | Key::Right => Some(if stroke.shift {
            RouteDecision::MoveFocusedKoma(1)
        } else {
            RouteDecision::FocusRelativeKoma(1)
        }),
        Key::J | Key::Down => Some(if stroke.shift {
            RouteDecision::MoveFocusedKomaToSheet(1)
        } else {
            RouteDecision::SwitchSheetRelative(1, false)
        }),
        Key::K | Key::Up => Some(if stroke.shift {
            RouteDecision::MoveFocusedKomaToSheet(-1)
        } else {
            RouteDecision::SwitchSheetRelative(-1, false)
        }),
        Key::Digit(n @ 1..=9) => Some(RouteDecision::SwitchSheet((n - 1) as usize)),
        Key::N => Some(if stroke.shift {
            RouteDecision::AddSheet
        } else {
            RouteDecision::OpenKomaPrompt
        }),
        Key::Space => Some(RouteDecision::OpenKomaPrompt),
        Key::T => Some(RouteDecision::CreateTerminal),
        Key::D => Some(RouteDecision::DuplicateFocusedKoma),
        Key::X => Some(if stroke.shift {
            RouteDecision::CloseCurrentSheet
        } else {
            RouteDecision::CloseFocusedKoma
        }),
        _ => None,
    }
}

fn route_koma_mode(stroke: KeyStroke) -> RouteDecision {
    if let Some(decision) = route_koma_or_sheet_nav(stroke) {
        return decision;
    }
    match stroke.key {
        Key::O => RouteDecision::ToggleOverview,
        Key::E => RouteDecision::EditFocusedUrl,
        Key::Minus if stroke.shift => RouteDecision::ResizeFocusedKoma(1),
        Key::Semicolon if stroke.shift => RouteDecision::ResizeFocusedKoma(1),
        Key::Equals | Key::Plus => RouteDecision::ResizeFocusedKoma(1),
        Key::Minus => RouteDecision::ResizeFocusedKoma(-1),
        Key::BracketLeft => RouteDecision::HistoryBackFocusedWeb,
        Key::BracketRight => RouteDecision::HistoryForwardFocusedWeb,
        Key::R if stroke.shift => RouteDecision::RenameCurrentSheet,
        Key::R => RouteDecision::RenameFocusedKoma,
        Key::W => RouteDecision::EnterWidthMode,
        Key::F => RouteDecision::ToggleMaximizeFocusedKoma,
        Key::C => RouteDecision::CenterFocusedKoma,
        Key::Tab => RouteDecision::ToggleDrawer,
        Key::A => RouteDecision::StowFocusedToDrawer,
        Key::U => RouteDecision::ReopenClosedKoma,
        Key::M => RouteDecision::ToggleNotifications,
        Key::I if stroke.shift => RouteDecision::OpenDevTools,
        Key::Escape | Key::I | Key::Return => RouteDecision::EnterContentInput,
        _ => RouteDecision::Consume,
    }
}

fn route_overview(stroke: KeyStroke) -> RouteDecision {
    if let Some(decision) = route_koma_or_sheet_nav(stroke) {
        return decision;
    }
    match stroke.key {
        Key::O | Key::Escape | Key::Return => RouteDecision::ExitOverview,
        Key::Digit(0) => RouteDecision::OverviewZoomReset,
        Key::Minus if stroke.shift => RouteDecision::OverviewZoom(1),
        Key::Semicolon if stroke.shift => RouteDecision::OverviewZoom(1),
        Key::Equals | Key::Plus => RouteDecision::OverviewZoom(1),
        Key::Minus => RouteDecision::OverviewZoom(-1),
        _ => RouteDecision::Consume,
    }
}

pub const PRIMARY_IS_LOGO: bool = cfg!(target_os = "macos");

fn route_global(stroke: KeyStroke) -> Option<RouteDecision> {
    route_global_with_primary(stroke, PRIMARY_IS_LOGO)
}

fn route_global_with_primary(stroke: KeyStroke, primary_is_logo: bool) -> Option<RouteDecision> {
    if stroke.ctrl && !stroke.alt && !stroke.logo && stroke.key == Key::Comma {
        return Some(RouteDecision::ToggleMode);
    }
    if stroke.ctrl && !stroke.alt && !stroke.logo && stroke.key == Key::Tab {
        let delta = if stroke.shift { -1 } else { 1 };
        return Some(RouteDecision::SwitchSheetRelative(delta, true));
    }
    let (primary, other_clear) = if primary_is_logo {
        (stroke.logo, !stroke.ctrl)
    } else {
        (stroke.ctrl, !stroke.logo)
    };
    if primary && stroke.alt && other_clear {
        return match stroke.key {
            Key::Left => Some(RouteDecision::FocusRelativeKoma(-1)),
            Key::Right => Some(RouteDecision::FocusRelativeKoma(1)),
            Key::Up => Some(RouteDecision::SwitchSheetRelative(-1, false)),
            Key::Down => Some(RouteDecision::SwitchSheetRelative(1, false)),
            Key::Digit(n @ 1..=9) => Some(RouteDecision::SwitchSheet((n - 1) as usize)),
            Key::I => Some(RouteDecision::OpenDevTools),
            _ => None,
        };
    }
    if primary && !stroke.alt && other_clear {
        return match stroke.key {
            Key::O if stroke.shift => Some(RouteDecision::ToggleOverview),
            Key::T => Some(if stroke.shift {
                RouteDecision::CreateTerminal
            } else {
                RouteDecision::OpenKomaPrompt
            }),
            Key::L => Some(RouteDecision::EditFocusedUrl),
            Key::Minus | Key::Semicolon if stroke.shift => Some(RouteDecision::ContentZoom(1)),
            Key::Equals | Key::Plus => Some(RouteDecision::ContentZoom(1)),
            Key::Minus => Some(RouteDecision::ContentZoom(-1)),
            Key::Digit(0) if !stroke.shift => Some(RouteDecision::ContentZoom(0)),
            Key::W => Some(if stroke.shift {
                RouteDecision::CloseCurrentSheet
            } else {
                RouteDecision::CloseFocusedKoma
            }),
            Key::R => Some(RouteDecision::ReloadFocusedWeb),
            Key::BracketLeft => Some(RouteDecision::HistoryBackFocusedWeb),
            Key::BracketRight => Some(RouteDecision::HistoryForwardFocusedWeb),
            _ => None,
        };
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(k: Key) -> KeyStroke {
        KeyStroke {
            key: k,
            ..Default::default()
        }
    }

    #[test]
    fn prompt_captures_escape_and_forwards_typing_and_clipboard_shortcuts() {
        assert_eq!(
            route_key(InputMode::KomaMode, true, key(Key::Escape)),
            RouteDecision::ClosePrompt
        );
        assert_eq!(
            route_key(InputMode::ContentInput, true, key(Key::H)),
            RouteDecision::Forward
        );
        assert_eq!(
            route_key(
                InputMode::ContentInput,
                true,
                KeyStroke {
                    logo: true,
                    key: Key::T,
                    ..Default::default()
                }
            ),
            RouteDecision::Forward
        );
    }

    #[test]
    fn primary_zoom_keys_control_content_except_in_overview() {
        let primary = |key| KeyStroke {
            logo: PRIMARY_IS_LOGO,
            ctrl: !PRIMARY_IS_LOGO,
            ..self::key(key)
        };
        for mode in [InputMode::ContentInput, InputMode::KomaMode] {
            for (key, step) in [
                (Key::Equals, 1),
                (Key::Plus, 1),
                (Key::Minus, -1),
                (Key::Digit(0), 0),
            ] {
                assert_eq!(
                    route_key(mode, false, primary(key)),
                    RouteDecision::ContentZoom(step)
                );
            }
        }
        assert_eq!(
            route_key(InputMode::Overview, false, primary(Key::Equals)),
            RouteDecision::OverviewZoom(1)
        );
        assert_eq!(
            route_key(InputMode::Overview, false, primary(Key::Digit(0))),
            RouteDecision::OverviewZoomReset
        );
    }

    #[test]
    fn unmodified_zero_zooms_only_in_overview() {
        let zero = key(Key::Digit(0));
        assert_eq!(
            route_key(InputMode::KomaMode, false, zero),
            RouteDecision::Consume
        );
        assert_eq!(
            route_key(InputMode::ContentInput, false, zero),
            RouteDecision::Forward
        );
        assert_eq!(
            route_key(InputMode::Overview, false, zero),
            RouteDecision::OverviewZoomReset
        );
    }

    #[test]
    fn global_shortcuts_bypass_content_input_mode() {
        assert_eq!(
            route_key(
                InputMode::ContentInput,
                false,
                KeyStroke {
                    ctrl: true,
                    key: Key::Comma,
                    ..Default::default()
                }
            ),
            RouteDecision::ToggleMode
        );
        assert_eq!(
            route_key(
                InputMode::ContentInput,
                false,
                KeyStroke {
                    logo: true,
                    key: Key::T,
                    ..Default::default()
                }
            ),
            RouteDecision::OpenKomaPrompt
        );
        assert_eq!(
            route_key(
                InputMode::ContentInput,
                false,
                KeyStroke {
                    logo: true,
                    alt: true,
                    key: Key::Left,
                    ..Default::default()
                }
            ),
            RouteDecision::FocusRelativeKoma(-1)
        );
        assert_eq!(
            route_key(
                InputMode::ContentInput,
                false,
                KeyStroke {
                    logo: true,
                    alt: true,
                    key: Key::Digit(2),
                    ..Default::default()
                }
            ),
            RouteDecision::SwitchSheet(1)
        );
        assert_eq!(
            route_key(
                InputMode::ContentInput,
                false,
                KeyStroke {
                    ctrl: true,
                    shift: true,
                    key: Key::Tab,
                    ..Default::default()
                }
            ),
            RouteDecision::SwitchSheetRelative(-1, true)
        );
        assert_eq!(
            route_key(
                InputMode::ContentInput,
                false,
                KeyStroke {
                    logo: true,
                    key: Key::W,
                    ..Default::default()
                }
            ),
            RouteDecision::CloseFocusedKoma
        );
        assert_eq!(
            route_key(
                InputMode::ContentInput,
                false,
                KeyStroke {
                    logo: true,
                    shift: true,
                    key: Key::W,
                    ..Default::default()
                }
            ),
            RouteDecision::CloseCurrentSheet
        );
    }

    #[test]
    fn koma_mode_routes_koma_sheet_and_prompt_actions() {
        assert_eq!(
            route_key(InputMode::KomaMode, false, key(Key::T)),
            RouteDecision::CreateTerminal
        );
        assert_eq!(
            route_key(InputMode::KomaMode, false, key(Key::N)),
            RouteDecision::OpenKomaPrompt
        );
        assert_eq!(
            route_key(
                InputMode::KomaMode,
                false,
                KeyStroke {
                    shift: true,
                    key: Key::N,
                    ..Default::default()
                }
            ),
            RouteDecision::AddSheet
        );
        assert_eq!(
            route_key(
                InputMode::KomaMode,
                false,
                KeyStroke {
                    shift: true,
                    key: Key::H,
                    ..Default::default()
                }
            ),
            RouteDecision::MoveFocusedKoma(-1)
        );
        assert_eq!(
            route_key(InputMode::KomaMode, false, key(Key::Minus)),
            RouteDecision::ResizeFocusedKoma(-1)
        );
        assert_eq!(
            route_key(InputMode::KomaMode, false, key(Key::D)),
            RouteDecision::DuplicateFocusedKoma
        );
        assert_eq!(
            route_key(InputMode::KomaMode, false, key(Key::X)),
            RouteDecision::CloseFocusedKoma
        );
        assert_eq!(
            route_key(
                InputMode::KomaMode,
                false,
                KeyStroke {
                    key: Key::X,
                    shift: true,
                    ..Default::default()
                }
            ),
            RouteDecision::CloseCurrentSheet
        );
        assert_eq!(
            route_key(InputMode::KomaMode, false, key(Key::Other)),
            RouteDecision::Consume
        );
        assert_eq!(
            route_key(InputMode::ContentInput, false, key(Key::T)),
            RouteDecision::Forward
        );
    }

    #[test]
    fn koma_mode_width_center_and_reload_keys() {
        let shift_r = KeyStroke {
            shift: true,
            key: Key::R,
            ..Default::default()
        };
        for (stroke, expected) in [
            (key(Key::W), RouteDecision::EnterWidthMode),
            (key(Key::R), RouteDecision::RenameFocusedKoma),
            (shift_r, RouteDecision::RenameCurrentSheet),
            (key(Key::F), RouteDecision::ToggleMaximizeFocusedKoma),
            (key(Key::C), RouteDecision::CenterFocusedKoma),
            (key(Key::U), RouteDecision::ReopenClosedKoma),
        ] {
            assert_eq!(route_key(InputMode::KomaMode, false, stroke), expected);
        }
        assert_eq!(
            route_key_with_overlay(InputMode::KomaMode, OverlayKind::Width, key(Key::Digit(2))),
            RouteDecision::ApplyWidthPreset(1)
        );
        assert_eq!(
            route_key_with_overlay(InputMode::KomaMode, OverlayKind::Width, key(Key::W)),
            RouteDecision::CycleWidthPreset
        );
        assert_eq!(
            route_key_with_overlay(InputMode::KomaMode, OverlayKind::Width, key(Key::Escape)),
            RouteDecision::ExitWidthMode
        );
        assert_eq!(
            route_key_with_overlay(InputMode::KomaMode, OverlayKind::Width, key(Key::Return)),
            RouteDecision::EnterContentInput
        );
        assert_eq!(
            route_key(InputMode::ContentInput, false, key(Key::F)),
            RouteDecision::Forward
        );
    }

    #[test]
    fn overview_routes_toggle_navigation_cross_sheet_move_zoom_and_exit() {
        assert_eq!(
            route_key(InputMode::KomaMode, false, key(Key::O)),
            RouteDecision::ToggleOverview
        );
        assert_eq!(
            route_key(
                InputMode::ContentInput,
                false,
                KeyStroke {
                    logo: true,
                    shift: true,
                    key: Key::O,
                    ..Default::default()
                }
            ),
            RouteDecision::ToggleOverview
        );
        assert_eq!(
            route_key(InputMode::Overview, false, key(Key::J)),
            RouteDecision::SwitchSheetRelative(1, false)
        );
        assert_eq!(
            route_key(
                InputMode::Overview,
                false,
                KeyStroke {
                    shift: true,
                    key: Key::K,
                    ..Default::default()
                }
            ),
            RouteDecision::MoveFocusedKomaToSheet(-1)
        );
        assert_eq!(
            route_key(InputMode::Overview, false, key(Key::Minus)),
            RouteDecision::OverviewZoom(-1)
        );
        assert_eq!(
            route_key(InputMode::Overview, false, key(Key::Equals)),
            RouteDecision::OverviewZoom(1)
        );
        assert_eq!(
            route_key(InputMode::Overview, false, key(Key::Digit(0))),
            RouteDecision::OverviewZoomReset
        );
        assert_eq!(
            route_key(InputMode::Overview, false, key(Key::Return)),
            RouteDecision::ExitOverview
        );
        assert_eq!(
            route_key(InputMode::Overview, false, key(Key::Escape)),
            RouteDecision::ExitOverview
        );
    }

    #[test]
    fn drawer_and_notification_overlays_route_selection_and_actions() {
        assert_eq!(
            route_key_with_overlay(InputMode::KomaMode, OverlayKind::None, key(Key::Tab)),
            RouteDecision::ToggleDrawer
        );
        assert_eq!(
            route_key_with_overlay(InputMode::KomaMode, OverlayKind::None, key(Key::A)),
            RouteDecision::StowFocusedToDrawer
        );
        assert_eq!(
            route_key_with_overlay(InputMode::KomaMode, OverlayKind::None, key(Key::M)),
            RouteDecision::ToggleNotifications
        );
        assert_eq!(
            route_key_with_overlay(InputMode::KomaMode, OverlayKind::Drawer, key(Key::Return)),
            RouteDecision::DrawerPlace
        );
        assert_eq!(
            route_key_with_overlay(InputMode::KomaMode, OverlayKind::Drawer, key(Key::U)),
            RouteDecision::DrawerRestore
        );
        assert_eq!(
            route_key_with_overlay(
                InputMode::KomaMode,
                OverlayKind::Notifications,
                key(Key::Return)
            ),
            RouteDecision::NotificationFocusSource
        );
    }

    #[test]
    fn jis_shift_minus_and_shift_semicolon_widen_and_zoom_in() {
        let shift_minus = KeyStroke {
            shift: true,
            key: Key::Minus,
            ..Default::default()
        };
        let shift_semi = KeyStroke {
            shift: true,
            key: Key::Semicolon,
            ..Default::default()
        };
        assert_eq!(
            route_key(InputMode::KomaMode, false, shift_minus),
            RouteDecision::ResizeFocusedKoma(1)
        );
        assert_eq!(
            route_key(InputMode::KomaMode, false, shift_semi),
            RouteDecision::ResizeFocusedKoma(1)
        );
        assert_eq!(
            route_key(InputMode::KomaMode, false, key(Key::Plus)),
            RouteDecision::ResizeFocusedKoma(1)
        );
        assert_eq!(
            route_key(InputMode::Overview, false, shift_minus),
            RouteDecision::OverviewZoom(1)
        );
        assert_eq!(
            route_key(InputMode::Overview, false, shift_semi),
            RouteDecision::OverviewZoom(1)
        );
    }

    #[test]
    fn non_macos_primary_ctrl_shortcuts_mirror_macos_cmd() {
        let ctrl_t = KeyStroke {
            ctrl: true,
            key: Key::T,
            ..Default::default()
        };
        let ctrl_alt_right = KeyStroke {
            ctrl: true,
            alt: true,
            key: Key::Right,
            ..Default::default()
        };
        assert_eq!(
            route_global_with_primary(ctrl_t, false),
            Some(RouteDecision::OpenKomaPrompt)
        );
        assert_eq!(
            route_global_with_primary(ctrl_alt_right, false),
            Some(RouteDecision::FocusRelativeKoma(1))
        );
        let ctrl_alt_up = KeyStroke {
            ctrl: true,
            alt: true,
            key: Key::Up,
            ..Default::default()
        };
        let cmd_opt_down = KeyStroke {
            logo: true,
            alt: true,
            key: Key::Down,
            ..Default::default()
        };
        assert_eq!(
            route_global_with_primary(ctrl_alt_up, false),
            Some(RouteDecision::SwitchSheetRelative(-1, false))
        );
        assert_eq!(
            route_global_with_primary(cmd_opt_down, true),
            Some(RouteDecision::SwitchSheetRelative(1, false))
        );
        assert_eq!(route_global_with_primary(ctrl_t, true), None);
    }

    #[test]
    fn devtools_shortcut_routes_without_breaking_content_input_return() {
        let cmd_opt_i = KeyStroke {
            logo: true,
            alt: true,
            key: Key::I,
            ..Default::default()
        };
        assert_eq!(
            route_global_with_primary(cmd_opt_i, true),
            Some(RouteDecision::OpenDevTools)
        );
        let shift_i = KeyStroke {
            shift: true,
            key: Key::I,
            ..Default::default()
        };
        assert_eq!(
            route_key(InputMode::KomaMode, false, shift_i),
            RouteDecision::OpenDevTools
        );
        assert_eq!(
            route_key(InputMode::KomaMode, false, key(Key::I)),
            RouteDecision::EnterContentInput
        );
    }

    #[test]
    fn parses_cross_platform_key_strokes_and_routes_overrides() {
        assert_eq!(
            parse_key_stroke_with_primary("mod+shift+t", true).unwrap(),
            KeyStroke {
                logo: true,
                shift: true,
                key: Key::T,
                ..Default::default()
            }
        );
        assert_eq!(
            parse_key_stroke_with_primary("mod+shift+t", false).unwrap(),
            KeyStroke {
                ctrl: true,
                shift: true,
                key: Key::T,
                ..Default::default()
            }
        );
        assert_eq!(
            parse_key_stroke("opt+left").unwrap(),
            parse_key_stroke("Alt+Left").unwrap()
        );
        assert_eq!(
            parse_key_stroke("X").unwrap(),
            KeyStroke {
                shift: true,
                key: Key::X,
                ..Default::default()
            }
        );
        assert!(parse_key_stroke("").is_err());
        assert!(parse_key_stroke("ctrl++").is_err());
        assert!(parse_key_stroke("hyper+a").is_err());

        let mut bindings = Keybindings::default();
        let alt_left = parse_key_stroke("alt+left").unwrap();
        let key_y = parse_key_stroke("y").unwrap();
        let key_x = parse_key_stroke("x").unwrap();
        let cmd_w = KeyStroke {
            logo: PRIMARY_IS_LOGO,
            ctrl: !PRIMARY_IS_LOGO,
            key: Key::W,
            ..Default::default()
        };
        bindings
            .global
            .insert(alt_left, Some(RouteDecision::FocusRelativeKoma(-1)));
        bindings.global.insert(cmd_w, None);
        bindings
            .koma
            .insert(key_y, Some(RouteDecision::DuplicateFocusedKoma));
        bindings.koma.insert(key_x, None);

        assert_eq!(
            route_key_with_bindings(
                InputMode::ContentInput,
                OverlayKind::None,
                alt_left,
                &bindings
            ),
            RouteDecision::FocusRelativeKoma(-1)
        );
        // Unbound global shortcut forwards in ContentInput instead of closing.
        assert_eq!(
            route_key_with_bindings(InputMode::ContentInput, OverlayKind::None, cmd_w, &bindings),
            RouteDecision::Forward
        );
        assert_eq!(
            route_key_with_bindings(InputMode::KomaMode, OverlayKind::None, key_y, &bindings),
            RouteDecision::DuplicateFocusedKoma
        );
        // Unbound Koma Mode shortcut is consumed rather than closing.
        assert_eq!(
            route_key_with_bindings(InputMode::KomaMode, OverlayKind::None, key_x, &bindings),
            RouteDecision::Consume
        );
    }
}
