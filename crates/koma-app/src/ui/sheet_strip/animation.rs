//! Strip animation types (`CloseGap`, `ReorderSlide`, `SheetSlide`) and frame stepping.

use std::collections::HashMap;

use koma_core::model::{InputMode, KomaId, SheetId};
use makepad_widgets::*;

use super::overview::OVERVIEW_SNAP_T_EPS;
use super::{KomaSlot, SheetStrip, SheetStripAction};

pub(super) const KOMA_CLOSE_SECONDS: f64 = 0.17;
pub(super) const KOMA_REORDER_SECONDS: f64 = 0.17;

#[derive(Default)]
pub(super) struct StripAnimation {
    pub(super) scroll_target_x: Option<f64>,
    pub(super) sheet_slide: Option<SheetSlide>,
    pub(super) close_gaps: HashMap<KomaId, CloseGap>,
    pub(super) trailing_close_gap: Option<CloseGap>,
    pub(super) reorder_slide: Option<ReorderSlide>,
    pub(super) animating: bool,
    pub(super) last_frame_time: Option<f64>,
    pub(super) next_frame: NextFrame,
}

impl StripAnimation {
    pub(super) fn remove_koma(&mut self, koma_id: &KomaId) {
        self.close_gaps.remove(koma_id);
        if let Some(slide) = &mut self.reorder_slide {
            slide.offsets.remove(koma_id);
        }
    }

    pub(super) fn close_gap_width(&self, koma_id: &KomaId) -> f64 {
        self.close_gaps.get(koma_id).map_or(0.0, CloseGap::width)
    }

    pub(super) fn reorder_offset(&self, koma_id: &KomaId) -> f64 {
        self.reorder_slide
            .as_ref()
            .map_or(0.0, |slide| slide.offset(koma_id))
    }
}

pub(super) struct SheetSlide {
    pub(super) prev_sheet_id: SheetId,
    pub(super) prev_scroll_x: f64,
    pub(super) offset_y: f64,
    pub(super) direction: f64,
}

pub(super) struct CloseGap {
    pub(super) distance: f64,
    pub(super) elapsed: Option<f64>,
}

impl CloseGap {
    pub(super) fn remaining(&self) -> f64 {
        let t = (1.0 - self.elapsed.unwrap_or(0.0) / KOMA_CLOSE_SECONDS).max(0.0);
        t * t * (3.0 - 2.0 * t)
    }

    pub(super) fn width(&self) -> f64 {
        self.distance * self.remaining()
    }

    pub(super) fn advance(&mut self, dt: f64) -> bool {
        self.elapsed = Some(self.elapsed.map_or(0.0, |elapsed| elapsed + dt));
        self.width() >= 0.5
    }

