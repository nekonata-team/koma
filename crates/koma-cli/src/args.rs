use std::num::{NonZeroU64, NonZeroUsize};
use std::path::{Path, PathBuf};

use clap::{error::ErrorKind, CommandFactory, Parser, Subcommand};
use koma_ipc::protocol::{
    DrawerCommand, DrawerKeepArgs, IpcCommand, IpcRequest, KomaCommand, KomaListArgs,
    KomaPlacement, KomaTarget, KomaTextArgs, NotificationCommand, NotificationListArgs,
    NotificationSendArgs, SheetCloseArgs, SheetCommand, TerminalCommand, TerminalNewArgs,
    TerminalTextArgs, WebClickArgs, WebCommand, WebEvalArgs, WebFillArgs, WebNavigateArgs,
    WebNewArgs, WebReadArgs, WebReloadArgs, WebSnapshotArgs, WorkspaceCommand,
};

fn parse_non_empty(raw: &str) -> Result<String, String> {
    if raw.trim().is_empty() {
        Err("must not be empty".into())
    } else {
        Ok(raw.to_owned())
    }
}

#[derive(Debug, clap::Args)]
struct RequiredKoma {
    /// Target a specific Koma.
    #[arg(
        long,
        value_name = "ID",
        allow_hyphen_values = true,
        value_parser = parse_non_empty
    )]
    koma: String,
}

#[derive(Debug, clap::Args)]
struct OptionalKoma {
    /// Target a specific Koma.
    #[arg(long, value_name = "ID", allow_hyphen_values = true)]
    koma: Option<String>,
}

#[derive(Debug, clap::Args)]
struct CreatePlacement {
    #[arg(long, value_name = "KOMA_ID", group = "create_placement")]
    left_of: Option<String>,
    #[arg(long, value_name = "KOMA_ID", group = "create_placement")]
    right_of: Option<String>,
    #[arg(long, value_name = "SHEET_ID", group = "create_placement")]
    end_of: Option<String>,
    /// Set initial width in points.
    #[arg(long)]
    width: Option<f64>,
    /// Focus the created Koma, switching Sheets if needed.
    #[arg(long)]
    focus: bool,
}

impl CreatePlacement {
    fn placement(&self) -> Option<KomaPlacement> {
        self.left_of
            .clone()
            .map(KomaPlacement::LeftOf)
            .or_else(|| self.right_of.clone().map(KomaPlacement::RightOf))
            .or_else(|| self.end_of.clone().map(KomaPlacement::EndOf))
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "koma",
    about = "Control a running Koma workspace",
    args_override_self = true
)]
pub(super) struct Args {
    /// Override the IPC socket path or port file.
    #[arg(long, global = true, value_name = "PATH", allow_hyphen_values = true)]
    pub(super) socket: Option<String>,
    /// Print JSON (also the default when stdout is not a terminal).
    #[arg(long, global = true)]
    pub(super) json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check whether Koma responds to IPC.
    Health,
    /// Open a Private Workspace.
    Private,
    /// Discover running Workspaces.
    #[command(subcommand)]
    Workspace(Workspace),
    /// List, create, or switch Profiles.
    #[command(subcommand)]
    Profile(Profile),
    /// Show Sheet and Koma state, or toggle Overview.
    Overview {
        #[command(subcommand)]
        command: Option<Overview>,
    },
    /// Inspect or manage Sheets.
    #[command(subcommand)]
    Sheet(Sheet),
    /// List, save, or apply Sheet Presets.
    #[command(subcommand)]
    Preset(Preset),
    /// List Komas on the specified or ambient Sheet.
    List {
        #[arg(long, value_name = "ID")]
        sheet: Option<String>,
    },
    /// Show the focused Koma on the active Sheet.
    Focused,
    /// Check whether --koma exists on any Sheet in the Workspace.
    Exists {
        #[command(flatten)]
        target: RequiredKoma,
    },
    /// Focus --koma, switching Sheets if needed.
    Focus {
        #[command(flatten)]
        target: RequiredKoma,
    },
    /// Move --koma to a specific position.
    Move {
        #[command(flatten)]
        target: RequiredKoma,
        #[arg(long, value_name = "KOMA_ID", group = "placement")]
        left_of: Option<String>,
        #[arg(long, value_name = "KOMA_ID", group = "placement")]
        right_of: Option<String>,
        #[arg(long, value_name = "SHEET_ID", group = "placement")]
        end_of: Option<String>,
    },
    /// Set the fixed width of --koma in points.
    Resize {
        #[command(flatten)]
        target: RequiredKoma,
        #[arg(long)]
        width: f64,
    },
    /// Close --koma.
    Close {
        #[command(flatten)]
        target: RequiredKoma,
    },
    #[command(subcommand)]
    Web(Web),
    #[command(subcommand)]
    Terminal(Terminal),
    /// Manage Drawer Items.
    #[command(subcommand)]
    Drawer(Drawer),
    /// Send or manage Notifications.
    #[command(subcommand)]
    Notification(Notification),
    /// Run a benchmark operation.
    #[command(subcommand)]
    Bench(Bench),
    /// Open Koma's config file in `$VISUAL` / `$EDITOR`, print its path, or reload config.
    Settings {
        /// Print the resolved config path without opening an editor.
        #[arg(long)]
        path: bool,
        #[command(subcommand)]
        command: Option<SettingsAction>,
    },
}

