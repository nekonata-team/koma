//! Workspace synchronization, child Koma widget lifecycle, focus management, and terminal access.

use std::cell::{Ref, RefCell, RefMut};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use koma_app::config::{CenterFocused, ScaleConfig};
use koma_app::ghostty_config::{CellHeight, TerminalSettings};
use koma_core::model::{InputMode, KomaId, KomaKind, KomaMeta, SheetId, WorkspaceState};
use makepad_app_terminal::term::color::Rgb;
use makepad_app_terminal::widget::{MpTerm, TerminalColors};
use makepad_widgets::*;

use crate::runtime::web_views::WebViewRuntime;
use crate::ui::web_koma::WebKoma;

use super::animation::{koma_start_x, CloseGap, ReorderSlide, SheetSlide};
use super::{KomaSlot, SheetSnapshot, SheetStrip, OVERVIEW_RENDER_EPS};

pub(super) enum KomaWidget {
    Terminal(WidgetRef),
    Web(WidgetRef),
}

impl KomaWidget {
    pub(super) fn as_widget(&self) -> &WidgetRef {
        match self {
            Self::Terminal(w) | Self::Web(w) => w,
        }
    }

    pub(super) fn as_widget_mut(&mut self) -> &mut WidgetRef {
        match self {
            Self::Terminal(w) | Self::Web(w) => w,
        }
    }

    pub(super) fn is_terminal(&self) -> bool {
        matches!(self, Self::Terminal(_))
    }

    pub(super) fn terminal(&self) -> Option<Ref<'_, MpTerm>> {
        match self {
            Self::Terminal(w) => w.borrow::<MpTerm>(),
            Self::Web(_) => None,
        }
    }

    pub(super) fn terminal_mut(&self) -> Option<RefMut<'_, MpTerm>> {
        match self {
            Self::Terminal(w) => w.borrow_mut::<MpTerm>(),
            Self::Web(_) => None,
        }
    }

    pub(super) fn web(&self) -> Option<Ref<'_, WebKoma>> {
        match self {
            Self::Web(w) => w.borrow::<WebKoma>(),
            Self::Terminal(_) => None,
        }
    }

    pub(super) fn web_mut(&self) -> Option<RefMut<'_, WebKoma>> {
        match self {
            Self::Web(w) => w.borrow_mut::<WebKoma>(),
            Self::Terminal(_) => None,
        }
    }

    pub(super) fn has_input_focus(&self, cx: &Cx) -> bool {
        match self {
            Self::Terminal(_) => self.terminal().is_some_and(|t| t.has_input_focus(cx)),
            Self::Web(_) => self.web().is_some_and(|w| w.has_input_focus(cx)),
        }
    }
}

#[derive(Debug, PartialEq)]
enum SheetSlideTransition {
    Keep,
    Clear,
    Start(SheetSlide),
}

#[derive(Debug, PartialEq)]
enum ScrollAction {
    None,
    RestoreSheet(f64),
    KeepAnchor(f64),
}

#[derive(Debug, PartialEq, Eq)]
enum RevealAction {
    None,
    RevealKoma(KomaId),
    ResetEmpty,
}

#[derive(Debug, PartialEq, Eq)]
enum OverviewTransition {
    None,
    Enter { from_sheet: usize },
    Exit,
    Stay,
}

struct SyncContext<'a> {
    old_sheets: &'a [SheetSnapshot],
    prev_mode: InputMode,
    prev_active: usize,
    prev_scroll_x: f64,
    sheet_scroll_x: &'a HashMap<SheetId, f64>,
    koma_gap: f64,
    usable_width: f64,
    viewport_size: Vec2d,
    is_overview_rendered: bool,
    has_sheet_slide: bool,
    close_gaps: &'a HashMap<KomaId, CloseGap>,
    reorder_slide: Option<&'a ReorderSlide>,
}

struct SyncPlan {
    stop_scroll_motion: bool,
    clear_motion_and_pinch: bool,
    reorder_slide: Option<ReorderSlide>,
    sheet_slide: SheetSlideTransition,
    clear_strip_drag: bool,
    scroll: ScrollAction,
    reveal: RevealAction,
    overview: OverviewTransition,
}