    pub(super) fn after_removal(
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

    pub(super) fn transfer(
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

pub(super) fn koma_start_x(komas: &[KomaSlot], koma_id: &KomaId, koma_gap: f64) -> Option<f64> {
    let mut x = 0.0;
    for slot in komas {
        if &slot.koma_id == koma_id {
            return Some(x);
        }
        x += slot.width + koma_gap;
    }
    None
}

pub(super) struct ReorderSlide {
    pub(super) offsets: HashMap<KomaId, f64>,
    pub(super) elapsed: Option<f64>,
}

impl ReorderSlide {
    pub(super) fn offset(&self, id: &KomaId) -> f64 {
        let t = (1.0 - self.elapsed.unwrap_or(0.0) / KOMA_REORDER_SECONDS).max(0.0);
        self.offsets.get(id).copied().unwrap_or(0.0) * t * t * (3.0 - 2.0 * t)
    }

    pub(super) fn between<'a>(
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

    pub(super) fn advance(&mut self, dt: f64) -> bool {
        let elapsed = self.elapsed.map_or(0.0, |elapsed| elapsed + dt);
        self.elapsed = Some(elapsed);
        elapsed < KOMA_REORDER_SECONDS
    }
}

impl SheetStrip {
    pub(super) fn start_animation(&mut self, cx: &mut Cx) {
        self.anim.animating = true;
        self.anim.last_frame_time = None;
        self.anim.next_frame = cx.new_next_frame();
        self.redraw(cx);
    }

    pub(super) fn step_animation(&mut self, cx: &mut Cx, time: f64) {
        let dt = self
            .anim
            .last_frame_time
            .map(|prev| (time - prev).clamp(0.001, 0.05))
            .unwrap_or(1.0 / 60.0);
        self.anim.last_frame_time = Some(time);
        self.anim.close_gaps.retain(|_, gap| gap.advance(dt));
        self.anim.trailing_close_gap.take_if(|gap| !gap.advance(dt));
        self.anim.reorder_slide.take_if(|slide| !slide.advance(dt));

        let (target_t, target_scale, target_x, target_y) = self.overview_camera_target();
        let k = 1.0 - (-18.0 * dt).exp();

        if let Some(target_sx) = self.anim.scroll_target_x {
            let cur_sx = self.scroll_bars.get_scroll_pos().x;
            let next_sx = cur_sx + (target_sx - cur_sx) * k;
            if (next_sx - target_sx).abs() < 1.0 {
                let _ = self
                    .scroll_bars
                    .set_scroll_pos_no_clip(cx, dvec2(target_sx, 0.0));
                self.anim.scroll_target_x = None;
            } else {
                let _ = self
                    .scroll_bars
                    .set_scroll_pos_no_clip(cx, dvec2(next_sx, 0.0));
            }
        }

        if let Some(mut slide) = self.anim.sheet_slide.take() {
            slide.offset_y += (0.0 - slide.offset_y) * k;
            if slide.offset_y.abs() < 1.5 {
                self.hide_sheet_webviews(cx, &slide.prev_sheet_id);
            } else {
                self.anim.sheet_slide = Some(slide);
            }
        }

        self.overview.t += (target_t - self.overview.t) * k;
        self.overview.cam_scale += (target_scale - self.overview.cam_scale) * k;
        self.overview.cam_x += (target_x - self.overview.cam_x) * k;
        self.overview.cam_y += (target_y - self.overview.cam_y) * k;

        let settled = self.anim.close_gaps.is_empty()
            && self.anim.trailing_close_gap.is_none()
            && self.anim.reorder_slide.is_none()
            && self.anim.scroll_target_x.is_none()
            && self.anim.sheet_slide.is_none()
            && (self.overview.t - target_t).abs() < OVERVIEW_SNAP_T_EPS
            && (self.overview.cam_scale - target_scale).abs() < OVERVIEW_SNAP_T_EPS
            && (self.overview.cam_x - target_x).abs() < 1.5
            && (self.overview.cam_y - target_y).abs() < 1.5;

        if settled {
            self.overview.t = target_t;
            self.overview.cam_scale = target_scale;
            self.overview.cam_x = target_x;
            self.overview.cam_y = target_y;
            self.anim.animating = false;
            self.anim.last_frame_time = None;
            if self.mode != InputMode::Overview {
                cx.widget_action(self.uid, SheetStripAction::OverviewSettled);
            }
        } else {
            self.anim.next_frame = cx.new_next_frame();
        }
        self.redraw(cx);
    }

    pub(super) fn animate_koma_close(&mut self, cx: &mut Cx, koma_id: &KomaId) {
        if self.is_overview_rendered()
            || self.anim.sheet_slide.is_some()
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
            CloseGap::transfer(&mut self.anim.close_gaps, closed, next, self.koma_gap);
        } else {
            let existing = self
                .anim
                .trailing_close_gap
                .as_ref()
                .map_or(0.0, CloseGap::width);
            self.anim.trailing_close_gap = Some(CloseGap::after_removal(
                &mut self.anim.close_gaps,
                closed,
                self.koma_gap,
                existing,
            ));
            self.anim.scroll_target_x = None;
        }
        if !self.anim.animating {
            self.start_animation(cx);
        }
    }

    pub(super) fn animate_scroll_to(&mut self, cx: &mut Cx, target_x: f64) {
        let cur_x = self.scroll_bars.get_scroll_pos().x;
        if (cur_x - target_x).abs() < 1.0 {
            self.anim.scroll_target_x = None;
            return;
        }
        self.scroll_bars.stop_motion();
        self.discard_scroll_continuation = true;
        self.anim.scroll_target_x = Some(target_x);
        self.start_animation(cx);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use koma_core::model::KomaKind;

    use super::*;
    use crate::ui::sheet_strip::STRIP_SPACING;

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
}
