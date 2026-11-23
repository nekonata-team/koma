use std::num::{NonZeroU64, NonZeroUsize};
use std::path::{Path, PathBuf};

use clap::{error::ErrorKind, CommandFactory, Parser, Subcommand};
use koma_ipc::protocol::{
    DrawerKeepArgs, IpcCommand, IpcRequest, KomaListArgs, KomaPlacement, KomaTarget, KomaTextArgs,
    NotificationSendArgs, TerminalNewArgs, TerminalTextArgs, WebClickArgs, WebEvalArgs,
    WebFillArgs, WebReadArgs, WebReloadArgs,
};

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
    /// Target a specific Koma.
    #[arg(long, global = true, value_name = "ID", allow_hyphen_values = true)]
    koma: Option<String>,
    // Keep existing options available before and after subcommands.
    /// Limit the number of lines returned by `koma terminal text`.
    #[arg(long, global = true)]
    lines: Option<NonZeroUsize>,
    /// Limit `koma web read` to a selector or element reference.
    #[arg(long, global = true, allow_hyphen_values = true)]
    within: Option<String>,
    /// Set the title for `drawer keep` or `notification send`.
    #[arg(long, global = true, allow_hyphen_values = true)]
    title: Option<String>,
    /// List only unread Notifications.
    #[arg(long, global = true)]
    unread: bool,
    /// Enable KomaAPI for the Web Koma.
    #[arg(long, global = true, conflicts_with = "no_koma_api")]
    koma_api: bool,
    /// Disable KomaAPI for the Web Koma.
    #[arg(long, global = true)]
    no_koma_api: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check whether Koma responds to IPC.
    Health,
    /// Open a Private Workspace.
    Private,
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
    Exists,
    /// Focus --koma, switching Sheets if needed.
    Focus,
    /// Move --koma to a specific position.
    Move {
        #[arg(long, value_name = "KOMA_ID", group = "placement")]
        left_of: Option<String>,
        #[arg(long, value_name = "KOMA_ID", group = "placement")]
        right_of: Option<String>,
        #[arg(long, value_name = "SHEET_ID", group = "placement")]
        end_of: Option<String>,
    },
    /// Set the fixed width of --koma in points.
    Resize {
        #[arg(long)]
        width: f64,
    },
    /// Close --koma.
    Close,
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
enum Sheet {
    Focus {
        number: usize,
    },
    Close,
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
    },
    Url,
    Devtools,
    Reload,
    Eval {
        expr: String,
    },
    /// Run a Node.js module against one Web Koma and print its JSON result.
    Script {
        /// Module exporting an async function, or "-" to read it from stdin.
        file: PathBuf,
        /// Maximum time for Node.js, including its browser connection.
        #[arg(long, default_value = "30", value_name = "SECONDS")]
        timeout: NonZeroU64,
    },
    Snapshot,
    Read,
    Text,
    Click {
        target: String,
    },
    Fill {
        target: String,
        text: String,
    },
    Type {
        text: String,
    },
}

#[derive(Debug, Subcommand)]
enum Terminal {
    New {
        cwd: Option<String>,
        /// Run an initial command in the new interactive shell.
        #[arg(long)]
        run: Option<String>,
    },
    Text,
    /// Send text to a Terminal Koma. Use `--` before text starting with a hyphen.
    Send {
        text: String,
    },
    /// Send a command followed by Enter.
    Run {
        command: String,
    },
    /// Send a key chord such as Enter, Shift+Tab, or Ctrl+C.
    Press {
        key: String,
    },
    /// Signal the foreground process group on macOS and Linux.
    Kill {
        #[arg(short, long, default_value = "TERM")]
        signal: String,
    },
}

#[derive(Debug, Subcommand)]
enum Drawer {
    List,
    Keep { input: String },
    Place { id: Option<String> },
    Discard { id: Option<String> },
    Restore,
}

#[derive(Debug, Subcommand)]
enum Notification {
    Send { body: String },
    List,
    Read { id: String },
    Unread { id: String },
}

#[derive(Debug, Subcommand)]
enum Bench {
    SpawnWebs { count: usize },
}