#[derive(Debug, Subcommand)]
enum SettingsAction {
    /// Reload `config.toml` and Ghostty settings in the running Koma instance.
    Reload,
}

#[derive(Debug, Subcommand)]
enum Overview {
    Toggle,
}

#[derive(Debug, Subcommand)]
enum Profile {
    List,
    New { name: String },
    Open { name: String },
}

#[derive(Debug, Subcommand)]
enum Workspace {
    /// List running Workspaces and their IPC endpoints.
    List,
}

#[derive(Debug, Subcommand)]
enum Sheet {
    Focus {
        number: usize,
    },
    /// Close the current Sheet, or a specific Sheet by ID.
    Close {
        /// Target a specific Sheet ID.
        #[arg(long, value_name = "ID", allow_hyphen_values = true)]
        sheet: Option<String>,
        /// Close even if the target Sheet contains open Komas.
        #[arg(long)]
        force: bool,
    },
    /// List all Sheets in the Workspace.
    List,
    /// Create an empty Sheet.
    New {
        label: String,
        #[arg(long)]
        focus: bool,
    },
}

#[derive(Debug, Subcommand)]
enum Preset {
    List,
    Save { name: String },
    Apply { name: String },
}

#[derive(Debug, Subcommand)]
enum Web {
    New {
        url: String,
        /// Enable KomaAPI for the Web Koma.
        #[arg(long)]
        koma_api: bool,
        #[command(flatten)]
        placement: CreatePlacement,
    },
    /// Navigate an existing Web Koma to a URL or search query.
    Navigate {
        #[command(flatten)]
        target: OptionalKoma,
        url: String,
    },
    /// Navigate back in browsing history.
    Back {
        #[command(flatten)]
        target: OptionalKoma,
    },
    /// Navigate forward in browsing history.
    Forward {
        #[command(flatten)]
        target: OptionalKoma,
    },
    Url {
        #[command(flatten)]
        target: OptionalKoma,
    },
    Devtools,
    Reload {
        #[command(flatten)]
        target: OptionalKoma,
        /// Enable KomaAPI for the Web Koma.
        #[arg(long, conflicts_with = "no_koma_api")]
        koma_api: bool,
        /// Disable KomaAPI for the Web Koma.
        #[arg(long)]
        no_koma_api: bool,
    },
    Eval {
        #[command(flatten)]
        target: OptionalKoma,
        expr: String,
    },
    /// Run a Node.js module against one Web Koma and print its JSON result.
    Script {
        #[command(flatten)]
        target: OptionalKoma,
        /// Module exporting an async function, or "-" to read it from stdin.
        file: PathBuf,
        /// Maximum time for Node.js, including its browser connection.
        #[arg(long, default_value = "30", value_name = "SECONDS")]
        timeout: NonZeroU64,
    },
    /// Save a PNG screenshot of a Web Koma.
    Screenshot {
        #[command(flatten)]
        target: OptionalKoma,
        /// Destination file path (defaults to a temporary PNG file).
        path: Option<PathBuf>,
    },
    Snapshot {
        #[command(flatten)]
        target: OptionalKoma,
        /// Limit snapshot to a selector or element reference.
        #[arg(long, allow_hyphen_values = true)]
        within: Option<String>,
        /// Include non-interactive visible semantic elements.
        #[arg(long)]
        full: bool,
    },
    Read {
        #[command(flatten)]
        target: OptionalKoma,
        /// Limit output to a selector or element reference.
        #[arg(long, allow_hyphen_values = true)]
        within: Option<String>,
    },
    Text {
        #[command(flatten)]
        target: OptionalKoma,
    },
    Click {
        #[command(flatten)]
        koma: OptionalKoma,
        target: String,
    },
    Fill {
        #[command(flatten)]
        koma: OptionalKoma,
        target: String,
        text: String,
    },
    Type {
        #[command(flatten)]
        target: OptionalKoma,
        text: String,
    },
    /// Dispatch a key press such as Enter, Escape, Tab, or Ctrl+A.
    Press {
        #[command(flatten)]
        target: OptionalKoma,
        key: String,
    },
    /// Scroll the page (down, up, top, bottom, pixel offset) or an element into view.
    Scroll {
        #[command(flatten)]
        target: OptionalKoma,
        #[arg(default_value = "down", allow_hyphen_values = true)]
        direction_or_target: String,
    },
}

