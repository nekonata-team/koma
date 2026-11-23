//! Core Koma domain model (UI- and Makepad-independent).

mod drawer;
mod notifications;
mod sheet_presets;

pub const DEFAULT_KOMA_WIDTH: f64 = 640.0;
pub const MIN_KOMA_WIDTH: f64 = 320.0;
pub const MAX_KOMA_WIDTH: f64 = 1200.0;
pub const KOMA_WIDTH_STEP: f64 = 80.0;
pub const MIN_PROPORTIONAL_WIDTH: f64 = 200.0;
pub const DEFAULT_WIDTH_PRESETS: [f64; 4] = [1.0 / 3.0, 0.5, 2.0 / 3.0, 1.0];
pub const MAX_NOTIFICATIONS: usize = 100;
pub const MAX_RECENTS: usize = 20;
const MAX_CLOSED_KOMAS: usize = 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentEntry {
    pub input: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonalSheetPreset {
    pub name: String,
    pub komas: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KomaKind {
    Terminal,
    Web,
}

impl KomaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Web => "web",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputMode {
    ContentInput,
    KomaMode,
    Overview,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KomaMeta {
    pub id: String,
    pub sheet_id: String,
    pub profile_id: String,
    pub kind: KomaKind,
    pub label: String,
    pub initial_url: Option<String>,
    pub cwd: Option<String>,
    pub command: Option<String>,
    /// Pixel width used when `ratio` is `None`.
    pub width: f64,
    /// Fraction of the strip's usable width; follows window resizes when set.
    pub ratio: Option<f64>,
    /// Sizing saved while maximized (`ratio == Some(1.0)`).
    pub restore: Option<(f64, Option<f64>)>,
    /// Whether `window.koma` API injection is enabled on local HTML pages.
    pub koma_api_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationEntry {
    pub id: String,
    pub koma_id: String,
    pub title: String,
    pub body: String,
    pub unread: bool,
}

impl KomaMeta {
    /// Niri-style proportion: `n` equal komas plus gaps exactly fill `usable`.
    pub fn effective_width(&self, usable: f64, gap: f64) -> f64 {
        match self.ratio {
            Some(r) => (r * usable - (1.0 - r) * gap).max(MIN_PROPORTIONAL_WIDTH),
            None => self.width,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SheetState {
    pub id: String,
    pub label: String,
    pub focused_koma_id: Option<String>,
    pub komas: Vec<KomaMeta>,
}

impl SheetState {
    pub fn focused_index(&self) -> Option<usize> {
        self.focused_koma_id
            .as_ref()
            .and_then(|id| self.komas.iter().position(|b| &b.id == id))
            .or_else(|| (!self.komas.is_empty()).then_some(0))
    }

    pub fn focused_koma(&self) -> Option<&KomaMeta> {
        self.focused_index().and_then(|idx| self.komas.get(idx))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkspaceState {
    pub profile_id: String,
    pub is_private: bool,
    pub mode: InputMode,
    active_sheet: usize,
    sheets: Vec<SheetState>,
    pub next_koma_seq: usize,
    pub next_sheet_seq: usize,
    pub drawer: Vec<KomaMeta>,
    pub discarded_drawer: Vec<KomaMeta>,
    closed_komas: Vec<(usize, KomaMeta)>,
    pub drawer_open: bool,
    pub drawer_selected: usize,
    pub notifications: Vec<NotificationEntry>,
    pub next_notification_seq: usize,
    pub notifications_open: bool,
    pub notification_selected: usize,
    pub recents: Vec<RecentEntry>,
    pub personal_presets: Vec<PersonalSheetPreset>,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceState {
    /// Clean initial state with one empty Sheet.
    pub fn new() -> Self {
        Self {
            profile_id: "default".into(),
            is_private: false,
            mode: InputMode::ContentInput,
            active_sheet: 0,
            next_koma_seq: 1,
            next_sheet_seq: 2,
            sheets: vec![SheetState {
                id: "sheet-1".into(),
                label: "Sheet 1".into(),
                focused_koma_id: None,
                komas: Vec::new(),
            }],
            drawer: Vec::new(),
            discarded_drawer: Vec::new(),
            closed_komas: Vec::new(),
            drawer_open: false,
            drawer_selected: 0,
            notifications: Vec::new(),
            next_notification_seq: 1,
            notifications_open: false,
            notification_selected: 0,
            recents: Vec::new(),
            personal_presets: Vec::new(),
        }
    }

    /// Restore the Sheet layout while initializing transient state as for a new Workspace.
    pub fn restore_layout(
        profile_id: String,
        mut sheets: Vec<SheetState>,
        active_sheet_index: usize,
    ) -> Result<Self, String> {
        if active_sheet_index >= sheets.len() {
            return Err("State has no valid active Sheet".into());
        }
        for sheet in &mut sheets {
            sheet.focused_koma_id = sheet
                .focused_koma_id
                .take()
                .filter(|id| sheet.komas.iter().any(|koma| &koma.id == id));
        }
        Ok(Self {
            profile_id,
            sheets,
            active_sheet: active_sheet_index,
            ..Self::new()
        })
    }

    pub fn set_profile_id(&mut self, id: String) {
        for koma in self
            .sheets
            .iter_mut()
            .flat_map(|sheet| &mut sheet.komas)
            .chain(&mut self.drawer)
            .chain(&mut self.discarded_drawer)
            .chain(self.closed_komas.iter_mut().map(|(_, koma)| koma))
        {
            koma.profile_id.clone_from(&id);
        }
        self.profile_id = id;
    }

    /// Two-Sheet fixture used by unit tests.
    pub fn poc_default() -> Self {
        let profile_id = "profile-a".to_string();
        Self {
            profile_id: profile_id.clone(),
            is_private: false,
            mode: InputMode::ContentInput,
            active_sheet: 0,
            next_koma_seq: 1,
            next_sheet_seq: 3,
            drawer: Vec::new(),
            discarded_drawer: Vec::new(),
            closed_komas: Vec::new(),
            drawer_open: false,
            drawer_selected: 0,
            notifications: Vec::new(),
            next_notification_seq: 1,
            notifications_open: false,
            notification_selected: 0,
            recents: Vec::new(),
            personal_presets: Vec::new(),
            sheets: vec![
                SheetState {
                    id: "sheet-1".into(),
                    label: "Sheet 1".into(),
                    focused_koma_id: Some("sheet1-term".into()),
                    komas: vec![
                        KomaMeta {
                            id: "sheet1-term".into(),
                            sheet_id: "sheet-1".into(),
                            profile_id: profile_id.clone(),
                            kind: KomaKind::Terminal,
                            label: "Shell 1".into(),
                            initial_url: None,
                            cwd: None,
                            command: None,
                            width: DEFAULT_KOMA_WIDTH,
                            ratio: None,
                            restore: None,
                            koma_api_enabled: false,
                        },
                        KomaMeta {
                            id: "sheet1-web".into(),
                            sheet_id: "sheet-1".into(),
                            profile_id: profile_id.clone(),
                            kind: KomaKind::Web,
                            label: "Makepad".into(),
                            initial_url: Some("https://makepad.nl".into()),
                            cwd: None,
                            command: None,
                            width: DEFAULT_KOMA_WIDTH,
                            ratio: None,
                            restore: None,
                            koma_api_enabled: false,
                        },
                    ],
                },
                SheetState {
                    id: "sheet-2".into(),
                    label: "Sheet 2".into(),
                    focused_koma_id: Some("sheet2-term".into()),
                    komas: vec![
                        KomaMeta {
                            id: "sheet2-term".into(),
                            sheet_id: "sheet-2".into(),
                            profile_id: profile_id.clone(),
                            kind: KomaKind::Terminal,
                            label: "Shell 2".into(),
                            initial_url: None,
                            cwd: None,
                            command: None,
                            width: DEFAULT_KOMA_WIDTH,
                            ratio: None,
                            restore: None,
                            koma_api_enabled: false,
                        },
                        KomaMeta {
                            id: "sheet2-web".into(),
                            sheet_id: "sheet-2".into(),
                            profile_id,
                            kind: KomaKind::Web,
                            label: "Example".into(),
                            initial_url: Some("https://example.com".into()),
                            cwd: None,
                            command: None,
                            width: DEFAULT_KOMA_WIDTH,
                            ratio: None,
                            restore: None,
                            koma_api_enabled: false,
                        },
                    ],
                },
            ],
        }
    }

    pub fn sheets(&self) -> &[SheetState] {
        &self.sheets
    }

    pub fn active_sheet_index(&self) -> usize {
        self.active_sheet
    }

    pub fn current_sheet(&self) -> &SheetState {
        &self.sheets[self.active_sheet]
    }

    fn current_sheet_mut(&mut self) -> &mut SheetState {
        &mut self.sheets[self.active_sheet]
    }

    pub fn focused_koma(&self) -> Option<&KomaMeta> {
        self.current_sheet().focused_koma()
    }

    pub fn switch_sheet(&mut self, index: usize) -> bool {
        if index >= self.sheets.len() || index == self.active_sheet {
            return false;
        }
        self.active_sheet = index;
        true
    }

    pub fn switch_sheet_relative(&mut self, delta: isize, wrap: bool) -> bool {
        if self.sheets.len() <= 1 || delta == 0 {
            return false;
        }
        let len = self.sheets.len() as isize;
        let next = if wrap {
            (self.active_sheet as isize + delta).rem_euclid(len) as usize
        } else {
            (self.active_sheet as isize + delta).clamp(0, len - 1) as usize
        };
        if next == self.active_sheet {
            return false;
        }
        self.active_sheet = next;
        true
    }

    pub fn focus_koma_by_id(&mut self, koma_id: &str) -> bool {
        let Some((sheet_idx, _)) = self
            .sheets
            .iter()
            .enumerate()
            .find(|(_, d)| d.komas.iter().any(|b| b.id == koma_id))
        else {
            return false;
        };
        let changed = self.active_sheet != sheet_idx
            || self.sheets[sheet_idx].focused_koma_id.as_deref() != Some(koma_id);
        self.active_sheet = sheet_idx;
        self.sheets[sheet_idx].focused_koma_id = Some(koma_id.to_string());
        changed
    }

    pub fn focus_relative_koma(&mut self, delta: isize) -> bool {
        let sheet = self.current_sheet_mut();
        let Some(cur_idx) = sheet.focused_index() else {
            return false;
        };
        let max_idx = sheet.komas.len().saturating_sub(1) as isize;
        let next_idx = (cur_idx as isize + delta).clamp(0, max_idx) as usize;
        let next_id = sheet.komas[next_idx].id.clone();
        if sheet.focused_koma_id.as_deref() == Some(next_id.as_str()) && next_idx == cur_idx {
            return false;
        }
        sheet.focused_koma_id = Some(next_id);
        true
    }

    pub fn move_focused_koma(&mut self, delta: isize) -> bool {
        let sheet = self.current_sheet_mut();
        let Some(cur_idx) = sheet.focused_index() else {
            return false;
        };
        let target = cur_idx as isize + delta;
        if target < 0 || target >= sheet.komas.len() as isize {
            return false;
        }
        sheet.komas.swap(cur_idx, target as usize);
        true
    }

    pub fn move_focused_koma_to_sheet(&mut self, delta: isize) -> bool {
        if delta == 0 || self.sheets.len() <= 1 {
            return false;
        }
        let target = self.active_sheet as isize + delta;
        if target < 0 || target >= self.sheets.len() as isize {
            return false;
        }
        let target_idx = target as usize;
        let Some(koma_id) = self.focused_koma().map(|b| b.id.clone()) else {
            return false;
        };
        let insert_idx = self.sheets[target_idx]
            .focused_index()
            .map(|i| i + 1)
            .unwrap_or(0);
        self.move_koma_to_position(&koma_id, target_idx, insert_idx)
    }

    pub fn move_koma_to_position(
        &mut self,
        koma_id: &str,
        target_sheet_idx: usize,
        target_index: usize,
    ) -> bool {
        self.place_koma(koma_id, target_sheet_idx, target_index, true)
    }

    pub fn move_koma_without_focusing(
        &mut self,
        koma_id: &str,
        target_sheet_idx: usize,
        target_index: usize,
    ) -> bool {
        self.place_koma(koma_id, target_sheet_idx, target_index, false)
    }

    fn place_koma(
        &mut self,
        koma_id: &str,
        target_sheet_idx: usize,
        target_index: usize,
        focus_moved: bool,
    ) -> bool {
        if target_sheet_idx >= self.sheets.len() {
            return false;
        }
        let Some((src_sheet_idx, src_idx)) = self.sheets.iter().enumerate().find_map(|(di, d)| {
            d.komas
                .iter()
                .position(|b| b.id == koma_id)
                .map(|bi| (di, bi))
        }) else {
            return false;
        };

        let mut adjusted = target_index.min(self.sheets[target_sheet_idx].komas.len());
        if src_sheet_idx == target_sheet_idx {
            if src_idx < adjusted {
                adjusted = adjusted.saturating_sub(1);
            }
            if src_idx == adjusted && !focus_moved {
                return false;
            }
            if src_idx == adjusted && self.active_sheet == target_sheet_idx {
                let changed =
                    self.sheets[target_sheet_idx].focused_koma_id.as_deref() != Some(koma_id);
                self.sheets[target_sheet_idx].focused_koma_id = Some(koma_id.to_string());
                return changed;
            }
        }

        let src_sheet = &mut self.sheets[src_sheet_idx];
        let mut koma = src_sheet.komas.remove(src_idx);
        if src_sheet_idx != target_sheet_idx
            && (focus_moved || src_sheet.focused_koma_id.as_deref() == Some(koma_id))
        {
            src_sheet.focused_koma_id = if src_sheet.komas.is_empty() {
                None
            } else {
                let next_idx = src_idx.min(src_sheet.komas.len() - 1);
                Some(src_sheet.komas[next_idx].id.clone())
            };
        }

        let target_sheet = &mut self.sheets[target_sheet_idx];
        let target_was_empty = src_sheet_idx != target_sheet_idx && target_sheet.komas.is_empty();
        koma.sheet_id = target_sheet.id.clone();
        let clamped = adjusted.min(target_sheet.komas.len());
        target_sheet.komas.insert(clamped, koma);
        if focus_moved || target_was_empty {
            target_sheet.focused_koma_id = Some(koma_id.to_string());
        }
        if focus_moved {
            self.active_sheet = target_sheet_idx;
        }
        true
    }

    pub fn set_koma_width(&mut self, koma_id: &str, width: f64) -> Result<f64, String> {
        if !width.is_finite() || width <= 0.0 {
            return Err("Koma width must be finite and positive".into());
        }
        let koma = self
            .koma_by_id_mut(koma_id)
            .ok_or_else(|| format!("Koma not found: {koma_id}"))?;
        koma.width = width.clamp(MIN_KOMA_WIDTH, MAX_KOMA_WIDTH);
        koma.ratio = None;
        koma.restore = None;
        Ok(koma.width)
    }

    /// Steps the pixel width (converting a proportional width first).
    pub fn resize_focused_koma(&mut self, delta: f64, usable: f64, gap: f64) -> bool {
        let sheet = self.current_sheet_mut();
        let Some(cur_idx) = sheet.focused_index() else {
            return false;
        };
        let koma = &mut sheet.komas[cur_idx];
        let current = koma.effective_width(usable, gap);
        let next = (current + delta).clamp(MIN_KOMA_WIDTH, MAX_KOMA_WIDTH);
        if (next - current).abs() < 0.5 {
            return false;
        }
        koma.width = next;
        koma.ratio = None;
        koma.restore = None;
        true
    }

    /// Cycles the focused Koma through `presets`; an unlisted width starts at the first.
    pub fn cycle_width_preset(&mut self, presets: &[f64]) -> bool {
        let Some(&first) = presets.first() else {
            return false;
        };
        let Some(idx) = self.current_sheet().focused_index() else {
            return false;
        };
        let koma = &mut self.current_sheet_mut().komas[idx];
        let next = koma
            .ratio
            .and_then(|r| presets.iter().position(|p| (p - r).abs() < 1e-3))
            .map_or(first, |i| presets[(i + 1) % presets.len()]);
        koma.ratio = Some(next);
        koma.restore = None;
        true
    }

    pub fn apply_width_preset(&mut self, presets: &[f64], preset_idx: usize) -> bool {
        let Some(&ratio) = presets.get(preset_idx) else {
            return false;
        };
        let Some(idx) = self.current_sheet().focused_index() else {
            return false;
        };
        let koma = &mut self.current_sheet_mut().komas[idx];
        koma.ratio = Some(ratio);
        koma.restore = None;
        true
    }

    pub fn rename_koma(&mut self, koma_id: &str, name: &str) -> Result<String, String> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err("Koma name cannot be empty".into());
        }
        for sheet in &mut self.sheets {
            if let Some(koma) = sheet.komas.iter_mut().find(|b| b.id == koma_id) {
                koma.label = trimmed.to_string();
                return Ok(koma.label.clone());
            }
        }
        Err(format!("Koma not found: {koma_id}"))
    }

    pub fn rename_sheet(&mut self, sheet_id: &str, name: &str) -> Result<String, String> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err("Sheet name cannot be empty".into());
        }
        let Some(sheet) = self.sheets.iter_mut().find(|d| d.id == sheet_id) else {
            return Err(format!("Sheet not found: {sheet_id}"));
        };
        sheet.label = trimmed.to_string();
        Ok(sheet.label.clone())
    }

    /// Toggles the focused Koma between full width and its previous sizing.
    pub fn toggle_maximize_focused_koma(&mut self) -> bool {
        let Some(idx) = self.current_sheet().focused_index() else {
            return false;
        };
        let koma = &mut self.current_sheet_mut().komas[idx];
        match koma.restore.take() {
            Some((width, ratio)) => {
                koma.width = width;
                koma.ratio = ratio;
            }
            None => {
                koma.restore = Some((koma.width, koma.ratio));
                koma.ratio = Some(1.0);
            }
        }
        true
    }

    pub fn insert_koma_after_focused(
        &mut self,
        kind: KomaKind,
        label: String,
        initial_url: Option<String>,
        cwd: Option<String>,
    ) -> KomaMeta {
        self.insert_koma_with_command(kind, label, initial_url, cwd, None)
    }

    pub fn insert_koma_with_command(
        &mut self,
        kind: KomaKind,
        label: String,
        initial_url: Option<String>,
        cwd: Option<String>,
        command: Option<String>,
    ) -> KomaMeta {
        let id = self.allocate_koma_id();
        let profile_id = self.profile_id.clone();
        let sheet = self.current_sheet_mut();
        let insert_idx = sheet
            .focused_index()
            .map(|idx| (idx + 1).min(sheet.komas.len()))
            .unwrap_or(0);
        let koma = KomaMeta {
            id: id.clone(),
            sheet_id: sheet.id.clone(),
            profile_id,
            kind,
            label,
            initial_url,
            cwd,
            command,
            width: DEFAULT_KOMA_WIDTH,
            ratio: None,
            restore: None,
            koma_api_enabled: false,
        };
        sheet.komas.insert(insert_idx, koma.clone());
        sheet.focused_koma_id = Some(id);
        koma
    }

    pub fn duplicate_focused_koma(
        &mut self,
        session_prefixes: &[String],
        extra_session_names: &[String],
        live_url: Option<String>,
    ) -> Result<KomaMeta, String> {
        let koma = self
            .focused_koma()
            .cloned()
            .ok_or_else(|| "No focused koma to duplicate".to_string())?;
        match koma.kind {
            KomaKind::Web => {
                let url = live_url
                    .or(koma.initial_url)
                    .ok_or_else(|| "Web koma has no URL to duplicate".to_string())?;
                let label = crate::input::derive_url_label(&url);
                Ok(self.insert_koma_after_focused(KomaKind::Web, label, Some(url), None))
            }
            KomaKind::Terminal => {
                let command = koma.command.as_deref().and_then(|cmd| {
                    self.next_session_command(cmd, session_prefixes, extra_session_names)
                });
                let label = if let Some(cmd) = &command {
                    cmd.split_whitespace()
                        .last()
                        .map(str::to_string)
                        .unwrap_or_else(|| koma.label.clone())
                } else {
                    koma.label.clone()
                };
                Ok(self.insert_koma_with_command(
                    KomaKind::Terminal,
                    label,
                    None,
                    koma.cwd,
                    command,
                ))
            }
        }
    }

    fn next_session_command(
        &self,
        cmd: &str,
        session_prefixes: &[String],
        extra_session_names: &[String],
    ) -> Option<String> {
        let prefix = session_prefixes
            .iter()
            .find(|p| !p.is_empty() && cmd.starts_with(p.as_str()))?;
        let rest = cmd[prefix.len()..].trim();
        if rest.is_empty() {
            return None;
        }
        let base = rest
            .rsplit_once('-')
            .and_then(|(head, tail)| {
                (!head.is_empty() && tail.chars().all(|c| c.is_ascii_digit())).then_some(head)
            })
            .unwrap_or(rest);
        for n in 2usize..1000 {
            let candidate_name = format!("{base}-{n}");
            let candidate = format!("{prefix}{candidate_name}");
            let used_externally = extra_session_names.iter().any(|s| s == &candidate_name);
            let used = used_externally
                || self
                    .sheets
                    .iter()
                    .flat_map(|d| d.komas.iter())
                    .chain(self.drawer.iter())
                    .any(|b| b.command.as_deref() == Some(candidate.as_str()));
            if !used {
                return Some(candidate);
            }
        }
        None
    }

    pub fn remove_focused_koma(&mut self) -> Option<KomaMeta> {
        let id = self.current_sheet().focused_koma_id.clone()?;
        self.remove_koma_by_id(&id)
    }

    pub fn close_koma(&mut self, id: &str, prefer_left: bool) -> Option<KomaMeta> {
        let (index, koma) = self.take_koma(id, prefer_left)?;
        if self.closed_komas.len() == MAX_CLOSED_KOMAS {
            self.closed_komas.remove(0);
        }
        self.closed_komas.push((index, koma.clone()));
        Some(koma)
    }

    pub fn reopen_closed_koma(&mut self) -> Option<KomaMeta> {
        let (original_index, mut koma) = self.closed_komas.pop()?;
        let origin = self
            .sheets
            .iter()
            .position(|sheet| sheet.id == koma.sheet_id);
        self.active_sheet = origin.unwrap_or(self.active_sheet);
        let sheet = self.current_sheet_mut();
        let index = if origin.is_some() {
            original_index.min(sheet.komas.len())
        } else {
            sheet.focused_index().map_or(0, |index| index + 1)
        };
        koma.sheet_id.clone_from(&sheet.id);
        if koma.kind == KomaKind::Terminal {
            koma.command = None;
        }
        sheet.komas.insert(index, koma.clone());
        sheet.focused_koma_id = Some(koma.id.clone());
        Some(koma)
    }

    pub fn remove_koma_by_id(&mut self, koma_id: &str) -> Option<KomaMeta> {
        self.take_koma(koma_id, false).map(|(_, koma)| koma)
    }

    fn take_koma(&mut self, koma_id: &str, prefer_left: bool) -> Option<(usize, KomaMeta)> {
        for sheet in &mut self.sheets {
            if let Some(idx) = sheet.komas.iter().position(|b| b.id == koma_id) {
                let removed = sheet.komas.remove(idx);
                if sheet.focused_koma_id.as_deref() == Some(koma_id) {
                    sheet.focused_koma_id = if sheet.komas.is_empty() {
                        None
                    } else {
                        let next_idx = if prefer_left {
                            idx.saturating_sub(1)
                        } else {
                            idx.min(sheet.komas.len() - 1)
                        };
                        Some(sheet.komas[next_idx].id.clone())
                    };
                }
                return Some((idx, removed));
            }
        }
        None
    }

    pub fn add_sheet(&mut self, with_default_terminal: bool) -> usize {
        let index = self.append_sheet(None, with_default_terminal);
        self.active_sheet = index;
        index
    }

    pub fn add_named_sheet(&mut self, label: &str, focus: bool) -> Result<&SheetState, String> {
        let label = label.trim();
        if label.is_empty() {
            return Err("Sheet label must not be empty".into());
        }
        let index = self.append_sheet(Some(label), false);
        if focus {
            self.active_sheet = index;
        }
        Ok(&self.sheets[index])
    }

    fn append_sheet(&mut self, custom_label: Option<&str>, with_default_terminal: bool) -> usize {
        let (sheet_id, label) = self.allocate_sheet_identity();
        let mut komas = Vec::new();
        let mut focused_koma_id = None;
        if with_default_terminal {
            let koma_id = self.allocate_koma_id();
            focused_koma_id = Some(koma_id.clone());
            komas.push(KomaMeta {
                id: koma_id,
                sheet_id: sheet_id.clone(),
                profile_id: self.profile_id.clone(),
                kind: KomaKind::Terminal,
                label: "Shell".into(),
                initial_url: None,
                cwd: None,
                command: None,
                width: DEFAULT_KOMA_WIDTH,
                ratio: None,
                restore: None,
                koma_api_enabled: false,
            });
        }
        self.sheets.push(SheetState {
            id: sheet_id,
            label: custom_label.map_or(label, str::to_owned),
            focused_koma_id,
            komas,
        });
        self.sheets.len() - 1
    }

    pub fn remove_current_sheet(&mut self) -> Result<SheetState, String> {
        if self.sheets.len() <= 1 {
            return Err("Cannot close the last sheet".into());
        }
        let removed = self.sheets.remove(self.active_sheet);
        self.active_sheet = self.active_sheet.min(self.sheets.len() - 1);
        Ok(removed)
    }

    pub fn record_recent(&mut self, input: impl Into<String>, label: impl Into<String>) {
        if self.is_private {
            return;
        }
        let input = input.into();
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return;
        }
        let trimmed_str = trimmed.to_string();
        let label_str = label.into().trim().to_string();
        let label_str = if label_str.is_empty() {
            trimmed_str.clone()
        } else {
            label_str
        };
        self.recents.retain(|r| r.input != trimmed_str);
        self.recents.insert(
            0,
            RecentEntry {
                input: trimmed_str,
                label: label_str,
            },
        );
        self.recents.truncate(MAX_RECENTS);
    }

    pub fn update_koma_url(&mut self, koma_id: &str, url: String, label: Option<String>) -> bool {
        let Some(koma) = self.koma_by_id_mut(koma_id) else {
            return false;
        };
        if koma.kind != KomaKind::Web {
            return false;
        }
        let mut changed = koma.initial_url.as_deref() != Some(url.as_str());
        koma.initial_url = Some(url);
        if let Some(new_label) = label.filter(|l| !l.trim().is_empty()) {
            if koma.label != new_label {
                koma.label = new_label;
                changed = true;
            }
        }
        changed
    }

    pub fn update_terminal_metadata(
        &mut self,
        id: &str,
        label: Option<String>,
        cwd: Option<String>,
    ) -> bool {
        let Some(koma) = self
            .sheets
            .iter_mut()
            .flat_map(|sheet| &mut sheet.komas)
            .chain(&mut self.drawer)
            .find(|koma| koma.id == id && koma.kind == KomaKind::Terminal)
        else {
            return false;
        };
        let mut changed = false;
        if let Some(label) = label.filter(|label| !label.trim().is_empty() && *label != koma.label)
        {
            koma.label = label;
            changed = true;
        }
        if let Some(cwd) = cwd.filter(|cwd| koma.cwd.as_ref() != Some(cwd)) {
            koma.cwd = Some(cwd);
            changed = true;
        }
        changed
    }

    pub fn koma_by_id(&self, id: &str) -> Option<&KomaMeta> {
        self.sheets
            .iter()
            .flat_map(|d| d.komas.iter())
            .find(|b| b.id == id)
    }

    pub fn set_koma_api_enabled(&mut self, id: &str, enabled: bool) {
        if let Some(koma) = self.koma_by_id_mut(id).filter(|b| b.kind == KomaKind::Web) {
            koma.koma_api_enabled = enabled;
        }
    }

    fn koma_by_id_mut(&mut self, id: &str) -> Option<&mut KomaMeta> {
        self.sheets
            .iter_mut()
            .flat_map(|d| d.komas.iter_mut())
            .find(|b| b.id == id)
    }

    pub fn sheet_web_koma(&self, sheet_idx: usize) -> Option<&KomaMeta> {
        self.sheets
            .get(sheet_idx)?
            .komas
            .iter()
            .find(|b| b.kind == KomaKind::Web)
    }

    pub fn sheet_terminal_koma(&self, sheet_idx: usize) -> Option<&KomaMeta> {
        self.sheets
            .get(sheet_idx)?
            .komas
            .iter()
            .find(|b| b.kind == KomaKind::Terminal)
    }

    /// Resolves a target Koma using explicit ID, ambient `KOMA_KOMA_ID`, or active Sheet.
    pub fn resolve_koma(
        &self,
        explicit_koma_id: Option<&str>,
        caller_koma_id: Option<&str>,
        target_kind: KomaKind,
    ) -> Result<&KomaMeta, String> {
        if let Some(id) = explicit_koma_id {
            let koma = self
                .koma_by_id(id)
                .ok_or_else(|| format!("Koma not found: {id}"))?;
            if koma.kind != target_kind {
                return Err(format!(
                    "Koma {id} is {} (expected {})",
                    koma.kind.as_str(),
                    target_kind.as_str()
                ));
            }
            return Ok(koma);
        }

        if let Some(caller_id) = caller_koma_id {
            if let Some(sheet) = self
                .sheets
                .iter()
                .find(|d| d.komas.iter().any(|b| b.id == caller_id))
            {
                if target_kind == KomaKind::Terminal {
                    if let Some(caller) = sheet
                        .komas
                        .iter()
                        .find(|b| b.id == caller_id && b.kind == KomaKind::Terminal)
                    {
                        return Ok(caller);
                    }
                }
                return resolve_in_sheet(sheet, target_kind);
            }
        }

        resolve_in_sheet(self.current_sheet(), target_kind)
    }

    fn allocate_koma_id(&mut self) -> String {
        loop {
            let candidate = format!("koma-{}", self.next_koma_seq);
            self.next_koma_seq += 1;
            let used = self.koma_by_id(&candidate).is_some()
                || self.drawer.iter().any(|b| b.id == candidate)
                || self.discarded_drawer.iter().any(|b| b.id == candidate);
            if !used {
                return candidate;
            }
        }
    }

    fn allocate_sheet_identity(&mut self) -> (String, String) {
        loop {
            let seq = self.next_sheet_seq;
            self.next_sheet_seq += 1;
            let id = format!("sheet-{seq}");
            if self.sheets.iter().all(|d| d.id != id) {
                return (id, format!("Sheet {seq}"));
            }
        }
    }
}

fn step_index(current: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    let next = (current.min(len - 1) as isize + delta).clamp(0, (len - 1) as isize);
    next as usize
}

fn resolve_in_sheet(sheet: &SheetState, target_kind: KomaKind) -> Result<&KomaMeta, String> {
    if let Some(focused) = sheet.focused_koma() {
        if focused.kind == target_kind {
            return Ok(focused);
        }
    }
    sheet
        .komas
        .iter()
        .find(|b| b.kind == target_kind)
        .ok_or_else(|| format!("No {} koma on sheet {}", target_kind.as_str(), sheet.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reopens_explicitly_closed_komas_in_reverse_order_and_restores_focus() {
        let mut state = WorkspaceState::poc_default();
        let original = state.current_sheet().komas.clone();
        let terminal = &original[0].id;
        let web = &original[1].id;
        let term = state.koma_by_id_mut(terminal).unwrap();
        term.cwd = Some("/work".into());
        term.command = Some("example-command".into());
        state.close_koma(web, false).unwrap();
        state.close_koma(terminal, false).unwrap();
        state.remove_koma_by_id("sheet2-term").unwrap();
        state.switch_sheet(1);

        let restored = state.reopen_closed_koma().unwrap();
        assert_eq!(restored.id, *terminal);
        assert_eq!(restored.cwd.as_deref(), Some("/work"));
        assert_eq!(restored.command, None);
        assert_eq!(state.active_sheet_index(), 0);
        assert_eq!(state.focused_koma(), Some(&restored));

        assert_eq!(state.reopen_closed_koma().as_ref(), Some(&original[1]));
        assert_eq!(state.current_sheet().komas, [restored, original[1].clone()]);
        assert_eq!(state.focused_koma().map(|koma| &koma.id), Some(web));
        assert!(state.reopen_closed_koma().is_none());
    }

    #[test]
    fn reopens_next_to_focus_when_the_original_sheet_was_removed() {
        let mut state = WorkspaceState::poc_default();
        let id = state.current_sheet().komas[0].id.clone();
        state.close_koma(&id, false).unwrap();
        state.remove_current_sheet().unwrap();
        let target = state.current_sheet().id.clone();
        let focused_id = state.focused_koma().unwrap().id.clone();
        let restored = state.reopen_closed_koma().unwrap();
        assert_eq!(restored.sheet_id, target);
        assert_eq!(state.current_sheet().komas[0].id, focused_id);
        assert_eq!(state.current_sheet().komas[1].id, id);
        assert_eq!(state.focused_koma(), Some(&restored));
    }

    #[test]
    fn restore_layout_requires_a_valid_active_sheet() {
        assert!(WorkspaceState::restore_layout("profile-a".into(), Vec::new(), 0).is_err());
        let state = WorkspaceState::new();
        assert!(WorkspaceState::restore_layout("profile-a".into(), state.sheets, 1).is_err());
    }

    #[test]
    fn restore_layout_drops_a_stale_koma_focus() {
        let mut sheets = WorkspaceState::poc_default().sheets;
        sheets[1].focused_koma_id = Some("missing".into());
        let state = WorkspaceState::restore_layout("profile-a".into(), sheets, 1).unwrap();
        assert_eq!(state.active_sheet_index(), 1);
        assert_eq!(state.current_sheet().focused_koma_id, None);
    }

    #[test]
    fn ambient_targeting_follows_caller_sheet() {
        let state = WorkspaceState::poc_default();
        assert_eq!(
            state
                .resolve_koma(None, Some("sheet1-term"), KomaKind::Web)
                .unwrap()
                .id,
            "sheet1-web"
        );
        assert_eq!(
            state
                .resolve_koma(None, Some("sheet2-term"), KomaKind::Web)
                .unwrap()
                .id,
            "sheet2-web"
        );
        assert_eq!(
            state
                .resolve_koma(None, Some("sheet2-term"), KomaKind::Terminal)
                .unwrap()
                .id,
            "sheet2-term"
        );
    }

    #[test]
    fn explicit_koma_overrides_ambient_and_rejects_mismatch() {
        let state = WorkspaceState::poc_default();
        assert_eq!(
            state
                .resolve_koma(Some("sheet2-web"), Some("sheet1-term"), KomaKind::Web)
                .unwrap()
                .id,
            "sheet2-web"
        );
        assert!(state
            .resolve_koma(Some("missing"), Some("sheet1-term"), KomaKind::Web)
            .is_err());
        assert!(state
            .resolve_koma(Some("sheet1-term"), Some("sheet1-term"), KomaKind::Web)
            .is_err());
    }

    #[test]
    fn koma_insertion_focus_reorder_resize_and_removal_update_sheet() {
        let mut state = WorkspaceState::poc_default();
        assert_eq!(state.focused_koma().unwrap().id, "sheet1-term");

        let created = state.insert_koma_after_focused(
            KomaKind::Web,
            "Docs".into(),
            Some("https://docs.rs".into()),
            None,
        );
        let ids: Vec<&str> = state
            .current_sheet()
            .komas
            .iter()
            .map(|b| b.id.as_str())
            .collect();
        assert_eq!(ids, vec!["sheet1-term", created.id.as_str(), "sheet1-web"]);
        assert_eq!(state.focused_koma().unwrap().id, created.id);

        assert!(state.move_focused_koma(1));
        let moved_ids: Vec<&str> = state
            .current_sheet()
            .komas
            .iter()
            .map(|b| b.id.as_str())
            .collect();
        assert_eq!(
            moved_ids,
            vec!["sheet1-term", "sheet1-web", created.id.as_str()]
        );

        assert!(state.resize_focused_koma(KOMA_WIDTH_STEP, 1260.0, 10.0));
        assert_eq!(
            state.focused_koma().unwrap().width,
            DEFAULT_KOMA_WIDTH + KOMA_WIDTH_STEP
        );

        let removed = state.remove_focused_koma().unwrap();
        assert_eq!(removed.id, created.id);
        assert_eq!(state.focused_koma().unwrap().id, "sheet1-web");

        assert!(state.focus_relative_koma(-1));
        assert_eq!(state.focused_koma().unwrap().id, "sheet1-term");

        // Removing a koma on an inactive sheet updates that sheet without altering active focus.
        let sheet2_before = state.sheets[1].komas.len();
        let inactive = state.remove_koma_by_id("sheet2-term").unwrap();
        assert_eq!(inactive.id, "sheet2-term");
        assert_eq!(state.sheets[1].komas.len(), sheet2_before - 1);
        assert_ne!(
            state.sheets[1].focused_koma_id.as_deref(),
            Some("sheet2-term")
        );
        assert_eq!(state.focused_koma().unwrap().id, "sheet1-term");
        assert!(state.remove_koma_by_id("sheet2-term").is_none());

        // Closing with prefer-left focuses the left neighbor when one exists.
        let right = state.insert_koma_after_focused(KomaKind::Terminal, "Right".into(), None, None);
        let middle =
            state.insert_koma_after_focused(KomaKind::Terminal, "Middle".into(), None, None);
        assert_eq!(state.focused_koma().unwrap().id, middle.id);
        let id = state.focused_koma().unwrap().id.clone();
        state.close_koma(&id, true);
        assert_eq!(state.focused_koma().unwrap().id, right.id);
    }

    #[test]
    fn width_presets_cycle_maximize_toggles_and_resize_clears_ratio() {
        let mut state = WorkspaceState::poc_default();
        let presets = DEFAULT_WIDTH_PRESETS;
        assert!(state.cycle_width_preset(&presets));
        assert_eq!(state.focused_koma().unwrap().ratio, Some(presets[0]));
        for expected in [presets[1], presets[2], presets[3], presets[0]] {
            state.cycle_width_preset(&presets);
            assert_eq!(state.focused_koma().unwrap().ratio, Some(expected));
        }
        assert!(!state.cycle_width_preset(&[]));

        // Two 1/2 komas plus one gap fill the usable width.
        state.cycle_width_preset(&presets);
        let half = state.focused_koma().unwrap().effective_width(1000.0, 10.0);
        assert!((half * 2.0 + 10.0 - 1000.0).abs() < 1e-6);

        assert!(state.toggle_maximize_focused_koma());
        assert_eq!(state.focused_koma().unwrap().ratio, Some(1.0));
        assert!(state.toggle_maximize_focused_koma());
        assert_eq!(state.focused_koma().unwrap().ratio, Some(0.5));

        assert!(state.resize_focused_koma(KOMA_WIDTH_STEP, 1000.0, 10.0));
        let koma = state.focused_koma().unwrap();
        assert_eq!(koma.ratio, None);
        assert_eq!(koma.width, half + KOMA_WIDTH_STEP);

        assert!(state.apply_width_preset(&presets, 2));
        assert_eq!(state.focused_koma().unwrap().ratio, Some(presets[2]));
        assert!(!state.apply_width_preset(&presets, 99));

        assert_eq!(
            state.rename_koma("sheet1-term", "  API Server  ").unwrap(),
            "API Server"
        );
        assert_eq!(state.focused_koma().unwrap().label, "API Server");
        assert!(state.rename_koma("sheet1-term", "   ").is_err());

        assert_eq!(state.rename_sheet("sheet-1", "  Work  ").unwrap(), "Work");
        assert_eq!(state.current_sheet().label, "Work");
        assert!(state.rename_sheet("sheet-1", "").is_err());
    }

    #[test]
    fn sheet_creation_and_relative_switching_work() {
        let mut state = WorkspaceState::poc_default();
        let idx = state.add_sheet(true);
        assert_eq!(idx, 2);
        assert_eq!(state.active_sheet, 2);
        assert_eq!(state.current_sheet().komas.len(), 1);
        assert_eq!(state.focused_koma().unwrap().kind, KomaKind::Terminal);

        assert!(state.switch_sheet_relative(1, true));
        assert_eq!(state.active_sheet, 0);
        assert!(state.switch_sheet_relative(-1, true));
        assert_eq!(state.active_sheet, 2);
        assert!(!state.switch_sheet_relative(1, false));
    }

    #[test]
    fn move_focused_koma_to_sheet_and_position_updates_sheets_cleanly() {
        let mut state = WorkspaceState::poc_default();
        assert!(!state.move_focused_koma_to_sheet(-1));

        // Move sheet1-term from Sheet 1 to Sheet 2 after focused sheet2-term.
        assert!(state.move_focused_koma_to_sheet(1));
        assert_eq!(state.active_sheet, 1);
        assert_eq!(
            state.sheets[0]
                .komas
                .iter()
                .map(|b| b.id.as_str())
                .collect::<Vec<_>>(),
            vec!["sheet1-web"]
        );
        assert_eq!(
            state.sheets[0].focused_koma_id.as_deref(),
            Some("sheet1-web")
        );
        assert_eq!(
            state.sheets[1]
                .komas
                .iter()
                .map(|b| b.id.as_str())
                .collect::<Vec<_>>(),
            vec!["sheet2-term", "sheet1-term", "sheet2-web"]
        );
        assert_eq!(state.focused_koma().unwrap().id, "sheet1-term");
        assert_eq!(state.focused_koma().unwrap().sheet_id, "sheet-2");

        // Same-sheet reorder via drop gap index.
        assert!(state.move_koma_to_position("sheet1-term", 1, 3));
        assert_eq!(
            state.sheets[1]
                .komas
                .iter()
                .map(|b| b.id.as_str())
                .collect::<Vec<_>>(),
            vec!["sheet2-term", "sheet2-web", "sheet1-term"]
        );

        // Move last koma out of Sheet 1 leaving it empty.
        assert!(state.move_koma_to_position("sheet1-web", 1, 0));
        assert!(state.sheets[0].komas.is_empty());
        assert_eq!(state.sheets[0].focused_koma_id, None);
        assert!(!state.move_koma_to_position("sheet1-web", 99, 0));
    }

    #[test]
    fn recents_dedup_cap_and_skip_when_private() {
        let mut state = WorkspaceState::new();
        for i in 0..(MAX_RECENTS + 3) {
            state.record_recent(format!("https://ex{i}.com"), format!("Ex {i}"));
        }
        assert_eq!(state.recents.len(), MAX_RECENTS);
        assert_eq!(
            state.recents[0].input,
            format!("https://ex{}.com", MAX_RECENTS + 2)
        );

        state.record_recent("https://ex5.com", "Ex 5 Updated");
        assert_eq!(state.recents.len(), MAX_RECENTS);
        assert_eq!(state.recents[0].input, "https://ex5.com");
        assert_eq!(state.recents[0].label, "Ex 5 Updated");

        let mut priv_state = WorkspaceState::new();
        priv_state.is_private = true;
        priv_state.record_recent("https://secret.example", "Secret");
        assert!(priv_state.recents.is_empty());
    }

    #[test]
    fn sheet_close_rejects_last_sheet_and_adjusts_active_index() {
        let mut state = WorkspaceState::new();
        assert!(state.remove_current_sheet().is_err());

        let mut multi = WorkspaceState::poc_default();
        multi.active_sheet = 1;
        let removed = multi.remove_current_sheet().unwrap();
        assert_eq!(removed.id, "sheet-2");
        assert_eq!(multi.sheets.len(), 1);
        assert_eq!(multi.active_sheet, 0);
    }

    #[test]
    fn duplicate_koma_applies_session_prefixes_or_opens_plain_shell() {
        let mut state = WorkspaceState::new();
        state.insert_koma_with_command(
            KomaKind::Terminal,
            "dev".into(),
            None,
            Some("/tmp".into()),
            Some("zmx attach dev".into()),
        );
        let prefixes = vec!["zmx attach ".to_string()];
        let dup1 = state.duplicate_focused_koma(&prefixes, &[], None).unwrap();
        assert_eq!(dup1.command.as_deref(), Some("zmx attach dev-2"));
        assert_eq!(dup1.cwd.as_deref(), Some("/tmp"));

        let dup2 = state
            .duplicate_focused_koma(&prefixes, &["dev-3".into()], None)
            .unwrap();
        assert_eq!(dup2.command.as_deref(), Some("zmx attach dev-4"));

        let plain = state.duplicate_focused_koma(&[], &[], None).unwrap();
        assert_eq!(plain.command, None);
        assert_eq!(plain.cwd.as_deref(), Some("/tmp"));
    }
}
