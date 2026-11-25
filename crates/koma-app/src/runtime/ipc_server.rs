//! Background Unix Domain Socket IPC server waking the Makepad UI thread via `SignalToUI`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

use koma_ipc::protocol::{IpcRequest, IpcResponse};
use makepad_micro_serde::*;
use makepad_widgets::makepad_platform::thread::SignalToUI;

pub struct PendingIpc {
    pub req: IpcRequest,
    pub reply: mpsc::SyncSender<IpcResponse>,
}

pub struct IpcServer {
    rx: mpsc::Receiver<PendingIpc>,
    socket_path: String,
    stop: Arc<AtomicBool>,
    _endpoint_lock: Option<std::fs::File>,
    private_marker: Option<PathBuf>,
}

impl IpcServer {
    pub fn start(socket_path: String, shared_endpoint: bool) -> Result<Self, String> {
        let endpoint_lock = if shared_endpoint {
            let lock_path = format!("{socket_path}.lock");
            Some(
                super::profiles::try_lock(Path::new(&lock_path))?
                    .ok_or_else(|| format!("IPC endpoint already in use: {socket_path}"))?,
            )
        } else {
            None
        };
        let (tx, rx) = mpsc::channel::<PendingIpc>();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);

        #[cfg(unix)]
        {
            use std::os::unix::net::UnixListener;
            use std::time::Duration;

            let _ = std::fs::remove_file(&socket_path);
            let listener = UnixListener::bind(&socket_path)
                .map_err(|e| format!("Failed to bind IPC socket {socket_path}: {e}"))?;
            listener
                .set_nonblocking(true)
                .map_err(|e| format!("Failed to configure IPC socket: {e}"))?;

            std::thread::Builder::new()
                .name("koma-ipc-server".into())
                .spawn(move || {
                    while !stop_flag.load(Ordering::Relaxed) {
                        match listener.accept() {
                            Ok((stream, _)) => {
                                let tx = tx.clone();
                                std::thread::spawn(move || {
                                    let timeout = Duration::from_secs(10);
                                    let _ = stream.set_read_timeout(Some(timeout));
                                    let _ = stream.set_write_timeout(Some(timeout));
                                    if let Ok(cloned) = stream.try_clone() {
                                        handle_client(cloned, stream, tx);
                                    }
                                });
                            }
                            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                                std::thread::sleep(Duration::from_millis(25));
                            }
                            Err(_) => break,
                        }
                    }
                })
                .map_err(|e| format!("Failed to spawn IPC accept thread: {e}"))?;
        }

        #[cfg(not(unix))]
        {
            use std::net::TcpListener;
            use std::time::Duration;

            let listener = TcpListener::bind("127.0.0.1:0")
                .map_err(|e| format!("Failed to bind IPC TCP listener: {e}"))?;
            let port = listener
                .local_addr()
                .map_err(|e| format!("Failed to inspect IPC TCP port: {e}"))?
                .port();
            std::fs::write(&socket_path, port.to_string())
                .map_err(|e| format!("Failed to write IPC port file {socket_path}: {e}"))?;
            listener
                .set_nonblocking(true)
                .map_err(|e| format!("Failed to configure IPC TCP listener: {e}"))?;

            std::thread::Builder::new()
                .name("koma-ipc-server".into())
                .spawn(move || {
                    while !stop_flag.load(Ordering::Relaxed) {
                        match listener.accept() {
                            Ok((stream, _)) => {
                                let tx = tx.clone();
                                std::thread::spawn(move || {
                                    let timeout = Duration::from_secs(10);
                                    let _ = stream.set_read_timeout(Some(timeout));
                                    let _ = stream.set_write_timeout(Some(timeout));
                                    if let Ok(cloned) = stream.try_clone() {
                                        handle_client(cloned, stream, tx);
                                    }
                                });
                            }
                            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                                std::thread::sleep(Duration::from_millis(25));
                            }
                            Err(_) => break,
                        }
                    }
                })
                .map_err(|e| format!("Failed to spawn IPC accept thread: {e}"))?;
        }

        Ok(Self {
            rx,
            socket_path,
            stop,
            _endpoint_lock: endpoint_lock,
            private_marker: None,
        })
    }

    pub fn publish_private_endpoint(&mut self, id: &str) -> Result<(), String> {
        use koma_ipc::workspaces::{state_root, PRIVATE_ENDPOINTS};

        let dir = state_root()?.join(PRIVATE_ENDPOINTS);
        let marker = dir.join(id);
        let temp = marker.with_extension("tmp");
        std::fs::create_dir_all(&dir)
            .and_then(|()| std::fs::write(&temp, &self.socket_path))
            .and_then(|()| std::fs::rename(&temp, &marker))
            .map_err(|err| format!("Could not publish {}: {err}", marker.display()))?;
        self.private_marker = Some(marker);
        Ok(())
    }

    pub fn try_recv(&self) -> Option<PendingIpc> {
        self.rx.try_recv().ok()
    }
}

fn handle_client<R: std::io::Read, W: std::io::Write>(
    reader: R,
    mut writer: W,
    tx: mpsc::Sender<PendingIpc>,
) {
    use std::io::{BufRead, BufReader};
    use std::time::Duration;

    let mut line = String::new();
    let resp = match BufReader::new(reader).read_line(&mut line) {
        Ok(0) => return,
        Ok(_) => match IpcRequest::deserialize_json(line.trim()) {
            Ok(req) => {
                let (reply_tx, reply_rx) = mpsc::sync_channel(1);
                if tx
                    .send(PendingIpc {
                        req,
                        reply: reply_tx,
                    })
                    .is_err()
                {
                    IpcResponse::err("App IPC channel closed")
                } else {
                    SignalToUI::set_ui_signal();
                    reply_rx
                        .recv_timeout(Duration::from_secs(10))
                        .unwrap_or_else(|_| IpcResponse::err("IPC request timed out"))
                }
            }
            Err(e) => IpcResponse::err(format!("Invalid IPC request JSON: {e:?}")),
        },
        Err(e) => IpcResponse::err(format!("IPC read error: {e}")),
    };

    let out = format!("{}\n", resp.serialize_json());
    let _ = writer.write_all(out.as_bytes());
    let _ = writer.flush();
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(marker) = &self.private_marker {
            let _ = std::fs::remove_file(marker);
        }
        let _ = std::fs::remove_file(&self.socket_path);
    }
}
