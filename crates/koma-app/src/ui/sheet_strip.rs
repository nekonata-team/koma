//! Horizontally scrollable strip of Komas that zooms out GPU-side into a Niri-like live-canvas Overview.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use koma_app::config::CenterFocused;
use koma_app::ghostty_config::{CellHeight, TerminalSettings};
use koma_core::model::{
    InputMode, KomaId, KomaKind, KomaMeta, SheetId, WorkspaceState, MIN_PROPORTIONAL_WIDTH,
};
use makepad_app_terminal::term::color::Rgb;
use makepad_app_terminal::widget::{MpTerm, TerminalColors};
use makepad_browser_view::ViewOp;
use makepad_widgets::event::ScrollPhase;
use makepad_widgets::makepad_draw::DrawList2d;
use makepad_widgets::turtle::RowAlign;
use makepad_widgets::*;

use crate::runtime::web_views::WebViewRuntime;
use crate::ui::web_koma::WebKoma;

mod overview;

pub const STRIP_SPACING: f64 = 10.0;
const HEADER_HEIGHT: f64 = 22.0;
const DEFAULT_OVERVIEW_SCALE: f64 = 0.44;
const OVERVIEW_RENDER_EPS: f64 = 0.002;
const DRAG_THRESHOLD_PX: f64 = 8.0;
const KOMA_CLOSE_SECONDS: f64 = 0.17;
const KOMA_REORDER_SECONDS: f64 = 0.17;

enum PinchTarget {
    Terminal(ContentPinch),
    Web(KomaId),
}

impl PinchTarget {
    fn id(&self) -> &KomaId {
        match self {
            Self::Terminal(pinch) => &pinch.target,
            Self::Web(id) => id,
        }
    }
}

struct ContentPinch {
    target: KomaId,
    // Work in tenths of the base scale and keep fractional input between events.
    desired: f64,
    shown: f64,
}

impl ContentPinch {
    fn new(target: KomaId, scale: f64) -> Self {
        let tenths = (scale * 10.0).round();
        Self {
            target,
            desired: tenths,
            shown: tenths,
        }
    }

