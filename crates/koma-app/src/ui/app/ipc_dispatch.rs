//! IPC request dispatch for `App` (runs on the UI thread via `SignalToUI`).

use std::sync::mpsc::SyncSender;

use koma_app::domain::sheet_presets::available_sheet_presets;
use koma_core::input::{
    expand_tilde, parse_open_koma_input_with_engine, resolve_web_url_with_engine, KomaInput,
};
use koma_core::model::{InsertKomaOptions, Koma, KomaContent, KomaId, SheetId, SheetState};
use koma_ipc::protocol::{
    DrawerCommand, DrawerItemContent, DrawerItemSummary, DrawerKeepArgs, IpcCommand, IpcPayload,
    IpcResponse, KomaCommand, KomaFocusedData, KomaPlacement, KomaSummary, KomaSummaryContent,
    KomaTarget, KomaTextArgs, NotificationCommand, NotificationListArgs, NotificationSendArgs,
    NotificationSummary, PresetSummary, SheetCloseArgs, SheetCommand, SheetListEntry, SheetSummary,
    TerminalCommand, TerminalNewArgs, TerminalTextArgs, WebClickArgs, WebCommand, WebEvalArgs,
    WebFillArgs, WebKomaData, WebNavigateArgs, WebNewArgs, WebReadArgs, WebReloadArgs,
    WebSnapshotArgs, WorkspaceCommand, WorkspaceEndpoint,
};
use makepad_app_terminal::widget::MpTerm;
use makepad_widgets::*;

use super::{derive_cwd_label, home_dir_string, latest_web_url_with, App};
use crate::runtime::ipc_server::PendingIpc;
use crate::runtime::profiles::{self, Profile};
use crate::runtime::web_views::{is_local_html_url, WebViewRuntime};

type IpcReply = SyncSender<IpcResponse>;

fn send_result(reply: IpcReply, result: Result<IpcPayload, String>) {
    let _ = reply.send(match result {
        Ok(payload) => IpcResponse::Ok(payload),
        Err(err) => IpcResponse::err(err),
    });
}

fn validate_koma_api_url(enabled: bool, url: &str) -> Result<(), String> {
    if enabled && !is_local_html_url(url) {
        return Err("--koma-api requires a local .html or .htm file".into());
    }
    Ok(())
}

