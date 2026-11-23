use std::collections::HashSet;

use koma_core::model::{InputMode, KomaKind};
use makepad_widgets::*;

use super::{
    intersect_rect, BgKind, BgPanDrag, KomaSlot, OverviewCardHit, OverviewDrag, SheetStrip,
    SheetStripAction, DEFAULT_OVERVIEW_SCALE, DRAG_THRESHOLD_PX, OVERVIEW_RENDER_EPS,
};

const SHEET_GAP: f64 = 72.0;
const SHEET_LABEL_OFFSET_Y: f64 = 28.0;

const CANVAS_ORIGIN: f64 = 16384.0;
const CANVAS_ROOT_SIZE: f64 = 65536.0;
const MIN_OVERVIEW_SCALE: f64 = 0.25;
const MAX_OVERVIEW_SCALE: f64 = 0.85;
const OVERVIEW_SCALE_STEP: f64 = 0.08;
const OVERVIEW_SNAP_T_EPS: f64 = 0.004;

impl SheetStrip {
    pub fn is_overview_rendered(&self) -> bool {
        self.mode == InputMode::Overview || self.overview_t > OVERVIEW_RENDER_EPS
    }

    pub fn adjust_overview_zoom(&mut self, cx: &mut Cx, delta_step: isize) {
        let next = (self.overview_scale + delta_step as f64 * OVERVIEW_SCALE_STEP)
            .clamp(MIN_OVERVIEW_SCALE, MAX_OVERVIEW_SCALE);
        if (next - self.overview_scale).abs() < 0.001 {
            return;
        }
        self.overview_scale = next;
        self.start_animation(cx);
    }

    pub fn reset_overview_zoom(&mut self, cx: &mut Cx) {
        self.overview_scale = DEFAULT_OVERVIEW_SCALE;
        self.cam_pan_offset = dvec2(0.0, 0.0);
        self.start_animation(cx);
    }

    fn sheet_row_step(viewport_h: f64) -> f64 {
        viewport_h + SHEET_GAP
    }

    pub(super) fn sheet_y_top(&self, sheet_idx: usize, viewport_h: f64) -> f64 {
        sheet_idx as f64 * Self::sheet_row_step(viewport_h)
    }

    fn overview_camera_target(&self) -> (f64, f64, f64, f64) {
        let vp = self.effective_viewport_size();
        if self.mode != InputMode::Overview {
            let scroll_x = self.scroll_bars.get_scroll_pos().x;
            let cx_pos = scroll_x + vp.x * 0.5;
            let cy_pos = self.sheet_y_top(self.active_sheet, vp.y) + vp.y * 0.5;
            return (0.0, 1.0, cx_pos, cy_pos);
        }

        let s = self
            .overview_scale
            .clamp(MIN_OVERVIEW_SCALE, MAX_OVERVIEW_SCALE);
        let vis_w = vp.x / s;
        let vis_h = vp.y / s;

        let active_sheet = self.sheets.get(self.active_sheet);
        let base_x = match active_sheet {
            Some(sheet) => {
                let row_w = self.sheet_total_width(sheet);
                if row_w <= vis_w - 80.0 {
                    row_w * 0.5
                } else if let Some(bi) = sheet.komas.iter().position(|b| b.focused) {
                    let (sx, ex) = self.koma_x_span(sheet, bi);
                    (sx + ex) * 0.5
                } else {
                    row_w * 0.5
                }
            }
            None => vp.x * 0.5,
        };

        let total_h = (self.sheets.len().max(1) as f64) * Self::sheet_row_step(vp.y) - SHEET_GAP;
        let base_y = if total_h <= vis_h - 80.0 {
            total_h * 0.5
        } else {
            self.sheet_y_top(self.active_sheet, vp.y) + vp.y * 0.5
        };

        (
            1.0,
            s,
            base_x + self.cam_pan_offset.x,
            base_y + self.cam_pan_offset.y,
        )
    }

    pub(super) fn start_animation(&mut self, cx: &mut Cx) {
        self.animating = true;
        self.last_frame_time = None;
        self.next_frame = cx.new_next_frame();
        self.redraw(cx);
    }