#[derive(Debug, Subcommand)]
enum Terminal {
    New {
        cwd: Option<String>,
        /// Run an initial command in the new interactive shell.
        #[arg(long)]
        run: Option<String>,
        #[command(flatten)]
        placement: CreatePlacement,
    },
    Text {
        #[command(flatten)]
        target: OptionalKoma,
        /// Limit the number of lines returned.
        #[arg(long)]
        lines: Option<NonZeroUsize>,
    },
    /// Send text to a Terminal Koma. Use `--` before text starting with a hyphen.
    Send {
        #[command(flatten)]
        target: OptionalKoma,
        text: String,
    },
    /// Send a command followed by Enter.
    Run {
        #[command(flatten)]
        target: OptionalKoma,
        command: String,
    },
    /// Send a key chord such as Enter, Shift+Tab, or Ctrl+C.
    Press {
        #[command(flatten)]
        target: OptionalKoma,
        key: String,
    },
    /// Signal the foreground process group on macOS and Linux.
    Kill {
        #[command(flatten)]
        target: OptionalKoma,
        #[arg(short, long, default_value = "TERM")]
        signal: String,
    },
}

#[derive(Debug, Subcommand)]
enum Drawer {
    List,
    Keep {
        input: String,
        /// Set the title for the kept Drawer Item.
        #[arg(long, allow_hyphen_values = true)]
        title: Option<String>,
        /// Enable KomaAPI for the Web Koma.
        #[arg(long)]
        koma_api: bool,
    },
    Place {
        #[command(flatten)]
        target: OptionalKoma,
        id: Option<String>,
    },
    Discard {
        #[command(flatten)]
        target: OptionalKoma,
        id: Option<String>,
    },
    Restore,
}

#[derive(Debug, Subcommand)]
enum Notification {
    Send {
        #[command(flatten)]
        target: OptionalKoma,
        body: String,
        /// Set the Notification title.
        #[arg(long, allow_hyphen_values = true)]
        title: Option<String>,
    },
    List {
        /// List only unread Notifications.
        #[arg(long)]
        unread: bool,
        #[command(flatten)]
        target: OptionalKoma,
        /// Limit the number of Notifications returned.
        #[arg(long)]
        limit: Option<NonZeroUsize>,
    },
    Read {
        id: String,
    },
    Unread {
        id: String,
    },
}

#[derive(Debug, Subcommand)]
enum Bench {
    SpawnWebs { count: usize },
}

impl Args {
    pub(super) fn validate(self) -> Result<Self, clap::Error> {
        match &self.command {
            Command::Move {
                left_of: None,
                right_of: None,
                end_of: None,
                ..
            } => {
                return Err(Self::command().error(
                    ErrorKind::MissingRequiredArgument,
                    "one of --left-of, --right-of, or --end-of is required",
                ));
            }
            Command::Resize { width, .. }
            | Command::Web(Web::New {
                placement:
                    CreatePlacement {
                        width: Some(width), ..
                    },
                ..
            })
            | Command::Terminal(Terminal::New {
                placement:
                    CreatePlacement {
                        width: Some(width), ..
                    },
                ..
            }) if !width.is_finite() || *width <= 0.0 => {
                return Err(Self::command().error(
                    ErrorKind::ValueValidation,
                    "--width must be finite and positive",
                ));
            }
            _ => {}
        }
        Ok(self)
    }