fn summarize_koma(rt: &WebViewRuntime, sheet: &SheetState, koma: &Koma) -> KomaSummary {
    let content = match &koma.content {
        KomaContent::Terminal { .. } => KomaSummaryContent::Terminal,
        KomaContent::Web { initial_url, .. } => KomaSummaryContent::Web {
            url: latest_web_url_with(rt, koma).unwrap_or_else(|| initial_url.clone()),
        },
    };
    KomaSummary {
        id: koma.id.to_string(),
        label: koma.label.clone(),
        content,
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
        match req.command {
            IpcCommand::Koma(cmd) => send_result(reply, self.dispatch_koma(cx, cmd, caller)),
            IpcCommand::Sheet(cmd) => send_result(reply, self.dispatch_sheet(cx, cmd)),
            IpcCommand::Web(cmd) => self.dispatch_web(cx, cmd, caller, reply),
            IpcCommand::Terminal(cmd) => {
                send_result(reply, self.dispatch_terminal(cx, cmd, caller));
            }
            IpcCommand::Drawer(cmd) => send_result(reply, self.dispatch_drawer(cx, cmd)),
            IpcCommand::Notification(cmd) => {
                send_result(reply, self.dispatch_notification(cx, cmd, caller));
            }
            IpcCommand::Workspace(cmd) => self.dispatch_workspace(cx, cmd, reply),
        }
    }

    fn dispatch_koma(
        &mut self,
        cx: &mut Cx,
        cmd: KomaCommand,
        caller: Option<&KomaId>,
    ) -> Result<IpcPayload, String> {
        match cmd {
            KomaCommand::List(args) => {
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
                }
                .ok_or_else(|| format!("Sheet not found: {}", args.sheet_id.unwrap_or_default()))?;
                let rt = self.web_runtime().borrow();
                let komas = sheet
                    .komas
                    .iter()
                    .map(|koma| summarize_koma(&rt, sheet, koma))
                    .collect();
                Ok(IpcPayload::KomaList {
                    sheet_id: sheet.id.to_string(),
                    komas,
                })
            }
            KomaCommand::Focused => {
                let sheet = self.state.current_sheet();
                let koma = sheet
                    .focused_koma()
                    .map(|koma| summarize_koma(&self.web_runtime().borrow(), sheet, koma));
                Ok(IpcPayload::KomaFocused(KomaFocusedData {
                    sheet_id: sheet.id.to_string(),
                    koma,
                }))
            }
            KomaCommand::Exists { koma_id } => {
                let koma_id = KomaId::from(koma_id);
                let exists = self.state.koma_by_id(&koma_id).is_some();
                Ok(IpcPayload::KomaExists {
                    koma_id: koma_id.into_string(),
                    exists,
                })
            }
            KomaCommand::Focus { koma_id } => {
                let koma_id = KomaId::from(koma_id);
                if self.state.koma_by_id(&koma_id).is_none() {
                    return Err(format!("Koma not found: {koma_id}"));
                }
                self.state.drawer_open = false;
                self.state.notifications_open = false;
                self.width_mode_open = false;
                self.state.focus_koma_by_id(&koma_id);
                self.close_prompt(cx);
                self.set_mode(cx, koma_core::model::InputMode::ContentInput, true);
                Ok(IpcPayload::KomaId {
                    koma_id: koma_id.into_string(),
                })
            }
            KomaCommand::Move { koma_id, placement } => {
                let koma_id = KomaId::from(koma_id);
                self.move_koma_from_ipc(cx, &koma_id, &placement)?;
                Ok(IpcPayload::KomaId {
                    koma_id: koma_id.into_string(),
                })
            }
            KomaCommand::Resize { koma_id, width } => {
                let koma_id = KomaId::from(koma_id);
                let width = self.state.set_koma_width(&koma_id, width)?;
                self.sync_after_mutation(cx, false);
                Ok(IpcPayload::KomaResized {
                    koma_id: koma_id.into_string(),
                    width,
                })
            }
            KomaCommand::Close { koma_id } => {
                let koma_id = KomaId::from(koma_id);
                self.close_koma_by_id(cx, &koma_id, false)?;
                Ok(IpcPayload::KomaId {
                    koma_id: koma_id.into_string(),
                })
            }
        }
    }

    fn dispatch_sheet(&mut self, cx: &mut Cx, cmd: SheetCommand) -> Result<IpcPayload, String> {
        match cmd {
            SheetCommand::Close(SheetCloseArgs { sheet_id, force }) => {
                if let Some(id) = sheet_id {
                    self.close_sheet_by_id(cx, &SheetId::from(id), force)?;
                } else {
                    self.close_current_sheet(cx)?;
                }
                Ok(IpcPayload::Ack)
            }
            SheetCommand::List => {
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
                Ok(IpcPayload::SheetList { sheets })
            }
            SheetCommand::New { label, focus } => {
                let sheet = self.state.add_named_sheet(&label, focus)?;
                let sheet_id = sheet.id.to_string();
                self.sync_after_mutation(cx, focus);
                Ok(IpcPayload::SheetId { sheet_id })
            }
            SheetCommand::Focus { sheet_index } => {
                let idx = sheet_index
                    .checked_sub(1)
                    .filter(|&i| i < self.state.sheets().len())
                    .ok_or_else(|| {
                        format!(
                            "Sheet index out of range (expected 1..={})",
                            self.state.sheets().len()
                        )
                    })?;
                self.switch_sheet(cx, idx);
                Ok(IpcPayload::Ack)
            }
            SheetCommand::PresetList => {
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
                Ok(IpcPayload::PresetList { presets })
            }
            SheetCommand::PresetSave { name } => {
                let preset = self.state.save_current_sheet_as_preset(&name)?;
                self.save_state(cx);
                self.show_toast(cx, format!("Saved preset: {}", preset.name));
                Ok(IpcPayload::PresetSaved { name: preset.name })
            }
            SheetCommand::PresetApply { name } => {
                let idx = self.apply_sheet_preset_by_name(cx, &name)?;
                Ok(IpcPayload::PresetApplied {
                    sheet_index: idx + 1,
                })
            }
            SheetCommand::Overview => {
                let rt = self.web_runtime().borrow();
                let sheets = self
                    .state
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
                    .collect();
                Ok(IpcPayload::Overview { sheets })
            }
            SheetCommand::OverviewToggle => {
                self.toggle_overview(cx);
                Ok(IpcPayload::Ack)
            }
        }
    }

    fn dispatch_web(
        &mut self,
        cx: &mut Cx,
        cmd: WebCommand,
        caller: Option<&KomaId>,
        reply: IpcReply,
    ) {
        match cmd {
            WebCommand::Devtools => {
                send_result(
                    reply,
                    self.open_devtools_for_focused_web(cx).map(|koma| {
                        let url = koma.initial_url().map(str::to_owned);
                        IpcPayload::WebKoma(WebKomaData {
                            koma_id: koma.id.into_string(),
                            url,
                        })
                    }),
                );
            }
            WebCommand::New(WebNewArgs {
                url: raw_url,
                koma_api,
                placement,
                width,
                focus,
            }) => {
                let result = (|| {
                    let home_dir = home_dir_string();
                    let (url, label) = resolve_web_url_with_engine(
                        &raw_url,
                        home_dir.as_deref(),
                        self.config.search_engine,
                    )?;
                    validate_koma_api_url(koma_api, &url)?;
                    let target = placement
                        .as_ref()
                        .map(|p| self.resolve_placement(p))
                        .transpose()?;
                    let opts = InsertKomaOptions {
                        target,
                        width,
                        focus: (focus || placement.is_some()).then_some(focus),
                    };
                    let koma = self.create_koma_with_options(
                        cx,
                        KomaInput::Web {
                            url: url.clone(),
                            label,
                        },
                        None,
                        caller,
                        opts,
                    )?;
                    if koma_api {
                        self.state.set_koma_api_enabled(&koma.id, true);
                        self.web_runtime()
                            .borrow_mut()
                            .set_koma_api_enabled(cx, &koma.id, true);
                        self.save_state(cx);
                    }
                    Ok(IpcPayload::WebKoma(WebKomaData {
                        koma_id: koma.id.into_string(),
                        url: Some(url),
                    }))
                })();
                send_result(reply, result);
            }
            WebCommand::Navigate(WebNavigateArgs {
                koma_id,
                url: raw_url,
            }) => {
                let result = (|| {
                    let koma_id = self.resolve_web_koma(koma_id, caller)?;
                    let home_dir = home_dir_string();
                    let (url, label) = resolve_web_url_with_engine(
                        &raw_url,
                        home_dir.as_deref(),
                        self.config.search_engine,
                    )?;
                    if !self.web_runtime().borrow_mut().navigate(cx, &koma_id, &url) {
                        return Err(format!("Web view not open: {koma_id}"));
                    }
                    self.state
                        .update_koma_url(&koma_id, url.clone(), Some(label));
                    self.sync_after_mutation(cx, false);
                    Ok(IpcPayload::WebKoma(WebKomaData {
                        koma_id: koma_id.into_string(),
                        url: Some(url),
                    }))
                })();
                send_result(reply, result);
            }
            WebCommand::Back(KomaTarget { koma_id }) => {
                let result = self.resolve_web_koma(koma_id, caller).and_then(|koma_id| {
                    if !self.web_runtime().borrow_mut().go_back(cx, &koma_id) {
                        return Err(format!("Web view not open: {koma_id}"));
                    }
                    Ok(IpcPayload::KomaId {
                        koma_id: koma_id.into_string(),
                    })
                });
                send_result(reply, result);
            }
            WebCommand::Forward(KomaTarget { koma_id }) => {
                let result = self.resolve_web_koma(koma_id, caller).and_then(|koma_id| {
                    if !self.web_runtime().borrow_mut().go_forward(cx, &koma_id) {
                        return Err(format!("Web view not open: {koma_id}"));
                    }
                    Ok(IpcPayload::KomaId {
                        koma_id: koma_id.into_string(),
                    })
                });
                send_result(reply, result);
            }
            WebCommand::Reload(WebReloadArgs { koma_id, koma_api }) => {
                let result = (|| {
                    let koma_id = self.resolve_web_koma(koma_id, caller)?;
                    if let Some(enable) = koma_api {
                        let current_url = self.latest_web_url(&koma_id).unwrap_or_default();
                        validate_koma_api_url(enable, &current_url)?;
                        self.state.set_koma_api_enabled(&koma_id, enable);
                        self.web_runtime()
                            .borrow_mut()
                            .set_koma_api_enabled(cx, &koma_id, enable);
                        self.save_state(cx);
                    }
                    self.web_runtime().borrow_mut().reload(cx, &koma_id);
                    Ok(IpcPayload::KomaId {
                        koma_id: koma_id.into_string(),
                    })
                })();
                send_result(reply, result);
            }
            WebCommand::Read(WebReadArgs { koma_id, within }) => {
                self.with_resolved_web_koma(koma_id, caller, reply, |rt, id, reply| {
                    rt.read_for_ipc(cx, &id, within.as_deref(), reply);
                });
            }
            WebCommand::Text(KomaTarget { koma_id }) => {
                self.with_resolved_web_koma(koma_id, caller, reply, |rt, id, reply| {
                    rt.text_for_ipc(cx, &id, reply);
                });
            }
            WebCommand::Click(WebClickArgs { koma_id, selector }) => {
                self.with_resolved_web_koma(koma_id, caller, reply, |rt, id, reply| {
                    rt.click_for_ipc(cx, &id, &selector, reply);
                });
            }
            WebCommand::Fill(WebFillArgs {
                koma_id,
                selector,
                text,
            }) => {
                self.with_resolved_web_koma(koma_id, caller, reply, |rt, id, reply| {
                    rt.fill_for_ipc(cx, &id, &selector, &text, reply);
                });
            }
            WebCommand::Url(KomaTarget { koma_id }) => {
                let result = self.resolve_web_koma(koma_id, caller).map(|koma_id| {
                    let (url, title) = self.latest_web_url_title(&koma_id);
                    IpcPayload::WebUrl {
                        koma_id: koma_id.into_string(),
                        url,
                        title,
                    }
                });
                send_result(reply, result);
            }
            WebCommand::Eval(WebEvalArgs { koma_id, script }) => {
                self.with_resolved_web_koma(koma_id, caller, reply, |rt, id, reply| {
                    rt.eval_for_ipc(cx, &id, &script, reply);
                });
            }
            WebCommand::ScriptTarget(KomaTarget { koma_id }) => {
                self.with_resolved_web_koma(koma_id, caller, reply, |rt, id, reply| {
                    rt.script_target_for_ipc(cx, id, reply);
                });
            }
            WebCommand::Snapshot(WebSnapshotArgs {
                koma_id,
                within,
                full,
            }) => {
                self.with_resolved_web_koma(koma_id, caller, reply, |rt, id, reply| {
                    rt.snapshot_for_ipc(cx, &id, within.as_deref(), full, reply);
                });
            }
            WebCommand::Type(KomaTextArgs { koma_id, text }) => {
                let result = self.resolve_web_koma(koma_id, caller).and_then(|koma_id| {
                    self.web_runtime()
                        .borrow_mut()
                        .commit_text(cx, &koma_id, &text)?;
                    Ok(IpcPayload::KomaId {
                        koma_id: koma_id.into_string(),
                    })
                });
                send_result(reply, result);
            }
            WebCommand::Press(KomaTextArgs { koma_id, text }) => {
                self.with_resolved_web_koma(koma_id, caller, reply, |rt, id, reply| {
                    rt.press_for_ipc(cx, &id, &text, reply);
                });
            }
            WebCommand::Scroll(KomaTextArgs { koma_id, text }) => {
                self.with_resolved_web_koma(koma_id, caller, reply, |rt, id, reply| {
                    rt.scroll_for_ipc(cx, &id, &text, reply);
                });
            }
            WebCommand::BenchSpawnWebs { count } => {
                let mut rt = self.web_runtime().borrow_mut();
                let mut total = rt.view_count();
                let result = (|| {
                    for i in 1..=count {
                        let bid = KomaId::from(format!("bench-web-{i}"));
                        let url = if i % 2 == 1 {
                            "https://example.com"
                        } else {
                            "https://makepad.nl"
                        };
                        total = rt.open_extra_koma(cx, &bid, url)?;
                    }
                    Ok(IpcPayload::BenchSpawnWebs { total_views: total })
                })();
                send_result(reply, result);
            }
        }
    }

    fn dispatch_terminal(
        &mut self,
        cx: &mut Cx,
        cmd: TerminalCommand,
        caller: Option<&KomaId>,
    ) -> Result<IpcPayload, String> {
        match cmd {
            TerminalCommand::New(TerminalNewArgs {
                cwd,
                run,
                placement,
                width,
                focus,
            }) => {
                if run
                    .as_deref()
                    .is_some_and(|command| command.trim().is_empty())
                {
                    return Err("Terminal command must not be empty".into());
                }
                let home_dir = home_dir_string();
                let cwd = cwd
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|p| expand_tilde(p, home_dir.as_deref()));
                let target = placement
                    .as_ref()
                    .map(|p| self.resolve_placement(p))
                    .transpose()?;
                let opts = InsertKomaOptions {
                    target,
                    width,
                    focus: (focus || placement.is_some()).then_some(focus),
                };
                let koma = self.create_koma_with_options(
                    cx,
                    KomaInput::Terminal { cwd, command: None },
                    None,
                    caller,
                    opts,
                )?;
                if let Some(command) = run {
                    self.with_terminal_from_ipc(cx, &koma.id, |cx, term| {
                        term.run_command(cx, &command)
                    })
                    .map_err(|err| {
                        format!("Koma {} created, initial command failed: {err}", koma.id)
                    })?;
                }
                Ok(IpcPayload::KomaId {
                    koma_id: koma.id.into_string(),
                })
            }
            TerminalCommand::Run(args) => {
                self.terminal_input(cx, args, caller, MpTerm::run_command)
            }
            TerminalCommand::Press(args) => {
                self.terminal_input(cx, args, caller, MpTerm::press_key)
            }
            TerminalCommand::Kill(args) => {
                self.terminal_input(cx, args, caller, |term, _, signal| {
                    term.signal_foreground(signal)
                })
            }
            TerminalCommand::Text(TerminalTextArgs { koma_id, lines }) => {
                let koma_id = self
                    .state
                    .resolve_terminal(koma_id.map(KomaId::from).as_ref(), caller)?
                    .id
                    .clone();
                let fetch_lines = lines.map(|n| n.saturating_add(80));
                let rows = self
                    .with_strip(cx, |strip, _| strip.terminal_rows(&koma_id, fetch_lines))
                    .flatten()
                    .ok_or_else(|| format!("Terminal not ready: {koma_id}"))?;
                Ok(IpcPayload::Text {
                    koma_id: koma_id.into_string(),
                    text: trim_terminal_rows(rows, lines),
                })
            }
            TerminalCommand::Send(KomaTextArgs { koma_id, text }) => {
                let koma_id = self
                    .state
                    .resolve_terminal(koma_id.map(KomaId::from).as_ref(), caller)?
                    .id
                    .clone();
                let sent = self
                    .with_strip_mut(cx, |strip, _| strip.send_terminal_text(&koma_id, &text))
                    .unwrap_or(false);
                if !sent {
                    return Err(format!("Terminal not ready: {koma_id}"));
                }
                Ok(IpcPayload::KomaId {
                    koma_id: koma_id.into_string(),
                })
            }
        }
    }

    fn dispatch_drawer(&mut self, cx: &mut Cx, cmd: DrawerCommand) -> Result<IpcPayload, String> {
        match cmd {
            DrawerCommand::List => {
                let items = self
                    .state
                    .drawer
                    .iter()
                    .map(|b| DrawerItemSummary {
                        id: b.id.to_string(),
                        label: b.label.clone(),
                        content: match &b.content {
                            KomaContent::Terminal { cwd, .. } => {
                                DrawerItemContent::Terminal { cwd: cwd.clone() }
                            }
                            KomaContent::Web { initial_url, .. } => DrawerItemContent::Web {
                                url: initial_url.clone(),
                            },
                        },
                    })
                    .collect();
                Ok(IpcPayload::DrawerList { items })
            }
            DrawerCommand::Keep(DrawerKeepArgs {
                input,
                title,
                koma_api,
            }) => {
                let home_dir = home_dir_string();
                let expanded = self.config.expand_alias(&input)?.unwrap_or(input);
                match parse_open_koma_input_with_engine(
                    &expanded,
                    home_dir.as_deref(),
                    self.config.search_engine,
                )? {
                    KomaInput::Terminal { cwd, .. } => {
                        let label = title.unwrap_or_else(|| derive_cwd_label(cwd.as_deref()));
                        let item = self
                            .state
                            .keep_koma_in_drawer(label, KomaContent::terminal(cwd, None));
                        self.sync_after_mutation(cx, false);
                        Ok(IpcPayload::KomaId {
                            koma_id: item.id.into_string(),
                        })
                    }
                    KomaInput::Web { url, label } => {
                        validate_koma_api_url(koma_api, &url)?;
                        let item = self.state.keep_koma_in_drawer(
                            title.unwrap_or(label),
                            KomaContent::Web {
                                initial_url: url.clone(),
                                koma_api_enabled: koma_api,
                            },
                        );
                        if koma_api {
                            self.web_runtime()
                                .borrow_mut()
                                .set_koma_api_enabled(cx, &item.id, true);
                        }
                        self.sync_after_mutation(cx, false);
                        Ok(IpcPayload::WebKoma(WebKomaData {
                            koma_id: item.id.into_string(),
                            url: Some(url),
                        }))
                    }
                }
            }
            DrawerCommand::Place(KomaTarget { koma_id }) => {
                let koma = self.place_drawer_item(cx, koma_id.map(KomaId::from).as_ref())?;
                Ok(IpcPayload::KomaId {
                    koma_id: koma.id.into_string(),
                })
            }
            DrawerCommand::Discard(KomaTarget { koma_id }) => {
                let item = self.discard_drawer_item(cx, koma_id.map(KomaId::from).as_ref())?;
                Ok(IpcPayload::KomaId {
                    koma_id: item.id.into_string(),
                })
            }
            DrawerCommand::Restore => {
                let item = self.restore_discarded_drawer_item(cx)?;
                Ok(IpcPayload::KomaId {
                    koma_id: item.id.into_string(),
                })
            }
        }
    }

    fn dispatch_notification(
        &mut self,
        cx: &mut Cx,
        cmd: NotificationCommand,
        caller: Option<&KomaId>,
    ) -> Result<IpcPayload, String> {
        match cmd {
            NotificationCommand::Send(NotificationSendArgs {
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
                let entry = self.state.push_notification(source, title, &body);
                self.render_notifications_overlay(cx);
                self.update_status(cx);
                self.show_notification_toast(cx, &entry);
                Ok(IpcPayload::NotificationSent {
                    koma_id: entry.koma_id.into_string(),
                    notification_id: entry.id,
                })
            }
            NotificationCommand::List(NotificationListArgs {
                unread_only,
                koma_id,
                limit,
            }) => {
                let iter = self
                    .state
                    .notifications
                    .iter()
                    .filter(|n| !unread_only || n.unread)
                    .filter(|n| koma_id.as_deref().is_none_or(|id| n.koma_id.as_str() == id))
                    .map(|n| NotificationSummary {
                        id: n.id.clone(),
                        koma_id: n.koma_id.to_string(),
                        title: n.title.clone(),
                        body: n.body.clone(),
                        unread: n.unread,
                    });
                let notifications = match limit {
                    Some(n) => iter.take(n).collect(),
                    None => iter.collect(),
                };
                Ok(IpcPayload::NotificationList { notifications })
            }
            NotificationCommand::Read { id } => self.mark_notification_from_ipc(cx, &id, false),
            NotificationCommand::Unread { id } => self.mark_notification_from_ipc(cx, &id, true),
        }
    }

    fn dispatch_workspace(&mut self, cx: &mut Cx, cmd: WorkspaceCommand, reply: IpcReply) {
        match cmd {
            WorkspaceCommand::Health => {
                let _ = reply.send(IpcResponse::ok());
            }
            WorkspaceCommand::Inspect => {
                let _ = reply.send(IpcResponse::Ok(IpcPayload::WorkspaceEndpoint(
                    WorkspaceEndpoint {
                        id: self.state.profile_id.clone(),
                        is_private: self.is_private,
                        socket: self.sock_path.clone(),
                    },
                )));
            }
            WorkspaceCommand::ProfileList => {
                send_result(
                    reply,
                    profiles::root()
                        .and_then(|root| Profile::list(&root))
                        .map(|profiles| IpcPayload::ProfileList { profiles }),
                );
            }
            WorkspaceCommand::ProfileOpen { id } => {
                self.open_profile(cx, id, false, move |result| {
                    send_result(reply, result.map(|()| IpcPayload::Ack));
                });
            }
            WorkspaceCommand::ProfileNew { id } => {
                self.open_profile(cx, id, true, move |result| {
                    send_result(reply, result.map(|()| IpcPayload::Ack));
                });
            }
            WorkspaceCommand::ProfileActivate { id } => {
                let result = if !self.is_private && id == self.state.profile_id {
                    self.ui.window(cx, ids!(main_window)).show(cx);
                    Ok(IpcPayload::Ack)
                } else {
                    Err("IPC endpoint belongs to a different Profile".into())
                };
                send_result(reply, result);
            }
            WorkspaceCommand::Private => {
                send_result(
                    reply,
                    self.spawn_private_workspace(cx).map(|()| IpcPayload::Ack),
                );
            }
            WorkspaceCommand::SettingsReload => {
                let result = self.reload_configs(cx).map(|()| {
                    self.show_toast(cx, "Reloaded config");
                    IpcPayload::Ack
                });
                send_result(reply, result);
            }
        }
    }

    fn with_resolved_web_koma(
        &self,
        koma_id: Option<String>,
        caller: Option<&KomaId>,
        reply: IpcReply,
        f: impl FnOnce(&mut WebViewRuntime, KomaId, IpcReply),
    ) {
        match self.resolve_web_koma(koma_id, caller) {
            Ok(id) => f(&mut self.web_runtime().borrow_mut(), id, reply),
            Err(e) => {
                let _ = reply.send(IpcResponse::err(e));
            }
        }
    }

    fn terminal_input(
        &self,
        cx: &mut Cx,
        args: KomaTextArgs,
        caller: Option<&KomaId>,
        operation: impl FnOnce(&mut MpTerm, &mut Cx, &str) -> Result<(), String>,
    ) -> Result<IpcPayload, String> {
        let koma = self
            .state
            .resolve_terminal(args.koma_id.map(KomaId::from).as_ref(), caller)?;
        self.with_terminal_from_ipc(cx, &koma.id, |cx, term| operation(term, cx, &args.text))?;
        Ok(IpcPayload::KomaId {
            koma_id: koma.id.to_string(),
        })
    }

    fn with_terminal_from_ipc(
        &self,
        cx: &mut Cx,
        koma_id: &KomaId,
        operation: impl FnOnce(&mut Cx, &mut MpTerm) -> Result<(), String>,
    ) -> Result<(), String> {
        self.with_strip(cx, |strip, cx| {
            strip.with_terminal(koma_id, |term| operation(cx, term))
        })
        .ok_or_else(|| "Sheet Strip unavailable".to_string())?
    }

    fn resolve_placement(&self, placement: &KomaPlacement) -> Result<(usize, usize), String> {
        let mut sheets = self.state.sheets().iter().enumerate();
        match placement {
            KomaPlacement::EndOf(id) => {
                let id = SheetId::from(id.as_str());
                sheets
                    .find(|(_, sheet)| sheet.id == id)
                    .map(|(index, sheet)| (index, sheet.komas.len()))
                    .ok_or_else(|| format!("Sheet not found: {id}"))
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
                    .ok_or_else(|| format!("Koma not found: {id}"))
            }
        }
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
        let (sheet_index, koma_index) = self.resolve_placement(placement)?;
        if self
            .state
            .move_koma_without_focusing(koma_id, sheet_index, koma_index)
        {
            self.sync_after_mutation(cx, false);
        }
        Ok(())
    }

    fn mark_notification_from_ipc(
        &mut self,
        cx: &mut Cx,
        id: &str,
        unread: bool,
    ) -> Result<IpcPayload, String> {
        self.state.mark_notification_read(id, unread)?;
        self.render_notifications_overlay(cx);
        self.update_status(cx);
        Ok(IpcPayload::Ack)
    }

    fn resolve_web_koma(
        &self,
        koma_id: Option<String>,
        caller_koma_id: Option<&KomaId>,
    ) -> Result<KomaId, String> {
        self.state
            .resolve_web(koma_id.map(KomaId::from).as_ref(), caller_koma_id)
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