fn plan_sheet_sync(
    ctx: &SyncContext<'_>,
    state: &WorkspaceState,
    reveal_focused: bool,
) -> SyncPlan {
    let new_active = state.active_sheet_index();
    let in_overview = state.mode == InputMode::Overview;
    let prev_sheet_id = ctx
        .old_sheets
        .get(ctx.prev_active)
        .map(|d| d.sheet_id.clone());
    let same_sheet = ctx
        .old_sheets
        .get(ctx.prev_active)
        .zip(state.sheets().get(new_active))
        .filter(|(old, new)| old.sheet_id == new.id);
    let stop_scroll_motion = prev_sheet_id.is_some() && same_sheet.is_none();

    let same_order = same_sheet.is_some_and(|(old, new)| {
        old.komas
            .iter()
            .map(|slot| &slot.koma_id)
            .eq(new.komas.iter().map(|koma| &koma.id))
    });
    let old_focused_anchor = if !reveal_focused && !in_overview {
        same_sheet.and_then(|(old, new)| {
            let fid = old.focused_koma_id.as_ref()?;
            (new.focused_koma_id.as_ref() == Some(fid)).then(|| {
                koma_start_x(&old.komas, fid, ctx.koma_gap).map(|old_x| (fid.clone(), old_x))
            })?
        })
    } else {
        None
    };

    let clear_motion_and_pinch = in_overview || !same_order;
    let reorder_slide = if clear_motion_and_pinch {
        same_sheet
            .filter(|_| {
                !in_overview
                    && !ctx.is_overview_rendered
                    && !ctx.has_sheet_slide
                    && ctx.viewport_size.x > 1.0
                    && old_focused_anchor.is_none()
            })
            .and_then(|(old, new)| {
                ReorderSlide::between(
                    &old.komas,
                    new.komas.iter().map(|koma| &koma.id),
                    ctx.koma_gap,
                    ctx.close_gaps,
                    ctx.reorder_slide,
                )
            })
    } else {
        None
    };

    let sheet_slide = if ctx.prev_active != new_active && !in_overview && ctx.viewport_size.y > 1.0
    {
        if let Some(prev_id) = prev_sheet_id {
            let prev_exists = state.sheets().iter().any(|d| d.id == prev_id);
            let new_differs = state
                .sheets()
                .get(new_active)
                .is_some_and(|d| d.id != prev_id);
            if prev_exists && new_differs {
                let direction = if new_active > ctx.prev_active {
                    1.0
                } else {
                    -1.0
                };
                SheetSlideTransition::Start(SheetSlide {
                    prev_sheet_id: prev_id,
                    prev_scroll_x: ctx.prev_scroll_x,
                    offset_y: direction * ctx.viewport_size.y,
                    direction,
                })
            } else {
                SheetSlideTransition::Clear
            }
        } else {
            SheetSlideTransition::Keep
        }
    } else if in_overview {
        SheetSlideTransition::Clear
    } else {
        SheetSlideTransition::Keep
    };

    let scroll = if ctx.prev_active != new_active {
        let restored = state
            .sheets()
            .get(new_active)
            .and_then(|d| ctx.sheet_scroll_x.get(&d.id).copied())
            .unwrap_or(0.0);
        ScrollAction::RestoreSheet(restored)
    } else if let Some((fid, old_x)) = old_focused_anchor {
        let new_x = state.sheets().get(new_active).and_then(|sheet| {
            let mut x = 0.0;
            for koma in &sheet.komas {
                if koma.id == fid {
                    return Some(x);
                }
                x += koma.effective_width(ctx.usable_width, ctx.koma_gap) + ctx.koma_gap;
            }
            None
        });
        match new_x {
            Some(nx) if (nx - old_x).abs() >= 0.5 => {
                ScrollAction::KeepAnchor((ctx.prev_scroll_x + (nx - old_x)).max(0.0))
            }
            _ => ScrollAction::None,
        }
    } else {
        ScrollAction::None
    };

    let reveal = if reveal_focused {
        match state.sheets().get(new_active) {
            Some(sheet) => {
                if let Some(focused) = sheet.focused_koma() {
                    RevealAction::RevealKoma(focused.id.clone())
                } else if sheet.komas.is_empty() {
                    RevealAction::ResetEmpty
                } else {
                    RevealAction::None
                }
            }
            None => RevealAction::None,
        }
    } else {
        RevealAction::None
    };

    let overview = if ctx.prev_mode != InputMode::Overview && state.mode == InputMode::Overview {
        OverviewTransition::Enter {
            from_sheet: ctx.prev_active,
        }
    } else if ctx.prev_mode == InputMode::Overview && state.mode != InputMode::Overview {
        OverviewTransition::Exit
    } else if state.mode == InputMode::Overview {
        OverviewTransition::Stay
    } else {
        OverviewTransition::None
    };

    SyncPlan {
        stop_scroll_motion,
        clear_motion_and_pinch,
        reorder_slide,
        sheet_slide,
        clear_strip_drag: in_overview || ctx.prev_active != new_active,
        scroll,
        reveal,
        overview,
    }
}

