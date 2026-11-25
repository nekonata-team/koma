//! Workspace synchronization, child Koma widget lifecycle, focus management, and terminal access.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use koma_app::config::CenterFocused;
use koma_app::ghostty_config::{CellHeight, TerminalSettings};
use koma_core::model::{InputMode, KomaId, KomaKind, KomaMeta, SheetId, WorkspaceState};
use makepad_app_terminal::term::color::Rgb;
use makepad_app_terminal::widget::{MpTerm, TerminalColors};
use makepad_widgets::*;

use crate::runtime::web_views::WebViewRuntime;
use crate::ui::web_koma::WebKoma;

use super::animation::{koma_start_x, ReorderSlide, SheetSlide};
use super::{KomaSlot, SheetSnapshot, SheetStrip, OVERVIEW_RENDER_EPS};

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
            self.anim.reorder_slide = same_sheet
                .filter(|_| {
                    !in_overview
                        && !self.is_overview_rendered()
                        && self.anim.sheet_slide.is_none()
                        && self.last_viewport.size.x > 1.0
                        && old_focused_anchor.is_none()
                })
                .and_then(|(old, new)| {
                    ReorderSlide::between(
                        &old.komas,
                        new.komas.iter().map(|koma| &koma.id),
                        self.koma_gap,
                        &self.anim.close_gaps,
                        self.anim.reorder_slide.as_ref(),
                    )
                });
            trigger_slide = self.anim.reorder_slide.is_some();
            self.anim.close_gaps.clear();
            self.anim.trailing_close_gap = None;
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
                    self.anim.sheet_slide = Some(SheetSlide {
                        prev_sheet_id: prev_id,
                        prev_scroll_x,
                        offset_y: direction * self.last_viewport.size.y,
                        direction,
                    });
                    trigger_slide = true;
                } else {
                    self.anim.sheet_slide = None;
                }
            }
        } else if in_overview {
            self.anim.sheet_slide = None;
        }

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
            self.anim.scroll_target_x = None;
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
                    self.anim.scroll_target_x = None;
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
                    self.anim.scroll_target_x = None;
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
            self.overview.cam_scale = 1.0;
            self.overview.cam_x = scroll_x + vp.x * 0.5;
            self.overview.cam_y = self.sheet_y_top(prev_active, vp.y) + vp.y * 0.5;
            self.overview.cam_pan_offset = dvec2(0.0, 0.0);
            self.overview.t = self.overview.t.max(0.005);
            self.start_animation(cx);
        } else if prev_mode == InputMode::Overview && state.mode != InputMode::Overview {
            self.overview.clear_interaction();
            if self.overview.t > OVERVIEW_RENDER_EPS {
                self.start_animation(cx);
            }
        } else if state.mode == InputMode::Overview {
            self.overview.cam_pan_offset = dvec2(0.0, 0.0);
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
}
