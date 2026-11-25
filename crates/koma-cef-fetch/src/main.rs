use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("CEF fetch crate is inside the Koma workspace");
    makepad_cef_fetch::run_from_workspace(workspace)
}
