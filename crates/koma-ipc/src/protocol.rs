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
    Health,
    WorkspacePrivate,
    ProfileList,
    ProfileNew {
        id: String,
    },
    ProfileOpen {
        id: String,
    },
    ProfileActivate {
        id: String,
    },
    Overview,
    OverviewToggle,
    KomaList(KomaListArgs),
    KomaFocused,
    KomaExists {
        koma_id: String,
    },
    KomaFocus {
        koma_id: String,
    },
    KomaMove {
        koma_id: String,
        placement: KomaPlacement,
    },
    KomaResize {
        koma_id: String,
        width: f64,
    },
    KomaClose {
        koma_id: String,
    },
    SheetFocus {
        sheet_index: usize,
    },
    SheetClose,
    SheetList,
    SheetNew {
        label: String,
        focus: bool,
    },
    PresetList,
    PresetSave {
        name: String,
    },
    PresetApply {
        name: String,
    },
    WebNew {
        url: String,
        koma_api: bool,
    },
    WebUrl(KomaTarget),
    WebDevtools,
    WebReload(WebReloadArgs),
    WebEval(WebEvalArgs),
    WebScriptTarget(KomaTarget),
    WebSnapshot(KomaTarget),
    WebRead(WebReadArgs),
    WebText(KomaTarget),
    WebClick(WebClickArgs),
    WebFill(WebFillArgs),
    WebType(KomaTextArgs),
    TerminalNew(TerminalNewArgs),
    TerminalText(TerminalTextArgs),
    TerminalSend(KomaTextArgs),
    TerminalRun(KomaTextArgs),
    TerminalPress(KomaTextArgs),
    TerminalKill(KomaTextArgs),
    DrawerList,
    DrawerKeep(DrawerKeepArgs),
    DrawerPlace(KomaTarget),
    DrawerDiscard(KomaTarget),
    DrawerRestore,
    NotificationSend(NotificationSendArgs),
    NotificationList {
        unread_only: bool,
    },
    NotificationRead {
        id: String,
    },
    NotificationUnread {
        id: String,
    },
    SettingsReload,
    BenchSpawnWebs {
        count: usize,
    },
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct KomaSummary {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub url: Option<String>,
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
pub struct DrawerItemSummary {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub url: Option<String>,
    pub cwd: Option<String>,
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

    #[test]
    fn request_and_response_json_round_trip() {
        let requests = [
            IpcRequest::new(IpcCommand::Health),
            IpcRequest {
                caller_koma_id: Some("b-1".into()),
                profile_id: None,
                command: IpcCommand::WebFill(WebFillArgs {
                    koma_id: None,
                    selector: "#email".into(),
                    text: "alice@example.com".into(),
                }),
            },
            IpcRequest::new(IpcCommand::DrawerKeep(DrawerKeepArgs {
                input: "https://example.com".into(),
                title: None,
                koma_api: false,
            })),
        ];
        for req in requests {
            let json = req.serialize_json();
            assert!(!json.contains(",}"), "trailing comma in {json}");
            assert_eq!(IpcRequest::deserialize_json(&json).unwrap(), req);
        }

        let responses = [
            IpcResponse::ok(),
            IpcResponse::err("boom"),
            IpcResponse::Ok(IpcPayload::WebRead(WebReadData {
                koma_id: "b-2".into(),
                markdown: Some("# Hi".into()),
                title: None,
                url: None,
            })),
        ];
        for resp in responses {
            let json = resp.serialize_json();
            assert!(!json.contains(",}"), "trailing comma in {json}");
            assert_eq!(IpcResponse::deserialize_json(&json).unwrap(), resp);
        }
    }
}