impl SheetStrip {
    pub fn set_shared_web_runtime(&mut self, runtime: Rc<RefCell<WebViewRuntime>>) {
        self.shared_web_runtime = Some(runtime);
    }

    pub fn set_terminal_settings(&mut self, settings: TerminalSettings) {
        self.terminal_settings = settings;
    }

    pub fn set_terminal_shell(&mut self, shell: Option<String>) {
        self.terminal_shell = shell;
    }

    pub fn set_center_focused(&mut self, mode: CenterFocused) {
        self.center_focused = mode;
    }

    pub fn set_scale(&mut self, scale: ScaleConfig) {
        self.scale = scale;
    }

    pub fn set_koma_gap(&mut self, gap: f64) {
        self.koma_gap = gap.max(0.0);
        let viewport_w = self.effective_viewport_size().x;
        self.refresh_slot_widths(viewport_w);
    }

    pub fn sync_sheet_view(
        &mut self,
        cx: &mut Cx,
        state: &WorkspaceState,
        sock_path: &str,
        path_env: &str,
        reveal_focused: bool,
    ) {
        let prev_mode = self.mode;
        let prev_active = self.active_sheet;
        let prev_scroll_x = self.scroll_bars.get_scroll_pos().x;
        if let Some(prev_id) = self.sheets.get(prev_active).map(|d| d.sheet_id.clone()) {
            self.sheet_scroll_x.insert(prev_id, prev_scroll_x);
        }

        let plan = plan_sheet_sync(
            &SyncContext {
                old_sheets: &self.sheets,
                prev_mode,
                prev_active,
                prev_scroll_x,
                sheet_scroll_x: &self.sheet_scroll_x,
                koma_gap: self.koma_gap,
                usable_width: self.usable_width(),
                viewport_size: self.last_viewport.size,
                is_overview_rendered: self.is_overview_rendered(),
                has_sheet_slide: self.anim.sheet_slide.is_some(),
                close_gaps: &self.anim.close_gaps,
                reorder_slide: self.anim.reorder_slide.as_ref(),
            },
            state,
            reveal_focused,
        );

        self.mode = state.mode;
        self.active_sheet = state.active_sheet_index();
        if plan.stop_scroll_motion {
            self.scroll_bars.stop_motion();
            self.discard_scroll_continuation = true;
        }

        let mut trigger_slide = false;
        if plan.clear_motion_and_pinch {
            trigger_slide = plan.reorder_slide.is_some();
            self.anim.reorder_slide = plan.reorder_slide;
            self.anim.close_gaps.clear();
            self.anim.trailing_close_gap = None;
            self.end_content_pinch(cx);
        }

        match plan.sheet_slide {
            SheetSlideTransition::Start(slide) => {
                self.anim.sheet_slide = Some(slide);
                trigger_slide = true;
            }
            SheetSlideTransition::Clear => self.anim.sheet_slide = None,
            SheetSlideTransition::Keep => {}
        }

        self.sync_child_widgets(cx, state, sock_path, path_env);
        self.sheets = self.build_sheet_snapshots(state);

        match plan.scroll {
            ScrollAction::RestoreSheet(restored) => {
                self.anim.scroll_target_x = None;
                self.pending_center_koma = None;
                let _ = self
                    .scroll_bars
                    .set_scroll_pos_no_clip(cx, dvec2(restored, 0.0));
            }
            ScrollAction::KeepAnchor(target_x) => {
                self.anim.scroll_target_x = None;
                let _ = self
                    .scroll_bars
                    .set_scroll_pos_no_clip(cx, dvec2(target_x, 0.0));
            }
            ScrollAction::None => {}
        }
        if plan.clear_strip_drag {
            self.strip_drag = None;
        }

        match plan.reveal {
            RevealAction::RevealKoma(focused_id) => self.ensure_koma_visible(cx, &focused_id),
            RevealAction::ResetEmpty => {
                self.anim.scroll_target_x = None;
                let _ = self.scroll_bars.set_scroll_pos_no_clip(cx, dvec2(0.0, 0.0));
            }
            RevealAction::None => {}
        }

        self.apply_sync_animations(cx, trigger_slide, plan.overview);
        self.redraw(cx);
    }

