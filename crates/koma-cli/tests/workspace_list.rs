#![cfg(unix)]

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::Command;
use std::thread;

use koma_ipc::protocol::{
    IpcCommand, IpcPayload, IpcRequest, IpcResponse, WorkspaceCommand, WorkspaceEndpoint,
};
use makepad_micro_serde::{DeJson, SerJson};

struct TestDir(PathBuf);

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn discovers_live_workspaces_without_default_and_routes_to_selected_socket() {
    let root = TestDir(std::env::temp_dir().join(format!("koma-ws-{}", std::process::id())));
    fs::create_dir_all(root.0.join("private-workspaces")).unwrap();
    for profile in ["work", "closed"] {
        fs::create_dir_all(root.0.join("profiles").join(profile)).unwrap();
    }
    fs::write(
        root.0.join("profiles/closed/endpoint"),
        root.0.join("closed.sock").to_str().unwrap(),
    )
    .unwrap();

    let mut expected = Vec::new();
    let mut servers = Vec::new();
    for (id, marker, is_private) in [
        ("work", "profiles/work/endpoint", false),
        ("private-1", "private-workspaces/private-1", true),
        ("private-2", "private-workspaces/private-2", true),
    ] {
        let socket = root.0.join(format!("{id}.sock"));
        let listener = UnixListener::bind(&socket).unwrap();
        let socket = socket.to_str().unwrap().to_owned();
        fs::write(root.0.join(marker), &socket).unwrap();
        let endpoint = WorkspaceEndpoint {
            id: id.into(),
            is_private,
            socket,
        };
        expected.push(endpoint.clone());
        servers.push(thread::spawn(move || {
            let respond = |command, response: IpcResponse| {
                let (mut stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                let req = IpcRequest::deserialize_json(line.trim()).unwrap();
                assert_eq!(req, IpcRequest::new(IpcCommand::Workspace(command)));
                writeln!(stream, "{}", response.serialize_json()).unwrap();
            };
            respond(
                WorkspaceCommand::Inspect,
                IpcResponse::Ok(IpcPayload::WorkspaceEndpoint(endpoint.clone())),
            );
            if endpoint.id == "private-2" {
                respond(WorkspaceCommand::Health, IpcResponse::ok());
            }
        }));
    }

    let cli = || {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_koma"));
        cmd.env("KOMA_STATE_DIR", &root.0)
            .env("KOMA_SOCKET", &expected[0].socket)
            .env("KOMA_PROFILE", "work")
            .env("KOMA_KOMA_ID", "koma-1");
        cmd
    };
    let output = cli()
        .args(["workspace", "list", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    let json = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        json.trim(),
        format!("{{\"workspaces\":{}}}", expected.serialize_json())
    );

    let output = cli()
        .args(["--socket", &expected[2].socket, "health"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    for server in servers {
        server.join().unwrap();
    }
}
