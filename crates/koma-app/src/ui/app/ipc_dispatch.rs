//! IPC request dispatch for `App` (runs on the UI thread via `SignalToUI`).

use koma_app::domain::sheet_presets::available_sheet_presets;
use koma_core::input::{
    expand_tilde, parse_open_koma_input_with_engine, resolve_web_url_with_engine, KomaInput,
};
use koma_core::model::{KomaId, KomaKind, KomaMeta, SheetId, SheetState};
use koma_ipc::protocol::{
    DrawerItemSummary, DrawerKeepArgs, IpcCommand, IpcPayload, IpcResponse, KomaFocusedData,
    KomaPlacement, KomaSummary, KomaTarget, KomaTextArgs, NotificationSendArgs,
    NotificationSummary, PresetSummary, SheetListEntry, SheetSummary, TerminalNewArgs,
    TerminalTextArgs, WebClickArgs, WebEvalArgs, WebFillArgs, WebKomaData, WebReadArgs,
    WebReloadArgs,
};
use makepad_app_terminal::widget::MpTerm;
use makepad_widgets::*;

use super::{derive_cwd_label, App};
use crate::runtime::ipc_server::PendingIpc;
use crate::runtime::web_views::is_local_html_url;
use crate::ui::sheet_strip::SheetStrip;

fn summarize_koma(
    rt: &crate::runtime::web_views::WebViewRuntime,
    sheet: &SheetState,
    koma: &KomaMeta,
) -> KomaSummary {
    let url = (koma.kind == KomaKind::Web)
        .then(|| {
            rt.koma_url_title(&koma.id)
                .map(|(url, _)| url)
                .or_else(|| koma.initial_url.clone())
        })
        .flatten();
    KomaSummary {
        id: koma.id.to_string(),
        kind: koma.kind.as_str().into(),
        label: koma.label.clone(),
        url,
        is_focused: sheet.focused_koma_id.as_ref() == Some(&koma.id),
    }
}

pub(super) fn trim_terminal_rows(mut rows: Vec<String>, lines: Option<usize>) -> String {
    while rows.last().is_some_and(|r| r.trim().is_empty()) {
        rows.pop();
    }
    if let Some(n) = lines {
        let start = rows.len().saturating_sub(n);
        return rows[start..].join("\n");
    }
    rows.join("\n")
}

impl App {
    pub(super) fn drain_ipc(&mut self, cx: &mut Cx) {
        let mut pending = Vec::new();
        if let Some(ipc) = &self.ipc {
            while let Some(item) = ipc.try_recv() {
                pending.push(item);
            }
        }
        for item in pending {
            self.handle_ipc_request(cx, item);
        }
    }