    fn update(&mut self, scale: f64) -> Option<f64> {
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        self.desired = (self.desired * scale).clamp(5.0, 20.0);
        let delta = self.desired - self.shown;
        // Require a full step in either direction before reflowing the content.
        let steps = (delta.abs() + 1e-9).floor().copysign(delta);
        if steps == 0.0 {
            return None;
        }
        self.shown += steps;
        Some(self.shown / 10.0)
    }
}

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.SheetStripBase = #(SheetStrip::register_widget(vm))
    mod.widgets.SheetStrip = set_type_default() do mod.widgets.SheetStripBase{
        width: Fill
        height: Fill
        scroll_bars: ScrollBars{
            show_scroll_x: true
            show_scroll_y: false
        }
        term_template: MpTerm{
            focus_on_first_draw: false
        }
        web_template: WebKoma{}
        draw_strip_bg +: {
            color: #x0f111a
        }
        draw_overview_bg +: {
            color: #x080a10
        }
        draw_sheet_bg +: {
            color: #x121622
        }
        draw_border +: {
            color: #x293045
        }
        draw_header +: {
            color: #x161a26
        }
        draw_drop_bar +: {
            color: #x38bdf8
        }
        draw_title +: {
            color: #xe2e8f0
            text_style: theme.font_regular{
                font_size: 11
            }
        }
        draw_sheet_label +: {
            color: #x94a3b8
            text_style: theme.font_regular{
                font_size: 20
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub enum SheetStripAction {
    #[default]
    None,
    FocusKoma {
        koma_id: KomaId,
        enter_content_input: bool,
    },
    EnterOverviewTarget {
        sheet_idx: usize,
        koma_id: Option<KomaId>,
    },
    ExitOverview,
    MoveKoma {
        koma_id: KomaId,
        target_sheet: usize,
        target_index: usize,
    },
    OverviewSettled,
}

#[derive(Clone, Debug)]
struct KomaSlot {
    koma_id: KomaId,
    kind: KomaKind,
    label: String,
    detail: String,
    raw_width: f64,
    ratio: Option<f64>,
    width: f64,
    focused: bool,
}

#[derive(Clone, Debug)]
struct StripDrag {
    koma_id: KomaId,
    start_abs: Vec2d,
    active: bool,
    drop_index: Option<usize>,
}

#[derive(Clone, Debug)]
struct SheetSnapshot {
    sheet_id: SheetId,
    label: String,
    focused_koma_id: Option<KomaId>,
    komas: Vec<KomaSlot>,
}

#[derive(Clone, Debug)]
struct OverviewCardHit {
    sheet_idx: usize,
    koma_id: KomaId,
    world_rect: Rect,
}

#[derive(Clone, Debug)]
struct OverviewDrag {
    koma_id: KomaId,
    origin_sheet: usize,
    start_abs: Vec2d,
    active: bool,
    drop_target: Option<(usize, usize)>,
}

#[derive(Clone, Copy, Debug)]
enum BgKind {
    Sheet(usize),
    Outer,
}

#[derive(Clone, Copy, Debug)]
struct BgPanDrag {
    kind: BgKind,
    start_abs: Vec2d,
    last_abs: Vec2d,
    moved: bool,
}

struct SheetSlide {
    prev_sheet_id: SheetId,
    prev_scroll_x: f64,
    offset_y: f64,
    direction: f64,
}

struct CloseGap {
    distance: f64,
    elapsed: Option<f64>,
}

impl CloseGap {
    fn remaining(&self) -> f64 {
        let t = (1.0 - self.elapsed.unwrap_or(0.0) / KOMA_CLOSE_SECONDS).max(0.0);
        t * t * (3.0 - 2.0 * t)
    }

    fn width(&self) -> f64 {
        self.distance * self.remaining()
    }

    fn advance(&mut self, dt: f64) -> bool {
        self.elapsed = Some(self.elapsed.map_or(0.0, |elapsed| elapsed + dt));
        self.width() >= 0.5
    }

    fn after_removal(
        gaps: &mut HashMap<KomaId, Self>,
        closed: &KomaSlot,
        koma_gap: f64,
        existing: f64,
    ) -> Self {
        let carried = gaps.remove(&closed.koma_id).map_or(0.0, |gap| gap.width());
        Self {
            distance: carried + closed.width + koma_gap + existing,
            elapsed: None,
        }
    }

    fn transfer(
        gaps: &mut HashMap<KomaId, Self>,
        closed: &KomaSlot,
        next: &KomaSlot,
        koma_gap: f64,
    ) {
        let existing = gaps.get(&next.koma_id).map_or(0.0, Self::width);
        let gap = Self::after_removal(gaps, closed, koma_gap, existing);
        gaps.insert(next.koma_id.clone(), gap);
    }
}

fn koma_start_x(komas: &[KomaSlot], koma_id: &KomaId, koma_gap: f64) -> Option<f64> {
    let mut x = 0.0;
    for slot in komas {
        if &slot.koma_id == koma_id {
            return Some(x);
        }
        x += slot.width + koma_gap;
    }
    None
}

struct ReorderSlide {
    offsets: HashMap<KomaId, f64>,
    elapsed: Option<f64>,
}

impl ReorderSlide {
    fn offset(&self, id: &KomaId) -> f64 {
        let t = (1.0 - self.elapsed.unwrap_or(0.0) / KOMA_REORDER_SECONDS).max(0.0);
        self.offsets.get(id).copied().unwrap_or(0.0) * t * t * (3.0 - 2.0 * t)
    }

    fn between<'a>(
        old: &[KomaSlot],
        new_ids: impl ExactSizeIterator<Item = &'a KomaId>,
        gap: f64,
        close_gaps: &HashMap<KomaId, CloseGap>,
        previous: Option<&Self>,
    ) -> Option<Self> {
        if old.len() != new_ids.len() {
            return None;
        }
        let mut positions = HashMap::new();
        let mut x = 0.0;
        for slot in old {
            x += close_gaps.get(&slot.koma_id).map_or(0.0, CloseGap::width);
            let carried = previous.map_or(0.0, |slide| slide.offset(&slot.koma_id));
            positions.insert(&slot.koma_id, (x + carried, slot.width));
            x += slot.width + gap;
        }
        let mut offsets = HashMap::new();
        let mut x = 0.0;
        for id in new_ids {
            let (old_x, width) = positions.remove(id)?;
            if (old_x - x).abs() >= 0.5 {
                offsets.insert(id.to_owned(), old_x - x);
            }
            x += width + gap;
        }
        (!offsets.is_empty()).then_some(Self {
            offsets,
            elapsed: None,
        })
    }

    fn advance(&mut self, dt: f64) -> bool {
        let elapsed = self.elapsed.map_or(0.0, |elapsed| elapsed + dt);
        self.elapsed = Some(elapsed);
        elapsed < KOMA_REORDER_SECONDS
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct SheetStrip {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[walk]
    walk: Walk,
    #[redraw]
    #[live]
    draw_strip_bg: DrawColor,
    #[live]
    scroll_bars: ScrollBars,
    #[live]
    draw_overview_bg: DrawColor,
    #[live]
    draw_sheet_bg: DrawColor,
    #[live]
    draw_border: DrawColor,
    #[live]
    draw_header: DrawColor,
    #[live]
    draw_drop_bar: DrawColor,
    #[live]
    draw_title: DrawText,
    #[live]
    draw_sheet_label: DrawText,
    #[live]
    term_template: ScriptObjectRef,
    #[live]
    web_template: ScriptObjectRef,
    #[rust]
    shared_web_runtime: Option<Rc<RefCell<WebViewRuntime>>>,
    #[rust]
    terminal_settings: TerminalSettings,
    #[rust]
    terminal_shell: Option<String>,
    #[rust]
    koma_widgets: HashMap<KomaId, WidgetRef>,
    #[rust]
    sheets: Vec<SheetSnapshot>,
    #[rust]
    active_sheet: usize,
    #[rust]
    sheet_scroll_x: HashMap<SheetId, f64>,
    #[rust]
    card_rects: Vec<(KomaId, Rect)>,
    #[rust]
    content_pinch: Option<PinchTarget>,
    #[rust(InputMode::ContentInput)]
    mode: InputMode,
    #[rust]
    pending_reveal_koma: Option<KomaId>,
    #[rust]
    pending_center_koma: Option<KomaId>,
    #[rust]
    scroll_target_x: Option<f64>,
    #[rust]
    discard_scroll_continuation: bool,
    #[rust]
    center_focused: CenterFocused,
    #[rust(STRIP_SPACING)]
    koma_gap: f64,
    #[rust]
    strip_drag: Option<StripDrag>,
    #[rust]
    sheet_slide: Option<SheetSlide>,
    #[rust]
    close_gaps: HashMap<KomaId, CloseGap>,
    #[rust]
    trailing_close_gap: Option<CloseGap>,
    #[rust]
    reorder_slide: Option<ReorderSlide>,

    #[rust]
    overview_draw_list: Option<DrawList2d>,
    #[rust]
    overview_t: f64,
    #[rust(DEFAULT_OVERVIEW_SCALE)]
    overview_scale: f64,
    #[rust(1.0)]
    cam_scale: f64,
    #[rust]
    cam_x: f64,
    #[rust]
    cam_y: f64,
    #[rust]
    cam_pan_offset: Vec2d,
    #[rust]
    last_viewport: Rect,
    #[rust((1.0, 0.0, 0.0))]
    last_transform: (f64, f64, f64),
    #[rust]
    animating: bool,
    #[rust]
    last_frame_time: Option<f64>,
    #[rust]
    next_frame: NextFrame,
    #[rust]
    overview_card_rects: Vec<OverviewCardHit>,
    #[rust]
    overview_sheet_rects: Vec<(usize, Rect)>,
    #[rust]
    hovered_koma: Option<KomaId>,
    #[rust]
    overview_drag: Option<OverviewDrag>,
    #[rust]
    bg_pan_drag: Option<BgPanDrag>,
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
        let prev_sheet_id = self.sheets.get(prev_active).map(|d| d.sheet_id.clone());
        if let Some(prev_id) = &prev_sheet_id {
            self.sheet_scroll_x.insert(prev_id.clone(), prev_scroll_x);
        }

        self.mode = state.mode;
        self.active_sheet = state.active_sheet_index();
        let in_overview = state.mode == InputMode::Overview;
        let same_sheet = self
            .sheets
            .get(prev_active)
            .zip(state.sheets().get(self.active_sheet))
            .filter(|(old, new)| old.sheet_id == new.id);
        if prev_sheet_id.is_some() && same_sheet.is_none() {
            self.scroll_bars.stop_motion();
            self.discard_scroll_continuation = true;
        }
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
                    koma_start_x(&old.komas, fid, self.koma_gap).map(|old_x| (fid.clone(), old_x))
                })?
            })
        } else {
            None
        };
        let mut trigger_slide = false;
        if in_overview || !same_order {
            self.reorder_slide = same_sheet
                .filter(|_| {
                    !in_overview
                        && !self.is_overview_rendered()
                        && self.sheet_slide.is_none()
                        && self.last_viewport.size.x > 1.0
                        && old_focused_anchor.is_none()
                })
                .and_then(|(old, new)| {
                    ReorderSlide::between(
                        &old.komas,
                        new.komas.iter().map(|koma| &koma.id),
                        self.koma_gap,
                        &self.close_gaps,
                        self.reorder_slide.as_ref(),
                    )
                });
            trigger_slide = self.reorder_slide.is_some();
            self.close_gaps.clear();
            self.trailing_close_gap = None;
        }

        if in_overview || !same_order {
            self.end_content_pinch(cx);
        }

        if prev_active != self.active_sheet && !in_overview && self.last_viewport.size.y > 1.0 {
            if let Some(prev_id) = prev_sheet_id {
                let prev_exists = state.sheets().iter().any(|d| d.id == prev_id);
                let new_differs = state
                    .sheets()
                    .get(self.active_sheet)
                    .is_some_and(|d| d.id != prev_id);
                if prev_exists && new_differs {
                    let direction = if self.active_sheet > prev_active {
                        1.0
                    } else {
                        -1.0
                    };
                    self.sheet_slide = Some(SheetSlide {
                        prev_sheet_id: prev_id,
                        prev_scroll_x,
                        offset_y: direction * self.last_viewport.size.y,
                        direction,
                    });
                    trigger_slide = true;
                } else {
                    self.sheet_slide = None;
                }
            }
        } else if in_overview {
            self.sheet_slide = None;
        }

        for (sheet_idx, sheet) in state.sheets().iter().enumerate() {
            let is_active = sheet_idx == self.active_sheet;
            let is_sliding_prev = self
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

        let usable = self.usable_width();
        self.sheets = state
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
            .collect();

        if prev_active != self.active_sheet {
            self.scroll_target_x = None;
            self.pending_center_koma = None;
            self.strip_drag = None;
            let restored = self
                .sheets
                .get(self.active_sheet)
                .and_then(|d| self.sheet_scroll_x.get(&d.sheet_id).copied())
                .unwrap_or(0.0);
            let _ = self
                .scroll_bars
                .set_scroll_pos_no_clip(cx, dvec2(restored, 0.0));
        } else if let Some((fid, old_x)) = old_focused_anchor {
            if let Some(new_x) = self
                .sheets
                .get(self.active_sheet)
                .and_then(|sheet| koma_start_x(&sheet.komas, &fid, self.koma_gap))
            {
                let delta = new_x - old_x;
                if delta.abs() >= 0.5 {
                    self.scroll_target_x = None;
                    let _ = self
                        .scroll_bars
                        .set_scroll_pos_no_clip(cx, dvec2((prev_scroll_x + delta).max(0.0), 0.0));
                }
            }
        }
        if in_overview {
            self.strip_drag = None;
        }

        if let Some(sheet) = self.sheets.get(self.active_sheet) {
            if reveal_focused {
                if let Some(focused_id) = sheet.focused_koma_id.clone() {
                    self.ensure_koma_visible(cx, &focused_id);
                } else if sheet.komas.is_empty() {
                    self.scroll_target_x = None;
                    let _ = self.scroll_bars.set_scroll_pos_no_clip(cx, dvec2(0.0, 0.0));
                }
            }
        }

        if trigger_slide {
            self.start_animation(cx);
        }

        let vp = self.effective_viewport_size();
        if prev_mode != InputMode::Overview && state.mode == InputMode::Overview {
            let scroll_x = self.scroll_bars.get_scroll_pos().x;
            self.cam_scale = 1.0;
            self.cam_x = scroll_x + vp.x * 0.5;
            self.cam_y = self.sheet_y_top(prev_active, vp.y) + vp.y * 0.5;
            self.cam_pan_offset = dvec2(0.0, 0.0);
            self.overview_t = self.overview_t.max(0.005);
            self.start_animation(cx);
        } else if prev_mode == InputMode::Overview && state.mode != InputMode::Overview {
            self.hovered_koma = None;
            self.overview_drag = None;
            self.bg_pan_drag = None;
            self.cam_pan_offset = dvec2(0.0, 0.0);
            if self.overview_t > OVERVIEW_RENDER_EPS {
                self.start_animation(cx);
            }
        } else if state.mode == InputMode::Overview {
            self.cam_pan_offset = dvec2(0.0, 0.0);
            self.start_animation(cx);
        }

        self.redraw(cx);
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
                    term.cwd = koma.cwd.as_deref().map(PathBuf::from);
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
                    term.ensure_started(cx);
                }
                self.koma_widgets.insert(koma.id.clone(), widget);
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
                        web.bind_shared(Rc::clone(rt), koma.id.clone(), url.clone());
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
                self.koma_widgets.insert(koma.id.clone(), widget);
            }
        }
    }

    fn cancel_terminal_interaction(&self, cx: &mut Cx, koma_id: &KomaId) {
        if let Some(widget) = self.koma_widgets.get(koma_id) {
            if let Some(mut term) = widget.borrow_mut::<MpTerm>() {
                term.cancel_gestures(cx);
                if term.has_input_focus(cx) {
                    cx.set_key_focus(Area::Empty);
                }
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

    fn animate_koma_close(&mut self, cx: &mut Cx, koma_id: &KomaId) {
        if self.is_overview_rendered()
            || self.sheet_slide.is_some()
            || self.last_viewport.size.x <= 1.0
        {
            return;
        }
        let Some(sheet) = self.sheets.get(self.active_sheet) else {
            return;
        };
        let Some(index) = sheet.komas.iter().position(|slot| &slot.koma_id == koma_id) else {
            return;
        };
        let closed = &sheet.komas[index];
        if let Some(next) = sheet.komas.get(index + 1) {
            CloseGap::transfer(&mut self.close_gaps, closed, next, self.koma_gap);
        } else {
            let existing = self
                .trailing_close_gap
                .as_ref()
                .map_or(0.0, CloseGap::width);
            self.trailing_close_gap = Some(CloseGap::after_removal(
                &mut self.close_gaps,
                closed,
                self.koma_gap,
                existing,
            ));
            self.scroll_target_x = None;
        }
        if !self.animating {
            self.start_animation(cx);
        }
    }

    pub fn remove_koma(&mut self, cx: &mut Cx, koma_id: &KomaId, kind: KomaKind) {
        self.close_gaps.remove(koma_id);
        if let Some(slide) = &mut self.reorder_slide {
            slide.offsets.remove(koma_id);
        }
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
        self.overview_card_rects.retain(|c| &c.koma_id != koma_id);
        if self.hovered_koma.as_ref() == Some(koma_id) {
            self.hovered_koma = None;
        }
        if self.pending_reveal_koma.as_ref() == Some(koma_id) {
            self.pending_reveal_koma = None;
        }
        if let Some(widget) = self.koma_widgets.remove(koma_id) {
            if kind == KomaKind::Terminal {
                if let Some(mut term) = widget.borrow_mut::<MpTerm>() {
                    term.unload(cx);
                }
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
        if let Some(mut term) = widget.borrow_mut::<MpTerm>() {
            let ready = term.focus_area().is_valid(cx);
            term.take_key_focus(cx);
            ready
        } else if let Some(mut web) = widget.borrow_mut::<WebKoma>() {
            web.focus(cx)
        } else {
            true
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
            if let Some(term) = widget.borrow::<MpTerm>() {
                if term.has_input_focus(cx) {
                    cx.set_key_focus(Area::Empty);
                }
            } else if let Some(mut web) = widget.borrow_mut::<WebKoma>() {
                if web.has_input_focus(cx) {
                    web.blur(cx);
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
            let Some(widget) = self.koma_widgets.get(&slot.koma_id) else {
                continue;
            };
            let focused = match slot.kind {
                KomaKind::Terminal => widget
                    .borrow::<MpTerm>()
                    .is_some_and(|t| t.has_input_focus(cx)),
                KomaKind::Web => widget
                    .borrow::<WebKoma>()
                    .is_some_and(|w| w.has_input_focus(cx)),
            };
            if focused {
                return Some(slot.koma_id.clone());
            }
        }
        None
    }

    pub fn koma_id_for_widget_uid(&self, uid: WidgetUid) -> Option<KomaId> {
        self.koma_widgets
            .iter()
            .find(|(_, w)| w.widget_uid() == uid)
            .map(|(id, _)| id.clone())
    }

    pub fn terminal_rows(
        &self,
        koma_id: &KomaId,
        fetch_lines: Option<usize>,
    ) -> Option<Vec<String>> {
        let widget = self.koma_widgets.get(koma_id)?;
        let term = widget.borrow::<MpTerm>()?;
        let (rows, _, _) = term.ai_screen_rows(fetch_lines)?;
        Some(rows)
    }

    pub fn send_terminal_text(&mut self, koma_id: &KomaId, text: &str) -> bool {
        let Some(widget) = self.koma_widgets.get(koma_id) else {
            return false;
        };
        widget
            .borrow_mut::<MpTerm>()
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
            .and_then(|widget| widget.borrow_mut::<MpTerm>())
            .ok_or_else(|| format!("Terminal not ready: {koma_id}"))?;
        operation(&mut term)
    }

    pub fn adjust_content_zoom(&mut self, cx: &mut Cx, koma_id: &KomaId, step: i8) {
        self.end_content_pinch(cx);
        let Some(widget) = self.koma_widgets.get(koma_id) else {
            return;
        };
        let next = |factor: f64| {
            if step == 0 {
                1.0
            } else {
                ((factor * 10.0).round() + f64::from(step)).clamp(5.0, 20.0) / 10.0
            }
        };
        if let Some(mut term) = widget.borrow_mut::<MpTerm>() {
            let factor = next(term.cell_size().1);
            term.set_presentation_font_scale(cx, factor);
        } else if let Some(mut web) = widget.borrow_mut::<WebKoma>() {
            let factor = next(web.page_zoom());
            web.set_page_zoom(cx, factor, step == 0);
        }
    }

    fn send_web_pinch(&self, cx: &mut Cx, koma_id: &KomaId, op: ViewOp) {
        if let Some(web) = self
            .koma_widgets
            .get(koma_id)
            .and_then(|w| w.borrow::<WebKoma>())
        {
            web.send_view_op(cx, op);
        }
    }

    pub fn end_content_pinch(&mut self, cx: &mut Cx) {
        if let Some(PinchTarget::Web(id)) = self.content_pinch.take() {
            self.send_web_pinch(cx, &id, ViewOp::EndVisualPinch);
        }
    }

    fn handle_content_pinch(&mut self, cx: &mut Cx, event: &Event, phase: PinchPhase, scale: f64) {
        match phase {
            PinchPhase::Begin => {
                self.end_content_pinch(cx);
                if self.sheet_slide.is_some() {
                    return;
                }
                let Some(sheet) = self.sheets.get(self.active_sheet) else {
                    return;
                };
                self.content_pinch = sheet.komas.iter().rev().find_map(|slot| {
                    let widget = self.koma_widgets.get(&slot.koma_id)?;
                    let area = widget.area();
                    let Hit::FingerPinch(hit) = event.hits(cx, area) else {
                        return None;
                    };
                    let id = slot.koma_id.clone();
                    match slot.kind {
                        KomaKind::Terminal => {
                            let factor = widget.borrow::<MpTerm>()?.cell_size().1;
                            Some(PinchTarget::Terminal(ContentPinch::new(id, factor)))
                        }
                        KomaKind::Web => {
                            let local = hit.abs - area.rect(cx).pos;
                            let sent = widget.borrow::<WebKoma>()?.send_view_op(
                                cx,
                                ViewOp::BeginVisualPinch {
                                    x: local.x,
                                    y: local.y,
                                },
                            );
                            sent.then_some(PinchTarget::Web(id))
                        }
                    }
                });
            }
            PinchPhase::Update => match &mut self.content_pinch {
                Some(PinchTarget::Terminal(pinch)) => {
                    if let Some(factor) = pinch.update(scale) {
                        let id = pinch.target.clone();
                        let _ = self.with_terminal(&id, |term| {
                            term.set_presentation_font_scale(cx, factor);
                            Ok(())
                        });
                    }
                }
                Some(PinchTarget::Web(id)) => {
                    let id = id.clone();
                    self.send_web_pinch(cx, &id, ViewOp::UpdateVisualPinch { scale });
                }
                None => {}
            },
            PinchPhase::End => self.end_content_pinch(cx),
        }
    }

    pub fn usable_width(&self) -> f64 {
        (self.effective_viewport_size().x - 2.0 * self.koma_gap).max(MIN_PROPORTIONAL_WIDTH)
    }

    fn refresh_slot_widths(&mut self, viewport_w: f64) {
        let usable = (viewport_w - 2.0 * self.koma_gap).max(MIN_PROPORTIONAL_WIDTH);
        let rt = self.shared_web_runtime.as_ref().map(|r| r.borrow());
        for sheet in &mut self.sheets {
            for slot in &mut sheet.komas {
                slot.width = match slot.ratio {
                    Some(r) => (r * usable - (1.0 - r) * self.koma_gap).max(MIN_PROPORTIONAL_WIDTH),
                    None => slot.raw_width,
                };
                if slot.kind == KomaKind::Web {
                    if let Some(live_url) = rt
                        .as_ref()
                        .and_then(|r| r.koma_url_title(&slot.koma_id).map(|(u, _)| u))
                        .filter(|u| !u.is_empty())
                    {
                        slot.detail = live_url;
                    }
                }
            }
        }
    }

    fn effective_viewport_size(&self) -> Vec2d {
        if self.last_viewport.size.x > 1.0 && self.last_viewport.size.y > 1.0 {
            self.last_viewport.size
        } else {
            dvec2(1264.0, 680.0)
        }
    }

    fn koma_x_span(&self, sheet: &SheetSnapshot, koma_idx: usize) -> (f64, f64) {
        let start_x = self.koma_gap
            + sheet.komas[..koma_idx]
                .iter()
                .map(|s| s.width + self.koma_gap)
                .sum::<f64>();
        (start_x, start_x + sheet.komas[koma_idx].width)
    }

    fn sheet_total_width(&self, sheet: &SheetSnapshot) -> f64 {
        if sheet.komas.is_empty() {
            640.0
        } else {
            self.koma_gap * 2.0
                + sheet.komas.iter().map(|b| b.width).sum::<f64>()
                + self.koma_gap * sheet.komas.len().saturating_sub(1) as f64
        }
    }

    fn sheet_gap_x(&self, sheet: &SheetSnapshot, insert_idx: usize) -> f64 {
        if insert_idx == 0 || sheet.komas.is_empty() {
            self.koma_gap * 0.5
        } else if insert_idx >= sheet.komas.len() {
            let (_, ex) = self.koma_x_span(sheet, sheet.komas.len() - 1);
            ex + self.koma_gap * 0.5
        } else {
            let (sx, _) = self.koma_x_span(sheet, insert_idx);
            sx - self.koma_gap * 0.5
        }
    }

    fn target_scroll_x_for_koma(&self, koma_id: &KomaId, view_w: f64) -> Option<f64> {
        if view_w <= 1.0 {
            return None;
        }
        let sheet = self.sheets.get(self.active_sheet)?;
        let target_idx = sheet.komas.iter().position(|s| &s.koma_id == koma_id)?;
        let (start_x, end_x) = self.koma_x_span(sheet, target_idx);
        let want_left = (start_x - self.koma_gap).max(0.0);
        let want_right = end_x + self.koma_gap;
        let cur_x = self
            .scroll_target_x
            .unwrap_or_else(|| self.scroll_bars.get_scroll_pos().x);

        if want_left < cur_x {
            Some(want_left)
        } else if want_right > cur_x + view_w {
            Some((want_right - view_w).max(0.0))
        } else {
            None
        }
    }

    fn target_center_scroll_x_for_koma(&self, koma_id: &KomaId, view_w: f64) -> Option<f64> {
        if view_w <= 1.0 {
            return None;
        }
        let sheet = self.sheets.get(self.active_sheet)?;
        let target_idx = sheet.komas.iter().position(|s| &s.koma_id == koma_id)?;
        let (start_x, end_x) = self.koma_x_span(sheet, target_idx);
        let max_scroll = (self.sheet_total_width(sheet) - view_w).max(0.0);
        Some(((start_x + end_x) * 0.5 - view_w * 0.5).clamp(0.0, max_scroll))
    }

    fn target_reveal_scroll_x_for_koma(&self, koma_id: &KomaId, view_w: f64) -> Option<f64> {
        match self.center_focused {
            CenterFocused::Never => self.target_scroll_x_for_koma(koma_id, view_w),
            CenterFocused::Always => self.target_center_scroll_x_for_koma(koma_id, view_w),
            CenterFocused::OnOverflow => {
                self.target_scroll_x_for_koma(koma_id, view_w)?;
                self.target_center_scroll_x_for_koma(koma_id, view_w)
            }
        }
    }

    fn animate_scroll_to(&mut self, cx: &mut Cx, target_x: f64) {
        let cur_x = self.scroll_bars.get_scroll_pos().x;
        if (cur_x - target_x).abs() < 1.0 {
            self.scroll_target_x = None;
            return;
        }
        self.scroll_bars.stop_motion();
        self.discard_scroll_continuation = true;
        self.scroll_target_x = Some(target_x);
        self.start_animation(cx);
    }

    pub fn center_focused_koma(&mut self, cx: &mut Cx) {
        let Some(koma_id) = self
            .sheets
            .get(self.active_sheet)
            .and_then(|d| d.focused_koma_id.clone())
        else {
            return;
        };
        let view_w = self.scroll_bars.get_scroll_view_visible().x;
        if view_w <= 1.0 {
            self.pending_center_koma = Some(koma_id);
            return;
        }
        self.pending_center_koma = None;
        if let Some(target_x) = self.target_center_scroll_x_for_koma(&koma_id, view_w) {
            self.animate_scroll_to(cx, target_x);
        }
    }

    fn ensure_koma_visible(&mut self, cx: &mut Cx, koma_id: &KomaId) {
        let view_w = self.scroll_bars.get_scroll_view_visible().x;
        if view_w <= 1.0 {
            self.pending_reveal_koma = Some(koma_id.clone());
            return;
        }
        self.pending_reveal_koma = None;
        if let Some(target_x) = self.target_reveal_scroll_x_for_koma(koma_id, view_w) {
            self.animate_scroll_to(cx, target_x);
        }
    }

    fn slot_colors(&self, focused: bool, hovered: bool) -> (Vec4f, Vec4f) {
        if self.mode == InputMode::Overview {
            if focused {
                return (vec4(0.22, 0.74, 0.97, 1.0), vec4(0.11, 0.25, 0.38, 1.0));
            }
            if hovered {
                return (vec4(0.48, 0.64, 0.97, 1.0), vec4(0.14, 0.19, 0.30, 1.0));
            }
            return (vec4(0.16, 0.19, 0.27, 1.0), vec4(0.09, 0.10, 0.15, 1.0));
        }
        if !focused {
            return (vec4(0.16, 0.19, 0.27, 1.0), vec4(0.09, 0.10, 0.15, 1.0));
        }
        match self.mode {
            InputMode::KomaMode => (vec4(0.22, 0.74, 0.97, 1.0), vec4(0.11, 0.22, 0.33, 1.0)),
            InputMode::ContentInput => (vec4(0.23, 0.51, 0.96, 1.0), vec4(0.13, 0.17, 0.27, 1.0)),
            InputMode::Overview => (vec4(0.22, 0.74, 0.97, 1.0), vec4(0.11, 0.25, 0.38, 1.0)),
        }
    }

    fn draw_card(
        &mut self,
        cx: &mut Cx2d,
        scope: &mut Scope,
        slot: &KomaSlot,
        card_walk: Walk,
        highlight_glow: bool,
        hovered: bool,
    ) -> Rect {
        let (border_color, header_color) = self.slot_colors(slot.focused, hovered);
        let card_preview = cx.peek_walk_turtle(card_walk);
        if highlight_glow && (slot.focused || hovered) {
            let glow_rect = Rect {
                pos: card_preview.pos - dvec2(4.0, 4.0),
                size: card_preview.size + dvec2(8.0, 8.0),
            };
            self.draw_border.color = border_color;
            self.draw_border.draw_abs(cx, glow_rect);
        } else {
            self.draw_border.color = border_color;
            self.draw_border.draw_abs(cx, card_preview);
        }

        cx.begin_turtle(
            card_walk,
            Layout {
                flow: Flow::Down,
                padding: Inset {
                    left: 2.0,
                    right: 2.0,
                    top: 2.0,
                    bottom: 2.0,
                },
                spacing: 0.0,
                ..Layout::default()
            },
        );

        let header_walk = Walk::new(Size::fill(), Size::Fixed(HEADER_HEIGHT));
        let header_rect = cx.peek_walk_turtle(header_walk);
        self.draw_header.color = header_color;
        self.draw_header.draw_abs(cx, header_rect);

        cx.begin_turtle(
            header_walk,
            Layout {
                flow: Flow::Right {
                    row_align: RowAlign::Top,
                    wrap: false,
                },
                padding: Inset {
                    left: 8.0,
                    right: 8.0,
                    top: 0.0,
                    bottom: 0.0,
                },
                align: Align { x: 0.0, y: 0.5 },
                ..Layout::default()
            },
        );
        let marker = if slot.focused {
            match self.mode {
                InputMode::KomaMode => "[KOMA] ",
                InputMode::Overview => "[OVW] ",
                InputMode::ContentInput => "* ",
            }
        } else {
            ""
        };
        let kind_tag = match slot.kind {
            KomaKind::Terminal => "TERM",
            KomaKind::Web => "WEB",
        };
        let size_tag = match slot.ratio {
            Some(r) => format!("{}%", (r * 100.0).round() as i32),
            None => format!("{}px", slot.width.round() as i32),
        };
        let detail = if slot.detail.is_empty() || slot.detail == slot.label {
            String::new()
        } else {
            format!(" · {}", slot.detail)
        };
        let title = format!("{marker}{kind_tag} | {}{detail} ({size_tag})", slot.label);
        self.draw_title
            .draw_walk(cx, Walk::fit(), Align::default(), &title);
        cx.end_turtle();

        if let Some(widget) = self.koma_widgets.get_mut(&slot.koma_id) {
            widget.draw_walk_all(cx, scope, Walk::new(Size::fill(), Size::fill()));
        }

        cx.end_turtle()
    }
}

impl Widget for SheetStrip {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if matches!(event, Event::Signal | Event::Shutdown) {
            for widget in self.koma_widgets.values_mut() {
                widget.handle_event(cx, event, scope);
            }
            if matches!(event, Event::Signal) && self.is_overview_rendered() {
                self.redraw(cx);
            }
            return;
        }

        if let Event::Scroll(scroll) = event {
            match scroll.phase {
                ScrollPhase::Began => self.discard_scroll_continuation = false,
                ScrollPhase::Changed
                | ScrollPhase::Ended
                | ScrollPhase::Momentum
                | ScrollPhase::MomentumEnded
                    if self.discard_scroll_continuation =>
                {
                    return;
                }
                _ => {}
            }
        }

        if self.animating {
            if let Some(ne) = self.next_frame.is_event(event) {
                self.step_animation(cx, ne.time);
            }
        }

        if self.is_overview_rendered() {
            self.handle_overview_event(cx, event);
            return;
        }

        if let Event::Pinch(pinch) = event {
            self.handle_content_pinch(cx, event, pinch.phase, pinch.scale);
            return;
        }

        let mut actions = Vec::new();
        self.scroll_bars
            .handle_main_event(cx, event, scope, &mut actions);
        let _ = self.scroll_bars.catch_fling_on_press(cx, event);

        match event {
            Event::MouseMove(me) => {
                if let Some(mut drag) = self.strip_drag.take() {
                    if !drag.active && (me.abs - drag.start_abs).length() > DRAG_THRESHOLD_PX {
                        drag.active = true;
                    }
                    if drag.active {
                        let index = self
                            .card_rects
                            .iter()
                            .position(|(id, rect)| {
                                let offset =
                                    self.reorder_slide.as_ref().map_or(0.0, |s| s.offset(id));
                                me.abs.x < rect.pos.x - offset + rect.size.x * 0.5
                            })
                            .unwrap_or(self.card_rects.len());
                        drag.drop_index = Some(index);
                        self.redraw(cx);
                        self.strip_drag = Some(drag);
                        return;
                    }
                    self.strip_drag = Some(drag);
                }
            }
            Event::MouseUp(_) => {
                if let Some(drag) = self.strip_drag.take() {
                    if drag.active {
                        if let Some(target_index) = drag.drop_index {
                            cx.widget_action(
                                self.uid,
                                SheetStripAction::MoveKoma {
                                    koma_id: drag.koma_id,
                                    target_sheet: self.active_sheet,
                                    target_index,
                                },
                            );
                        }
                        self.redraw(cx);
                        return;
                    }
                }
            }
            Event::MouseDown(me) => {
                let strip_rect = self.draw_strip_bg.area().rect(cx);
                if strip_rect.contains(me.abs) {
                    if let Some((clicked_id, card_rect)) = self
                        .card_rects
                        .iter()
                        .rev()
                        .find(|(_, rect)| rect.contains(me.abs))
                        .cloned()
                    {
                        let already_focused = self
                            .sheets
                            .get(self.active_sheet)
                            .and_then(|d| d.focused_koma_id.as_ref())
                            == Some(&clicked_id);
                        let in_header = me.abs.y <= card_rect.pos.y + HEADER_HEIGHT + 4.0;
                        if in_header && !me.button.is_secondary() {
                            self.strip_drag = Some(StripDrag {
                                koma_id: clicked_id.clone(),
                                start_abs: me.abs,
                                active: false,
                                drop_index: None,
                            });
                        }
                        if !already_focused || self.mode == InputMode::KomaMode || in_header {
                            cx.widget_action(
                                self.uid,
                                SheetStripAction::FocusKoma {
                                    koma_id: clicked_id,
                                    enter_content_input: true,
                                },
                            );
                        }
                    }
                }
            }
            _ => {}
        }

        let is_scroll = matches!(event, Event::Scroll(_));
        let horiz_scroll = matches!(event, Event::Scroll(e) if is_horizontal_scroll(e.scroll));
        if horiz_scroll {
            self.scroll_target_x = None;
        }
        let axes = self.scroll_bars.scroll_container_axes(cx, event);
        let strip_claims_x = horiz_scroll && axes[0];

        cx.begin_scroll_container([strip_claims_x, false]);
        if let Some(sheet) = self.sheets.get(self.active_sheet) {
            for slot in &sheet.komas {
                if let Some(widget) = self.koma_widgets.get_mut(&slot.koma_id) {
                    if is_scroll
                        && horiz_scroll
                        && (slot.kind == KomaKind::Terminal || strip_claims_x)
                    {
                        continue;
                    }
                    widget.handle_event(cx, event, scope);
                }
            }
        }
        cx.end_scroll_container([strip_claims_x, false]);

        if !is_scroll || horiz_scroll {
            self.scroll_bars
                .handle_scroll_event(cx, event, scope, &mut actions);
        }
        if !actions.is_empty() {
            cx.redraw_area_and_children(self.scroll_bars.area());
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let outer_rect = cx.peek_walk_turtle(walk);
        self.last_viewport = outer_rect;
        self.draw_strip_bg.draw_abs(cx, outer_rect);

        if self.is_overview_rendered() {
            return self.draw_overview_canvas(cx, scope, walk);
        }

        if outer_rect.size.x > 1.0 {
            self.refresh_slot_widths(outer_rect.size.x);
            if let Some(center_id) = self.pending_center_koma.take() {
                if let Some(target_x) =
                    self.target_center_scroll_x_for_koma(&center_id, outer_rect.size.x)
                {
                    self.scroll_bars
                        .set_scroll_pos_no_clip(cx, dvec2(target_x, 0.0));
                }
            } else if let Some(pending_id) = self.pending_reveal_koma.take() {
                if let Some(target_x) =
                    self.target_reveal_scroll_x_for_koma(&pending_id, outer_rect.size.x)
                {
                    self.scroll_bars
                        .set_scroll_pos_no_clip(cx, dvec2(target_x, 0.0));
                }
            }
        }

        self.card_rects.clear();
        if let Some(slide) = &self.sheet_slide {
            let prev_id = slide.prev_sheet_id.clone();
            let prev_scroll_x = slide.prev_scroll_x;
            let offset_y = slide.offset_y;
            let direction = slide.direction;
            let prev_slots = self
                .sheets
                .iter()
                .find(|d| d.sheet_id == prev_id)
                .map(|d| d.komas.clone())
                .unwrap_or_default();
            let cur_slots = self
                .sheets
                .get(self.active_sheet)
                .map(|d| d.komas.clone())
                .unwrap_or_default();
            let cur_scroll_x = self.scroll_bars.get_scroll_pos().x;
            let row_h = (outer_rect.size.y - 2.0 * self.koma_gap).max(40.0);

            cx.begin_turtle(
                Walk::fixed(outer_rect.size.x, outer_rect.size.y).with_abs_pos(outer_rect.pos),
                Layout {
                    clip_x: true,
                    clip_y: true,
                    ..Layout::default()
                },
            );
            let prev_y =
                outer_rect.pos.y + self.koma_gap + offset_y - direction * outer_rect.size.y;
            self.draw_sheet_row_at(
                cx,
                scope,
                &prev_slots,
                dvec2(outer_rect.pos.x + self.koma_gap - prev_scroll_x, prev_y),
                row_h,
                false,
            );
            let cur_y = outer_rect.pos.y + self.koma_gap + offset_y;
            self.draw_sheet_row_at(
                cx,
                scope,
                &cur_slots,
                dvec2(outer_rect.pos.x + self.koma_gap - cur_scroll_x, cur_y),
                row_h,
                true,
            );
            cx.end_turtle();
            return DrawStep::done();
        }

        self.scroll_bars.begin(
            cx,
            walk,
            Layout {
                flow: Flow::Right {
                    row_align: RowAlign::Top,
                    wrap: false,
                },
                padding: Inset {
                    left: self.koma_gap,
                    right: self.koma_gap,
                    top: self.koma_gap,
                    bottom: self.koma_gap,
                },
                spacing: self.koma_gap,
                ..Layout::default()
            },
        );

        let slots = self
            .sheets
            .get(self.active_sheet)
            .map(|d| d.komas.clone())
            .unwrap_or_default();

        if slots.is_empty() {
            let color = self.draw_title.color;
            if let Some(gap) = &self.trailing_close_gap {
                self.draw_title.color.w *= (1.0 - gap.remaining()) as f32;
            }
            self.draw_title.draw_walk(
                cx,
                Walk::fit(),
                Align::default(),
                "Empty Sheet — Cmd+T: Palette | Cmd+Shift+T: Terminal | Ctrl+,: Koma Mode",
            );
            self.draw_title.color = color;
        } else {
            for slot in &slots {
                let gap = self
                    .close_gaps
                    .get(&slot.koma_id)
                    .map_or(0.0, CloseGap::width);
                let offset = self
                    .reorder_slide
                    .as_ref()
                    .map_or(0.0, |s| s.offset(&slot.koma_id));
                let mut card_walk =
                    Walk::new(Size::Fixed(slot.width), Size::fill()).with_margin_left(gap + offset);
                // Translate the Koma without changing the row's scroll extent.
                card_walk.margin.right = -offset;
                let drawn_rect = self.draw_card(cx, scope, slot, card_walk, false, false);
                self.card_rects.push((slot.koma_id.clone(), drawn_rect));
            }
            let trailing = self
                .trailing_close_gap
                .as_ref()
                .map_or(0.0, CloseGap::width);
            cx.walk_turtle(Walk::fixed(8.0 + trailing, 0.0));
        }

        self.scroll_bars.end(cx);

        if let Some(drag) = &self.strip_drag {
            if drag.active {
                if let (Some(ti), Some(sheet)) =
                    (drag.drop_index, self.sheets.get(self.active_sheet))
                {
                    let motion_gap = sheet
                        .komas
                        .iter()
                        .take(ti.saturating_add(1))
                        .filter_map(|slot| self.close_gaps.get(&slot.koma_id))
                        .map(CloseGap::width)
                        .sum::<f64>();
                    let gap_x = self.sheet_gap_x(sheet, ti) + motion_gap
                        - self.scroll_bars.get_scroll_pos().x;
                    let bar_rect = Rect {
                        pos: dvec2(
                            outer_rect.pos.x + gap_x - 2.0,
                            outer_rect.pos.y + self.koma_gap,
                        ),
                        size: dvec2(4.0, (outer_rect.size.y - 2.0 * self.koma_gap).max(40.0)),
                    };
                    self.draw_drop_bar.draw_abs(cx, bar_rect);
                }
            }
        }

        DrawStep::done()
    }
}

impl SheetStrip {
    fn draw_sheet_row_at(
        &mut self,
        cx: &mut Cx2d,
        scope: &mut Scope,
        slots: &[KomaSlot],
        origin: Vec2d,
        row_height: f64,
        record_hits: bool,
    ) {
        cx.begin_turtle(
            Walk::new(Size::fit(), Size::Fixed(row_height)).with_abs_pos(origin),
            Layout {
                flow: Flow::Right {
                    row_align: RowAlign::Top,
                    wrap: false,
                },
                spacing: self.koma_gap,
                ..Layout::default()
            },
        );
        if slots.is_empty() {
            self.draw_title.draw_walk(
                cx,
                Walk::fit(),
                Align::default(),
                "Empty Sheet — Cmd+T: Palette | Cmd+Shift+T: Terminal | Ctrl+,: Koma Mode",
            );
        } else {
            for slot in slots {
                let card_walk = Walk::new(Size::Fixed(slot.width), Size::Fixed(row_height));
                let drawn_rect = self.draw_card(cx, scope, slot, card_walk, false, false);
                if record_hits {
                    self.card_rects.push((slot.koma_id.clone(), drawn_rect));
                }
            }
        }
        cx.end_turtle();
    }
}

fn is_horizontal_scroll(scroll: Vec2d) -> bool {
    scroll.x != 0.0 && (scroll.y == 0.0 || scroll.x.abs() > scroll.y.abs())
}

fn intersect_rect(a: Rect, b: Rect) -> Rect {
    let x1 = a.pos.x.max(b.pos.x);
    let y1 = a.pos.y.max(b.pos.y);
    let x2 = (a.pos.x + a.size.x).min(b.pos.x + b.size.x);
    let y2 = (a.pos.y + a.size.y).min(b.pos.y + b.size.y);
    Rect {
        pos: dvec2(x1, y1),
        size: dvec2((x2 - x1).max(0.0), (y2 - y1).max(0.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(id: &str, width: f64) -> KomaSlot {
        KomaSlot {
            koma_id: id.into(),
            kind: KomaKind::Terminal,
            label: String::new(),
            detail: String::new(),
            raw_width: width,
            ratio: None,
            width,
            focused: false,
        }
    }

    #[test]
    fn content_pinch_accumulates_small_updates_and_steps_on_reversal() {
        let mut pinch = ContentPinch::new("target".into(), 1.0);
        assert_eq!(pinch.update(1.05), None);
        assert_eq!(pinch.update(1.05), Some(1.1));
        assert_eq!(pinch.update(0.98), None);
        assert_eq!(pinch.update(0.9), Some(1.0));
    }

    #[test]
    fn content_pinch_clamps_without_sticking_at_either_limit() {
        let mut pinch = ContentPinch::new("target".into(), 1.0);
        assert_eq!(pinch.update(100.0), Some(2.0));
        assert_eq!(pinch.update(f64::NAN), None);
        assert_eq!(pinch.update(0.95), Some(1.9));
        assert_eq!(pinch.update(0.001), Some(0.5));
        assert_eq!(pinch.update(1.2), Some(0.6));
    }

    #[test]
    fn closing_again_preserves_the_successors_intermediate_position() {
        let (a, b, c) = (slot("a", 320.0), slot("b", 480.0), slot("c", 640.0));
        let mut gaps = HashMap::new();
        CloseGap::transfer(&mut gaps, &a, &b, STRIP_SPACING);
        let gap = gaps.get_mut(&b.koma_id).unwrap();
        gap.elapsed = Some(KOMA_CLOSE_SECONDS / 2.0);
        let displayed_c_offset = gap.width() + b.width + STRIP_SPACING;

        CloseGap::transfer(&mut gaps, &b, &c, STRIP_SPACING);
        assert_eq!(gaps.len(), 1);
        let gap = gaps.get_mut(&c.koma_id).unwrap();
        assert_eq!(gap.width(), displayed_c_offset);
        gap.elapsed = Some(KOMA_CLOSE_SECONDS);
        assert_eq!(gap.width(), 0.0);
    }

    #[test]
    fn reorder_starts_at_the_drawn_positions_even_when_interrupted() {
        let a = slot("a", 320.0);
        let b = slot("b", 480.0);
        let old_b_x = a.width + STRIP_SPACING;
        let gaps = HashMap::new();
        let ab = [a.clone(), b.clone()];
        let mut slide = ReorderSlide::between(
            &ab,
            [&b.koma_id, &a.koma_id].into_iter(),
            STRIP_SPACING,
            &gaps,
            None,
        )
        .unwrap();
        assert_eq!(slide.offset(&b.koma_id), old_b_x);
        assert_eq!(slide.offset(&a.koma_id), -(b.width + STRIP_SPACING));
        slide.advance(0.0);
        assert!(slide.advance(KOMA_REORDER_SECONDS / 2.0));

        let displayed_b_x = slide.offset(&b.koma_id);
        let ba = [b, a];
        let mut reversed = ReorderSlide::between(
            &ba,
            ab.iter().map(|slot| &slot.koma_id),
            STRIP_SPACING,
            &gaps,
            Some(&slide),
        )
        .unwrap();
        assert_eq!(old_b_x + reversed.offset(&ba[0].koma_id), displayed_b_x);
        reversed.advance(0.0);
        assert!(!reversed.advance(KOMA_REORDER_SECONDS));
        assert_eq!(reversed.offset(&ba[0].koma_id), 0.0);
        assert!(ReorderSlide::between(
            &ab,
            [&"c".into(), &ab[0].koma_id].into_iter(),
            STRIP_SPACING,
            &gaps,
            None,
        )
        .is_none());
    }

    #[test]
    fn closing_the_tail_preserves_the_intermediate_strip_width() {
        let (a, b, c) = (slot("a", 320.0), slot("b", 480.0), slot("c", 640.0));
        let mut gaps = HashMap::new();
        CloseGap::transfer(&mut gaps, &a, &b, STRIP_SPACING);
        let mut trailing = CloseGap::after_removal(&mut gaps, &c, STRIP_SPACING, 0.0);
        for gap in [&mut trailing, gaps.get_mut(&b.koma_id).unwrap()] {
            gap.advance(0.0);
            gap.advance(KOMA_CLOSE_SECONDS / 2.0);
        }
        let old_width = gaps[&b.koma_id].width() + b.width + STRIP_SPACING + trailing.width();
        let mut new_trailing =
            CloseGap::after_removal(&mut gaps, &b, STRIP_SPACING, trailing.width());
        assert!(gaps.is_empty());
        assert_eq!(new_trailing.width(), old_width);
        new_trailing.advance(0.0);
        assert!(!new_trailing.advance(KOMA_CLOSE_SECONDS));
    }

    #[test]
    fn koma_start_x_tracks_scroll_offset_when_inserting_left_of_focus() {
        let (a, x, b) = (slot("a", 320.0), slot("x", 400.0), slot("b", 480.0));
        let before = [a.clone(), b.clone()];
        let after_left_insert = [a, x.clone(), b];
        let old_x = koma_start_x(&before, &"b".into(), STRIP_SPACING).unwrap();
        let new_x = koma_start_x(&after_left_insert, &"b".into(), STRIP_SPACING).unwrap();
        assert_eq!(new_x - old_x, x.width + STRIP_SPACING);
    }

    #[test]
    fn zero_delta_scroll_does_not_count_as_horizontal_scroll() {
        assert!(!is_horizontal_scroll(dvec2(0.0, 0.0)));
        assert!(!is_horizontal_scroll(dvec2(0.0, 12.0)));
        assert!(!is_horizontal_scroll(dvec2(3.0, 12.0)));
        assert!(is_horizontal_scroll(dvec2(12.0, 0.0)));
        assert!(is_horizontal_scroll(dvec2(-12.0, 4.0)));
    }
}
