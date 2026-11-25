//! Pointer drag, pinch zoom, and horizontal scroll routing for `SheetStrip`.

use koma_core::model::{InputMode, KomaId, KomaKind};
use makepad_app_terminal::widget::MpTerm;
use makepad_browser_view::ViewOp;
use makepad_widgets::*;

use crate::ui::web_koma::WebKoma;

use super::{SheetStrip, SheetStripAction, DRAG_THRESHOLD_PX, HEADER_HEIGHT};

pub(super) enum PinchTarget {
    Terminal(ContentPinch),
    Web(KomaId),
}

impl PinchTarget {
    pub(super) fn id(&self) -> &KomaId {
        match self {
            Self::Terminal(pinch) => &pinch.target,
            Self::Web(id) => id,
        }
    }
}

pub(super) struct ContentPinch {
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

#[derive(Clone, Debug)]
pub(super) struct StripDrag {
    pub(super) koma_id: KomaId,
    pub(super) start_abs: Vec2d,
    pub(super) active: bool,
    pub(super) drop_index: Option<usize>,
}

pub(super) fn is_horizontal_scroll(scroll: Vec2d) -> bool {
    scroll.x != 0.0 && (scroll.y == 0.0 || scroll.x.abs() > scroll.y.abs())
}

impl SheetStrip {
    pub fn adjust_content_zoom(&mut self, cx: &mut Cx, koma_id: &KomaId, step: i8) {
        self.end_content_pinch(cx);
        let Some(widget) = self.koma_widgets.get(koma_id) else {
            return;
        };
        let next = |factor: f64, default: f64| {
            if step == 0 {
                default
            } else {
                ((factor * 10.0).round() + f64::from(step)).clamp(5.0, 20.0) / 10.0
            }
        };
        if let Some(mut term) = widget.borrow_mut::<MpTerm>() {
            let factor = next(term.cell_size().1, self.scale.terminal);
            term.set_presentation_font_scale(cx, factor);
        } else if let Some(mut web) = widget.borrow_mut::<WebKoma>() {
            let factor = next(web.page_zoom(), self.scale.web);
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

    pub(super) fn handle_content_pinch(
        &mut self,
        cx: &mut Cx,
        event: &Event,
        phase: PinchPhase,
        scale: f64,
    ) {
        match phase {
            PinchPhase::Begin => {
                self.end_content_pinch(cx);
                if self.anim.sheet_slide.is_some() {
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

    /// Handles strip header dragging and click-to-focus. Returns `true` when an active drag
    /// consumed the event and child widgets should not receive it.
    pub(super) fn handle_strip_pointer_event(&mut self, cx: &mut Cx, event: &Event) -> bool {
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
                                let offset = self.anim.reorder_offset(id);
                                me.abs.x < rect.pos.x - offset + rect.size.x * 0.5
                            })
                            .unwrap_or(self.card_rects.len());
                        drag.drop_index = Some(index);
                        self.redraw(cx);
                        self.strip_drag = Some(drag);
                        return true;
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
                        return true;
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
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn zero_delta_scroll_does_not_count_as_horizontal_scroll() {
        assert!(!is_horizontal_scroll(dvec2(0.0, 0.0)));
        assert!(!is_horizontal_scroll(dvec2(0.0, 12.0)));
        assert!(!is_horizontal_scroll(dvec2(3.0, 12.0)));
        assert!(is_horizontal_scroll(dvec2(12.0, 0.0)));
        assert!(is_horizontal_scroll(dvec2(-12.0, 4.0)));
    }
}
