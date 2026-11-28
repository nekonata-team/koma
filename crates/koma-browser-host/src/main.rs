use std::path::Path;

fn main() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("Browser Host crate is inside the Koma workspace");
    // macOS で Browser Host が 2 つ目の Dock アイコンを出さないようにする
    makepad_app_browser_host::makepad_widgets::makepad_platform::set_menu_bar_app(true);
    makepad_app_browser_host::run_with_cef_workspace(workspace, "just setup");
}