impl Args {
    pub(super) fn validate(self) -> Result<Self, clap::Error> {
        // clap checks conflicts within a command, not between different levels.
        if self.koma_api && self.no_koma_api {
            return Err(Self::command().error(
                ErrorKind::ArgumentConflict,
                "--koma-api and --no-koma-api are mutually exclusive",
            ));
        }
        match &self.command {
            Command::Exists
            | Command::Focus
            | Command::Move { .. }
            | Command::Resize { .. }
            | Command::Close
                if self.koma.as_deref().is_none_or(|id| id.trim().is_empty()) =>
            {
                return Err(Self::command().error(
                    ErrorKind::MissingRequiredArgument,
                    "--koma <ID> is required",
                ));
            }
            Command::Move {
                left_of: None,
                right_of: None,
                end_of: None,
            } => {
                return Err(Self::command().error(
                    ErrorKind::MissingRequiredArgument,
                    "one of --left-of, --right-of, or --end-of is required",
                ));
            }
            Command::Resize { width } if !width.is_finite() || *width <= 0.0 => {
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

    pub(super) fn into_request(self) -> IpcRequest {
        let koma_api = match (self.koma_api, self.no_koma_api) {
            (true, _) => Some(true),
            (_, true) => Some(false),
            _ => None,
        };
        let koma_id = self.koma;
        let command = match self.command {
            Command::Health => IpcCommand::Health,
            Command::Private => IpcCommand::WorkspacePrivate,
            Command::Profile(Profile::List) => IpcCommand::ProfileList,
            Command::Profile(Profile::New { name }) => IpcCommand::ProfileNew { id: name },
            Command::Profile(Profile::Open { name }) => IpcCommand::ProfileOpen { id: name },
            Command::Overview { command: None } => IpcCommand::Overview,
            Command::Overview {
                command: Some(Overview::Toggle),
            } => IpcCommand::OverviewToggle,
            Command::Sheet(Sheet::Focus { number }) => IpcCommand::SheetFocus {
                sheet_index: number,
            },
            Command::Sheet(Sheet::Close) => IpcCommand::SheetClose,
            Command::Sheet(Sheet::List) => IpcCommand::SheetList,
            Command::Sheet(Sheet::New { label, focus }) => IpcCommand::SheetNew { label, focus },
            Command::Preset(Preset::List) => IpcCommand::PresetList,
            Command::Preset(Preset::Save { name }) => IpcCommand::PresetSave { name },
            Command::Preset(Preset::Apply { name }) => IpcCommand::PresetApply { name },
            Command::List { sheet } => IpcCommand::KomaList(KomaListArgs { sheet_id: sheet }),
            Command::Focused => IpcCommand::KomaFocused,
            Command::Exists => IpcCommand::KomaExists {
                koma_id: koma_id.unwrap(),
            },
            Command::Focus => IpcCommand::KomaFocus {
                koma_id: koma_id.unwrap(),
            },
            Command::Move {
                left_of,
                right_of,
                end_of,
            } => IpcCommand::KomaMove {
                koma_id: koma_id.unwrap(),
                placement: left_of
                    .map(KomaPlacement::LeftOf)
                    .or_else(|| right_of.map(KomaPlacement::RightOf))
                    .or_else(|| end_of.map(KomaPlacement::EndOf))
                    .unwrap(),
            },
            Command::Resize { width } => IpcCommand::KomaResize {
                koma_id: koma_id.unwrap(),
                width,
            },
            Command::Close => IpcCommand::KomaClose {
                koma_id: koma_id.unwrap(),
            },
            Command::Web(Web::New { url }) => IpcCommand::WebNew {
                url,
                koma_api: koma_api.unwrap_or(false),
            },
            Command::Web(Web::Url) => IpcCommand::WebUrl(KomaTarget { koma_id }),
            Command::Web(Web::Devtools) => IpcCommand::WebDevtools,
            Command::Web(Web::Reload) => IpcCommand::WebReload(WebReloadArgs { koma_id, koma_api }),
            Command::Web(Web::Eval { expr }) => IpcCommand::WebEval(WebEvalArgs {
                koma_id,
                script: expr,
            }),
            Command::Web(Web::Script { .. }) => IpcCommand::WebScriptTarget(KomaTarget { koma_id }),
            Command::Web(Web::Snapshot) => IpcCommand::WebSnapshot(KomaTarget { koma_id }),
            Command::Web(Web::Read) => IpcCommand::WebRead(WebReadArgs {
                koma_id,
                within: self.within,
            }),
            Command::Web(Web::Text) => IpcCommand::WebText(KomaTarget { koma_id }),
            Command::Web(Web::Click { target }) => IpcCommand::WebClick(WebClickArgs {
                koma_id,
                selector: target,
            }),
            Command::Web(Web::Fill { target, text }) => IpcCommand::WebFill(WebFillArgs {
                koma_id,
                selector: target,
                text,
            }),
            Command::Web(Web::Type { text }) => IpcCommand::WebType(KomaTextArgs { koma_id, text }),
            Command::Terminal(Terminal::New { cwd, run }) => {
                IpcCommand::TerminalNew(TerminalNewArgs { cwd, run })
            }
            Command::Terminal(Terminal::Text) => IpcCommand::TerminalText(TerminalTextArgs {
                koma_id,
                lines: self.lines.map(NonZeroUsize::get),
            }),
            Command::Terminal(Terminal::Send { text }) => {
                IpcCommand::TerminalSend(KomaTextArgs { koma_id, text })
            }
            Command::Terminal(Terminal::Run { command }) => IpcCommand::TerminalRun(KomaTextArgs {
                koma_id,
                text: command,
            }),
            Command::Terminal(Terminal::Press { key }) => {
                IpcCommand::TerminalPress(KomaTextArgs { koma_id, text: key })
            }
            Command::Terminal(Terminal::Kill { signal }) => {
                IpcCommand::TerminalKill(KomaTextArgs {
                    koma_id,
                    text: signal,
                })
            }
            Command::Drawer(Drawer::List) => IpcCommand::DrawerList,
            Command::Drawer(Drawer::Keep { input }) => IpcCommand::DrawerKeep(DrawerKeepArgs {
                input,
                title: self.title,
                koma_api: koma_api.unwrap_or(false),
            }),
            Command::Drawer(Drawer::Place { id }) => IpcCommand::DrawerPlace(KomaTarget {
                koma_id: koma_id.or(id),
            }),
            Command::Drawer(Drawer::Discard { id }) => IpcCommand::DrawerDiscard(KomaTarget {
                koma_id: koma_id.or(id),
            }),
            Command::Drawer(Drawer::Restore) => IpcCommand::DrawerRestore,
            Command::Notification(Notification::Send { body }) => {
                IpcCommand::NotificationSend(NotificationSendArgs {
                    koma_id,
                    title: self.title,
                    body,
                })
            }
            Command::Notification(Notification::List) => IpcCommand::NotificationList {
                unread_only: self.unread,
            },
            Command::Notification(Notification::Read { id }) => IpcCommand::NotificationRead { id },
            Command::Notification(Notification::Unread { id }) => {
                IpcCommand::NotificationUnread { id }
            }
            Command::Bench(Bench::SpawnWebs { count }) => IpcCommand::BenchSpawnWebs { count },
            Command::Settings {
                command: Some(SettingsAction::Reload),
                ..
            } => IpcCommand::SettingsReload,
            Command::Settings { .. } => unreachable!("settings is handled locally"),
        };
        IpcRequest {
            caller_koma_id: std::env::var("KOMA_KOMA_ID").ok().filter(|s| !s.is_empty()),
            profile_id: std::env::var("KOMA_PROFILE").ok().filter(|s| !s.is_empty()),
            command,
        }
    }

    pub(super) fn web_script(&self) -> Option<(&Path, NonZeroU64)> {
        let Command::Web(Web::Script { file, timeout }) = &self.command else {
            return None;
        };
        Some((file, *timeout))
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
            (&["private"][..], IpcCommand::WorkspacePrivate),
            (&["profile", "list"], IpcCommand::ProfileList),
            (
                &["profile", "new", "work"],
                IpcCommand::ProfileNew { id: "work".into() },
            ),
            (
                &["profile", "open", "work"],
                IpcCommand::ProfileOpen { id: "work".into() },
            ),
            (&["overview"], IpcCommand::Overview),
            (&["overview", "toggle"], IpcCommand::OverviewToggle),
            (&["sheet", "close"], IpcCommand::SheetClose),
            (&["preset", "list"], IpcCommand::PresetList),
            (
                &["preset", "save", "Work"],
                IpcCommand::PresetSave {
                    name: "Work".into(),
                },
            ),
            (
                &["preset", "apply", "Work"],
                IpcCommand::PresetApply {
                    name: "Work".into(),
                },
            ),
            (&["web", "devtools"], IpcCommand::WebDevtools),
            (&["settings", "reload"], IpcCommand::SettingsReload),
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
            IpcCommand::WebScriptTarget(KomaTarget {
                koma_id: Some("koma-2".into())
            })
        );
        assert!(parse(&["web", "script", "--timeout", "0", "test.mjs"]).is_err());
    }

    #[test]
    fn validates_koma_targets_and_move_placement() {
        assert!(parse(&["close"]).is_err());
        assert!(parse(&["move", "--koma", "b1"]).is_err());
        assert!(parse(&["move", "--koma", "b1", "--left-of", "b2", "--end-of", "d1",]).is_err());
        assert!(parse(&["resize", "--koma", "b1", "--width", "NaN"]).is_err());
        let request = parse(&["--koma", "b1", "move", "--end-of", "d1"])
            .unwrap()
            .into_request();
        assert_eq!(
            request.command,
            IpcCommand::KomaMove {
                koma_id: "b1".into(),
                placement: KomaPlacement::EndOf("d1".into()),
            }
        );
    }

    #[test]
    fn keeps_flags_available_around_subcommands() {
        let args = parse(&[
            "--socket", "one", "--koma", "b1", "web", "read", "--socket", "two", "--within", "@e1",
            "--json",
        ])
        .unwrap();
        assert!(args.json);
        assert_eq!(args.socket.as_deref(), Some("two"));
        let req = args.into_request();
        assert_eq!(
            req.command,
            IpcCommand::WebRead(WebReadArgs {
                koma_id: Some("b1".into()),
                within: Some("@e1".into()),
            })
        );

        let req = parse(&[
            "drawer",
            "keep",
            "example.com",
            "--title",
            "Ex",
            "--no-koma-api",
        ])
        .unwrap()
        .into_request();
        assert_eq!(
            req.command,
            IpcCommand::DrawerKeep(DrawerKeepArgs {
                input: "example.com".into(),
                title: Some("Ex".into()),
                koma_api: false,
            })
        );

        let req = parse(&["notification", "send", "Build done", "--title", "-CI"])
            .unwrap()
            .into_request();
        assert_eq!(
            req.command,
            IpcCommand::NotificationSend(NotificationSendArgs {
                koma_id: None,
                title: Some("-CI".into()),
                body: "Build done".into(),
            })
        );
    }

    #[test]
    fn accepts_hyphenated_payloads_after_separator() {
        let req = parse(&["terminal", "send", "--", "-n"])
            .unwrap()
            .into_request();
        assert_eq!(
            req.command,
            IpcCommand::TerminalSend(KomaTextArgs {
                koma_id: None,
                text: "-n".into(),
            })
        );
    }

    #[test]
    fn validates_lines_and_conflicting_flags() {
        let req = parse(&["terminal", "text", "--lines", "20"])
            .unwrap()
            .into_request();
        assert_eq!(
            req.command,
            IpcCommand::TerminalText(TerminalTextArgs {
                koma_id: None,
                lines: Some(20),
            })
        );
        assert!(parse(&["terminal", "text", "--lines", "0"]).is_err());
        let err = parse(&["--koma-api", "web", "reload", "--no-koma-api"]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
    }
}
