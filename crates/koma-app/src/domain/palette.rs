//! Candidate list for the floating Command / Open Palette (pure logic).

use crate::config::Config;
use crate::domain::router::RouteDecision;
use crate::domain::sheet_presets::available_sheet_presets;
use koma_core::model::WorkspaceState;

pub const MAX_CANDIDATES: usize = 10;

#[derive(Clone, Debug, PartialEq)]
pub enum PaletteAction {
    /// Resolve the typed text as URL / search / `:terminal [path] [command]`.
    Open,
    OpenInput(String),
    Prefill(String),
    ApplyPreset(String),
    FocusKoma(String),
    SwitchProfile(String),
    Run(RouteDecision),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PaletteItem {
    pub label: String,
    pub action: PaletteAction,
}

const COMMANDS: [(&str, RouteDecision); 22] = [
    ("Toggle overview", RouteDecision::ToggleOverview),
    ("New terminal", RouteDecision::CreateTerminal),
    ("Duplicate koma", RouteDecision::DuplicateFocusedKoma),
    ("New sheet", RouteDecision::AddSheet),
    ("Close sheet", RouteDecision::CloseCurrentSheet),
    (
        "Save sheet as preset",
        RouteDecision::OpenSaveSheetPresetPrompt,
    ),
    ("New private workspace", RouteDecision::OpenPrivateWorkspace),
    ("New Profile", RouteDecision::NewProfile),
    ("Switch Profile", RouteDecision::SwitchProfile),
    ("Open DevTools", RouteDecision::OpenDevTools),
    ("Close koma", RouteDecision::CloseFocusedKoma),
    ("Reopen closed Koma", RouteDecision::ReopenClosedKoma),
    ("Cycle width preset", RouteDecision::CycleWidthPreset),
    ("Toggle maximize", RouteDecision::ToggleMaximizeFocusedKoma),
    ("Center koma", RouteDecision::CenterFocusedKoma),
    ("Edit URL", RouteDecision::EditFocusedUrl),
    ("Rename koma", RouteDecision::RenameFocusedKoma),
    ("Rename sheet", RouteDecision::RenameCurrentSheet),
    ("Reload config", RouteDecision::ReloadConfig),
    ("Reload web", RouteDecision::ReloadFocusedWeb),
    ("Web back", RouteDecision::HistoryBackFocusedWeb),
    ("Web forward", RouteDecision::HistoryForwardFocusedWeb),
];

/// Typed text first (when present), then Aliases / Recents / Sheet Presets / Komas / Commands.
/// A leading `>` restricts candidates to commands.
pub fn candidates(query: &str, state: &WorkspaceState, config: &Config) -> Vec<PaletteItem> {
    let query = query.trim();
    if let Some(cmd_query) = query.strip_prefix('>') {
        let needle = cmd_query.trim().to_lowercase();
        let mut items: Vec<PaletteItem> = COMMANDS
            .iter()
            .filter(|(name, _)| needle.is_empty() || name.to_lowercase().contains(&needle))
            .map(|(name, route)| PaletteItem {
                label: format!("> {name}"),
                action: PaletteAction::Run(*route),
            })
            .collect();
        items.truncate(MAX_CANDIDATES);
        return items;
    }

    let needle = query.to_lowercase();
    let matches = |text: &str| needle.is_empty() || text.to_lowercase().contains(&needle);
    let mut items = Vec::new();
    if !query.is_empty() {
        items.push(PaletteItem {
            label: format!("Open: {query}"),
            action: PaletteAction::Open,
        });
    }
    for (key, value) in &config.aliases {
        if value.contains("{}") {
            let label = format!("[Alias] :{key} <arg> — {value}");
            if matches(&label) {
                items.push(PaletteItem {
                    label,
                    action: PaletteAction::Prefill(format!(":{key} ")),
                });
            }
        } else {
            let label = format!("[Alias] :{key} — {value}");
            if matches(&label) {
                items.push(PaletteItem {
                    label,
                    action: PaletteAction::OpenInput(value.clone()),
                });
            }
        }
    }
    let recents = if query.is_empty() {
        &state.recents[..state.recents.len().min(5)]
    } else {
        &state.recents[..]
    };
    for rec in recents {
        let label = format!("[Recent] {} — {}", rec.label, rec.input);
        if matches(&label) {
            items.push(PaletteItem {
                label,
                action: PaletteAction::OpenInput(rec.input.clone()),
            });
        }
    }
    for p in available_sheet_presets(&state.personal_presets, &config.sheet_presets) {
        let label = format!("[Preset] {} ({} komas)", p.name, p.komas.len());
        if matches(&label) {
            items.push(PaletteItem {
                label,
                action: PaletteAction::ApplyPreset(p.name.to_owned()),
            });
        }
    }
    for sheet in state.sheets() {
        for koma in &sheet.komas {
            let label = format!("{} / {} [{}]", sheet.label, koma.label, koma.kind.as_str());
            if matches(&label) {
                items.push(PaletteItem {
                    label,
                    action: PaletteAction::FocusKoma(koma.id.clone()),
                });
            }
        }
    }
    items.extend(
        COMMANDS
            .iter()
            .filter(|(name, _)| matches(name))
            .map(|(name, route)| PaletteItem {
                label: format!("> {name}"),
                action: PaletteAction::Run(*route),
            }),
    );
    items.truncate(MAX_CANDIDATES);
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SheetPresetConfig;
    use std::collections::BTreeMap;

    #[test]
    fn empty_query_lists_items_without_open_item() {
        let items = candidates("", &WorkspaceState::poc_default(), &Config::default());
        assert_eq!(items.len(), MAX_CANDIDATES);
        assert!(items.iter().all(|i| i.action != PaletteAction::Open));
    }

    #[test]
    fn typed_text_comes_first_then_matching_commands_and_komas() {
        let state = WorkspaceState::poc_default();
        let items = candidates("term", &state, &Config::default());
        assert_eq!(items[0].action, PaletteAction::Open);
        assert!(items
            .iter()
            .any(|i| i.action == PaletteAction::Run(RouteDecision::CreateTerminal)));
        assert!(items
            .iter()
            .any(|i| i.action == PaletteAction::FocusKoma("sheet1-term".into())));
        assert!(!items
            .iter()
            .any(|i| i.action == PaletteAction::FocusKoma("sheet1-web".into())));
    }

    #[test]
    fn leading_gt_restricts_to_commands_only() {
        let mut state = WorkspaceState::poc_default();
        state.record_recent("https://private.example", "Private");
        let items = candidates("> priv", &state, &Config::default());
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].action,
            PaletteAction::Run(RouteDecision::OpenPrivateWorkspace)
        );
    }

    #[test]
    fn lists_only_available_presets_in_precedence_order() {
        let mut state = WorkspaceState::poc_default();
        state.save_current_sheet_as_preset("Personal").unwrap();
        let config = Config::parse(
            r#"
[[sheet_presets]]
name = "personal"
komas = [":terminal /tmp"]

[[sheet_presets]]
name = "Work"
komas = [":terminal"]

[[sheet_presets]]
name = "work"
komas = [":terminal", ":terminal /tmp"]
"#,
        )
        .unwrap();

        let presets: Vec<_> = candidates("", &state, &config)
            .into_iter()
            .filter_map(|item| match item.action {
                PaletteAction::ApplyPreset(_) => Some(item.label),
                _ => None,
            })
            .collect();
        assert_eq!(
            presets,
            ["[Preset] Personal (2 komas)", "[Preset] Work (1 komas)"]
        );
    }

    #[test]
    fn includes_aliases_recents_and_presets() {
        let mut state = WorkspaceState::poc_default();
        state.record_recent("https://docs.rs", "Docs");
        let mut aliases = BTreeMap::new();
        aliases.insert("gh".into(), "https://github.com".into());
        aliases.insert("zmx".into(), ":terminal ~ zmx attach {}".into());
        let cfg = Config {
            aliases,
            sheet_presets: vec![SheetPresetConfig {
                name: "Review".into(),
                komas: vec![":terminal".into(), "https://github.com".into()],
            }],
            ..Config::default()
        };
        let items = candidates("", &state, &cfg);
        assert!(items
            .iter()
            .any(|i| i.action == PaletteAction::OpenInput("https://github.com".into())));
        assert!(items
            .iter()
            .any(|i| i.action == PaletteAction::Prefill(":zmx ".into())));
        assert!(items
            .iter()
            .any(|i| i.action == PaletteAction::OpenInput("https://docs.rs".into())));
        assert!(items.iter().any(|i| matches!(
            &i.action,
            PaletteAction::ApplyPreset(name) if name == "Review"
        )));
    }
}
