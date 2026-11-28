use super::{Koma, KomaContent, PersonalSheetPreset, WorkspaceState};

impl WorkspaceState {
    pub fn save_current_sheet_as_preset(
        &mut self,
        name: impl Into<String>,
    ) -> Result<PersonalSheetPreset, String> {
        let name = name.into().trim().to_string();
        if name.is_empty() {
            return Err("Preset name cannot be empty".into());
        }
        let sheet = self.current_sheet();
        if sheet.komas.is_empty() {
            return Err("Cannot save an empty sheet as a preset".into());
        }
        let komas: Vec<String> = sheet
            .komas
            .iter()
            .map(|b| match &b.content {
                KomaContent::Web { initial_url, .. } => initial_url.clone(),
                KomaContent::Terminal { cwd, command } => {
                    let cwd = cwd.as_deref().filter(|s| !s.trim().is_empty());
                    let cmd = command.as_deref().filter(|s| !s.trim().is_empty());
                    let quote_cwd = |c: &str| {
                        if c.chars().any(char::is_whitespace) {
                            format!("\"{c}\"")
                        } else {
                            c.to_string()
                        }
                    };
                    match (cwd, cmd) {
                        (Some(c), Some(m)) => format!(":terminal {} {m}", quote_cwd(c)),
                        (Some(c), None) => format!(":terminal {}", quote_cwd(c)),
                        (None, Some(m)) => format!(":terminal {m}"),
                        (None, None) => ":terminal".into(),
                    }
                }
            })
            .collect();
        if komas.is_empty() {
            return Err("Sheet has no restorable komas".into());
        }
        let preset = PersonalSheetPreset {
            name: name.clone(),
            komas,
        };
        if let Some(existing) = self
            .personal_presets
            .iter_mut()
            .find(|p| p.name.eq_ignore_ascii_case(&name))
        {
            *existing = preset.clone();
        } else {
            self.personal_presets.push(preset.clone());
        }
        Ok(preset)
    }

    pub fn apply_sheet_preset(
        &mut self,
        name: &str,
        inputs: &[String],
        home_dir: Option<&str>,
        new_sheet: bool,
    ) -> Result<(usize, Vec<Koma>), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Preset name cannot be empty".into());
        }
        if inputs.is_empty() {
            return Err("Sheet preset has no komas".into());
        }
        let mut parsed = Vec::with_capacity(inputs.len());
        for raw in inputs {
            parsed.push(crate::input::parse_open_koma_input(raw, home_dir)?);
        }
        let mut removed = Vec::new();
        let idx = if new_sheet {
            let idx = self.add_sheet(false);
            self.sheets[idx].label = name.to_string();
            idx
        } else {
            let sheet = self.current_sheet_mut();
            removed = std::mem::take(&mut sheet.komas);
            sheet.focused_koma_id = None;
            sheet.label = name.to_string();
            self.active_sheet
        };
        for item in parsed {
            match item {
                crate::input::KomaInput::Terminal { cwd, command } => {
                    let label = match cwd.as_deref().filter(|s| !s.trim().is_empty()) {
                        Some(path) => {
                            let tail = path
                                .trim_end_matches('/')
                                .rsplit('/')
                                .next()
                                .filter(|s| !s.is_empty())
                                .unwrap_or("/");
                            format!("Shell ({tail})")
                        }
                        None => command
                            .as_deref()
                            .and_then(|c| c.split_whitespace().last())
                            .map(str::to_string)
                            .unwrap_or_else(|| "Shell".into()),
                    };
                    self.insert_koma_after_focused(label, KomaContent::terminal(cwd, command));
                }
                crate::input::KomaInput::Web { url, label } => {
                    self.insert_koma_after_focused(label, KomaContent::web(url));
                }
            }
        }
        Ok((idx, removed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheet_preset_saves_applies_and_stays_atomic_on_invalid_input() {
        let mut state = WorkspaceState::new();
        state.insert_koma_after_focused("Shell".into(), KomaContent::terminal(None, None));
        state.insert_koma_after_focused("Makepad".into(), KomaContent::web("https://makepad.nl"));
        let preset = state.save_current_sheet_as_preset("Work").unwrap();
        assert_eq!(preset.name, "Work");
        assert_eq!(
            preset.komas,
            vec![":terminal".to_string(), "https://makepad.nl".to_string()]
        );

        let before = state.clone();
        assert!(state
            .apply_sheet_preset("Bad", &[":terminal".into(), "".into()], None, true)
            .is_err());
        assert_eq!(state, before);

        let (idx, removed) = state
            .apply_sheet_preset("Work", &preset.komas, None, true)
            .unwrap();
        assert_eq!(idx, 1);
        assert!(removed.is_empty());
        assert_eq!(state.sheets[1].label, "Work");
        assert_eq!(state.sheets[1].komas.len(), 2);
        assert!(state.sheets[1].komas[0].is_terminal());
        assert_eq!(
            state.sheets[1].komas[1].initial_url(),
            Some("https://makepad.nl")
        );
    }

    #[test]
    fn sheet_preset_round_trips_terminal_cwd_with_whitespace() {
        let mut state = WorkspaceState::new();
        state.insert_koma_after_focused(
            "Spaces".into(),
            KomaContent::terminal(Some("/Users/test/My Projects".into()), None),
        );
        state.insert_koma_after_focused(
            "SpacesCmd".into(),
            KomaContent::terminal(
                Some("/Users/test/My Projects".into()),
                Some("cargo test".into()),
            ),
        );

        let preset = state.save_current_sheet_as_preset("Spaces").unwrap();
        assert_eq!(
            preset.komas,
            vec![
                ":terminal \"/Users/test/My Projects\"".to_string(),
                ":terminal \"/Users/test/My Projects\" cargo test".to_string(),
            ]
        );

        let (idx, _) = state
            .apply_sheet_preset("Spaces", &preset.komas, None, true)
            .unwrap();
        let komas = &state.sheets[idx].komas;
        assert_eq!(komas.len(), 2);
        assert_eq!(komas[0].cwd(), Some("/Users/test/My Projects"));
        assert_eq!(komas[0].command(), None);
        assert_eq!(komas[1].cwd(), Some("/Users/test/My Projects"));
        assert_eq!(komas[1].command(), Some("cargo test"));
    }
}
