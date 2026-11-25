use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::time::Duration;

use koma_ipc::protocol::{IpcCommand, IpcPayload, IpcRequest, WorkspaceCommand, WorkspaceEndpoint};
use koma_ipc::transport::send_request_with_timeout;
use koma_ipc::workspaces::{state_root, PRIVATE_ENDPOINTS};
use makepad_micro_serde::SerJson;

pub(super) fn list(json: bool) -> Result<(), String> {
    let root = state_root()?;
    let mut workspaces = Vec::new();
    let request = IpcRequest::new(IpcCommand::Workspace(WorkspaceCommand::Inspect));
    for (directory, is_private) in [("profiles", false), (PRIVATE_ENDPOINTS, true)] {
        let dir = root.join(directory);
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == ErrorKind::NotFound => continue,
            Err(err) => return Err(format!("Could not list {}: {err}", dir.display())),
        };
        for entry in entries {
            let entry = entry.map_err(|err| format!("Could not list {}: {err}", dir.display()))?;
            let path = entry.path();
            let marker = if is_private {
                path.clone()
            } else {
                path.join("endpoint")
            };
            if let Some(workspace) = inspect(&marker, &request).filter(|workspace| {
                workspace.is_private == is_private
                    && path.file_name().and_then(|name| name.to_str()) == Some(&workspace.id)
            }) {
                workspaces.push(workspace);
            }
        }
    }
    workspaces.sort_by(|a, b| (a.is_private, &a.id).cmp(&(b.is_private, &b.id)));
    if json {
        println!("{{\"workspaces\":{}}}", workspaces.serialize_json());
    } else {
        for workspace in workspaces {
            let kind = if workspace.is_private {
                "private"
            } else {
                "profile"
            };
            println!("{kind}\t{}\t{}", workspace.id, workspace.socket);
        }
    }
    Ok(())
}

fn inspect(marker: &Path, request: &IpcRequest) -> Option<WorkspaceEndpoint> {
    if !marker.is_file() || marker.extension().is_some() {
        return None;
    }
    let socket = fs::read_to_string(marker).ok()?;
    let response = send_request_with_timeout(&socket, request, Duration::from_millis(500)).ok()?;
    match response.into_result().ok()? {
        IpcPayload::WorkspaceEndpoint(workspace) if workspace.socket == socket => Some(workspace),
        _ => None,
    }
}
