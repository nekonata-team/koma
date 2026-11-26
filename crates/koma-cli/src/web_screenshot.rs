use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use koma_ipc::protocol::WebScriptTargetData;
use makepad_micro_serde::{DeJson, JsonValue};

const TIMEOUT: Duration = Duration::from_secs(10);

pub(super) fn capture(
    output: Option<&Path>,
    target: &WebScriptTargetData,
) -> Result<PathBuf, String> {
    let dest = match output {
        Some(path) => path.to_path_buf(),
        None => {
            let ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            std::env::temp_dir().join(format!("koma-{}-{ms}.png", target.koma_id))
        }
    };
    let png = capture_png_over_cdp(target.port, &target.target_id)?;
    std::fs::write(&dest, &png)
        .map_err(|e| format!("Failed to write screenshot to {}: {e}", dest.display()))?;
    Ok(dest)
}

fn capture_png_over_cdp(port: u16, target_id: &str) -> Result<Vec<u8>, String> {
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT)
        .map_err(|e| format!("Cannot connect to CDP on 127.0.0.1:{port}: {e}"))?;
    let _ = stream.set_read_timeout(Some(TIMEOUT));
    let _ = stream.set_write_timeout(Some(TIMEOUT));

    let req = format!(
        "GET /devtools/page/{target_id} HTTP/1.1\r\n\
         Host: 127.0.0.1:{port}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\r\n"
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("CDP handshake write failed: {e}"))?;

    let mut reader = WsReader::new(stream);
    reader.read_http_upgrade()?;

    let payload = r#"{"id":1,"method":"Page.captureScreenshot","params":{"format":"png"}}"#;
    write_ws_frame(&mut reader.stream, 0x1, payload.as_bytes())?;

    loop {
        let (opcode, data) = reader.read_message()?;
        match opcode {
            0x9 => write_ws_frame(&mut reader.stream, 0xA, &data)?,
            0x8 => return Err("CDP connection closed before screenshot completed".into()),
            0x1 => {
                let text = String::from_utf8(data)
                    .map_err(|e| format!("Invalid UTF-8 in CDP response: {e}"))?;
                let val = JsonValue::deserialize_json(&text)
                    .map_err(|e| format!("Invalid JSON in CDP response: {e:?}"))?;
                if val.key("id").and_then(|v| v.as_u64()) != Some(1) {
                    continue;
                }
                if let Some(err) = val.key("error") {
                    let msg = err
                        .key("message")
                        .and_then(|v| v.string())
                        .map_or("Page.captureScreenshot failed", String::as_str);
                    return Err(format!("CDP error: {msg}"));
                }
                let b64 = val
                    .key("result")
                    .and_then(|r| r.key("data"))
                    .and_then(|d| d.string())
                    .ok_or_else(|| "CDP response missing screenshot data".to_string())?;
                return decode_base64(b64);
            }
            _ => {}
        }
    }
}

struct WsReader {
    stream: TcpStream,
    buf: Vec<u8>,
    pos: usize,
}