    pub(super) fn settings_path_only(&self) -> Option<bool> {
        match &self.command {
            Command::Settings {
                path,
                command: None,
            } => Some(*path),
            _ => None,
        }
    }

    pub(super) fn lists_workspaces(&self) -> bool {
        matches!(self.command, Command::Workspace(Workspace::List))
    }

    pub(super) fn into_request(self) -> IpcRequest {
        let command = match self.command {
            Command::Health => IpcCommand::Workspace(WorkspaceCommand::Health),
            Command::Private => IpcCommand::Workspace(WorkspaceCommand::Private),
            Command::Workspace(Workspace::List) => unreachable!("workspace list is local"),
            Command::Profile(Profile::List) => IpcCommand::Workspace(WorkspaceCommand::ProfileList),
            Command::Profile(Profile::New { name }) => {
                IpcCommand::Workspace(WorkspaceCommand::ProfileNew { id: name })
            }
            Command::Profile(Profile::Open { name }) => {
                IpcCommand::Workspace(WorkspaceCommand::ProfileOpen { id: name })
            }
            Command::Overview { command: None } => IpcCommand::Sheet(SheetCommand::Overview),
            Command::Overview {
                command: Some(Overview::Toggle),
            } => IpcCommand::Sheet(SheetCommand::OverviewToggle),
            Command::Sheet(Sheet::Focus { number }) => IpcCommand::Sheet(SheetCommand::Focus {
                sheet_index: number,
            }),
            Command::Sheet(Sheet::Close { sheet, force }) => {
                IpcCommand::Sheet(SheetCommand::Close(SheetCloseArgs {
                    sheet_id: sheet,
                    force,
                }))
            }
            Command::Sheet(Sheet::List) => IpcCommand::Sheet(SheetCommand::List),
            Command::Sheet(Sheet::New { label, focus }) => {
                IpcCommand::Sheet(SheetCommand::New { label, focus })
            }
            Command::Preset(Preset::List) => IpcCommand::Sheet(SheetCommand::PresetList),
            Command::Preset(Preset::Save { name }) => {
                IpcCommand::Sheet(SheetCommand::PresetSave { name })
            }
            Command::Preset(Preset::Apply { name }) => {
                IpcCommand::Sheet(SheetCommand::PresetApply { name })
            }
            Command::List { sheet } => {
                IpcCommand::Koma(KomaCommand::List(KomaListArgs { sheet_id: sheet }))
            }
            Command::Focused => IpcCommand::Koma(KomaCommand::Focused),
            Command::Exists { target } => IpcCommand::Koma(KomaCommand::Exists {
                koma_id: target.koma,
            }),
            Command::Focus { target } => IpcCommand::Koma(KomaCommand::Focus {
                koma_id: target.koma,
            }),
            Command::Move {
                target,
                left_of,
                right_of,
                end_of,
            } => IpcCommand::Koma(KomaCommand::Move {
                koma_id: target.koma,
                placement: left_of
                    .map(KomaPlacement::LeftOf)
                    .or_else(|| right_of.map(KomaPlacement::RightOf))
                    .or_else(|| end_of.map(KomaPlacement::EndOf))
                    .unwrap(),
            }),
            Command::Resize { target, width } => IpcCommand::Koma(KomaCommand::Resize {
                koma_id: target.koma,
                width,
            }),
            Command::Close { target } => IpcCommand::Koma(KomaCommand::Close {
                koma_id: target.koma,
            }),
            Command::Web(Web::New {
                url,
                koma_api,
                placement,
            }) => IpcCommand::Web(WebCommand::New(WebNewArgs {
                url,
                koma_api,
                placement: placement.placement(),
                width: placement.width,
                focus: placement.focus,
            })),
            Command::Web(Web::Navigate { target, url }) => {
                IpcCommand::Web(WebCommand::Navigate(WebNavigateArgs {
                    koma_id: target.koma,
                    url,
                }))
            }
            Command::Web(Web::Back { target }) => IpcCommand::Web(WebCommand::Back(KomaTarget {
                koma_id: target.koma,
            })),
            Command::Web(Web::Forward { target }) => {
                IpcCommand::Web(WebCommand::Forward(KomaTarget {
                    koma_id: target.koma,
                }))
            }
            Command::Web(Web::Url { target }) => IpcCommand::Web(WebCommand::Url(KomaTarget {
                koma_id: target.koma,
            })),
            Command::Web(Web::Devtools) => IpcCommand::Web(WebCommand::Devtools),
            Command::Web(Web::Reload {
                target,
                koma_api,
                no_koma_api,
            }) => {
                let koma_api = match (koma_api, no_koma_api) {
                    (true, _) => Some(true),
                    (_, true) => Some(false),
                    _ => None,
                };
                IpcCommand::Web(WebCommand::Reload(WebReloadArgs {
                    koma_id: target.koma,
                    koma_api,
                }))
            }
            Command::Web(Web::Eval { target, expr }) => {
                IpcCommand::Web(WebCommand::Eval(WebEvalArgs {
                    koma_id: target.koma,
                    script: expr,
                }))
            }
            Command::Web(Web::Script { target, .. })
            | Command::Web(Web::Screenshot { target, .. }) => {
                IpcCommand::Web(WebCommand::ScriptTarget(KomaTarget {
                    koma_id: target.koma,
                }))
            }
            Command::Web(Web::Snapshot {
                target,
                within,
                full,
            }) => IpcCommand::Web(WebCommand::Snapshot(WebSnapshotArgs {
                koma_id: target.koma,
                within,
                full,
            })),
            Command::Web(Web::Read { target, within }) => {
                IpcCommand::Web(WebCommand::Read(WebReadArgs {
                    koma_id: target.koma,
                    within,
                }))
            }
            Command::Web(Web::Text { target }) => IpcCommand::Web(WebCommand::Text(KomaTarget {
                koma_id: target.koma,
            })),
            Command::Web(Web::Click { koma, target }) => {
                IpcCommand::Web(WebCommand::Click(WebClickArgs {
                    koma_id: koma.koma,
                    selector: target,
                }))
            }
            Command::Web(Web::Fill { koma, target, text }) => {
                IpcCommand::Web(WebCommand::Fill(WebFillArgs {
                    koma_id: koma.koma,
                    selector: target,
                    text,
                }))
            }
            Command::Web(Web::Type { target, text }) => {
                IpcCommand::Web(WebCommand::Type(KomaTextArgs {
                    koma_id: target.koma,
                    text,
                }))
            }
            Command::Web(Web::Press { target, key }) => {
                IpcCommand::Web(WebCommand::Press(KomaTextArgs {
                    koma_id: target.koma,
                    text: key,
                }))
            }
            Command::Web(Web::Scroll {
                target,
                direction_or_target,
            }) => IpcCommand::Web(WebCommand::Scroll(KomaTextArgs {
                koma_id: target.koma,
                text: direction_or_target,
            })),
            Command::Terminal(Terminal::New {
                cwd,
                run,
                placement,
            }) => IpcCommand::Terminal(TerminalCommand::New(TerminalNewArgs {
                cwd,
                run,
                placement: placement.placement(),
                width: placement.width,
                focus: placement.focus,
            })),
            Command::Terminal(Terminal::Text { target, lines }) => {
                IpcCommand::Terminal(TerminalCommand::Text(TerminalTextArgs {
                    koma_id: target.koma,
                    lines: lines.map(NonZeroUsize::get),
                }))
            }
            Command::Terminal(Terminal::Send { target, text }) => {
                IpcCommand::Terminal(TerminalCommand::Send(KomaTextArgs {
                    koma_id: target.koma,
                    text,
                }))
            }
            Command::Terminal(Terminal::Run { target, command }) => {
                IpcCommand::Terminal(TerminalCommand::Run(KomaTextArgs {
                    koma_id: target.koma,
                    text: command,
                }))
            }
            Command::Terminal(Terminal::Press { target, key }) => {
                IpcCommand::Terminal(TerminalCommand::Press(KomaTextArgs {
                    koma_id: target.koma,
                    text: key,
                }))
            }
            Command::Terminal(Terminal::Kill { target, signal }) => {
                IpcCommand::Terminal(TerminalCommand::Kill(KomaTextArgs {
                    koma_id: target.koma,
                    text: signal,
                }))
            }
            Command::Drawer(Drawer::List) => IpcCommand::Drawer(DrawerCommand::List),
            Command::Drawer(Drawer::Keep {
                input,
                title,
                koma_api,
            }) => IpcCommand::Drawer(DrawerCommand::Keep(DrawerKeepArgs {
                input,
                title,
                koma_api,
            })),
            Command::Drawer(Drawer::Place { target, id }) => {
                IpcCommand::Drawer(DrawerCommand::Place(KomaTarget {
                    koma_id: target.koma.or(id),
                }))
            }
            Command::Drawer(Drawer::Discard { target, id }) => {
                IpcCommand::Drawer(DrawerCommand::Discard(KomaTarget {
                    koma_id: target.koma.or(id),
                }))
            }
            Command::Drawer(Drawer::Restore) => IpcCommand::Drawer(DrawerCommand::Restore),
            Command::Notification(Notification::Send {
                target,
                body,
                title,
            }) => IpcCommand::Notification(NotificationCommand::Send(NotificationSendArgs {
                koma_id: target.koma,
                title,
                body,
            })),
            Command::Notification(Notification::List {
                unread,
                target,
                limit,
            }) => IpcCommand::Notification(NotificationCommand::List(NotificationListArgs {
                unread_only: unread,
                koma_id: target.koma,
                limit: limit.map(NonZeroUsize::get),
            })),
            Command::Notification(Notification::Read { id }) => {
                IpcCommand::Notification(NotificationCommand::Read { id })
            }
            Command::Notification(Notification::Unread { id }) => {
                IpcCommand::Notification(NotificationCommand::Unread { id })
            }
            Command::Bench(Bench::SpawnWebs { count }) => {
                IpcCommand::Web(WebCommand::BenchSpawnWebs { count })
            }
            Command::Settings {
                command: Some(SettingsAction::Reload),
                ..
            } => IpcCommand::Workspace(WorkspaceCommand::SettingsReload),
            Command::Settings { .. } => unreachable!("settings is handled locally"),
        };
        IpcRequest {
            caller_koma_id: std::env::var("KOMA_KOMA_ID").ok().filter(|s| !s.is_empty()),
            profile_id: std::env::var("KOMA_PROFILE").ok().filter(|s| !s.is_empty()),
            command,
        }
    }

