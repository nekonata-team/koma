use std::num::{NonZeroU64, NonZeroUsize};
use std::path::{Path, PathBuf};

use clap::{error::ErrorKind, CommandFactory, Parser, Subcommand};
use koma_ipc::protocol::{
    DrawerKeepArgs, IpcCommand, IpcRequest, KomaListArgs, KomaPlacement, KomaTarget, KomaTextArgs,
    NotificationSendArgs, TerminalNewArgs, TerminalTextArgs, WebClickArgs, WebEvalArgs,
    WebFillArgs, WebReadArgs, WebReloadArgs,
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
        /// Enable KomaAPI for the Web Koma.
        #[arg(long)]
        koma_api: bool,
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
    Snapshot {
        #[command(flatten)]
        target: OptionalKoma,
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
}

#[derive(Debug, Subcommand)]
enum Terminal {
    New {
        cwd: Option<String>,
        /// Run an initial command in the new interactive shell.
        #[arg(long)]
        run: Option<String>,
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
            Command::Resize { width, .. } if !width.is_finite() || *width <= 0.0 => {
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
            Command::Exists { target } => IpcCommand::KomaExists {
                koma_id: target.koma,
            },
            Command::Focus { target } => IpcCommand::KomaFocus {
                koma_id: target.koma,
            },
            Command::Move {
                target,
                left_of,
                right_of,
                end_of,
            } => IpcCommand::KomaMove {
                koma_id: target.koma,
                placement: left_of
                    .map(KomaPlacement::LeftOf)
                    .or_else(|| right_of.map(KomaPlacement::RightOf))
                    .or_else(|| end_of.map(KomaPlacement::EndOf))
                    .unwrap(),
            },
            Command::Resize { target, width } => IpcCommand::KomaResize {
                koma_id: target.koma,
                width,
            },
            Command::Close { target } => IpcCommand::KomaClose {
                koma_id: target.koma,
            },
            Command::Web(Web::New { url, koma_api }) => IpcCommand::WebNew { url, koma_api },
            Command::Web(Web::Url { target }) => IpcCommand::WebUrl(KomaTarget {
                koma_id: target.koma,
            }),
            Command::Web(Web::Devtools) => IpcCommand::WebDevtools,
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
                IpcCommand::WebReload(WebReloadArgs {
                    koma_id: target.koma,
                    koma_api,
                })
            }
            Command::Web(Web::Eval { target, expr }) => IpcCommand::WebEval(WebEvalArgs {
                koma_id: target.koma,
                script: expr,
            }),
            Command::Web(Web::Script { target, .. }) => IpcCommand::WebScriptTarget(KomaTarget {
                koma_id: target.koma,
            }),
            Command::Web(Web::Snapshot { target }) => IpcCommand::WebSnapshot(KomaTarget {
                koma_id: target.koma,
            }),
            Command::Web(Web::Read { target, within }) => IpcCommand::WebRead(WebReadArgs {
                koma_id: target.koma,
                within,
            }),
            Command::Web(Web::Text { target }) => IpcCommand::WebText(KomaTarget {
                koma_id: target.koma,
            }),
            Command::Web(Web::Click { koma, target }) => IpcCommand::WebClick(WebClickArgs {
                koma_id: koma.koma,
                selector: target,
            }),
            Command::Web(Web::Fill { koma, target, text }) => IpcCommand::WebFill(WebFillArgs {
                koma_id: koma.koma,
                selector: target,
                text,
            }),
            Command::Web(Web::Type { target, text }) => IpcCommand::WebType(KomaTextArgs {
                koma_id: target.koma,
                text,
            }),
            Command::Terminal(Terminal::New { cwd, run }) => {
                IpcCommand::TerminalNew(TerminalNewArgs { cwd, run })
            }
            Command::Terminal(Terminal::Text { target, lines }) => {
                IpcCommand::TerminalText(TerminalTextArgs {
                    koma_id: target.koma,
                    lines: lines.map(NonZeroUsize::get),
                })
            }
            Command::Terminal(Terminal::Send { target, text }) => {
                IpcCommand::TerminalSend(KomaTextArgs {
                    koma_id: target.koma,
                    text,
                })
            }
            Command::Terminal(Terminal::Run { target, command }) => {
                IpcCommand::TerminalRun(KomaTextArgs {
                    koma_id: target.koma,
                    text: command,
                })
            }
            Command::Terminal(Terminal::Press { target, key }) => {
                IpcCommand::TerminalPress(KomaTextArgs {
                    koma_id: target.koma,
                    text: key,
                })
            }
            Command::Terminal(Terminal::Kill { target, signal }) => {
                IpcCommand::TerminalKill(KomaTextArgs {
                    koma_id: target.koma,
                    text: signal,
                })
            }
            Command::Drawer(Drawer::List) => IpcCommand::DrawerList,
            Command::Drawer(Drawer::Keep {
                input,
                title,
                koma_api,
            }) => IpcCommand::DrawerKeep(DrawerKeepArgs {
                input,
                title,
                koma_api,
            }),
            Command::Drawer(Drawer::Place { target, id }) => IpcCommand::DrawerPlace(KomaTarget {
                koma_id: target.koma.or(id),
            }),
            Command::Drawer(Drawer::Discard { target, id }) => {
                IpcCommand::DrawerDiscard(KomaTarget {
                    koma_id: target.koma.or(id),
                })
            }
            Command::Drawer(Drawer::Restore) => IpcCommand::DrawerRestore,
            Command::Notification(Notification::Send {
                target,
                body,
                title,
            }) => IpcCommand::NotificationSend(NotificationSendArgs {
                koma_id: target.koma,
                title,
                body,
            }),
            Command::Notification(Notification::List { unread }) => IpcCommand::NotificationList {
                unread_only: unread,
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
        let Command::Web(Web::Script { file, timeout, .. }) = &self.command else {
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
        assert!(parse(&["close", "--koma", "  "]).is_err());
        assert!(parse(&["move", "--koma", "b1"]).is_err());
        assert!(parse(&["move", "--koma", "b1", "--left-of", "b2", "--end-of", "d1"]).is_err());
        assert!(parse(&["resize", "--koma", "b1", "--width", "NaN"]).is_err());
        let request = parse(&["move", "--koma", "b1", "--end-of", "d1"])
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
            "--koma-api",
        ])
        .unwrap()
        .into_request();
        assert_eq!(
            req.command,
            IpcCommand::DrawerKeep(DrawerKeepArgs {
                input: "example.com".into(),
                title: Some("Ex".into()),
                koma_api: true,
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
