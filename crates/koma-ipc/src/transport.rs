//! Platform-abstracted IPC client transport (`UnixStream` on Unix, local TCP port file elsewhere).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use makepad_micro_serde::*;

use crate::protocol::{IpcRequest, IpcResponse};

#[cfg(unix)]
pub const DEFAULT_SOCKET_PATH: &str = "/tmp/koma.sock";
#[cfg(not(unix))]
pub const DEFAULT_SOCKET_PATH: &str = "koma.port";

/// Resolves IPC endpoint path in order: explicit `--socket`, `$KOMA_SOCKET`, OS default.
pub fn resolve_socket_path(explicit: Option<&str>) -> String {
    if let Some(path) = explicit.filter(|s| !s.is_empty()) {
        return path.to_string();
    }
    if let Ok(env_path) = std::env::var("KOMA_SOCKET") {
        if !env_path.is_empty() {
            return env_path;
        }
    }
    #[cfg(unix)]
    {
        DEFAULT_SOCKET_PATH.to_string()
    }
    #[cfg(not(unix))]
    {
        std::env::temp_dir()
            .join(DEFAULT_SOCKET_PATH)
            .display()
            .to_string()
    }
}

fn round_trip_stream<S: Read + Write>(
    mut stream: S,
    req: &IpcRequest,
) -> Result<IpcResponse, String> {
    let payload = req.serialize_json();
    stream
        .write_all(payload.as_bytes())
        .and_then(|_| stream.write_all(b"\n"))
        .and_then(|_| stream.flush())
        .map_err(|e| format!("Failed to write IPC request: {e}"))?;

    let mut line = String::new();
    let mut reader = BufReader::new(stream);
    reader
        .read_line(&mut line)
        .map_err(|e| format!("Failed to read IPC response: {e}"))?;
    if line.trim().is_empty() {
        return Err("Empty IPC response from app".into());
    }
    IpcResponse::deserialize_json(line.trim())
        .map_err(|e| format!("Invalid IPC response JSON: {e:?}"))
}

pub fn send_request(socket_path: &str, req: &IpcRequest) -> Result<IpcResponse, String> {
    send_request_with_timeout(socket_path, req, Duration::from_secs(10))
}

#[cfg(unix)]
pub fn send_request_with_timeout(
    socket_path: &str,
    req: &IpcRequest,
    timeout: Duration,
) -> Result<IpcResponse, String> {
    use std::os::unix::net::UnixStream;

    let stream = UnixStream::connect(socket_path)
        .map_err(|e| format!("Failed to connect to {socket_path}: {e}"))?;
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    round_trip_stream(stream, req)
}

#[cfg(not(unix))]
pub use send_request_via_port_file_with_timeout as send_request_with_timeout;

pub fn send_request_via_port_file(
    port_file: &str,
    req: &IpcRequest,
) -> Result<IpcResponse, String> {
    send_request_via_port_file_with_timeout(port_file, req, Duration::from_secs(10))
}

pub fn send_request_via_port_file_with_timeout(
    port_file: &str,
    req: &IpcRequest,
    timeout: Duration,
) -> Result<IpcResponse, String> {
    let port_str = std::fs::read_to_string(port_file)
        .map_err(|e| format!("Failed to read IPC port file {port_file}: {e}"))?;
    let port: u16 = port_str
        .trim()
        .parse()
        .map_err(|_| format!("Invalid IPC port in {port_file}"))?;
    let stream = TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), timeout)
        .map_err(|e| format!("Failed to connect to 127.0.0.1:{port}: {e}"))?;
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    round_trip_stream(stream, req)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::protocol::{IpcCommand, WorkspaceCommand};
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;

    #[test]
    fn uds_round_trip() {
        let sock = std::env::temp_dir().join(format!("koma-test-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&sock);
        let listener = UnixListener::bind(&sock).expect("bind test uds");

        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut line = String::new();
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            reader.read_line(&mut line).expect("read");
            let req = IpcRequest::deserialize_json(line.trim()).expect("parse req");
            assert_eq!(req.command, IpcCommand::Workspace(WorkspaceCommand::Health));
            let resp = IpcResponse::ok();
            let out = format!("{}\n", resp.serialize_json());
            stream.write_all(out.as_bytes()).expect("write");
        });

        let resp = send_request(
            sock.to_str().unwrap(),
            &IpcRequest::new(IpcCommand::Workspace(WorkspaceCommand::Health)),
        )
        .unwrap();
        assert!(resp.is_ok());
        handle.join().unwrap();
        let _ = std::fs::remove_file(&sock);
    }

    #[test]
    fn tcp_port_file_round_trip() {
        let port_file = std::env::temp_dir().join(format!("koma-test-{}.port", std::process::id()));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind tcp");
        let port = listener.local_addr().unwrap().port();
        std::fs::write(&port_file, port.to_string()).unwrap();

        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut line = String::new();
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            reader.read_line(&mut line).expect("read");
            let req = IpcRequest::deserialize_json(line.trim()).expect("parse req");
            assert_eq!(req.command, IpcCommand::Workspace(WorkspaceCommand::Health));
            let out = format!("{}\n", IpcResponse::ok().serialize_json());
            stream.write_all(out.as_bytes()).expect("write");
        });

        let resp = send_request_via_port_file(
            port_file.to_str().unwrap(),
            &IpcRequest::new(IpcCommand::Workspace(WorkspaceCommand::Health)),
        )
        .unwrap();
        assert!(resp.is_ok());
        handle.join().unwrap();
        let _ = std::fs::remove_file(&port_file);
    }
}
