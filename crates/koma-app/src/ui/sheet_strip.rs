//! Horizontally scrollable strip of Komas that zooms out GPU-side into a Niri-like live-canvas Overview.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use koma_app::config::CenterFocused;
use koma_app::ghostty_config::TerminalSettings;
use koma_core::model::{InputMode, KomaId, KomaKind, SheetId, MIN_PROPORTIONAL_WIDTH};
use makepad_widgets::event::ScrollPhase;
use makepad_widgets::turtle::RowAlign;
use makepad_widgets::*;

use crate::runtime::web_views::WebViewRuntime;

mod animation;
mod input;
mod overview;
mod widgets;

use animation::{CloseGap, StripAnimation};
use input::{is_horizontal_scroll, PinchTarget, StripDrag};
use overview::OverviewState;

pub const STRIP_SPACING: f64 = 10.0;
const HEADER_HEIGHT: f64 = 22.0;
const DEFAULT_OVERVIEW_SCALE: f64 = 0.44;
const OVERVIEW_RENDER_EPS: f64 = 0.002;
const DRAG_THRESHOLD_PX: f64 = 8.0;

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
            color: theme.color_surface
        }
        draw_overview_bg +: {
            color: theme.color_surface_dim
        }
        draw_sheet_bg +: {
            color: theme.color_surface_container_low
        }
        draw_border +: {
            color: theme.color_outline_variant
        }
        draw_header +: {
            color: theme.color_surface_container
        }
        draw_drop_bar +: {
            color: theme.color_primary
        }
        draw_title +: {
            color: theme.color_on_surface
            text_style: theme.font_regular{
                font_size: 11
            }
        }
        draw_sheet_label +: {
            color: theme.color_on_surface_variant
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
struct SheetSnapshot {
    sheet_id: SheetId,
    label: String,
    focused_koma_id: Option<KomaId>,
    komas: Vec<KomaSlot>,
}

#[derive(Clone, Copy, Debug)]
struct StripThemeColors {
    dark: bool,
    koma_mode_border: Vec4f,
    koma_mode_header: Vec4f,
    overview_focus_header: Vec4f,
    inactive_border: Vec4f,
    inactive_header: Vec4f,
    hover_border: Vec4f,
    hover_header: Vec4f,
}

impl Default for StripThemeColors {
    fn default() -> Self {
        Self::for_scheme(true)
    }
}

impl StripThemeColors {
    fn for_scheme(dark: bool) -> Self {
        if dark {
            Self {
                dark: true,
                koma_mode_border: vec4(0.22, 0.74, 0.97, 1.0),
                koma_mode_header: vec4(0.11, 0.22, 0.33, 1.0),
                overview_focus_header: vec4(0.11, 0.25, 0.38, 1.0),
                inactive_border: vec4(0.16, 0.19, 0.27, 1.0),
                inactive_header: vec4(0.09, 0.10, 0.15, 1.0),
                hover_border: vec4(0.48, 0.64, 0.97, 1.0),
                hover_header: vec4(0.14, 0.19, 0.30, 1.0),
            }
        } else {
            Self {
                dark: false,
                koma_mode_border: vec4(0.02, 0.52, 0.78, 1.0),
                koma_mode_header: vec4(0.82, 0.93, 0.98, 1.0),
                overview_focus_header: vec4(0.82, 0.93, 0.98, 1.0),
                inactive_border: vec4(0.78, 0.81, 0.86, 1.0),
                inactive_header: vec4(0.91, 0.93, 0.96, 1.0),
                hover_border: vec4(0.35, 0.55, 0.95, 1.0),
                hover_header: vec4(0.88, 0.92, 0.98, 1.0),
            }
        }
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
    discard_scroll_continuation: bool,
    #[rust]
    center_focused: CenterFocused,
    #[rust]
    theme_colors: StripThemeColors,
    #[rust(STRIP_SPACING)]
    koma_gap: f64,
    #[rust]
    strip_drag: Option<StripDrag>,
    #[rust]
    last_viewport: Rect,
    #[rust]
    anim: StripAnimation,
    #[rust]
    overview: OverviewState,
}

impl SheetStrip {
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
            .anim
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
        let t = &self.theme_colors;
        if self.mode == InputMode::Overview {
            if focused {
                return (t.koma_mode_border, t.overview_focus_header);
            }
            if hovered {
                return (t.hover_border, t.hover_header);
            }
            return (t.inactive_border, t.inactive_header);
        }
        if !focused {
            return (t.inactive_border, t.inactive_header);
        }
        match self.mode {
            InputMode::KomaMode => (t.koma_mode_border, t.koma_mode_header),
            InputMode::ContentInput => {
                // Reuse `draw_drop_bar.color` so ContentInput focus tracks `theme.color_primary`.
                let accent = self.draw_drop_bar.color;
                let mix = if t.dark { 0.20 } else { 0.14 };
                let header = t.inactive_header * (1.0 - mix) + accent * mix;
                (accent, vec4(header.x, header.y, header.z, 1.0))
            }
            InputMode::Overview => (t.koma_mode_border, t.overview_focus_header),
        }
    }

    pub fn set_theme(&mut self, dark: bool) {
        self.theme_colors = StripThemeColors::for_scheme(dark);
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

        if self.anim.animating {
            if let Some(ne) = self.anim.next_frame.is_event(event) {
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

        if self.handle_strip_pointer_event(cx, event) {
            return;
        }

        let is_scroll = matches!(event, Event::Scroll(_));
        let horiz_scroll = matches!(event, Event::Scroll(e) if is_horizontal_scroll(e.scroll));
        if horiz_scroll {
            self.anim.scroll_target_x = None;
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
        if let Some(slide) = &self.anim.sheet_slide {
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
            if let Some(gap) = &self.anim.trailing_close_gap {
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
                let gap = self.anim.close_gap_width(&slot.koma_id);
                let offset = self.anim.reorder_offset(&slot.koma_id);
                let mut card_walk =
                    Walk::new(Size::Fixed(slot.width), Size::fill()).with_margin_left(gap + offset);
                // Translate the Koma without changing the row's scroll extent.
                card_walk.margin.right = -offset;
                let drawn_rect = self.draw_card(cx, scope, slot, card_walk, false, false);
                self.card_rects.push((slot.koma_id.clone(), drawn_rect));
            }
            let trailing = self
                .anim
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
                        .map(|slot| self.anim.close_gap_width(&slot.koma_id))
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