    fn sync_child_widgets(
        &mut self,
        cx: &mut Cx,
        state: &WorkspaceState,
        sock_path: &str,
        path_env: &str,
    ) {
        let in_overview = state.mode == InputMode::Overview;
        for (sheet_idx, sheet) in state.sheets().iter().enumerate() {
            let is_active = sheet_idx == self.active_sheet;
            let is_sliding_prev = self
                .anim
                .sheet_slide
                .as_ref()
                .is_some_and(|s| s.prev_sheet_id == sheet.id);
            for koma in &sheet.komas {
                self.ensure_koma_widget(
                    cx,
                    koma,
                    sock_path,
                    path_env,
                    is_active || in_overview || is_sliding_prev,
                );
                if in_overview {
                    if koma.kind == KomaKind::Terminal {
                        self.cancel_terminal_interaction(cx, &koma.id);
                    }
                } else if !is_active && !is_sliding_prev {
                    self.deactivate_hidden_koma(cx, koma);
                } else if koma.kind == KomaKind::Web {
                    if let Some(rt) = &self.shared_web_runtime {
                        rt.borrow_mut().set_koma_hidden(cx, &koma.id, false);
                    }
                }
            }
        }

        for koma in &state.drawer {
            self.deactivate_hidden_koma(cx, koma);
        }
    }

    fn build_sheet_snapshots(&self, state: &WorkspaceState) -> Vec<SheetSnapshot> {
        let usable = self.usable_width();
        state
            .sheets()
            .iter()
            .map(|sheet| {
                let focused_id = sheet.focused_koma().map(|b| b.id.clone());
                let komas = sheet
                    .komas
                    .iter()
                    .map(|b| {
                        let detail = match b.kind {
                            KomaKind::Web => self
                                .shared_web_runtime
                                .as_ref()
                                .and_then(|rt| rt.borrow().koma_url_title(&b.id).map(|(u, _)| u))
                                .filter(|u| !u.is_empty())
                                .or_else(|| b.initial_url.clone())
                                .unwrap_or_default(),
                            KomaKind::Terminal => b.cwd.clone().unwrap_or_default(),
                        };
                        KomaSlot {
                            koma_id: b.id.clone(),
                            kind: b.kind,
                            label: b.label.clone(),
                            detail,
                            raw_width: b.width,
                            ratio: b.ratio,
                            width: b.effective_width(usable, self.koma_gap),
                            focused: focused_id.as_ref() == Some(&b.id),
                        }
                    })
                    .collect();
                SheetSnapshot {
                    sheet_id: sheet.id.clone(),
                    label: sheet.label.clone(),
                    focused_koma_id: focused_id,
                    komas,
                }
            })
            .collect()
    }

    fn apply_sync_animations(
        &mut self,
        cx: &mut Cx,
        trigger_slide: bool,
        overview: OverviewTransition,
    ) {
        if trigger_slide {
            self.start_animation(cx);
        }

        let vp = self.effective_viewport_size();
        match overview {
            OverviewTransition::Enter { from_sheet } => {
                let scroll_x = self.scroll_bars.get_scroll_pos().x;
                self.overview.cam_scale = 1.0;
                self.overview.cam_x = scroll_x + vp.x * 0.5;
                self.overview.cam_y = Self::sheet_y_top(from_sheet, vp.y) + vp.y * 0.5;
                self.overview.cam_pan_offset = dvec2(0.0, 0.0);
                self.overview.t = self.overview.t.max(0.005);
                self.start_animation(cx);
            }
            OverviewTransition::Exit => {
                self.overview.clear_interaction();
                if self.overview.t > OVERVIEW_RENDER_EPS {
                    self.start_animation(cx);
                }
            }
            OverviewTransition::Stay => {
                self.overview.cam_pan_offset = dvec2(0.0, 0.0);
                self.start_animation(cx);
            }
            OverviewTransition::None => {}
        }
    }

