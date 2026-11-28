//! Newline-delimited JSON IPC protocol between `koma` CLI and the GUI app.

use makepad_micro_serde::*;

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct IpcRequest {
    pub caller_koma_id: Option<String>,
    pub profile_id: Option<String>,
    pub command: IpcCommand,
}

impl IpcRequest {
    pub fn new(command: IpcCommand) -> Self {
        Self {
            caller_koma_id: None,
            profile_id: None,
            command,
        }
    }
}

/// Target a specific koma (or omit to resolve the caller/focused koma).
#[derive(Clone, Debug, Default, PartialEq, SerJson, DeJson)]
pub struct KomaTarget {
    pub koma_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, SerJson, DeJson)]
pub struct WebNewArgs {
    pub url: String,
    pub koma_api: bool,
    pub placement: Option<KomaPlacement>,
    pub width: Option<f64>,
    pub focus: bool,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct WebNavigateArgs {
    pub koma_id: Option<String>,
    pub url: String,
}

#[derive(Clone, Debug, Default, PartialEq, SerJson, DeJson)]
pub struct WebSnapshotArgs {
    pub koma_id: Option<String>,
    pub within: Option<String>,
    pub full: bool,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct WebReloadArgs {
    pub koma_id: Option<String>,
    pub koma_api: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct WebEvalArgs {
    pub koma_id: Option<String>,
    pub script: String,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct WebReadArgs {
    pub koma_id: Option<String>,
    pub within: Option<String>,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct WebClickArgs {
    pub koma_id: Option<String>,
    pub selector: String,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct WebFillArgs {
    pub koma_id: Option<String>,
    pub selector: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct KomaTextArgs {
    pub koma_id: Option<String>,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, SerJson, DeJson)]
pub struct TerminalNewArgs {
    pub cwd: Option<String>,
    pub run: Option<String>,
    pub placement: Option<KomaPlacement>,
    pub width: Option<f64>,
    pub focus: bool,
}

#[derive(Clone, Debug, Default, PartialEq, SerJson, DeJson)]
pub struct SheetCloseArgs {
    pub sheet_id: Option<String>,
    pub force: bool,
}

#[derive(Clone, Debug, Default, PartialEq, SerJson, DeJson)]
pub struct NotificationListArgs {
    pub unread_only: bool,
    pub koma_id: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, SerJson, DeJson)]
pub struct TerminalTextArgs {
    pub koma_id: Option<String>,
    pub lines: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct DrawerKeepArgs {
    pub input: String,
    pub title: Option<String>,
    pub koma_api: bool,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct NotificationSendArgs {
    pub koma_id: Option<String>,
    pub title: Option<String>,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub enum KomaPlacement {
    LeftOf(String),
    RightOf(String),
    EndOf(String),
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct KomaListArgs {
    pub sheet_id: Option<String>,
}

// Variants that carry `Option` fields wrap a struct because `makepad-micro-serde`'s
// enum struct-variant serializer can emit a trailing comma when an `Option` is `None`.
#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub enum IpcCommand {
    Koma(KomaCommand),
    Sheet(SheetCommand),
    Web(WebCommand),
    Terminal(TerminalCommand),
    Drawer(DrawerCommand),
    Notification(NotificationCommand),
    Workspace(WorkspaceCommand),
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub enum KomaCommand {
    List(KomaListArgs),
    Focused,
    Exists {
        koma_id: String,
    },
    Focus {
        koma_id: String,
    },
    Move {
        koma_id: String,
        placement: KomaPlacement,
    },
    Resize {
        koma_id: String,
        width: f64,
    },
    Close {
        koma_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub enum SheetCommand {
    Focus { sheet_index: usize },
    Close(SheetCloseArgs),
    List,
    New { label: String, focus: bool },
    PresetList,
    PresetSave { name: String },
    PresetApply { name: String },
    Overview,
    OverviewToggle,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub enum WebCommand {
    New(WebNewArgs),
    Navigate(WebNavigateArgs),
    Back(KomaTarget),
    Forward(KomaTarget),
    Url(KomaTarget),
    Devtools,
    Reload(WebReloadArgs),
    Eval(WebEvalArgs),
    ScriptTarget(KomaTarget),
    Snapshot(WebSnapshotArgs),
    Read(WebReadArgs),
    Text(KomaTarget),
    Click(WebClickArgs),
    Fill(WebFillArgs),
    Type(KomaTextArgs),
    Press(KomaTextArgs),
    Scroll(KomaTextArgs),
    BenchSpawnWebs { count: usize },
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub enum TerminalCommand {
    New(TerminalNewArgs),
    Text(TerminalTextArgs),
    Send(KomaTextArgs),
    Run(KomaTextArgs),
    Press(KomaTextArgs),
    Kill(KomaTextArgs),
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub enum DrawerCommand {
    List,
    Keep(DrawerKeepArgs),
    Place(KomaTarget),
    Discard(KomaTarget),
    Restore,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub enum NotificationCommand {
    Send(NotificationSendArgs),
    List(NotificationListArgs),
    Read { id: String },
    Unread { id: String },
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub enum WorkspaceCommand {
    Health,
    Inspect,
    Private,
    ProfileList,
    ProfileNew { id: String },
    ProfileOpen { id: String },
    ProfileActivate { id: String },
    SettingsReload,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct WorkspaceEndpoint {
    pub id: String,
    pub is_private: bool,
    pub socket: String,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub enum KomaSummaryContent {
    Terminal,
    Web { url: String },
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct KomaSummary {
    pub id: String,
    pub label: String,
    pub content: KomaSummaryContent,
    pub is_focused: bool,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct KomaFocusedData {
    pub sheet_id: String,
    pub koma: Option<KomaSummary>,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct SheetSummary {
    pub id: String,
    pub label: String,
    pub is_active: bool,
    pub komas: Vec<KomaSummary>,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct SheetListEntry {
    pub id: String,
    pub label: String,
    pub is_active: bool,
    pub koma_count: usize,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub enum DrawerItemContent {
    Terminal {
        /// Absent until a path is given or reported by the shell (OSC 7).
        cwd: Option<String>,
    },
    Web {
        url: String,
    },
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct DrawerItemSummary {
    pub id: String,
    pub label: String,
    pub content: DrawerItemContent,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct NotificationSummary {
    pub id: String,
    pub koma_id: String,
    pub title: String,
    pub body: String,
    pub unread: bool,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct PresetSummary {
    pub name: String,
    pub komas: Vec<String>,
    pub personal: bool,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct WebKomaData {
    pub koma_id: String,
    pub url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct WebSnapshotData {
    pub koma_id: String,
    pub snapshot: Option<String>,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct WebScriptTargetData {
    pub koma_id: String,
    pub port: u16,
    pub target_id: String,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct WebReadData {
    pub koma_id: String,
    pub markdown: Option<String>,
    pub title: Option<String>,
    pub url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub enum IpcPayload {
    Ack,
    WorkspaceEndpoint(WorkspaceEndpoint),
    ProfileList {
        profiles: Vec<String>,
    },
    Overview {
        sheets: Vec<SheetSummary>,
    },
    PresetList {
        presets: Vec<PresetSummary>,
    },
    PresetSaved {
        name: String,
    },
    PresetApplied {
        sheet_index: usize,
    },
    KomaId {
        koma_id: String,
    },
    KomaList {
        sheet_id: String,
        komas: Vec<KomaSummary>,
    },
    KomaFocused(KomaFocusedData),
    KomaExists {
        koma_id: String,
        exists: bool,
    },
    KomaResized {
        koma_id: String,
        width: f64,
    },
    SheetList {
        sheets: Vec<SheetListEntry>,
    },
    SheetId {
        sheet_id: String,
    },
    WebKoma(WebKomaData),
    WebUrl {
        koma_id: String,
        url: String,
        title: String,
    },
    WebEval {
        koma_id: String,
        value: String,
    },
    WebSnapshot(WebSnapshotData),
    WebScriptTarget(WebScriptTargetData),
    WebRead(WebReadData),
    Text {
        koma_id: String,
        text: String,
    },
    DrawerList {
        items: Vec<DrawerItemSummary>,
    },
    NotificationSent {
        koma_id: String,
        notification_id: String,
    },
    NotificationList {
        notifications: Vec<NotificationSummary>,
    },
    BenchSpawnWebs {
        total_views: usize,
    },
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub enum IpcResponse {
    Ok(IpcPayload),
    Err(String),
}

impl IpcResponse {
    pub fn ok() -> Self {
        Self::Ok(IpcPayload::Ack)
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self::Err(message.into())
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok(_))
    }

    pub fn into_result(self) -> Result<IpcPayload, String> {
        match self {
            Self::Ok(payload) => Ok(payload),
            Self::Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_json_round_trip<T: SerJson + DeJson + PartialEq + std::fmt::Debug>(value: &T) {
        let json = value.serialize_json();
        assert!(!json.contains(",}"), "trailing comma in object: {json}");
        assert!(!json.contains(",]"), "trailing comma in array: {json}");
        assert_eq!(T::deserialize_json(&json).unwrap(), *value);
    }

    #[test]
    fn all_ipc_commands_json_round_trip() {
        let commands = vec![
            // Koma
            IpcCommand::Koma(KomaCommand::List(KomaListArgs { sheet_id: None })),
            IpcCommand::Koma(KomaCommand::List(KomaListArgs {
                sheet_id: Some("sheet-1".into()),
            })),
            IpcCommand::Koma(KomaCommand::Focused),
            IpcCommand::Koma(KomaCommand::Exists {
                koma_id: "k-1".into(),
            }),
            IpcCommand::Koma(KomaCommand::Focus {
                koma_id: "k-1".into(),
            }),
            IpcCommand::Koma(KomaCommand::Move {
                koma_id: "k-1".into(),
                placement: KomaPlacement::LeftOf("k-2".into()),
            }),
            IpcCommand::Koma(KomaCommand::Move {
                koma_id: "k-1".into(),
                placement: KomaPlacement::RightOf("k-2".into()),
            }),
            IpcCommand::Koma(KomaCommand::Move {
                koma_id: "k-1".into(),
                placement: KomaPlacement::EndOf("s-2".into()),
            }),
            IpcCommand::Koma(KomaCommand::Resize {
                koma_id: "k-1".into(),
                width: 640.5,
            }),
            IpcCommand::Koma(KomaCommand::Close {
                koma_id: "k-1".into(),
            }),
            // Sheet
            IpcCommand::Sheet(SheetCommand::Focus { sheet_index: 1 }),
            IpcCommand::Sheet(SheetCommand::Close(SheetCloseArgs::default())),
            IpcCommand::Sheet(SheetCommand::Close(SheetCloseArgs {
                sheet_id: Some("s-2".into()),
                force: true,
            })),
            IpcCommand::Sheet(SheetCommand::List),
            IpcCommand::Sheet(SheetCommand::New {
                label: "Work".into(),
                focus: true,
            }),
            IpcCommand::Sheet(SheetCommand::PresetList),
            IpcCommand::Sheet(SheetCommand::PresetSave { name: "Dev".into() }),
            IpcCommand::Sheet(SheetCommand::PresetApply { name: "Dev".into() }),
            IpcCommand::Sheet(SheetCommand::Overview),
            IpcCommand::Sheet(SheetCommand::OverviewToggle),
            // Web
            IpcCommand::Web(WebCommand::New(WebNewArgs::default())),
            IpcCommand::Web(WebCommand::New(WebNewArgs {
                url: "https://example.com".into(),
                koma_api: true,
                placement: Some(KomaPlacement::EndOf("s-2".into())),
                width: Some(800.0),
                focus: true,
            })),
            IpcCommand::Web(WebCommand::Navigate(WebNavigateArgs {
                koma_id: None,
                url: "https://example.com".into(),
            })),
            IpcCommand::Web(WebCommand::Navigate(WebNavigateArgs {
                koma_id: Some("k-1".into()),
                url: "https://example.com".into(),
            })),
            IpcCommand::Web(WebCommand::Back(KomaTarget::default())),
            IpcCommand::Web(WebCommand::Back(KomaTarget {
                koma_id: Some("k-1".into()),
            })),
            IpcCommand::Web(WebCommand::Forward(KomaTarget::default())),
            IpcCommand::Web(WebCommand::Url(KomaTarget::default())),
            IpcCommand::Web(WebCommand::Devtools),
            IpcCommand::Web(WebCommand::Reload(WebReloadArgs {
                koma_id: None,
                koma_api: None,
            })),
            IpcCommand::Web(WebCommand::Reload(WebReloadArgs {
                koma_id: Some("k-1".into()),
                koma_api: Some(true),
            })),
            IpcCommand::Web(WebCommand::Eval(WebEvalArgs {
                koma_id: None,
                script: "1 + 1".into(),
            })),
            IpcCommand::Web(WebCommand::ScriptTarget(KomaTarget::default())),
            IpcCommand::Web(WebCommand::Snapshot(WebSnapshotArgs::default())),
            IpcCommand::Web(WebCommand::Snapshot(WebSnapshotArgs {
                koma_id: Some("k-1".into()),
                within: Some("main".into()),
                full: true,
            })),
            IpcCommand::Web(WebCommand::Read(WebReadArgs {
                koma_id: None,
                within: None,
            })),
            IpcCommand::Web(WebCommand::Read(WebReadArgs {
                koma_id: Some("k-1".into()),
                within: Some("article".into()),
            })),
            IpcCommand::Web(WebCommand::Text(KomaTarget::default())),
            IpcCommand::Web(WebCommand::Click(WebClickArgs {
                koma_id: None,
                selector: "#submit".into(),
            })),
            IpcCommand::Web(WebCommand::Fill(WebFillArgs {
                koma_id: None,
                selector: "#email".into(),
                text: "alice@example.com".into(),
            })),
            IpcCommand::Web(WebCommand::Type(KomaTextArgs {
                koma_id: None,
                text: "hello".into(),
            })),
            IpcCommand::Web(WebCommand::Press(KomaTextArgs {
                koma_id: Some("k-1".into()),
                text: "Enter".into(),
            })),
            IpcCommand::Web(WebCommand::Scroll(KomaTextArgs {
                koma_id: None,
                text: "down".into(),
            })),
            IpcCommand::Web(WebCommand::BenchSpawnWebs { count: 4 }),
            // Terminal
            IpcCommand::Terminal(TerminalCommand::New(TerminalNewArgs::default())),
            IpcCommand::Terminal(TerminalCommand::New(TerminalNewArgs {
                cwd: Some("/tmp".into()),
                run: Some("ls".into()),
                placement: Some(KomaPlacement::RightOf("k-1".into())),
                width: Some(600.0),
                focus: true,
            })),
            IpcCommand::Terminal(TerminalCommand::Text(TerminalTextArgs::default())),
            IpcCommand::Terminal(TerminalCommand::Text(TerminalTextArgs {
                koma_id: Some("t-1".into()),
                lines: Some(50),
            })),
            IpcCommand::Terminal(TerminalCommand::Send(KomaTextArgs {
                koma_id: None,
                text: "echo hi".into(),
            })),
            IpcCommand::Terminal(TerminalCommand::Run(KomaTextArgs {
                koma_id: Some("t-1".into()),
                text: "pwd".into(),
            })),
            IpcCommand::Terminal(TerminalCommand::Press(KomaTextArgs {
                koma_id: None,
                text: "ctrl+c".into(),
            })),
            IpcCommand::Terminal(TerminalCommand::Kill(KomaTextArgs {
                koma_id: Some("t-1".into()),
                text: "TERM".into(),
            })),
            // Drawer
            IpcCommand::Drawer(DrawerCommand::List),
            IpcCommand::Drawer(DrawerCommand::Keep(DrawerKeepArgs {
                input: "https://example.com".into(),
                title: None,
                koma_api: false,
            })),
            IpcCommand::Drawer(DrawerCommand::Keep(DrawerKeepArgs {
                input: "https://example.com".into(),
                title: Some("Example".into()),
                koma_api: true,
            })),
            IpcCommand::Drawer(DrawerCommand::Place(KomaTarget::default())),
            IpcCommand::Drawer(DrawerCommand::Discard(KomaTarget {
                koma_id: Some("d-1".into()),
            })),
            IpcCommand::Drawer(DrawerCommand::Restore),
            // Notification
            IpcCommand::Notification(NotificationCommand::Send(NotificationSendArgs {
                koma_id: None,
                title: None,
                body: "done".into(),
            })),
            IpcCommand::Notification(NotificationCommand::Send(NotificationSendArgs {
                koma_id: Some("k-1".into()),
                title: Some("Build".into()),
                body: "passed".into(),
            })),
            IpcCommand::Notification(NotificationCommand::List(NotificationListArgs::default())),
            IpcCommand::Notification(NotificationCommand::List(NotificationListArgs {
                unread_only: true,
                koma_id: Some("k-1".into()),
                limit: Some(10),
            })),
            IpcCommand::Notification(NotificationCommand::Read { id: "n-1".into() }),
            IpcCommand::Notification(NotificationCommand::Unread { id: "n-1".into() }),
            // Workspace
            IpcCommand::Workspace(WorkspaceCommand::Health),
            IpcCommand::Workspace(WorkspaceCommand::Inspect),
            IpcCommand::Workspace(WorkspaceCommand::Private),
            IpcCommand::Workspace(WorkspaceCommand::ProfileList),
            IpcCommand::Workspace(WorkspaceCommand::ProfileNew { id: "work".into() }),
            IpcCommand::Workspace(WorkspaceCommand::ProfileOpen { id: "work".into() }),
            IpcCommand::Workspace(WorkspaceCommand::ProfileActivate { id: "work".into() }),
            IpcCommand::Workspace(WorkspaceCommand::SettingsReload),
        ];

        // Compile-time exhaustiveness guard so newly added variants are covered above.
        for cmd in &commands {
            match cmd {
                IpcCommand::Koma(c) => match c {
                    KomaCommand::List(_)
                    | KomaCommand::Focused
                    | KomaCommand::Exists { .. }
                    | KomaCommand::Focus { .. }
                    | KomaCommand::Move { .. }
                    | KomaCommand::Resize { .. }
                    | KomaCommand::Close { .. } => {}
                },
                IpcCommand::Sheet(c) => match c {
                    SheetCommand::Focus { .. }
                    | SheetCommand::Close(_)
                    | SheetCommand::List
                    | SheetCommand::New { .. }
                    | SheetCommand::PresetList
                    | SheetCommand::PresetSave { .. }
                    | SheetCommand::PresetApply { .. }
                    | SheetCommand::Overview
                    | SheetCommand::OverviewToggle => {}
                },
                IpcCommand::Web(c) => match c {
                    WebCommand::New(_)
                    | WebCommand::Navigate(_)
                    | WebCommand::Back(_)
                    | WebCommand::Forward(_)
                    | WebCommand::Url(_)
                    | WebCommand::Devtools
                    | WebCommand::Reload(_)
                    | WebCommand::Eval(_)
                    | WebCommand::ScriptTarget(_)
                    | WebCommand::Snapshot(_)
                    | WebCommand::Read(_)
                    | WebCommand::Text(_)
                    | WebCommand::Click(_)
                    | WebCommand::Fill(_)
                    | WebCommand::Type(_)
                    | WebCommand::Press(_)
                    | WebCommand::Scroll(_)
                    | WebCommand::BenchSpawnWebs { .. } => {}
                },
                IpcCommand::Terminal(c) => match c {
                    TerminalCommand::New(_)
                    | TerminalCommand::Text(_)
                    | TerminalCommand::Send(_)
                    | TerminalCommand::Run(_)
                    | TerminalCommand::Press(_)
                    | TerminalCommand::Kill(_) => {}
                },
                IpcCommand::Drawer(c) => match c {
                    DrawerCommand::List
                    | DrawerCommand::Keep(_)
                    | DrawerCommand::Place(_)
                    | DrawerCommand::Discard(_)
                    | DrawerCommand::Restore => {}
                },
                IpcCommand::Notification(c) => match c {
                    NotificationCommand::Send(_)
                    | NotificationCommand::List(_)
                    | NotificationCommand::Read { .. }
                    | NotificationCommand::Unread { .. } => {}
                },
                IpcCommand::Workspace(c) => match c {
                    WorkspaceCommand::Health
                    | WorkspaceCommand::Inspect
                    | WorkspaceCommand::Private
                    | WorkspaceCommand::ProfileList
                    | WorkspaceCommand::ProfileNew { .. }
                    | WorkspaceCommand::ProfileOpen { .. }
                    | WorkspaceCommand::ProfileActivate { .. }
                    | WorkspaceCommand::SettingsReload => {}
                },
            }
        }

        for (i, command) in commands.into_iter().enumerate() {
            let req = if i % 2 == 0 {
                IpcRequest::new(command)
            } else {
                IpcRequest {
                    caller_koma_id: Some("k-caller".into()),
                    profile_id: Some("default".into()),
                    command,
                }
            };
            assert_json_round_trip(&req);
        }
    }

    #[test]
    fn all_ipc_payloads_json_round_trip() {
        let sample_koma = KomaSummary {
            id: "k-1".into(),
            label: "Example".into(),
            content: KomaSummaryContent::Web {
                url: "https://example.com".into(),
            },
            is_focused: true,
        };
        let sample_koma_no_url = KomaSummary {
            id: "k-2".into(),
            label: "Shell".into(),
            content: KomaSummaryContent::Terminal,
            is_focused: false,
        };

        let payloads = vec![
            IpcPayload::Ack,
            IpcPayload::WorkspaceEndpoint(WorkspaceEndpoint {
                id: "default".into(),
                is_private: false,
                socket: "/tmp/koma.sock".into(),
            }),
            IpcPayload::ProfileList {
                profiles: vec!["default".into(), "work".into()],
            },
            IpcPayload::Overview {
                sheets: vec![SheetSummary {
                    id: "s-1".into(),
                    label: "Sheet 1".into(),
                    is_active: true,
                    komas: vec![sample_koma.clone(), sample_koma_no_url.clone()],
                }],
            },
            IpcPayload::PresetList {
                presets: vec![PresetSummary {
                    name: "Dev".into(),
                    komas: vec![":terminal".into()],
                    personal: true,
                }],
            },
            IpcPayload::PresetSaved { name: "Dev".into() },
            IpcPayload::PresetApplied { sheet_index: 1 },
            IpcPayload::KomaId {
                koma_id: "k-1".into(),
            },
            IpcPayload::KomaList {
                sheet_id: "s-1".into(),
                komas: vec![sample_koma.clone(), sample_koma_no_url],
            },
            IpcPayload::KomaFocused(KomaFocusedData {
                sheet_id: "s-1".into(),
                koma: None,
            }),
            IpcPayload::KomaFocused(KomaFocusedData {
                sheet_id: "s-1".into(),
                koma: Some(sample_koma),
            }),
            IpcPayload::KomaExists {
                koma_id: "k-1".into(),
                exists: true,
            },
            IpcPayload::KomaResized {
                koma_id: "k-1".into(),
                width: 720.0,
            },
            IpcPayload::SheetList {
                sheets: vec![SheetListEntry {
                    id: "s-1".into(),
                    label: "Sheet 1".into(),
                    is_active: true,
                    koma_count: 2,
                }],
            },
            IpcPayload::SheetId {
                sheet_id: "s-1".into(),
            },
            IpcPayload::WebKoma(WebKomaData {
                koma_id: "k-1".into(),
                url: None,
            }),
            IpcPayload::WebKoma(WebKomaData {
                koma_id: "k-1".into(),
                url: Some("https://example.com".into()),
            }),
            IpcPayload::WebUrl {
                koma_id: "k-1".into(),
                url: "https://example.com".into(),
                title: "Example".into(),
            },
            IpcPayload::WebEval {
                koma_id: "k-1".into(),
                value: "42".into(),
            },
            IpcPayload::WebSnapshot(WebSnapshotData {
                koma_id: "k-1".into(),
                snapshot: None,
            }),
            IpcPayload::WebSnapshot(WebSnapshotData {
                koma_id: "k-1".into(),
                snapshot: Some("@e1 [button] \"OK\"".into()),
            }),
            IpcPayload::WebScriptTarget(WebScriptTargetData {
                koma_id: "k-1".into(),
                port: 9222,
                target_id: "target-1".into(),
            }),
            IpcPayload::WebRead(WebReadData {
                koma_id: "k-1".into(),
                markdown: None,
                title: None,
                url: None,
            }),
            IpcPayload::WebRead(WebReadData {
                koma_id: "k-1".into(),
                markdown: Some("# Hi".into()),
                title: Some("Hi".into()),
                url: Some("https://example.com".into()),
            }),
            IpcPayload::Text {
                koma_id: "t-1".into(),
                text: "hello\nworld".into(),
            },
            IpcPayload::DrawerList {
                items: vec![
                    DrawerItemSummary {
                        id: "d-1".into(),
                        label: "Example".into(),
                        content: DrawerItemContent::Web {
                            url: "https://example.com".into(),
                        },
                    },
                    DrawerItemSummary {
                        id: "d-2".into(),
                        label: "Shell".into(),
                        content: DrawerItemContent::Terminal {
                            cwd: Some("/tmp".into()),
                        },
                    },
                ],
            },
            IpcPayload::NotificationSent {
                koma_id: "k-1".into(),
                notification_id: "n-1".into(),
            },
            IpcPayload::NotificationList {
                notifications: vec![NotificationSummary {
                    id: "n-1".into(),
                    koma_id: "k-1".into(),
                    title: "Build".into(),
                    body: "ok".into(),
                    unread: true,
                }],
            },
            IpcPayload::BenchSpawnWebs { total_views: 8 },
        ];

        // Compile-time exhaustiveness guard so newly added variants are covered above.
        for payload in &payloads {
            match payload {
                IpcPayload::Ack
                | IpcPayload::WorkspaceEndpoint(_)
                | IpcPayload::ProfileList { .. }
                | IpcPayload::Overview { .. }
                | IpcPayload::PresetList { .. }
                | IpcPayload::PresetSaved { .. }
                | IpcPayload::PresetApplied { .. }
                | IpcPayload::KomaId { .. }
                | IpcPayload::KomaList { .. }
                | IpcPayload::KomaFocused(_)
                | IpcPayload::KomaExists { .. }
                | IpcPayload::KomaResized { .. }
                | IpcPayload::SheetList { .. }
                | IpcPayload::SheetId { .. }
                | IpcPayload::WebKoma(_)
                | IpcPayload::WebUrl { .. }
                | IpcPayload::WebEval { .. }
                | IpcPayload::WebSnapshot(_)
                | IpcPayload::WebScriptTarget(_)
                | IpcPayload::WebRead(_)
                | IpcPayload::Text { .. }
                | IpcPayload::DrawerList { .. }
                | IpcPayload::NotificationSent { .. }
                | IpcPayload::NotificationList { .. }
                | IpcPayload::BenchSpawnWebs { .. } => {}
            }
        }

        for payload in payloads {
            assert_json_round_trip(&IpcResponse::Ok(payload));
        }
        assert_json_round_trip(&IpcResponse::err("boom"));
    }
}
