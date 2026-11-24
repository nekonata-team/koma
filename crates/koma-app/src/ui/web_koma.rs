//! Makepad widget presenting a single Web Koma backed by `WebViewRuntime`.

use std::cell::RefCell;
use std::rc::Rc;

use koma_core::model::KomaId;
use makepad_browser_view::{cef_input, ViewOp};
use makepad_widgets::image::DrawImage;
use makepad_widgets::*;

use crate::runtime::web_views::WebViewRuntime;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.WebKomaBase = #(WebKoma::register_widget(vm))
    mod.widgets.WebKoma = set_type_default() do mod.widgets.WebKomaBase{
        width: Fill
        height: Fill
        draw_empty +: {
            color: #x101521
        }
        // CEF frame rings already contain premultiplied pixels.
        draw_bg +: {
            pixel: fn() {
                return self.get_color() * self.opacity
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub enum WebKomaAction {
    #[default]
    None,
    Focused(KomaId),
}

#[derive(Script, ScriptHook, Widget)]
pub struct WebKoma {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[walk]
    walk: Walk,
    #[redraw]
    #[live]
    draw_bg: DrawImage,
    #[live]
    draw_empty: DrawColor,
    #[rust]
    shared_runtime: Option<Rc<RefCell<WebViewRuntime>>>,
    #[rust]
    koma_id: Option<KomaId>,
    #[rust]
    initial_url: String,
    #[rust]
    last_click_local: Option<Vec2d>,
}

impl WebKoma {
    pub fn bind_shared(
        &mut self,
        runtime: Rc<RefCell<WebViewRuntime>>,
        koma_id: KomaId,
        initial_url: String,
    ) {
        self.shared_runtime = Some(runtime);
        self.koma_id = Some(koma_id);
        self.initial_url = initial_url;
    }

    fn with_runtime<R>(&self, f: impl FnOnce(&mut WebViewRuntime) -> R) -> Option<R> {
        self.shared_runtime
            .as_ref()
            .map(|rt| f(&mut rt.borrow_mut()))
    }

    pub fn send_view_op(&self, cx: &mut Cx, op: ViewOp) -> bool {
        self.with_runtime(|rt| {
            let Some(id) = self.koma_id.as_ref().and_then(|id| rt.view_id(id)) else {
                return false;
            };
            rt.views_mut().send(id, op);
            rt.views_mut().service(cx);
            true
        }) == Some(true)
    }

    pub fn focus(&mut self, cx: &mut Cx) -> bool {
        let ready = self.draw_bg.area().is_valid(cx);
        cx.set_key_focus(self.draw_bg.area());
        self.with_runtime(|rt| rt.focus_koma(cx, self.koma_id.as_ref()));
        ready
    }

    pub fn blur(&mut self, cx: &mut Cx) {
        if cx.has_key_focus(self.draw_bg.area()) {
            cx.set_key_focus(Area::Empty);
            cx.hide_text_ime();
        }
        self.with_runtime(|rt| {
            if let Some(id) = self.koma_id.as_ref().and_then(|id| rt.view_id(id)) {
                rt.views_mut().send(id, ViewOp::Focus { on: false });
                rt.views_mut().service(cx);
            }
        });
    }

    pub fn has_input_focus(&self, cx: &Cx) -> bool {
        cx.has_key_focus(self.draw_bg.area())
    }
}

impl Widget for WebKoma {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        if let Event::Shutdown = event {
            return;
        }
        if matches!(event, Event::WindowGeomChange(_)) {
            self.redraw(cx);
        }

        if self
            .with_runtime(|rt| rt.pump_event(cx, event))
            .unwrap_or(false)
        {
            self.redraw(cx);
        }

        let Some(bid) = &self.koma_id else {
            return;
        };
        let Some(view_id) = self.with_runtime(|rt| rt.view_id(bid)).flatten() else {
            return;
        };

        let area = self.draw_bg.area();
        if !area.is_valid(cx) {
            return;
        }
        let rect = area.rect(cx);
        let hit = event.hits_with_capture_overload(cx, area, true);

        match &hit {
            Hit::KeyFocus(_) => {
                self.with_runtime(|rt| rt.focus_koma(cx, Some(bid)));
                self.redraw(cx);
            }
            Hit::KeyFocusLost(_) => {
                cx.hide_text_ime();
                self.with_runtime(|rt| {
                    rt.views_mut().send(view_id, ViewOp::CaptureLost);
                    rt.views_mut().send(view_id, ViewOp::Focus { on: false });
                });
            }
            Hit::FingerDown(e) => {
                self.last_click_local = Some(e.abs - rect.pos);
                cx.set_key_focus(area);
                self.with_runtime(|rt| rt.focus_koma(cx, Some(bid)));
                cx.widget_action(self.uid, WebKomaAction::Focused(bid.clone()));
                self.redraw(cx);
            }
            _ => {}
        }

        match &hit {
            Hit::FingerHoverOut(e) => {
                let at = e.abs - rect.pos;
                self.with_runtime(|rt| {
                    rt.views_mut().send(
                        view_id,
                        ViewOp::MouseMove {
                            x: at.x.round() as i32,
                            y: at.y.round() as i32,
                            modifiers: cef_input::cef_modifiers(e.modifiers, MouseButton::empty()),
                            leave: true,
                        },
                    );
                });
            }
            Hit::FingerUp(e) if e.cancelled => {
                self.with_runtime(|rt| {
                    rt.views_mut().send(view_id, ViewOp::CaptureLost);
                });
            }
            Hit::FingerScroll(scroll) if matches!(event, Event::Scroll(_)) => {
                if let Event::Scroll(ev) = event {
                    let mut s = scroll.clone();
                    if ev.handled_x.replace(true) {
                        s.scroll.x = 0.0;
                    }
                    if ev.handled_y.replace(true) {
                        s.scroll.y = 0.0;
                    }
                    self.with_runtime(|rt| {
                        rt.views_mut()
                            .forward_hit(cx, view_id, &Hit::FingerScroll(s), |abs| {
                                Some(abs - rect.pos)
                            });
                    });
                }
            }
            _ => {
                self.with_runtime(|rt| {
                    rt.views_mut()
                        .forward_hit(cx, view_id, &hit, |abs| Some(abs - rect.pos));
                });
            }
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        let rect = cx.peek_walk_turtle(walk);
        let dpi = cx.current_dpi_factor();
        let bid = self.koma_id.as_ref();

        let shown = bid.and_then(|bid| {
            self.with_runtime(|rt| {
                if rect.size.x > 0.0 && rect.size.y > 0.0 {
                    rt.ensure_koma(cx, bid, &self.initial_url, true, rect.size, dpi);
                    rt.set_koma_size(cx, bid, rect.size, dpi);
                }
                rt.koma_frame(bid)
            })
            .flatten()
        });

        self.draw_empty.draw_abs(cx, rect);
        match shown {
            Some(frame) if frame.info.width > 0 && frame.info.height > 0 => {
                self.draw_bg.image_scale = vec2(
                    frame.info.width as f32 / frame.alloc[0] as f32,
                    frame.info.height as f32 / frame.alloc[1] as f32,
                );
                self.draw_bg.draw_vars.set_texture(0, &frame.texture);
                self.draw_bg.opacity = 1.0;
            }
            _ => {
                self.draw_bg.image_scale = vec2(1.0, 1.0);
                self.draw_bg.draw_vars.empty_texture(0);
                self.draw_bg.opacity = 0.0;
            }
        }
        self.draw_bg.draw_walk(cx, walk);
        if self.has_input_focus(cx) {
            let ime_caret =
                bid.and_then(|bid| self.with_runtime(|rt| rt.koma_ime_caret(bid)).flatten());
            let raw_pos = match ime_caret {
                Some((x, y, _w, h)) => dvec2(x as f64, (y + h.max(16)) as f64),
                None => self.last_click_local.unwrap_or_else(|| dvec2(16.0, 24.0)),
            };
            let pos = dvec2(
                raw_pos.x.clamp(0.0, rect.size.x.max(1.0)),
                raw_pos.y.clamp(0.0, rect.size.y.max(1.0)),
            );
            cx.show_text_ime(self.draw_bg.area(), pos);
        }
        cx.add_nav_stop(self.draw_bg.area(), NavRole::TextInput, Inset::default());
        DrawStep::done()
    }
}