    pub(super) fn step_animation(&mut self, cx: &mut Cx, time: f64) {
        let dt = self
            .last_frame_time
            .map(|prev| (time - prev).clamp(0.001, 0.05))
            .unwrap_or(1.0 / 60.0);
        self.last_frame_time = Some(time);
        self.close_gaps.retain(|_, gap| gap.advance(dt));
        self.trailing_close_gap.take_if(|gap| !gap.advance(dt));
        self.reorder_slide.take_if(|slide| !slide.advance(dt));

        let (target_t, target_scale, target_x, target_y) = self.overview_camera_target();
        let k = 1.0 - (-18.0 * dt).exp();

        if let Some(target_sx) = self.scroll_target_x {
            let cur_sx = self.scroll_bars.get_scroll_pos().x;
            let next_sx = cur_sx + (target_sx - cur_sx) * k;
            if (next_sx - target_sx).abs() < 1.0 {
                let _ = self
                    .scroll_bars
                    .set_scroll_pos_no_clip(cx, dvec2(target_sx, 0.0));
                self.scroll_target_x = None;
            } else {
                let _ = self
                    .scroll_bars
                    .set_scroll_pos_no_clip(cx, dvec2(next_sx, 0.0));
            }
        }

        if let Some(mut slide) = self.sheet_slide.take() {
            slide.offset_y += (0.0 - slide.offset_y) * k;
            if slide.offset_y.abs() < 1.5 {
                self.hide_sheet_webviews(cx, &slide.prev_sheet_id);
            } else {
                self.sheet_slide = Some(slide);
            }
        }

        self.overview_t += (target_t - self.overview_t) * k;
        self.cam_scale += (target_scale - self.cam_scale) * k;
        self.cam_x += (target_x - self.cam_x) * k;
        self.cam_y += (target_y - self.cam_y) * k;

        let settled = self.close_gaps.is_empty()
            && self.trailing_close_gap.is_none()
            && self.reorder_slide.is_none()
            && self.scroll_target_x.is_none()
            && self.sheet_slide.is_none()
            && (self.overview_t - target_t).abs() < OVERVIEW_SNAP_T_EPS
            && (self.cam_scale - target_scale).abs() < OVERVIEW_SNAP_T_EPS
            && (self.cam_x - target_x).abs() < 1.5
            && (self.cam_y - target_y).abs() < 1.5;

        if settled {
            self.overview_t = target_t;
            self.cam_scale = target_scale;
            self.cam_x = target_x;
            self.cam_y = target_y;
            self.animating = false;
            self.last_frame_time = None;
            if self.mode != InputMode::Overview {
                cx.widget_action(self.uid, SheetStripAction::OverviewSettled);
            }
        } else {
            self.next_frame = cx.new_next_frame();
        }
        self.redraw(cx);
    }

    fn screen_to_world(&self, abs: Vec2d) -> Vec2d {
        let (s, tx, ty) = self.last_transform;
        let inv = 1.0 / s.max(0.05);
        dvec2(
            (abs.x - tx) * inv - CANVAS_ORIGIN,
            (abs.y - ty) * inv - CANVAS_ORIGIN,
        )
    }

    fn compute_drop_target(&self, world: Vec2d) -> Option<(usize, usize)> {
        if self.sheets.is_empty() {
            return None;
        }
        let vp = self.effective_viewport_size();
        let step = Self::sheet_row_step(vp.y);
        let sheet_idx = ((world.y + SHEET_GAP * 0.5) / step)
            .floor()
            .clamp(0.0, (self.sheets.len() - 1) as f64) as usize;
        let sheet = &self.sheets[sheet_idx];
        let mut insert_idx = sheet.komas.len();
        for (bi, _) in sheet.komas.iter().enumerate() {
            let (sx, ex) = self.koma_x_span(sheet, bi);
            let mid_x = (sx + ex) * 0.5;
            if world.x < mid_x {
                insert_idx = bi;
                break;
            }
        }
        Some((sheet_idx, insert_idx))
    }