impl WsReader {
    fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            buf: Vec::new(),
            pos: 0,
        }
    }

    fn read_http_upgrade(&mut self) -> Result<(), String> {
        let mut chunk = [0u8; 1024];
        loop {
            if let Some(end) = self.buf[self.pos..]
                .windows(4)
                .position(|w| w == b"\r\n\r\n")
            {
                let header_end = self.pos + end;
                let status_line = String::from_utf8_lossy(&self.buf[self.pos..header_end])
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_string();
                self.pos = header_end + 4;
                if !status_line.contains(" 101 ") {
                    return Err(format!("CDP WebSocket upgrade failed: {status_line}"));
                }
                return Ok(());
            }
            let n = self
                .stream
                .read(&mut chunk)
                .map_err(|e| format!("CDP handshake read failed: {e}"))?;
            if n == 0 {
                return Err("CDP closed connection during handshake".into());
            }
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }

    fn read_exact_bytes(&mut self, len: usize) -> Result<Vec<u8>, String> {
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            if self.pos < self.buf.len() {
                let take = (len - out.len()).min(self.buf.len() - self.pos);
                out.extend_from_slice(&self.buf[self.pos..self.pos + take]);
                self.pos += take;
                if self.pos == self.buf.len() {
                    self.buf.clear();
                    self.pos = 0;
                }
            } else {
                let mut tmp = vec![0u8; (len - out.len()).min(65536)];
                let n = self
                    .stream
                    .read(&mut tmp)
                    .map_err(|e| format!("CDP read failed: {e}"))?;
                if n == 0 {
                    return Err("Unexpected EOF from CDP".into());
                }
                out.extend_from_slice(&tmp[..n]);
            }
        }
        Ok(out)
    }

    fn read_message(&mut self) -> Result<(u8, Vec<u8>), String> {
        let mut message_opcode = None;
        let mut out = Vec::new();
        loop {
            let hdr = self.read_exact_bytes(2)?;
            let fin = (hdr[0] & 0x80) != 0;
            let opcode = hdr[0] & 0x0f;
            let masked = (hdr[1] & 0x80) != 0;
            let len7 = (hdr[1] & 0x7f) as usize;
            let payload_len = match len7 {
                126 => {
                    let b = self.read_exact_bytes(2)?;
                    u16::from_be_bytes([b[0], b[1]]) as usize
                }
                127 => {
                    let b = self.read_exact_bytes(8)?;
                    u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]) as usize
                }
                n => n,
            };
            let mask = if masked {
                let m = self.read_exact_bytes(4)?;
                Some([m[0], m[1], m[2], m[3]])
            } else {
                None
            };
            let mut payload = self.read_exact_bytes(payload_len)?;
            if let Some(m) = mask {
                for (i, byte) in payload.iter_mut().enumerate() {
                    *byte ^= m[i % 4];
                }
            }
            if opcode >= 0x8 {
                return Ok((opcode, payload));
            }
            if message_opcode.is_none() {
                message_opcode = Some(opcode);
            }
            out.extend_from_slice(&payload);
            if fin {
                return Ok((message_opcode.unwrap_or(opcode), out));
            }
        }
    }
}

fn write_ws_frame(stream: &mut TcpStream, opcode: u8, payload: &[u8]) -> Result<(), String> {
    let mut frame = Vec::with_capacity(14 + payload.len());
    frame.push(0x80 | (opcode & 0x0f));
    let len = payload.len();
    if len < 126 {
        frame.push(0x80 | len as u8);
    } else if len <= u16::MAX as usize {
        frame.push(0x80 | 126);
        frame.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        frame.push(0x80 | 127);
        frame.extend_from_slice(&(len as u64).to_be_bytes());
    }
    let mask = [0x12, 0x34, 0x56, 0x78];
    frame.extend_from_slice(&mask);
    for (i, &b) in payload.iter().enumerate() {
        frame.push(b ^ mask[i % 4]);
    }
    stream
        .write_all(&frame)
        .map_err(|e| format!("CDP write failed: {e}"))
}

fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u8;
    for byte in input.bytes() {
        let val = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\n' | b'\r' | b' ' | b'\t' => continue,
            other => return Err(format!("Invalid base64 byte: {other}")),
        };
        buf = (buf << 6) | u32::from(val);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn decodes_base64_and_captures_from_mock_cdp() {
        assert_eq!(decode_base64("SGVsbG8=").unwrap(), b"Hello");
        assert_eq!(decode_base64("iVBORw0K").unwrap(), b"\x89PNG\r\n");

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf);
            sock.write_all(
                b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n",
            )
            .unwrap();
            let _ = sock.read(&mut buf);
            let body = br#"{"id":1,"result":{"data":"iVBORw0K"}}"#;
            let mut resp = vec![0x81, body.len() as u8];
            resp.extend_from_slice(body);
            sock.write_all(&resp).unwrap();
        });

        let bytes = capture_png_over_cdp(port, "target-1").unwrap();
        server.join().unwrap();
        assert_eq!(bytes, b"\x89PNG\r\n");
    }
}
