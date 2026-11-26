//! `koma` CLI communicating with `koma-app` via IPC.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use clap::Parser;
use koma_ipc::protocol::{IpcCommand, IpcPayload, KomaSummary, WorkspaceCommand};
use koma_ipc::transport::{resolve_socket_path, send_request};
use makepad_micro_serde::SerJson;

mod args;
mod web_screenshot;
mod web_script;
mod workspace;

use args::Args;

fn resolve_config_path(koma_config: Option<&str>, home: Option<&str>) -> Result<PathBuf, String> {
    if let Some(path) = koma_config.map(str::trim).filter(|p| !p.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    let home = home
        .map(str::trim)
        .filter(|h| !h.is_empty())
        .ok_or_else(|| "HOME is not set and KOMA_CONFIG is empty".to_string())?;
    Ok(Path::new(home).join(".config/koma/config.toml"))
}

fn resolve_editor(visual: Option<&str>, editor: Option<&str>) -> String {
    for candidate in [visual, editor] {
        if let Some(val) = candidate.map(str::trim).filter(|s| !s.is_empty()) {
            return val.to_string();
        }
    }
    if cfg!(windows) {
        "notepad".into()
    } else {
        "vi".into()
    }
}

fn run_settings(path_only: bool, json: bool) -> Result<(), String> {
    let koma_config = std::env::var("KOMA_CONFIG").ok();
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok();
    let path = resolve_config_path(koma_config.as_deref(), home.as_deref())?;
    if path_only {
        if json {
            println!(
                "{{\"ok\":true,\"path\":{}}}",
                path.display().to_string().serialize_json()
            );
        } else {
            println!("{}", path.display());
        }
        return Ok(());
    }

    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    }
    if !path.exists() {
        std::fs::write(&path, "# Koma configuration\n")
            .map_err(|e| format!("Failed to create {}: {e}", path.display()))?;
    }

    let visual = std::env::var("VISUAL").ok();
    let editor = std::env::var("EDITOR").ok();
    let editor_cmd = resolve_editor(visual.as_deref(), editor.as_deref());
    let mut parts = editor_cmd.split_whitespace();
    let bin = parts
        .next()
        .ok_or_else(|| "Resolved editor command is empty".to_string())?;
    let status = Command::new(bin)
        .args(parts)
        .arg(&path)
        .status()
        .map_err(|e| format!("Failed to launch '{editor_cmd}': {e}"))?;
    if !status.success() {
        return Err(format!("Editor exited with status {status}"));
    }
    Ok(())
}