    pub(super) fn draw_overview_canvas(
        &mut self,
        cx: &mut Cx2d,
        scope: &mut Scope,
        walk: Walk,
    ) -> DrawStep {
        let viewport = cx.walk_turtle(walk);
        self.last_viewport = viewport;
        self.draw_overview_bg.draw_abs(cx, viewport);
        self.overview_card_rects.clear();
        self.overview_sheet_rects.clear();

        if viewport.size.x <= 1.0 || viewport.size.y <= 1.0 {
            return DrawStep::done();
        }
        self.refresh_slot_widths(viewport.size.x);

        if !self.animating && self.mode == InputMode::Overview {
            let (_, ts, tx_c, ty_c) = self.overview_camera_target();
            self.cam_scale = ts;
            self.cam_x = tx_c;
            self.cam_y = ty_c;
        }

        let s = self.cam_scale.max(0.05);
        let tx = viewport.pos.x + viewport.size.x * 0.5 - (self.cam_x + CANVAS_ORIGIN) * s;
        let ty = viewport.pos.y + viewport.size.y * 0.5 - (self.cam_y + CANVAS_ORIGIN) * s;
        self.last_transform = (s, tx, ty);

        let local_clip = Rect {
            pos: dvec2((viewport.pos.x - tx) / s, (viewport.pos.y - ty) / s),
            size: viewport.size / s,
        };
        let visible_world = Rect {
            pos: local_clip.pos - dvec2(CANVAS_ORIGIN, CANVAS_ORIGIN),
            size: local_clip.size,
        };

        let mut draw_list = self
            .overview_draw_list
            .take()
            .unwrap_or_else(|| DrawList2d::new(cx));
        draw_list.begin_always(cx);
        cx.begin_unclipped_root_turtle(
            dvec2(CANVAS_ROOT_SIZE, CANVAS_ROOT_SIZE),
            Layout::flow_overlay(),
        );
        cx.push_clip_rect(local_clip);

        let card_h = (viewport.size.y - 2.0 * self.koma_gap).max(160.0);
        let sheets = self.sheets.clone();
        let active_drag = self.overview_drag.clone().filter(|d| d.active);
        let hovered_id = self.hovered_koma.clone();
        let mut visible_web_ids = HashSet::new();

        for (sheet_idx, sheet) in sheets.iter().enumerate() {
            let sheet_y = self.sheet_y_top(sheet_idx, viewport.size.y);
            if sheet_y + viewport.size.y + SHEET_LABEL_OFFSET_Y < visible_world.pos.y
                || sheet_y - SHEET_LABEL_OFFSET_Y > visible_world.pos.y + visible_world.size.y
            {
                continue;
            }

            let is_active_sheet = sheet_idx == self.active_sheet;
            let row_w = self.sheet_total_width(sheet).max(viewport.size.x);
            let sheet_world_rect = Rect {
                pos: dvec2(0.0, sheet_y),
                size: dvec2(row_w, viewport.size.y),
            };
            self.overview_sheet_rects
                .push((sheet_idx, sheet_world_rect));

            let sheet_local_rect = Rect {
                pos: dvec2(CANVAS_ORIGIN, CANVAS_ORIGIN + sheet_y),
                size: dvec2(row_w, viewport.size.y),
            };
            if is_active_sheet && self.overview_t > 0.05 {
                let frame = Rect {
                    pos: sheet_local_rect.pos - dvec2(4.0, 4.0),
                    size: sheet_local_rect.size + dvec2(8.0, 8.0),
                };
                self.draw_border.color = vec4(0.18, 0.36, 0.56, 1.0);
                self.draw_border.draw_abs(cx, frame);
            }
            self.draw_sheet_bg.color = if is_active_sheet {
                vec4(0.08, 0.10, 0.16, 1.0)
            } else {
                vec4(0.06, 0.08, 0.12, 1.0)
            };
            self.draw_sheet_bg.draw_abs(cx, sheet_local_rect);

            if self.overview_t > 0.05 {
                let label_pos = dvec2(
                    CANVAS_ORIGIN + 6.0,
                    CANVAS_ORIGIN + sheet_y - SHEET_LABEL_OFFSET_Y,
                );
                let label_text = if is_active_sheet {
                    format!("* {} ({} komas)", sheet.label, sheet.komas.len())
                } else {
                    format!("{} ({} komas)", sheet.label, sheet.komas.len())
                };
                self.draw_sheet_label.draw_walk(
                    cx,
                    Walk::fit().with_abs_pos(label_pos),
                    Align::default(),
                    &label_text,
                );
            }

            for (bi, slot) in sheet.komas.iter().enumerate() {
                let (bx, _) = self.koma_x_span(sheet, bi);
                if bx + slot.width < visible_world.pos.x
                    || bx > visible_world.pos.x + visible_world.size.x
                {
                    continue;
                }
                let card_local_rect = Rect {
                    pos: dvec2(CANVAS_ORIGIN + bx, CANVAS_ORIGIN + sheet_y + self.koma_gap),
                    size: dvec2(slot.width, card_h),
                };
                let card_clip = intersect_rect(
                    local_clip,
                    Rect {
                        pos: card_local_rect.pos - dvec2(5.0, 5.0),
                        size: card_local_rect.size + dvec2(10.0, 10.0),
                    },
                );
                if card_clip.size.x > 0.0 && card_clip.size.y > 0.0 {
                    cx.push_clip_rect(card_clip);
                    let card_walk =
                        Walk::fixed(slot.width, card_h).with_abs_pos(card_local_rect.pos);
                    let is_hovered = hovered_id.as_ref() == Some(&slot.koma_id);
                    let is_selected = is_active_sheet && slot.focused;
                    let selected_or_hovered = KomaSlot {
                        focused: is_selected,
                        ..slot.clone()
                    };
                    let _ = self.draw_card(
                        cx,
                        scope,
                        &selected_or_hovered,
                        card_walk,
                        self.overview_t > 0.05,
                        is_hovered,
                    );
                    if slot.kind == KomaKind::Web {
                        visible_web_ids.insert(&slot.koma_id);
                    }
                    cx.pop_clip_rect();
                }
                self.overview_card_rects.push(OverviewCardHit {
                    sheet_idx,
                    koma_id: slot.koma_id.clone(),
                    world_rect: Rect {
                        pos: dvec2(bx, sheet_y + self.koma_gap),
                        size: dvec2(slot.width, card_h),
                    },
                });
            }

            if let Some(drag) = &active_drag {
                if let Some((td, ti)) = drag.drop_target {
                    if td == sheet_idx {
                        let gap_x = self.sheet_gap_x(sheet, ti);
                        let bar_rect = Rect {
                            pos: dvec2(
                                CANVAS_ORIGIN + gap_x - 3.0,
                                CANVAS_ORIGIN + sheet_y + self.koma_gap - 6.0,
                            ),
                            size: dvec2(6.0, card_h + 12.0),
                        };
                        self.draw_drop_bar.draw_abs(cx, bar_rect);
                    }
                }
            }
        }

        cx.pop_clip_rect();
        cx.end_pass_sized_turtle_no_clip();
        draw_list.end(cx);

        if let Some(rt) = &self.shared_web_runtime {
            let mut rt = rt.borrow_mut();
            for sheet in &sheets {
                for slot in &sheet.komas {
                    if slot.kind == KomaKind::Web {
                        rt.set_koma_hidden(
                            cx,
                            &slot.koma_id,
                            !visible_web_ids.contains(&slot.koma_id),
                        );
                    }
                }
            }
        }

        let mut mat = Mat4f::default();
        mat.v[0] = s as f32;
        mat.v[5] = s as f32;
        mat.v[12] = tx as f32;
        mat.v[13] = ty as f32;
        draw_list.set_view_transform(cx, &mat);
        self.overview_draw_list = Some(draw_list);

        DrawStep::done()
    }