    pub(super) fn web_script(&self) -> Option<(&Path, NonZeroU64)> {
        let Command::Web(Web::Script { file, timeout, .. }) = &self.command else {
            return None;
        };
        Some((file, *timeout))
    }

    pub(super) fn web_screenshot(&self) -> Option<Option<&Path>> {
        let Command::Web(Web::Screenshot { path, .. }) = &self.command else {
            return None;
        };
        Some(path.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(argv: &[&str]) -> Result<Args, clap::Error> {
        Args::try_parse_from(std::iter::once("koma").chain(argv.iter().copied()))
            .and_then(Args::validate)
    }

    #[test]
    fn maps_commands_to_ipc() {
        for (argv, expected) in [
            (
                &["private"][..],
                IpcCommand::Workspace(WorkspaceCommand::Private),
            ),
            (
                &["profile", "list"],
                IpcCommand::Workspace(WorkspaceCommand::ProfileList),
            ),
            (
                &["profile", "new", "work"],
                IpcCommand::Workspace(WorkspaceCommand::ProfileNew { id: "work".into() }),
            ),
            (
                &["profile", "open", "work"],
                IpcCommand::Workspace(WorkspaceCommand::ProfileOpen { id: "work".into() }),
            ),
            (&["overview"], IpcCommand::Sheet(SheetCommand::Overview)),
            (
                &["overview", "toggle"],
                IpcCommand::Sheet(SheetCommand::OverviewToggle),
            ),
            (
                &["sheet", "close"],
                IpcCommand::Sheet(SheetCommand::Close(SheetCloseArgs::default())),
            ),
            (
                &["sheet", "close", "--sheet", "s2", "--force"],
                IpcCommand::Sheet(SheetCommand::Close(SheetCloseArgs {
                    sheet_id: Some("s2".into()),
                    force: true,
                })),
            ),
            (
                &["web", "navigate", "https://example.com", "--koma", "k1"],
                IpcCommand::Web(WebCommand::Navigate(WebNavigateArgs {
                    koma_id: Some("k1".into()),
                    url: "https://example.com".into(),
                })),
            ),
            (
                &["web", "back", "--koma", "k1"],
                IpcCommand::Web(WebCommand::Back(KomaTarget {
                    koma_id: Some("k1".into()),
                })),
            ),
            (
                &["web", "forward"],
                IpcCommand::Web(WebCommand::Forward(KomaTarget { koma_id: None })),
            ),
            (
                &["web", "press", "Enter", "--koma", "k1"],
                IpcCommand::Web(WebCommand::Press(KomaTextArgs {
                    koma_id: Some("k1".into()),
                    text: "Enter".into(),
                })),
            ),
            (
                &["web", "scroll", "-200"],
                IpcCommand::Web(WebCommand::Scroll(KomaTextArgs {
                    koma_id: None,
                    text: "-200".into(),
                })),
            ),
            (
                &["web", "snapshot", "--within", "main", "--full"],
                IpcCommand::Web(WebCommand::Snapshot(WebSnapshotArgs {
                    koma_id: None,
                    within: Some("main".into()),
                    full: true,
                })),
            ),
            (
                &[
                    "web",
                    "new",
                    "https://example.com",
                    "--end-of",
                    "s2",
                    "--width",
                    "900",
                    "--focus",
                ],
                IpcCommand::Web(WebCommand::New(WebNewArgs {
                    url: "https://example.com".into(),
                    koma_api: false,
                    placement: Some(KomaPlacement::EndOf("s2".into())),
                    width: Some(900.0),
                    focus: true,
                })),
            ),
            (
                &[
                    "terminal",
                    "new",
                    ".",
                    "--left-of",
                    "k1",
                    "--width",
                    "720",
                    "--focus",
                ],
                IpcCommand::Terminal(TerminalCommand::New(TerminalNewArgs {
                    cwd: Some(".".into()),
                    run: None,
                    placement: Some(KomaPlacement::LeftOf("k1".into())),
                    width: Some(720.0),
                    focus: true,
                })),
            ),
            (
                &[
                    "notification",
                    "list",
                    "--unread",
                    "--koma",
                    "k1",
                    "--limit",
                    "5",
                ],
                IpcCommand::Notification(NotificationCommand::List(NotificationListArgs {
                    unread_only: true,
                    koma_id: Some("k1".into()),
                    limit: Some(5),
                })),
            ),
            (
                &["preset", "list"],
                IpcCommand::Sheet(SheetCommand::PresetList),
            ),
            (
                &["preset", "save", "Work"],
                IpcCommand::Sheet(SheetCommand::PresetSave {
                    name: "Work".into(),
                }),
            ),
            (
                &["preset", "apply", "Work"],
                IpcCommand::Sheet(SheetCommand::PresetApply {
                    name: "Work".into(),
                }),
            ),
            (&["web", "devtools"], IpcCommand::Web(WebCommand::Devtools)),
            (
                &["settings", "reload"],
                IpcCommand::Workspace(WorkspaceCommand::SettingsReload),
            ),
        ] {
            let req = parse(argv).unwrap().into_request();
            assert_eq!(req.command, expected);
        }
    }

    #[test]
    fn web_script_resolves_a_koma_without_sending_the_source_path() {
        let args = parse(&["web", "script", "--koma", "koma-2", "-"]).unwrap();
        let (file, timeout) = args.web_script().unwrap();
        assert_eq!(file, Path::new("-"));
        assert_eq!(timeout.get(), 30);
        assert_eq!(
            args.into_request().command,
            IpcCommand::Web(WebCommand::ScriptTarget(KomaTarget {
                koma_id: Some("koma-2".into())
            }))
        );
        assert!(parse(&["web", "script", "--timeout", "0", "test.mjs"]).is_err());

        let shot = parse(&["web", "screenshot", "--koma", "koma-2", "/tmp/out.png"]).unwrap();
        assert_eq!(shot.web_screenshot(), Some(Some(Path::new("/tmp/out.png"))));
        assert_eq!(
            shot.into_request().command,
            IpcCommand::Web(WebCommand::ScriptTarget(KomaTarget {
                koma_id: Some("koma-2".into())
            }))
        );
        assert!(parse(&["web", "new", "https://example.com", "--width", "0"]).is_err());
        assert!(parse(&["terminal", "new", "--left-of", "k1", "--end-of", "s1"]).is_err());
        assert!(parse(&["notification", "list", "--limit", "0"]).is_err());
    }

    #[test]
    fn validates_koma_targets_and_move_placement() {
        assert!(parse(&["close"]).is_err());
        assert!(parse(&["close", "--koma", "  "]).is_err());
        assert!(parse(&["move", "--koma", "b1"]).is_err());
        assert!(parse(&["move", "--koma", "b1", "--left-of", "b2", "--end-of", "d1"]).is_err());
        assert!(parse(&["resize", "--koma", "b1", "--width", "NaN"]).is_err());
        let request = parse(&["move", "--koma", "b1", "--end-of", "d1"])
            .unwrap()
            .into_request();
        assert_eq!(
            request.command,
            IpcCommand::Koma(KomaCommand::Move {
                koma_id: "b1".into(),
                placement: KomaPlacement::EndOf("d1".into()),
            })
        );
    }

    #[test]
    fn keeps_global_flags_available_around_subcommands() {
        let args = parse(&[
            "--socket", "one", "web", "read", "--koma", "b1", "--socket", "two", "--within", "@e1",
            "--json",
        ])
        .unwrap();
        assert!(args.json);
        assert_eq!(args.socket.as_deref(), Some("two"));
        let req = args.into_request();
        assert_eq!(
            req.command,
            IpcCommand::Web(WebCommand::Read(WebReadArgs {
                koma_id: Some("b1".into()),
                within: Some("@e1".into()),
            }))
        );

        let req = parse(&[
            "drawer",
            "keep",
            "example.com",
            "--title",
            "Ex",
            "--koma-api",
        ])
        .unwrap()
        .into_request();
        assert_eq!(
            req.command,
            IpcCommand::Drawer(DrawerCommand::Keep(DrawerKeepArgs {
                input: "example.com".into(),
                title: Some("Ex".into()),
                koma_api: true,
            }))
        );

        let req = parse(&["notification", "send", "Build done", "--title", "-CI"])
            .unwrap()
            .into_request();
        assert_eq!(
            req.command,
            IpcCommand::Notification(NotificationCommand::Send(NotificationSendArgs {
                koma_id: None,
                title: Some("-CI".into()),
                body: "Build done".into(),
            }))
        );
    }

    #[test]
    fn accepts_hyphenated_payloads_after_separator() {
        let req = parse(&["terminal", "send", "--", "-n"])
            .unwrap()
            .into_request();
        assert_eq!(
            req.command,
            IpcCommand::Terminal(TerminalCommand::Send(KomaTextArgs {
                koma_id: None,
                text: "-n".into(),
            }))
        );
    }

    #[test]
    fn validates_lines_and_conflicting_flags() {
        let req = parse(&["terminal", "text", "--lines", "20"])
            .unwrap()
            .into_request();
        assert_eq!(
            req.command,
            IpcCommand::Terminal(TerminalCommand::Text(TerminalTextArgs {
                koma_id: None,
                lines: Some(20),
            }))
        );
        assert!(parse(&["terminal", "text", "--lines", "0"]).is_err());
        let err = parse(&["web", "reload", "--koma-api", "--no-koma-api"]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn rejects_subcommand_options_on_unrelated_commands() {
        for argv in [
            &["terminal", "text", "--within", "@e1"][..],
            &["web", "read", "--lines", "10"],
            &["sheet", "list", "--unread"],
            &["drawer", "keep", "example.com", "--no-koma-api"],
            &["health", "--koma", "b1"],
            &["sheet", "list", "--koma", "b1"],
            &["web", "new", "https://example.com", "--koma", "b1"],
        ] {
            let err = parse(argv).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::UnknownArgument, "{argv:?}");
        }
    }
}