    fn ensure_koma_widget(
        &mut self,
        cx: &mut Cx,
        koma: &KomaMeta,
        sock_path: &str,
        path_env: &str,
        should_paint_now: bool,
    ) {
        if self.koma_widgets.contains_key(&koma.id) {
            return;
        }
        match koma.kind {
            KomaKind::Terminal => {
                let template = self.term_template.clone();
                let widget = cx.with_vm(|vm| WidgetRef::script_from_value(vm, template.into()));
                if let Some(mut term) = widget.borrow_mut::<MpTerm>() {
                    let settings = &self.terminal_settings;
                    let (height_scale, height_pixels) = match settings.cell_height {
                        Some(CellHeight::Pixels(px)) => (1.0, f64::from(px)),
                        Some(CellHeight::Percent(percent)) => {
                            (1.0 + f64::from(percent) / 100.0, 0.0)
                        }
                        None => (1.0, 0.0),
                    };
                    term.apply_system_font_families(
                        cx,
                        &settings.font_family,
                        &settings.font_family_bold,
                        settings.bold_weight.unwrap_or(700.0),
                    );
                    term.apply_font_overrides(
                        settings.font_size,
                        settings.bold_weight,
                        height_scale,
                        height_pixels,
                    );
                    let rgb = |[r, g, b]: [u8; 3]| Rgb::new(r, g, b);
                    term.set_color_overrides(TerminalColors {
                        foreground: settings.foreground.map(rgb),
                        background: settings.background.map(rgb),
                        cursor: settings.cursor_color.map(rgb),
                        palette: settings
                            .palette
                            .iter()
                            .map(|(&i, &c)| (i, rgb(c)))
                            .collect(),
                        background_opacity: settings.background_opacity,
                    });
                    term.cwd = koma
                        .cwd
                        .as_deref()
                        .map(PathBuf::from)
                        .or_else(koma_ipc::workspaces::home_dir);
                    term.shell = self.terminal_shell.clone();
                    term.command = koma.command.clone();
                    term.copy_on_select = settings.copy_on_select;
                    term.ligatures = settings.ligatures;
                    term.env = vec![
                        ("KOMA_KOMA_ID".into(), koma.id.to_string()),
                        ("KOMA_PROFILE".into(), koma.profile_id.clone()),
                        ("KOMA_SOCKET".into(), sock_path.to_string()),
                        ("PATH".into(), path_env.to_string()),
                    ];
                    term.set_presentation_font_scale(cx, self.scale.terminal);
                    term.ensure_started(cx);
                }
                self.koma_widgets
                    .insert(koma.id.clone(), KomaWidget::Terminal(widget));
            }
            KomaKind::Web => {
                let template = self.web_template.clone();
                let widget = cx.with_vm(|vm| WidgetRef::script_from_value(vm, template.into()));
                let url = koma
                    .initial_url
                    .as_deref()
                    .unwrap_or("https://example.com")
                    .to_string();
                if let Some(rt) = &self.shared_web_runtime {
                    if let Some(mut web) = widget.borrow_mut::<WebKoma>() {
                        web.bind_shared(
                            Rc::clone(rt),
                            koma.id.clone(),
                            url.clone(),
                            self.scale.web,
                        );
                    }
                    if !should_paint_now {
                        rt.borrow_mut().ensure_koma(
                            cx,
                            &koma.id,
                            &url,
                            false,
                            dvec2(koma.width, 680.0),
                            2.0,
                        );
                    }
                }
                self.koma_widgets
                    .insert(koma.id.clone(), KomaWidget::Web(widget));
            }
        }
    }

    fn cancel_terminal_interaction(&self, cx: &mut Cx, koma_id: &KomaId) {
        if let Some(mut term) = self
            .koma_widgets
            .get(koma_id)
            .and_then(KomaWidget::terminal_mut)
        {
            term.cancel_gestures(cx);
            if term.has_input_focus(cx) {
                cx.set_key_focus(Area::Empty);
            }
        }
    }

    pub(super) fn hide_sheet_webviews(&self, cx: &mut Cx, sheet_id: &SheetId) {
        let Some(rt) = &self.shared_web_runtime else {
            return;
        };
        let Some(sheet) = self.sheets.iter().find(|d| &d.sheet_id == sheet_id) else {
            return;
        };
        let mut rt = rt.borrow_mut();
        for slot in &sheet.komas {
            if slot.kind == KomaKind::Web {
                rt.set_koma_hidden(cx, &slot.koma_id, true);
            }
        }
    }