fn run(args: Args) -> Result<(), String> {
    let json = args.json || !std::io::stdout().is_terminal();
    if let Some(path_only) = args.settings_path_only() {
        return run_settings(path_only, json);
    }
    if args.lists_workspaces() {
        return workspace::list(json);
    }
    let sock = resolve_socket_path(args.socket.as_deref());
    let is_remote = args.socket.is_some() && sock != resolve_socket_path(None);
    let script = args
        .web_script()
        .map(|(path, timeout)| (path.to_path_buf(), timeout));
    let screenshot = args.web_screenshot().map(|p| p.map(Path::to_path_buf));
    let mut req = args.into_request();
    if is_remote {
        req.caller_koma_id = None;
        req.profile_id = None;
    }
    let is_health = matches!(req.command, IpcCommand::Workspace(WorkspaceCommand::Health));
    let resp = send_request(&sock, &req)?;
    if let Some((file, timeout)) = script {
        return match resp.into_result()? {
            IpcPayload::WebScriptTarget(target) => web_script::run(&file, timeout, target),
            _ => Err("Expected a Web Koma script target from Koma".into()),
        };
    }
    if let Some(output) = screenshot {
        let target = match resp.into_result()? {
            IpcPayload::WebScriptTarget(target) => target,
            _ => return Err("Expected a Web Koma target from Koma".into()),
        };
        let path = web_screenshot::capture(output.as_deref(), &target)?;
        let path_str = path.display().to_string();
        if json {
            let payload = IpcPayload::WebScreenshot {
                koma_id: target.koma_id,
                path: path_str,
            };
            println!(
                "{}",
                koma_ipc::protocol::IpcResponse::Ok(payload).serialize_json()
            );
        } else {
            println!("{path_str}");
        }
        return Ok(());
    }

    if json {
        println!("{}", resp.serialize_json());
        return resp.into_result().map(|_| ());
    }

    match resp.into_result()? {
        IpcPayload::Ack => {
            if is_health {
                println!("healthy");
            } else {
                println!("ok");
            }
        }
        IpcPayload::ProfileList { profiles } => {
            for id in profiles {
                println!("{id}");
            }
        }
        IpcPayload::WorkspaceEndpoint(_) => return Err("Unexpected Workspace endpoint".into()),
        IpcPayload::Overview { sheets } => {
            for sheet in sheets {
                let mark = if sheet.is_active { "*" } else { " " };
                println!("{mark} {} ({})", sheet.label, sheet.id);
                for b in sheet.komas {
                    print_koma("    ", b);
                }
            }
        }
        IpcPayload::KomaList { komas, .. } => {
            for koma in komas {
                print_koma("", koma);
            }
        }
        IpcPayload::KomaFocused(data) => {
            if let Some(koma) = data.koma {
                print_koma("", koma);
            }
        }
        IpcPayload::KomaExists { exists, .. } => println!("{exists}"),
        IpcPayload::KomaResized { width, .. } => println!("{width}"),
        IpcPayload::SheetList { sheets } => {
            for sheet in sheets {
                let active = if sheet.is_active { "*" } else { " " };
                println!(
                    "{active} {} ({}, {} Komas)",
                    sheet.label, sheet.id, sheet.koma_count
                );
            }
        }
        IpcPayload::SheetId { sheet_id } => println!("{sheet_id}"),
        IpcPayload::PresetList { presets } => {
            for p in presets {
                let kind = if p.personal { "personal" } else { "config" };
                println!("[{kind}] {}: {}", p.name, p.komas.join(", "));
            }
        }
        IpcPayload::WebKoma(data) => {
            if let Some(url) = data.url {
                println!("{} -> {url}", data.koma_id);
            } else {
                println!("{}", data.koma_id);
            }
        }
        IpcPayload::KomaId { koma_id } => println!("{koma_id}"),
        IpcPayload::WebUrl { url, .. } => println!("{url}"),
        IpcPayload::WebEval { value, .. } => println!("{value}"),
        IpcPayload::WebSnapshot(data) => println!("{}", data.snapshot.unwrap_or_default()),
        IpcPayload::WebScriptTarget(_) => return Err("Unexpected Web Koma script target".into()),
        IpcPayload::WebRead(data) => println!("{}", data.markdown.unwrap_or_default()),
        IpcPayload::WebScreenshot { path, .. } => println!("{path}"),
        IpcPayload::Text { text, .. } => println!("{text}"),
        IpcPayload::DrawerList { items } => {
            for item in items {
                let extra = item
                    .url
                    .or(item.cwd)
                    .map(|s| format!(" -> {s}"))
                    .unwrap_or_default();
                println!("[{}: {} ({}){extra}]", item.kind, item.label, item.id);
            }
        }
        IpcPayload::NotificationList { notifications } => {
            for n in notifications {
                let mark = if n.unread { "*" } else { " " };
                println!("{mark} {} [{}] {}: {}", n.id, n.koma_id, n.title, n.body);
            }
        }
        IpcPayload::PresetSaved { .. }
        | IpcPayload::PresetApplied { .. }
        | IpcPayload::NotificationSent { .. }
        | IpcPayload::BenchSpawnWebs { .. } => println!("ok"),
    }
    Ok(())
}

fn print_koma(indent: &str, koma: KomaSummary) {
    let focus = if koma.is_focused { "*" } else { "" };
    let extra = koma.url.map(|url| format!(" -> {url}")).unwrap_or_default();
    println!(
        "{indent}[{}: {}{} ({}){extra}]",
        koma.kind, koma.label, focus, koma.id
    );
}

fn main() -> ExitCode {
    let args = match Args::try_parse().and_then(Args::validate) {
        Ok(args) => args,
        Err(err) => {
            let status = if err.use_stderr() {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            };
            let _ = err.print();
            return status;
        }
    };
    if let Err(err) = run(args) {
        eprintln!("{err}");
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_config_path_and_editor() {
        assert_eq!(
            resolve_config_path(Some("/tmp/custom.toml"), Some("/Users/test")).unwrap(),
            PathBuf::from("/tmp/custom.toml")
        );
        assert_eq!(
            resolve_config_path(None, Some("/Users/test")).unwrap(),
            PathBuf::from("/Users/test/.config/koma/config.toml")
        );
        assert!(resolve_config_path(None, None).is_err());

        assert_eq!(resolve_editor(Some("nvim"), Some("vi")), "nvim");
        assert_eq!(resolve_editor(Some("  "), Some("hx")), "hx");
    }
}