    pub(super) fn handle_overview_event(&mut self, cx: &mut Cx, event: &Event) {
        let area_rect = self.draw_strip_bg.area().rect(cx);
        match event {
            Event::MouseMove(me) => {
                if let Some(mut drag) = self.overview_drag.take() {
                    if !drag.active && (me.abs - drag.start_abs).length() > DRAG_THRESHOLD_PX {
                        drag.active = true;
                    }
                    if drag.active {
                        let world = self.screen_to_world(me.abs);
                        drag.drop_target = self.compute_drop_target(world);
                        self.redraw(cx);
                    }
                    self.overview_drag = Some(drag);
                    return;
                }
                if let Some(mut pan) = self.bg_pan_drag.take() {
                    let delta = me.abs - pan.last_abs;
                    if !pan.moved && (me.abs - pan.start_abs).length() > DRAG_THRESHOLD_PX {
                        pan.moved = true;
                    }
                    if pan.moved {
                        let scale = self.cam_scale.max(0.1);
                        self.cam_pan_offset.x =
                            (self.cam_pan_offset.x - delta.x / scale).clamp(-4000.0, 4000.0);
                        self.cam_pan_offset.y =
                            (self.cam_pan_offset.y - delta.y / scale).clamp(-4000.0, 4000.0);
                        self.redraw(cx);
                    }
                    pan.last_abs = me.abs;
                    self.bg_pan_drag = Some(pan);
                    return;
                }
                if area_rect.contains(me.abs) {
                    let world = self.screen_to_world(me.abs);
                    let next_hover = self
                        .overview_card_rects
                        .iter()
                        .find(|c| c.world_rect.contains(world))
                        .map(|c| c.koma_id.clone());
                    if next_hover != self.hovered_koma {
                        self.hovered_koma = next_hover;
                        self.redraw(cx);
                    }
                } else if self.hovered_koma.take().is_some() {
                    self.redraw(cx);
                }
            }
            Event::MouseDown(me) if area_rect.contains(me.abs) => {
                let world = self.screen_to_world(me.abs);
                if me.button.is_secondary() {
                    self.bg_pan_drag = Some(BgPanDrag {
                        kind: BgKind::Outer,
                        start_abs: me.abs,
                        last_abs: me.abs,
                        moved: false,
                    });
                    return;
                }
                if let Some(card) = self
                    .overview_card_rects
                    .iter()
                    .find(|c| c.world_rect.contains(world))
                {
                    self.overview_drag = Some(OverviewDrag {
                        koma_id: card.koma_id.clone(),
                        origin_sheet: card.sheet_idx,
                        start_abs: me.abs,
                        active: false,
                        drop_target: None,
                    });
                    return;
                }
                let kind = self
                    .overview_sheet_rects
                    .iter()
                    .find(|(_, r)| r.contains(world))
                    .map(|(di, _)| BgKind::Sheet(*di))
                    .unwrap_or(BgKind::Outer);
                self.bg_pan_drag = Some(BgPanDrag {
                    kind,
                    start_abs: me.abs,
                    last_abs: me.abs,
                    moved: false,
                });
            }
            Event::MouseUp(me) => {
                if let Some(drag) = self.overview_drag.take() {
                    if drag.active {
                        if let Some((target_sheet, target_index)) = drag.drop_target {
                            cx.widget_action(
                                self.uid,
                                SheetStripAction::MoveKoma {
                                    koma_id: drag.koma_id,
                                    target_sheet,
                                    target_index,
                                },
                            );
                        }
                        self.redraw(cx);
                        return;
                    }
                    let world = self.screen_to_world(me.abs);
                    if let Some(hit) = self
                        .overview_card_rects
                        .iter()
                        .find(|c| c.world_rect.contains(world))
                    {
                        cx.widget_action(
                            self.uid,
                            SheetStripAction::EnterOverviewTarget {
                                sheet_idx: hit.sheet_idx,
                                koma_id: Some(hit.koma_id.clone()),
                            },
                        );
                    } else {
                        cx.widget_action(
                            self.uid,
                            SheetStripAction::EnterOverviewTarget {
                                sheet_idx: drag.origin_sheet,
                                koma_id: Some(drag.koma_id),
                            },
                        );
                    }
                    return;
                }
                if let Some(pan) = self.bg_pan_drag.take() {
                    if !pan.moved {
                        match pan.kind {
                            BgKind::Sheet(sheet_idx) => {
                                cx.widget_action(
                                    self.uid,
                                    SheetStripAction::EnterOverviewTarget {
                                        sheet_idx,
                                        koma_id: None,
                                    },
                                );
                            }
                            BgKind::Outer => {
                                cx.widget_action(self.uid, SheetStripAction::ExitOverview);
                            }
                        }
                    }
                }
            }
            Event::Scroll(se) if area_rect.contains(se.abs) => {
                se.handled_x.set(true);
                se.handled_y.set(true);
                if se.modifiers.logo || se.modifiers.control {
                    self.overview_scale = (self.overview_scale * (1.0 - se.scroll.y * 0.002))
                        .clamp(MIN_OVERVIEW_SCALE, MAX_OVERVIEW_SCALE);
                    self.cam_scale = self.overview_scale;
                } else {
                    let s = self.cam_scale.max(0.1);
                    self.cam_pan_offset.x =
                        (self.cam_pan_offset.x + se.scroll.x / s).clamp(-4000.0, 4000.0);
                    self.cam_pan_offset.y =
                        (self.cam_pan_offset.y + se.scroll.y / s).clamp(-4000.0, 4000.0);
                }
                self.redraw(cx);
            }
            _ => {}
        }
    }
}