    fn deactivate_hidden_koma(&mut self, cx: &mut Cx, koma: &KomaMeta) {
        match koma.kind {
            KomaKind::Terminal => self.cancel_terminal_interaction(cx, &koma.id),
            KomaKind::Web => {
                if let Some(rt) = &self.shared_web_runtime {
                    rt.borrow_mut().set_koma_hidden(cx, &koma.id, true);
                }
            }
        }
    }

    pub fn close_koma(&mut self, cx: &mut Cx, koma_id: &KomaId, kind: KomaKind) {
        self.animate_koma_close(cx, koma_id);
        self.remove_koma(cx, koma_id, kind);
    }

    pub fn remove_koma(&mut self, cx: &mut Cx, koma_id: &KomaId, kind: KomaKind) {
        self.anim.remove_koma(koma_id);
        for sheet in &mut self.sheets {
            sheet.komas.retain(|s| &s.koma_id != koma_id);
        }
        self.card_rects.retain(|(id, _)| id != koma_id);
        if self
            .content_pinch
            .as_ref()
            .is_some_and(|pinch| pinch.id() == koma_id)
        {
            self.end_content_pinch(cx);
        }
        self.overview.remove_koma(koma_id);
        if self.pending_reveal_koma.as_ref() == Some(koma_id) {
            self.pending_reveal_koma = None;
        }
        if let Some(widget) = self.koma_widgets.remove(koma_id) {
            if let Some(mut term) = widget.terminal_mut() {
                term.unload(cx);
            }
        }
        if kind == KomaKind::Web {
            if let Some(rt) = &self.shared_web_runtime {
                rt.borrow_mut().close_koma(cx, koma_id);
            }
        }
        self.redraw(cx);
    }

    pub fn focus_koma(&mut self, cx: &mut Cx, koma_id: Option<&KomaId>) -> bool {
        self.blur_except(cx, koma_id);
        let Some(id) = koma_id else {
            return true;
        };
        let Some(widget) = self.koma_widgets.get(id) else {
            return false;
        };
        match widget {
            KomaWidget::Terminal(_) => widget.terminal_mut().is_none_or(|mut term| {
                let ready = term.focus_area().is_valid(cx);
                term.take_key_focus(cx);
                ready
            }),
            KomaWidget::Web(_) => widget.web_mut().is_none_or(|mut web| web.focus(cx)),
        }
    }

    pub fn blur_all(&mut self, cx: &mut Cx) {
        self.blur_except(cx, None);
    }

    fn blur_except(&mut self, cx: &mut Cx, keep_id: Option<&KomaId>) {
        for (id, widget) in &self.koma_widgets {
            if keep_id == Some(id) {
                continue;
            }
            match widget {
                KomaWidget::Terminal(_) => {
                    if let Some(term) = widget.terminal() {
                        if term.has_input_focus(cx) {
                            cx.set_key_focus(Area::Empty);
                        }
                    }
                }
                KomaWidget::Web(_) => {
                    if let Some(mut web) = widget.web_mut() {
                        if web.has_input_focus(cx) {
                            web.blur(cx);
                        }
                    }
                }
            }
        }
        if keep_id.is_none() {
            if let Some(rt) = &self.shared_web_runtime {
                rt.borrow_mut().focus_koma(cx, None);
            }
        }
    }

    pub fn focused_widget_koma_id(&self, cx: &Cx) -> Option<KomaId> {
        let sheet = self.sheets.get(self.active_sheet)?;
        for slot in &sheet.komas {
            if self
                .koma_widgets
                .get(&slot.koma_id)
                .is_some_and(|widget| widget.has_input_focus(cx))
            {
                return Some(slot.koma_id.clone());
            }
        }
        None
    }

    pub fn koma_id_for_widget_uid(&self, uid: WidgetUid) -> Option<KomaId> {
        self.koma_widgets
            .iter()
            .find(|(_, w)| w.as_widget().widget_uid() == uid)
            .map(|(id, _)| id.clone())
    }

    pub fn terminal_rows(
        &self,
        koma_id: &KomaId,
        fetch_lines: Option<usize>,
    ) -> Option<Vec<String>> {
        let term = self.koma_widgets.get(koma_id)?.terminal()?;
        let (rows, _, _) = term.ai_screen_rows(fetch_lines)?;
        Some(rows)
    }

    pub fn send_terminal_text(&mut self, koma_id: &KomaId, text: &str) -> bool {
        self.koma_widgets
            .get(koma_id)
            .and_then(KomaWidget::terminal_mut)
            .is_some_and(|mut term| term.ai_type_bytes(text.as_bytes()))
    }

