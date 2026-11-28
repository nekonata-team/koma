//! OS-specific non-UI helpers (external URL launch, macOS default-browser integration).

pub fn copy_rgba_to_clipboard(width: usize, height: usize, rgba: Vec<u8>) -> Result<(), String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Failed to open clipboard: {e}"))?;
    clipboard
        .set_image(arboard::ImageData {
            width,
            height,
            bytes: std::borrow::Cow::Owned(rgba),
        })
        .map_err(|e| format!("Failed to copy screenshot to clipboard: {e}"))
}

pub fn open_external_app_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg(url);
        c
    };
    #[cfg(target_os = "linux")]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(url);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", "", url]);
        c
    };
    cmd.spawn()
        .map(|_| ())
        .map_err(|err| format!("Failed to open external URL {url}: {err}"))
}

#[cfg(target_os = "macos")]
pub mod default_browser {
    use std::sync::atomic::{AtomicBool, Ordering};

    use makepad_apple_sys::*;
    use makepad_widgets::{makepad_hal, Cx};

    const SCHEMES: [&str; 2] = ["https", "http"];
    static CHANGE_PENDING: AtomicBool = AtomicBool::new(false);

    #[derive(Debug)]
    pub struct DefaultBrowserStatus(pub String);

    pub fn show() {
        if !makepad_hal::dispatch_main(|| unsafe {
            let workspace: ObjcId = msg_send![class!(NSWorkspace), sharedWorkspace];
            Cx::post_action(DefaultBrowserStatus(status(workspace)));
        }) {
            Cx::post_action(DefaultBrowserStatus(
                "macOS workspace is unavailable".into(),
            ));
        }
    }

    pub fn set() {
        if CHANGE_PENDING.swap(true, Ordering::Relaxed) {
            return;
        }
        queue_request(0);
    }

    fn queue_request(index: usize) {
        if !makepad_hal::dispatch_main(move || unsafe { request(index) }) {
            finish("macOS workspace is unavailable".into());
        }
    }

    fn finish(message: String) {
        CHANGE_PENDING.store(false, Ordering::Relaxed);
        Cx::post_action(DefaultBrowserStatus(message));
    }

    // All NSWorkspace operations run on the AppKit thread. Its callback may run on another thread.
    unsafe fn request(index: usize) {
        let workspace: ObjcId = msg_send![class!(NSWorkspace), sharedWorkspace];
        let bundle: ObjcId = msg_send![class!(NSBundle), mainBundle];
        let app: ObjcId = msg_send![bundle, bundleURL];
        let extension: ObjcId = msg_send![app, pathExtension];
        let bundled: BOOL = msg_send![extension, isEqualToString: str_to_nsstring("app")];
        if bundled == NO {
            finish("Open Koma.app to set it as the default browser".into());
            return;
        }
        let is_koma = |scheme| {
            let handler = handler_url(workspace, scheme);
            let handler: ObjcId = msg_send![handler, URLByResolvingSymlinksInPath];
            let app: ObjcId = msg_send![app, URLByResolvingSymlinksInPath];
            let same: BOOL = msg_send![handler, isEqual: app];
            same != NO
        };
        let Some((index, scheme)) = SCHEMES
            .iter()
            .copied()
            .enumerate()
            .skip(index)
            .find(|&(_, scheme)| !is_koma(scheme))
        else {
            finish(if SCHEMES.into_iter().all(is_koma) {
                "Koma is the default browser".into()
            } else {
                format!("Change incomplete. {}", status(workspace))
            });
            return;
        };
        let supported: BOOL = msg_send![
            workspace,
            respondsToSelector: sel!(setDefaultApplicationAtURL:toOpenURLsWithScheme:completionHandler:)
        ];
        if supported == NO {
            finish("Use System Settings to change the default browser".into());
            return;
        }

        // macOS may change both schemes after the first request. Check before asking again.
        let completion = objc_block!(move |error: ObjcId| {
            if error == nil {
                queue_request(index + 1);
            } else {
                let description: ObjcId = msg_send![error, localizedDescription];
                if description != nil {
                    eprintln!("[koma:default-browser] {}", nsstring_to_string(description));
                }
                if !makepad_hal::dispatch_main(|| {
                    let workspace: ObjcId = msg_send![class!(NSWorkspace), sharedWorkspace];
                    finish(format!("Change not completed. {}", status(workspace)));
                }) {
                    finish("Default browser change not completed".into());
                }
            }
        });
        let () = msg_send![
            workspace,
            setDefaultApplicationAtURL: app
            toOpenURLsWithScheme: str_to_nsstring(scheme)
            completionHandler: &completion
        ];
    }

    unsafe fn handler_url(workspace: ObjcId, scheme: &str) -> ObjcId {
        let address = str_to_nsstring(&format!("{scheme}://example.com/"));
        let url: ObjcId = msg_send![class!(NSURL), URLWithString: address];
        msg_send![workspace, URLForApplicationToOpenURL: url]
    }

    unsafe fn status(workspace: ObjcId) -> String {
        let name = |scheme| {
            let url = handler_url(workspace, scheme);
            let filename: ObjcId = msg_send![url, lastPathComponent];
            let name: ObjcId = msg_send![filename, stringByDeletingPathExtension];
            if name == nil {
                "None".into()
            } else {
                nsstring_to_string(name)
            }
        };
        let http = name("http");
        let https = name("https");
        if http == https {
            format!("Default browser: {https}")
        } else {
            format!("Default browsers: HTTP: {http}, HTTPS: {https}")
        }
    }
}
