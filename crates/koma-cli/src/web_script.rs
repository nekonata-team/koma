use std::num::NonZeroU64;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use koma_ipc::protocol::WebScriptTargetData;

const SNAPSHOT_EXPR: &str = concat!(
    "(() => {\n",
    include_str!("../../koma-app/assets/WebDOM.js"),
    "\nreturn window.__komaWebDOM.komaSnapshot();\n})()"
);

pub(super) fn run(
    file: &Path,
    timeout: NonZeroU64,
    target: WebScriptTargetData,
) -> Result<(), String> {
    let mut child = Command::new("node")
        .args([
            "--input-type=module",
            "--eval",
            include_str!("web_script.mjs"),
            "--",
        ])
        .arg(file)
        .arg(target.port.to_string())
        .arg(target.target_id)
        .env("KOMA_SCRIPT_SNAPSHOT", SNAPSHOT_EXPR)
        .spawn()
        .map_err(|err| format!("Cannot start Node.js for koma web script: {err}"))?;
    let started = Instant::now();
    let limit = Duration::from_secs(timeout.get());
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("Web Koma script failed ({status})")),
            Ok(None) if started.elapsed() < limit => {
                std::thread::sleep(Duration::from_millis(5));
            }
            result => {
                let _ = child.kill();
                let _ = child.wait();
                return match result {
                    Err(err) => Err(format!("Cannot wait for the Web Koma script: {err}")),
                    _ => Err(format!("Web Koma script timed out after {timeout}s")),
                };
            }
        }
    }
}