    pub fn with_terminal<T>(
        &self,
        koma_id: &KomaId,
        operation: impl FnOnce(&mut MpTerm) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut term = self
            .koma_widgets
            .get(koma_id)
            .and_then(KomaWidget::terminal_mut)
            .ok_or_else(|| format!("Terminal not ready: {koma_id}"))?;
        operation(&mut term)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::ui::sheet_strip::STRIP_SPACING;

    fn snapshots_from(state: &WorkspaceState, usable: f64, gap: f64) -> Vec<SheetSnapshot> {
        state
            .sheets()
            .iter()
            .map(|sheet| {
                let focused_id = sheet.focused_koma().map(|k| k.id.clone());
                let komas = sheet
                    .komas
                    .iter()
                    .map(|k| KomaSlot {
                        koma_id: k.id.clone(),
                        kind: k.kind,
                        label: k.label.clone(),
                        detail: String::new(),
                        raw_width: k.width,
                        ratio: k.ratio,
                        width: k.effective_width(usable, gap),
                        focused: focused_id.as_ref() == Some(&k.id),
                    })
                    .collect();
                SheetSnapshot {
                    sheet_id: sheet.id.clone(),
                    label: sheet.label.clone(),
                    focused_koma_id: focused_id,
                    komas,
                }
            })
            .collect()
    }

    #[test]
    fn switching_sheets_plans_slide_scroll_restore_and_reveal() {
        let mut state = WorkspaceState::new();
        state.add_sheet(true);
        let usable = 1200.0;
        let old_sheets = snapshots_from(&state, usable, STRIP_SPACING);
        let sheet0_id = old_sheets[0].sheet_id.clone();
        let sheet1_id = old_sheets[1].sheet_id.clone();
        let sheet1_focus = state.sheets()[1].focused_koma().unwrap().id.clone();

        state.switch_sheet(1);
        let mut scroll_map = HashMap::new();
        scroll_map.insert(sheet0_id.clone(), 120.0);
        scroll_map.insert(sheet1_id, 240.0);

        let close_gaps = HashMap::new();
        let plan = plan_sheet_sync(
            &SyncContext {
                old_sheets: &old_sheets,
                prev_mode: InputMode::ContentInput,
                prev_active: 0,
                prev_scroll_x: 120.0,
                sheet_scroll_x: &scroll_map,
                koma_gap: STRIP_SPACING,
                usable_width: usable,
                viewport_size: dvec2(1264.0, 680.0),
                is_overview_rendered: false,
                has_sheet_slide: false,
                close_gaps: &close_gaps,
                reorder_slide: None,
            },
            &state,
            true,
        );

        assert!(plan.stop_scroll_motion);
        assert!(plan.clear_strip_drag);
        assert_eq!(plan.scroll, ScrollAction::RestoreSheet(240.0));
        assert_eq!(plan.reveal, RevealAction::RevealKoma(sheet1_focus));
        assert_eq!(
            plan.sheet_slide,
            SheetSlideTransition::Start(SheetSlide {
                prev_sheet_id: sheet0_id,
                prev_scroll_x: 120.0,
                offset_y: 680.0,
                direction: 1.0,
            })
        );
        assert_eq!(plan.overview, OverviewTransition::None);
    }

    #[test]
    fn inserting_left_of_focus_without_reveal_keeps_scroll_anchor() {
        let mut state = WorkspaceState::new();
        let first = state
            .insert_koma_after_focused(KomaKind::Terminal, "first".into(), None, None)
            .id;
        let second = state
            .insert_koma_after_focused(KomaKind::Terminal, "second".into(), None, None)
            .id;
        state.focus_koma_by_id(&second);

        let usable = 1200.0;
        let old_sheets = snapshots_from(&state, usable, STRIP_SPACING);

        // Insert a koma after `first` (to the left of focused `second`) while keeping focus on `second`.
        let (inserted, _) = state.insert_koma_after(
            KomaKind::Terminal,
            "inserted".into(),
            None,
            None,
            None,
            Some(&first),
        );
        state.focus_koma_by_id(&second);
        let inserted_w = inserted.effective_width(usable, STRIP_SPACING);

        let scroll_map = HashMap::new();
        let close_gaps = HashMap::new();
        let plan = plan_sheet_sync(
            &SyncContext {
                old_sheets: &old_sheets,
                prev_mode: InputMode::ContentInput,
                prev_active: 0,
                prev_scroll_x: 100.0,
                sheet_scroll_x: &scroll_map,
                koma_gap: STRIP_SPACING,
                usable_width: usable,
                viewport_size: dvec2(1264.0, 680.0),
                is_overview_rendered: false,
                has_sheet_slide: false,
                close_gaps: &close_gaps,
                reorder_slide: None,
            },
            &state,
            false,
        );

        assert!(!plan.stop_scroll_motion);
        assert!(plan.clear_motion_and_pinch);
        assert!(plan.reorder_slide.is_none());
        assert_eq!(
            plan.scroll,
            ScrollAction::KeepAnchor(100.0 + inserted_w + STRIP_SPACING)
        );
        assert_eq!(plan.reveal, RevealAction::None);
    }

    #[test]
    fn reordering_same_sheet_with_reveal_plans_reorder_slide() {
        let mut state = WorkspaceState::new();
        state.insert_koma_after_focused(KomaKind::Terminal, "first".into(), None, None);
        let second = state
            .insert_koma_after_focused(KomaKind::Terminal, "second".into(), None, None)
            .id;
        state.focus_koma_by_id(&second);

        let usable = 1200.0;
        let old_sheets = snapshots_from(&state, usable, STRIP_SPACING);
        assert!(state.move_focused_koma(-1));

        let scroll_map = HashMap::new();
        let close_gaps = HashMap::new();
        let plan = plan_sheet_sync(
            &SyncContext {
                old_sheets: &old_sheets,
                prev_mode: InputMode::KomaMode,
                prev_active: 0,
                prev_scroll_x: 0.0,
                sheet_scroll_x: &scroll_map,
                koma_gap: STRIP_SPACING,
                usable_width: usable,
                viewport_size: dvec2(1264.0, 680.0),
                is_overview_rendered: false,
                has_sheet_slide: false,
                close_gaps: &close_gaps,
                reorder_slide: None,
            },
            &state,
            true,
        );

        assert!(plan.clear_motion_and_pinch);
        assert!(plan.reorder_slide.is_some());
        assert_eq!(plan.sheet_slide, SheetSlideTransition::Keep);
        assert_eq!(plan.reveal, RevealAction::RevealKoma(second));
    }

    #[test]
    fn overview_transitions_and_empty_sheet_reset() {
        let mut state = WorkspaceState::new();
        state.add_sheet(false); // empty sheet at index 1
        state.switch_sheet(1);
        let usable = 1200.0;
        let old_sheets = snapshots_from(&state, usable, STRIP_SPACING);
        let scroll_map = HashMap::new();
        let close_gaps = HashMap::new();

        // Enter overview
        state.mode = InputMode::Overview;
        let enter = plan_sheet_sync(
            &SyncContext {
                old_sheets: &old_sheets,
                prev_mode: InputMode::ContentInput,
                prev_active: 1,
                prev_scroll_x: 0.0,
                sheet_scroll_x: &scroll_map,
                koma_gap: STRIP_SPACING,
                usable_width: usable,
                viewport_size: dvec2(1264.0, 680.0),
                is_overview_rendered: false,
                has_sheet_slide: true,
                close_gaps: &close_gaps,
                reorder_slide: None,
            },
            &state,
            true,
        );
        assert_eq!(enter.overview, OverviewTransition::Enter { from_sheet: 1 });
        assert_eq!(enter.sheet_slide, SheetSlideTransition::Clear);
        assert!(enter.clear_strip_drag);
        assert_eq!(enter.reveal, RevealAction::ResetEmpty);

        // Exit overview
        state.mode = InputMode::ContentInput;
        let exit = plan_sheet_sync(
            &SyncContext {
                old_sheets: &old_sheets,
                prev_mode: InputMode::Overview,
                prev_active: 1,
                prev_scroll_x: 0.0,
                sheet_scroll_x: &scroll_map,
                koma_gap: STRIP_SPACING,
                usable_width: usable,
                viewport_size: dvec2(1264.0, 680.0),
                is_overview_rendered: true,
                has_sheet_slide: false,
                close_gaps: &close_gaps,
                reorder_slide: None,
            },
            &state,
            false,
        );
        assert_eq!(exit.overview, OverviewTransition::Exit);
        assert_eq!(exit.reveal, RevealAction::None);
    }
}