    fn handle_ipc_request(&mut self, cx: &mut Cx, pending: PendingIpc) {
        let PendingIpc { req, reply } = pending;
        let caller = req.caller_koma_id.map(KomaId::from);
        let caller = caller.as_ref();
        let create_profile = matches!(req.command, IpcCommand::ProfileNew { .. });
        match req.command {
            IpcCommand::Health => {
                let _ = reply.send(IpcResponse::ok());
            }
            IpcCommand::KomaList(args) => {
                let sheet = match args.sheet_id.as_deref().map(SheetId::from) {
                    Some(id) => self.state.sheets().iter().find(|sheet| sheet.id == id),
                    None => Some(
                        caller
                            .and_then(|id| self.state.koma_by_id(id))
                            .and_then(|koma| {
                                self.state
                                    .sheets()
                                    .iter()
                                    .find(|sheet| Some(&sheet.id) == koma.sheet_id.as_ref())
                            })
                            .unwrap_or_else(|| self.state.current_sheet()),
                    ),
                };
                let resp = match sheet {
                    Some(sheet) => {
                        let rt = self.web_runtime().borrow();
                        let komas = sheet
                            .komas
                            .iter()
                            .map(|koma| summarize_koma(&rt, sheet, koma))
                            .collect();
                        IpcResponse::Ok(IpcPayload::KomaList {
                            sheet_id: sheet.id.to_string(),
                            komas,
                        })
                    }
                    None => IpcResponse::err(format!(
                        "Sheet not found: {}",
                        args.sheet_id.unwrap_or_default()
                    )),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::KomaFocused => {
                let sheet = self.state.current_sheet();
                let koma = sheet
                    .focused_koma()
                    .map(|koma| summarize_koma(&self.web_runtime().borrow(), sheet, koma));
                let _ = reply.send(IpcResponse::Ok(IpcPayload::KomaFocused(KomaFocusedData {
                    sheet_id: sheet.id.to_string(),
                    koma,
                })));
            }
            IpcCommand::KomaExists { koma_id } => {
                let koma_id = KomaId::from(koma_id);
                let exists = self.state.koma_by_id(&koma_id).is_some();
                let _ = reply.send(IpcResponse::Ok(IpcPayload::KomaExists {
                    koma_id: koma_id.into_string(),
                    exists,
                }));
            }
            IpcCommand::KomaFocus { koma_id } => {
                let koma_id = KomaId::from(koma_id);
                let resp = if self.state.koma_by_id(&koma_id).is_some() {
                    self.state.drawer_open = false;
                    self.state.notifications_open = false;
                    self.width_mode_open = false;
                    self.state.focus_koma_by_id(&koma_id);
                    self.close_prompt(cx);
                    self.set_mode(cx, koma_core::model::InputMode::ContentInput, true);
                    IpcResponse::Ok(IpcPayload::KomaId {
                        koma_id: koma_id.into_string(),
                    })
                } else {
                    IpcResponse::err(format!("Koma not found: {koma_id}"))
                };
                let _ = reply.send(resp);
            }
            IpcCommand::KomaMove { koma_id, placement } => {
                let koma_id = KomaId::from(koma_id);
                let resp = match self.move_koma_from_ipc(cx, &koma_id, &placement) {
                    Ok(()) => IpcResponse::Ok(IpcPayload::KomaId {
                        koma_id: koma_id.into_string(),
                    }),
                    Err(err) => IpcResponse::err(err),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::KomaResize { koma_id, width } => {
                let koma_id = KomaId::from(koma_id);
                let resp = match self.state.set_koma_width(&koma_id, width) {
                    Ok(width) => {
                        self.sync_after_mutation(cx, false);
                        IpcResponse::Ok(IpcPayload::KomaResized {
                            koma_id: koma_id.into_string(),
                            width,
                        })
                    }
                    Err(err) => IpcResponse::err(err),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::KomaClose { koma_id } => {
                let koma_id = KomaId::from(koma_id);
                let resp = match self.close_koma_by_id(cx, &koma_id, false) {
                    Ok(()) => IpcResponse::Ok(IpcPayload::KomaId {
                        koma_id: koma_id.into_string(),
                    }),
                    Err(err) => IpcResponse::err(err),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::ProfileList => {
                use crate::runtime::profiles::{self, Profile};
                let resp = match profiles::root().and_then(|root| Profile::list(&root)) {
                    Ok(profiles) => IpcResponse::Ok(IpcPayload::ProfileList { profiles }),
                    Err(err) => IpcResponse::err(err),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::ProfileOpen { id } | IpcCommand::ProfileNew { id } => {
                self.open_profile(cx, id, create_profile, move |result| {
                    let _ = reply.send(match result {
                        Ok(()) => IpcResponse::ok(),
                        Err(err) => IpcResponse::err(err),
                    });
                });
            }
            IpcCommand::ProfileActivate { id } => {
                let resp = if !self.is_private && id == self.state.profile_id {
                    self.ui.window(cx, ids!(main_window)).show(cx);
                    IpcResponse::ok()
                } else {
                    IpcResponse::err("IPC endpoint belongs to a different Profile")
                };
                let _ = reply.send(resp);
            }
            IpcCommand::WorkspacePrivate => {
                let resp = match self.spawn_private_workspace(cx) {
                    Ok(()) => IpcResponse::ok(),
                    Err(e) => IpcResponse::err(e),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::SheetClose => {
                let resp = match self.close_current_sheet(cx) {
                    Ok(()) => IpcResponse::ok(),
                    Err(e) => IpcResponse::err(e),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::SheetList => {
                let sheets = self
                    .state
                    .sheets()
                    .iter()
                    .enumerate()
                    .map(|(index, sheet)| SheetListEntry {
                        id: sheet.id.to_string(),
                        label: sheet.label.clone(),
                        is_active: index == self.state.active_sheet_index(),
                        koma_count: sheet.komas.len(),
                    })
                    .collect();
                let _ = reply.send(IpcResponse::Ok(IpcPayload::SheetList { sheets }));
            }
            IpcCommand::SheetNew { label, focus } => {
                let resp = match self.state.add_named_sheet(&label, focus) {
                    Ok(sheet) => {
                        let sheet_id = sheet.id.to_string();
                        self.sync_after_mutation(cx, focus);
                        IpcResponse::Ok(IpcPayload::SheetId { sheet_id })
                    }
                    Err(err) => IpcResponse::err(err),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::PresetList => {
                let presets = available_sheet_presets(
                    &self.state.personal_presets,
                    &self.config.sheet_presets,
                )
                .map(|p| PresetSummary {
                    name: p.name.to_owned(),
                    komas: p.komas.to_vec(),
                    personal: p.personal,
                })
                .collect();
                let _ = reply.send(IpcResponse::Ok(IpcPayload::PresetList { presets }));
            }
            IpcCommand::PresetSave { name } => {
                let resp = match self.state.save_current_sheet_as_preset(&name) {
                    Ok(preset) => {
                        self.save_state(cx);
                        self.show_toast(cx, format!("Saved preset: {}", preset.name));
                        IpcResponse::Ok(IpcPayload::PresetSaved { name: preset.name })
                    }
                    Err(e) => IpcResponse::err(e),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::PresetApply { name } => {
                let resp = match self.apply_sheet_preset_by_name(cx, &name) {
                    Ok(idx) => IpcResponse::Ok(IpcPayload::PresetApplied {
                        sheet_index: idx + 1,
                    }),
                    Err(e) => IpcResponse::err(e),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::WebDevtools => {
                let resp = match self.open_devtools_for_focused_web(cx) {
                    Ok(koma) => IpcResponse::Ok(IpcPayload::WebKoma(WebKomaData {
                        koma_id: koma.id.into_string(),
                        url: koma.initial_url,
                    })),
                    Err(e) => IpcResponse::err(e),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::Overview => {
                let sheets = {
                    let rt = self.web_runtime().borrow();
                    self.state
                        .sheets()
                        .iter()
                        .enumerate()
                        .map(|(i, sheet)| SheetSummary {
                            id: sheet.id.to_string(),
                            label: sheet.label.clone(),
                            is_active: i == self.state.active_sheet_index(),
                            komas: sheet
                                .komas
                                .iter()
                                .map(|koma| summarize_koma(&rt, sheet, koma))
                                .collect(),
                        })
                        .collect()
                };
                let _ = reply.send(IpcResponse::Ok(IpcPayload::Overview { sheets }));
            }
            IpcCommand::OverviewToggle => {
                self.toggle_overview(cx);
                let _ = reply.send(IpcResponse::ok());
            }
            IpcCommand::SettingsReload => {
                let resp = match self.reload_configs(cx) {
                    Ok(()) => {
                        self.show_toast(cx, "Reloaded config");
                        IpcResponse::ok()
                    }
                    Err(err) => IpcResponse::err(err),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::SheetFocus { sheet_index } => {
                let idx = sheet_index.checked_sub(1);
                let resp = match idx {
                    Some(i) if i < self.state.sheets().len() => {
                        self.switch_sheet(cx, i);
                        IpcResponse::ok()
                    }
                    _ => IpcResponse::err(format!(
                        "Sheet index out of range (expected 1..={})",
                        self.state.sheets().len()
                    )),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::WebNew {
                url: raw_url,
                koma_api,
            } => {
                let home_dir = std::env::var("HOME").ok();
                let resp = match resolve_web_url_with_engine(
                    &raw_url,
                    home_dir.as_deref(),
                    self.config.search_engine,
                ) {
                    Ok((url, label)) => {
                        if koma_api && !is_local_html_url(&url) {
                            IpcResponse::err("--koma-api requires a local .html or .htm file")
                        } else {
                            let koma = self.create_web_koma(cx, url.clone(), Some(label));
                            if koma_api {
                                self.state.set_koma_api_enabled(&koma.id, true);
                                self.web_runtime()
                                    .borrow_mut()
                                    .set_koma_api_enabled(cx, &koma.id, true);
                                self.save_state(cx);
                            }
                            IpcResponse::Ok(IpcPayload::WebKoma(WebKomaData {
                                koma_id: koma.id.into_string(),
                                url: Some(url),
                            }))
                        }
                    }
                    Err(e) => IpcResponse::err(e),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::WebReload(WebReloadArgs { koma_id, koma_api }) => {
                let resp = match self.resolve_web_koma(koma_id, caller) {
                    Ok(koma_id) => {
                        if let Some(enable) = koma_api {
                            let current_url = self
                                .web_runtime()
                                .borrow()
                                .koma_url_title(&koma_id)
                                .map(|(u, _)| u)
                                .filter(|u| !u.is_empty())
                                .or_else(|| {
                                    self.state
                                        .koma_by_id(&koma_id)
                                        .and_then(|b| b.initial_url.clone())
                                })
                                .unwrap_or_default();
                            if enable && !is_local_html_url(&current_url) {
                                let _ = reply.send(IpcResponse::err(
                                    "--koma-api requires a local .html or .htm page",
                                ));
                                return;
                            }
                            self.state.set_koma_api_enabled(&koma_id, enable);
                            self.web_runtime()
                                .borrow_mut()
                                .set_koma_api_enabled(cx, &koma_id, enable);
                            self.save_state(cx);
                        }
                        self.web_runtime().borrow_mut().reload(cx, &koma_id);
                        IpcResponse::Ok(IpcPayload::KomaId {
                            koma_id: koma_id.into_string(),
                        })
                    }
                    Err(e) => IpcResponse::err(e),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::WebRead(WebReadArgs { koma_id, within }) => {
                match self.resolve_web_koma(koma_id, caller) {
                    Ok(koma_id) => {
                        self.web_runtime().borrow_mut().read_for_ipc(
                            cx,
                            &koma_id,
                            within.as_deref(),
                            reply,
                        );
                    }
                    Err(e) => {
                        let _ = reply.send(IpcResponse::err(e));
                    }
                }
            }
            IpcCommand::WebText(KomaTarget { koma_id }) => {
                match self.resolve_web_koma(koma_id, caller) {
                    Ok(koma_id) => {
                        self.web_runtime()
                            .borrow_mut()
                            .text_for_ipc(cx, &koma_id, reply);
                    }
                    Err(e) => {
                        let _ = reply.send(IpcResponse::err(e));
                    }
                }
            }
            IpcCommand::WebClick(WebClickArgs { koma_id, selector }) => {
                match self.resolve_web_koma(koma_id, caller) {
                    Ok(koma_id) => {
                        self.web_runtime()
                            .borrow_mut()
                            .click_for_ipc(cx, &koma_id, &selector, reply);
                    }
                    Err(e) => {
                        let _ = reply.send(IpcResponse::err(e));
                    }
                }
            }
            IpcCommand::WebFill(WebFillArgs {
                koma_id,
                selector,
                text,
            }) => match self.resolve_web_koma(koma_id, caller) {
                Ok(koma_id) => {
                    self.web_runtime()
                        .borrow_mut()
                        .fill_for_ipc(cx, &koma_id, &selector, &text, reply);
                }
                Err(e) => {
                    let _ = reply.send(IpcResponse::err(e));
                }
            },
            IpcCommand::DrawerList => {
                let items = self
                    .state
                    .drawer
                    .iter()
                    .map(|b| DrawerItemSummary {
                        id: b.id.to_string(),
                        kind: b.kind.as_str().into(),
                        label: b.label.clone(),
                        url: b.initial_url.clone(),
                        cwd: b.cwd.clone(),
                    })
                    .collect();
                let _ = reply.send(IpcResponse::Ok(IpcPayload::DrawerList { items }));
            }
            IpcCommand::DrawerKeep(DrawerKeepArgs {
                input,
                title,
                koma_api,
            }) => {
                let home_dir = std::env::var("HOME").ok();
                let expanded = match self.config.expand_alias(&input) {
                    Ok(Some(v)) => v,
                    Ok(None) => input.clone(),
                    Err(err) => {
                        let _ = reply.send(IpcResponse::err(err));
                        return;
                    }
                };
                let resp = match parse_open_koma_input_with_engine(
                    &expanded,
                    home_dir.as_deref(),
                    self.config.search_engine,
                ) {
                    Ok(KomaInput::Terminal { cwd, .. }) => {
                        let label = title.unwrap_or_else(|| derive_cwd_label(cwd.as_deref()));
                        let item = self.state.keep_koma_in_drawer(
                            KomaKind::Terminal,
                            label,
                            None,
                            cwd,
                            false,
                        );
                        self.sync_after_mutation(cx, false);
                        IpcResponse::Ok(IpcPayload::KomaId {
                            koma_id: item.id.into_string(),
                        })
                    }
                    Ok(KomaInput::Web { url, label }) => {
                        if koma_api && !is_local_html_url(&url) {
                            IpcResponse::err("--koma-api requires a local .html or .htm file")
                        } else {
                            let item = self.state.keep_koma_in_drawer(
                                KomaKind::Web,
                                title.unwrap_or(label),
                                Some(url.clone()),
                                None,
                                koma_api,
                            );
                            if koma_api {
                                self.web_runtime()
                                    .borrow_mut()
                                    .set_koma_api_enabled(cx, &item.id, true);
                            }
                            self.sync_after_mutation(cx, false);
                            IpcResponse::Ok(IpcPayload::WebKoma(WebKomaData {
                                koma_id: item.id.into_string(),
                                url: Some(url),
                            }))
                        }
                    }
                    Err(e) => IpcResponse::err(e),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::DrawerPlace(KomaTarget { koma_id }) => {
                let resp = match self.place_drawer_item(cx, koma_id.map(KomaId::from).as_ref()) {
                    Ok(koma) => IpcResponse::Ok(IpcPayload::KomaId {
                        koma_id: koma.id.into_string(),
                    }),
                    Err(e) => IpcResponse::err(e),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::DrawerDiscard(KomaTarget { koma_id }) => {
                let resp = match self.discard_drawer_item(cx, koma_id.map(KomaId::from).as_ref()) {
                    Ok(item) => IpcResponse::Ok(IpcPayload::KomaId {
                        koma_id: item.id.into_string(),
                    }),
                    Err(e) => IpcResponse::err(e),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::DrawerRestore => {
                let resp = match self.restore_discarded_drawer_item(cx) {
                    Ok(item) => IpcResponse::Ok(IpcPayload::KomaId {
                        koma_id: item.id.into_string(),
                    }),
                    Err(e) => IpcResponse::err(e),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::NotificationSend(NotificationSendArgs {
                koma_id,
                title,
                body,
            }) => {
                let source = koma_id
                    .map(KomaId::from)
                    .or_else(|| caller.cloned())
                    .or_else(|| self.state.focused_koma().map(|b| b.id.clone()))
                    .unwrap_or_else(|| "system".into());
                let title = title.unwrap_or_else(|| "Koma".into());
                let entry = self.state.push_notification(source, title.clone(), &body);
                self.render_notifications_overlay(cx);
                self.update_status(cx);
                self.show_notification_toast(cx, &entry);
                let _ = reply.send(IpcResponse::Ok(IpcPayload::NotificationSent {
                    koma_id: entry.koma_id.into_string(),
                    notification_id: entry.id,
                }));
            }
            IpcCommand::NotificationList { unread_only } => {
                let notifications = self
                    .state
                    .notifications
                    .iter()
                    .filter(|n| !unread_only || n.unread)
                    .map(|n| NotificationSummary {
                        id: n.id.clone(),
                        koma_id: n.koma_id.to_string(),
                        title: n.title.clone(),
                        body: n.body.clone(),
                        unread: n.unread,
                    })
                    .collect();
                let _ = reply.send(IpcResponse::Ok(IpcPayload::NotificationList {
                    notifications,
                }));
            }
            IpcCommand::NotificationRead { id } => {
                self.handle_notification_mark(cx, &id, false, reply);
            }
            IpcCommand::NotificationUnread { id } => {
                self.handle_notification_mark(cx, &id, true, reply);
            }
            IpcCommand::TerminalNew(TerminalNewArgs { cwd, run }) => {
                if run
                    .as_deref()
                    .is_some_and(|command| command.trim().is_empty())
                {
                    let _ = reply.send(IpcResponse::err("Terminal command must not be empty"));
                    return;
                }
                let home_dir = std::env::var("HOME").ok();
                let cwd = cwd
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|p| expand_tilde(p, home_dir.as_deref()));
                let koma = self.create_terminal_koma(cx, cwd);
                let resp = match run {
                    Some(command) => match self.with_terminal_from_ipc(cx, &koma.id, |cx, term| {
                        term.run_command(cx, &command)
                    }) {
                        Ok(()) => IpcResponse::Ok(IpcPayload::KomaId {
                            koma_id: koma.id.into_string(),
                        }),
                        Err(err) => IpcResponse::err(format!(
                            "Koma {} created, initial command failed: {err}",
                            koma.id
                        )),
                    },
                    None => IpcResponse::Ok(IpcPayload::KomaId {
                        koma_id: koma.id.into_string(),
                    }),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::TerminalRun(args) => {
                let _ = reply.send(self.terminal_input(cx, args, caller, MpTerm::run_command));
            }
            IpcCommand::TerminalPress(args) => {
                let _ = reply.send(self.terminal_input(cx, args, caller, MpTerm::press_key));
            }
            IpcCommand::TerminalKill(args) => {
                let _ = reply.send(self.terminal_input(cx, args, caller, |term, _, signal| {
                    term.signal_foreground(signal)
                }));
            }
            IpcCommand::WebUrl(KomaTarget { koma_id }) => {
                let resp = match self.resolve_web_koma(koma_id, caller) {
                    Ok(koma_id) => {
                        let (url, title) = self
                            .web_runtime()
                            .borrow()
                            .koma_url_title(&koma_id)
                            .unwrap_or_else(|| {
                                let fallback = self
                                    .state
                                    .koma_by_id(&koma_id)
                                    .and_then(|b| b.initial_url.clone())
                                    .unwrap_or_default();
                                (fallback, String::new())
                            });
                        IpcResponse::Ok(IpcPayload::WebUrl {
                            koma_id: koma_id.into_string(),
                            url,
                            title,
                        })
                    }
                    Err(e) => IpcResponse::err(e),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::WebEval(WebEvalArgs { koma_id, script }) => {
                match self.resolve_web_koma(koma_id, caller) {
                    Ok(koma_id) => {
                        self.web_runtime()
                            .borrow_mut()
                            .eval_for_ipc(cx, &koma_id, &script, reply);
                    }
                    Err(e) => {
                        let _ = reply.send(IpcResponse::err(e));
                    }
                }
            }
            IpcCommand::WebScriptTarget(KomaTarget { koma_id }) => {
                match self.resolve_web_koma(koma_id, caller) {
                    Ok(koma_id) => self
                        .web_runtime()
                        .borrow_mut()
                        .script_target_for_ipc(cx, koma_id, reply),
                    Err(err) => {
                        let _ = reply.send(IpcResponse::err(err));
                    }
                }
            }
            IpcCommand::WebSnapshot(KomaTarget { koma_id }) => {
                match self.resolve_web_koma(koma_id, caller) {
                    Ok(koma_id) => {
                        self.web_runtime()
                            .borrow_mut()
                            .snapshot_for_ipc(cx, &koma_id, reply);
                    }
                    Err(e) => {
                        let _ = reply.send(IpcResponse::err(e));
                    }
                }
            }
            IpcCommand::WebType(KomaTextArgs { koma_id, text }) => {
                let resp = match self.resolve_web_koma(koma_id, caller) {
                    Ok(koma_id) => match self
                        .web_runtime()
                        .borrow_mut()
                        .commit_text(cx, &koma_id, &text)
                    {
                        Ok(()) => IpcResponse::Ok(IpcPayload::KomaId {
                            koma_id: koma_id.into_string(),
                        }),
                        Err(e) => IpcResponse::err(e),
                    },
                    Err(e) => IpcResponse::err(e),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::BenchSpawnWebs { count } => {
                let mut rt = self.web_runtime().borrow_mut();
                let mut total = rt.view_count();
                let mut err = None;
                for i in 1..=count {
                    let bid = KomaId::from(format!("bench-web-{i}"));
                    let url = if i % 2 == 1 {
                        "https://example.com"
                    } else {
                        "https://makepad.nl"
                    };
                    match rt.open_extra_koma(cx, &bid, url) {
                        Ok(n) => total = n,
                        Err(e) => {
                            err = Some(e);
                            break;
                        }
                    }
                }
                let resp = match err {
                    Some(e) => IpcResponse::err(e),
                    None => IpcResponse::Ok(IpcPayload::BenchSpawnWebs { total_views: total }),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::TerminalText(TerminalTextArgs { koma_id, lines }) => {
                let resp = match self.state.resolve_koma(
                    koma_id.map(KomaId::from).as_ref(),
                    caller,
                    KomaKind::Terminal,
                ) {
                    Ok(koma) => {
                        let koma_id = koma.id.clone();
                        let fetch_lines = lines.map(|n| n.saturating_add(80));
                        let rows = self
                            .ui
                            .widget(cx, ids!(sheet_strip))
                            .borrow::<SheetStrip>()
                            .and_then(|strip| strip.terminal_rows(&koma_id, fetch_lines));
                        match rows {
                            Some(rows) => IpcResponse::Ok(IpcPayload::Text {
                                koma_id: koma_id.into_string(),
                                text: trim_terminal_rows(rows, lines),
                            }),
                            None => IpcResponse::err(format!("Terminal not ready: {koma_id}")),
                        }
                    }
                    Err(e) => IpcResponse::err(e),
                };
                let _ = reply.send(resp);
            }
            IpcCommand::TerminalSend(KomaTextArgs { koma_id, text }) => {
                let resp = match self.state.resolve_koma(
                    koma_id.map(KomaId::from).as_ref(),
                    caller,
                    KomaKind::Terminal,
                ) {
                    Ok(koma) => {
                        let koma_id = koma.id.clone();
                        let sent = self
                            .ui
                            .widget(cx, ids!(sheet_strip))
                            .borrow_mut::<SheetStrip>()
                            .is_some_and(|mut strip| strip.send_terminal_text(&koma_id, &text));
                        if sent {
                            IpcResponse::Ok(IpcPayload::KomaId {
                                koma_id: koma_id.into_string(),
                            })
                        } else {
                            IpcResponse::err(format!("Terminal not ready: {koma_id}"))
                        }
                    }
                    Err(e) => IpcResponse::err(e),
                };
                let _ = reply.send(resp);
            }
        }
    }

    fn terminal_input(
        &self,
        cx: &mut Cx,
        args: KomaTextArgs,
        caller: Option<&KomaId>,
        operation: impl FnOnce(&mut MpTerm, &mut Cx, &str) -> Result<(), String>,
    ) -> IpcResponse {
        let result = self
            .state
            .resolve_koma(
                args.koma_id.map(KomaId::from).as_ref(),
                caller,
                KomaKind::Terminal,
            )
            .and_then(|koma| {
                self.with_terminal_from_ipc(cx, &koma.id, |cx, term| {
                    operation(term, cx, &args.text)
                })?;
                Ok(IpcPayload::KomaId {
                    koma_id: koma.id.to_string(),
                })
            });
        match result {
            Ok(payload) => IpcResponse::Ok(payload),
            Err(err) => IpcResponse::err(err),
        }
    }

    fn with_terminal_from_ipc(
        &self,
        cx: &mut Cx,
        koma_id: &KomaId,
        operation: impl FnOnce(&mut Cx, &mut MpTerm) -> Result<(), String>,
    ) -> Result<(), String> {
        let widget = self.ui.widget(cx, ids!(sheet_strip));
        let strip = widget
            .borrow::<SheetStrip>()
            .ok_or("Sheet Strip unavailable")?;
        strip.with_terminal(koma_id, |term| operation(cx, term))
    }

    fn move_koma_from_ipc(
        &mut self,
        cx: &mut Cx,
        koma_id: &KomaId,
        placement: &KomaPlacement,
    ) -> Result<(), String> {
        if self.state.koma_by_id(koma_id).is_none() {
            return Err(format!("Koma not found: {koma_id}"));
        }
        let mut sheets = self.state.sheets().iter().enumerate();
        let (sheet_index, koma_index) = match placement {
            KomaPlacement::EndOf(id) => {
                let id = SheetId::from(id.as_str());
                sheets
                    .find(|(_, sheet)| sheet.id == id)
                    .map(|(index, sheet)| (index, sheet.komas.len()))
                    .ok_or_else(|| format!("Sheet not found: {id}"))?
            }
            KomaPlacement::LeftOf(id) | KomaPlacement::RightOf(id) => {
                let id = KomaId::from(id.as_str());
                let offset = usize::from(matches!(placement, KomaPlacement::RightOf(_)));
                sheets
                    .find_map(|(index, sheet)| {
                        sheet
                            .komas
                            .iter()
                            .position(|koma| koma.id == id)
                            .map(|position| (index, position + offset))
                    })
                    .ok_or_else(|| format!("Koma not found: {id}"))?
            }
        };
        if self
            .state
            .move_koma_without_focusing(koma_id, sheet_index, koma_index)
        {
            self.sync_after_mutation(cx, false);
        }
        Ok(())
    }

    fn handle_notification_mark(
        &mut self,
        cx: &mut Cx,
        id: &str,
        unread: bool,
        reply: std::sync::mpsc::SyncSender<IpcResponse>,
    ) {
        let resp = match self.state.mark_notification_read(id, unread) {
            Ok(()) => {
                self.render_notifications_overlay(cx);
                self.update_status(cx);
                IpcResponse::ok()
            }
            Err(e) => IpcResponse::err(e),
        };
        let _ = reply.send(resp);
    }

    fn resolve_web_koma(
        &self,
        koma_id: Option<String>,
        caller_koma_id: Option<&KomaId>,
    ) -> Result<KomaId, String> {
        self.state
            .resolve_koma(
                koma_id.map(KomaId::from).as_ref(),
                caller_koma_id,
                KomaKind::Web,
            )
            .map(|b| b.id.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::trim_terminal_rows;

    #[test]
    fn trim_terminal_rows_drops_trailing_blanks_before_tail_slice() {
        let rows = vec![
            "line 1".into(),
            "line 2".into(),
            "line 3".into(),
            "".into(),
            "   ".into(),
            "".into(),
        ];
        assert_eq!(trim_terminal_rows(rows.clone(), Some(2)), "line 2\nline 3");
        assert_eq!(trim_terminal_rows(rows, None), "line 1\nline 2\nline 3");
    }
}
