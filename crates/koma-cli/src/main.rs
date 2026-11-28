//! `koma` CLI communicating with `koma-app` via IPC.

use std::io::IsTerminal;
use std::process::{Command, ExitCode};

use clap::Parser;
use koma_ipc::protocol::{
    DrawerItemContent, IpcCommand, IpcPayload, IpcRequest, IpcResponse, KomaSummary,
    KomaSummaryContent, WorkspaceCommand,
};
use koma_ipc::transport::{resolve_socket_path, send_request};
use makepad_micro_serde::SerJson;

mod args;
mod web_screenshot;
mod web_script;
mod workspace;

use args::{Args, CliAction, CliEnv};

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
    let path = koma_ipc::workspaces::config_path()
        .ok_or_else(|| "HOME is not set and KOMA_CONFIG is empty".to_string())?;
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

fn send_cli_request(
    socket_override: Option<&str>,
    mut req: IpcRequest,
) -> Result<IpcResponse, String> {
    let sock = resolve_socket_path(socket_override);
    if socket_override.is_some() && sock != resolve_socket_path(None) {
        req.caller_koma_id = None;
        req.profile_id = None;
    }
    send_request(&sock, &req)
}

fn run(args: Args) -> Result<(), String> {
    let json = args.json || !std::io::stdout().is_terminal();
    let socket_override = args.socket.clone();
    match args.into_action(CliEnv::from_process())? {
        CliAction::Settings { path_only } => run_settings(path_only, json),
        CliAction::WorkspaceList => workspace::list(json),
        CliAction::WebScript {
            request,
            file,
            timeout,
        } => {
            let resp = send_cli_request(socket_override.as_deref(), request)?;
            match resp.into_result()? {
                IpcPayload::WebScriptTarget(target) => web_script::run(&file, timeout, target),
                _ => Err("Expected a Web Koma script target from Koma".into()),
            }
        }
        CliAction::WebScreenshot { request, output } => {
            let resp = send_cli_request(socket_override.as_deref(), request)?;
            let target = match resp.into_result()? {
                IpcPayload::WebScriptTarget(target) => target,
                _ => return Err("Expected a Web Koma target from Koma".into()),
            };
            let path = web_screenshot::capture(output.as_deref(), &target)?;
            let path_str = path.display().to_string();
            if json {
                let output = web_screenshot::ScreenshotOutput {
                    koma_id: target.koma_id,
                    path: path_str,
                };
                println!("{}", output.to_json_response());
            } else {
                println!("{path_str}");
            }
            Ok(())
        }
        CliAction::Ipc(req) => {
            let is_health = matches!(req.command, IpcCommand::Workspace(WorkspaceCommand::Health));
            let resp = send_cli_request(socket_override.as_deref(), req)?;
            if json {
                println!("{}", resp.serialize_json());
                return resp.into_result().map(|_| ());
            }
            print_ipc_payload(resp.into_result()?, is_health)
        }
    }
}

fn print_ipc_payload(payload: IpcPayload, is_health: bool) -> Result<(), String> {
    match payload {
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
        IpcPayload::Text { text, .. } => println!("{text}"),
        IpcPayload::DrawerList { items } => {
            for item in items {
                let (kind, detail) = match item.content {
                    DrawerItemContent::Terminal { cwd } => ("terminal", cwd),
                    DrawerItemContent::Web { url } => ("web", Some(url)),
                };
                let extra = detail.map(|s| format!(" -> {s}")).unwrap_or_default();
                println!("[{kind}: {} ({}){extra}]", item.label, item.id);
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
    let (kind, extra) = match koma.content {
        KomaSummaryContent::Terminal => ("terminal", String::new()),
        KomaSummaryContent::Web { url } => ("web", format!(" -> {url}")),
    };
    println!(
        "{indent}[{kind}: {}{} ({}){extra}]",
        koma.label, focus, koma.id
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
    fn resolves_editor() {
        assert_eq!(resolve_editor(Some("nvim"), Some("vi")), "nvim");
        assert_eq!(resolve_editor(Some("  "), Some("hx")), "hx");
    }
}
