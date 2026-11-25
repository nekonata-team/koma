use std::path::Path;

fn main() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("Browser Host crate is inside the Koma workspace");
    makepad_app_browser_host::run_with_cef_workspace(workspace, "just setup");
}
