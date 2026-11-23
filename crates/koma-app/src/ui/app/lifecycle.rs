use std::cell::RefCell;
use std::rc::Rc;

use koma_app::config::{self, Config};
use koma_app::{ghostty_config, persistence};
use koma_core::model::KomaKind;
use koma_ipc::transport::resolve_socket_path;
use makepad_widgets::*;

use crate::runtime::{
    ipc_server::IpcServer,
    profiles::{self, Profile, DEFAULT_PROFILE},
    web_views::WebViewRuntime,
};
use crate::ui::sheet_strip::SheetStrip;

use super::App;

fn startup_profile() -> Result<Option<Profile>, String> {
    let mut args = std::env::args().skip(1);
    let mut id = None;
    let mut private = std::env::var_os("KOMA_PRIVATE").is_some();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--private" => private = true,
            "--profile" if id.is_none() => {
                id = Some(args.next().ok_or("--profile requires a name")?);
            }
            "--profile" => return Err("Specify --profile only once".into()),
            _ => {}
        }
    }
    if private {
        if id.is_some() {
            return Err("--private and --profile cannot be combined".into());
        }
        return Ok(None);
    }
    Ok(Some(Profile::find(
        &profiles::root()?,
        id.as_deref().unwrap_or(DEFAULT_PROFILE),
    )?))
}

fn startup_error(err: String) -> ! {
    eprintln!("[koma:startup] {err}");
    std::process::exit(1)
}

#[derive(Debug)]
pub(super) struct ProfileOpenError(pub String);

fn launch_profile(id: &str, create: bool) -> Result<(), String> {
    let root = profiles::root()?;
    let profile = if create {
        Profile::create(&root, id)?
    } else {
        Profile::find(&root, id)?
    };
    let Some(probe) = profile.try_lock()? else {
        return profile.activate();
    };
    drop(probe);
    let exe = std::env::current_exe().map_err(|e| format!("Failed to locate executable: {e}"))?;
    let mut child = std::process::Command::new(exe)
        .args(["--profile", &profile.id])
        .env_remove("KOMA_SOCKET")
        .env_remove("KOMA_PRIVATE")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to open Profile {}: {e}", profile.id))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

impl App {
    pub(super) fn open_profile(
        &mut self,
        cx: &mut Cx,
        id: String,
        create: bool,
        complete: impl FnOnce(Result<(), String>) + Send + 'static,
    ) {
        if !create && !self.is_private && id.trim().eq_ignore_ascii_case(&self.state.profile_id) {
            self.ui.window(cx, ids!(main_window)).show(cx);
            complete(Ok(()));
        } else {
            // Two Profiles can ask to switch to each other at the same time.
            std::thread::spawn(move || complete(launch_profile(&id, create)));
        }
    }

    pub(super) fn init_runtime(&mut self, cx: &mut Cx) {
        if self.web_runtime.is_some() {
            return;
        }

        let profile = startup_profile().unwrap_or_else(|err| startup_error(err));
        self.is_private = profile.is_none();
        if let Some(profile) = &profile {
            match profile.try_lock() {
                Ok(Some(lock)) => self.profile_lock = Some(lock),
                Ok(None) => match profile.activate() {
                    Ok(()) => std::process::exit(0),
                    Err(err) => startup_error(err),
                },
                Err(err) => startup_error(err),
            }
            let endpoint = profile.dir.join("endpoint");
            if let Err(err) = std::fs::remove_file(&endpoint) {
                if err.kind() != std::io::ErrorKind::NotFound {
                    startup_error(format!("Could not clear {}: {err}", endpoint.display()));
                }
            }
        }

        let home = profiles::home();
        let mut startup_err: Option<String> = None;
        if let Some(cfg_path) = config::config_path(home.as_deref()) {
            match Config::load(&cfg_path) {
                Ok(cfg) => self.config = cfg,
                Err(err) => {
                    eprintln!("[koma:config] {err}");
                    startup_err = Some(err);
                }
            }
        }
        let (socket, shared_endpoint) = match &profile {
            Some(profile) if profile.id == DEFAULT_PROFILE => (resolve_socket_path(None), true),
            Some(_) => (profiles::process_socket("profile"), false),
            None => match std::env::var("KOMA_SOCKET").ok().filter(|s| !s.is_empty()) {
                Some(path) => (path, true),
                None => (profiles::process_socket("private"), false),
            },
        };
        self.sock_path = socket;
        self.ipc = Some(
            IpcServer::start(self.sock_path.clone(), shared_endpoint)
                .unwrap_or_else(|err| startup_error(err)),
        );

        let browser_dir = if let Some(profile) = profile {
            let new_state = match persistence::load(&profile.dir) {
                Ok(Some(loaded)) => {
                    self.state = loaded;
                    false
                }
                Ok(None) => true,
                Err(err) => {
                    eprintln!("[koma:state] {err}");
                    startup_err = Some(err);
                    false
                }
            };
            self.state.set_profile_id(profile.id);
            if new_state {
                if let Err(err) = persistence::save(&profile.dir, &self.state) {
                    startup_err = Some(err);
                }
            }
            let endpoint = profile.dir.join("endpoint");
            let tmp = profile.dir.join("endpoint.tmp");
            if let Err(err) = std::fs::write(&tmp, &self.sock_path)
                .and_then(|()| std::fs::rename(&tmp, &endpoint))
            {
                startup_error(format!("Could not publish {}: {err}", endpoint.display()));
            }
            let browser_dir = profile.dir.join("browser");
            self.state_dir = Some(profile.dir);
            browser_dir
        } else {
            let pid = std::process::id();
            self.state.is_private = true;
            self.state.profile_id = format!("private-{pid}");
            let cef_dir = std::env::temp_dir().join(format!("koma-private-cef-{pid}"));
            self.private_cef_dir = Some(cef_dir.clone());
            cef_dir
        };
        let web_runtime = Rc::new(RefCell::new(WebViewRuntime::new(browser_dir)));
        self.web_runtime = Some(Rc::clone(&web_runtime));

        let mut path_env = std::env::var("PATH").unwrap_or_default();
        if let Some(bin_dir) = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.display().to_string()))
        {
            path_env = if path_env.is_empty() {
                bin_dir
            } else {
                format!("{bin_dir}:{path_env}")
            };
        }
        self.path_env = path_env;

        let (terminal_settings, diagnostics) = ghostty_config::load(
            home.as_deref().map(std::path::Path::new),
            makepad_widgets::makepad_draw::host_dark().unwrap_or(true),
        );
        for diagnostic in &diagnostics {
            eprintln!("[koma:ghostty] {diagnostic}");
        }
        if startup_err.is_none() {
            startup_err = diagnostics.into_iter().next();
        }
        if let Some(mut strip) = self
            .ui
            .widget(cx, ids!(sheet_strip))
            .borrow_mut::<SheetStrip>()
        {
            strip.set_shared_web_runtime(Rc::clone(&web_runtime));
            strip.set_terminal_settings(terminal_settings);
            strip.set_terminal_shell(self.config.terminal.shell.clone());
            strip.set_center_focused(self.config.center_focused);
            strip.set_koma_gap(self.config.koma_gap);
        }
        {
            let mut rt = web_runtime.borrow_mut();
            for koma in self
                .state
                .sheets()
                .iter()
                .flat_map(|d| d.komas.iter())
                .chain(self.state.drawer.iter())
            {
                if koma.kind == KomaKind::Web && koma.koma_api_enabled {
                    rt.set_koma_api_enabled(cx, &koma.id, true);
                }
            }
        }
        self.sync_strip(cx, true);
        self.apply_focus(cx);
        self.update_status(cx);
        if let Some(err) = startup_err {
            self.show_toast(cx, err);
        }
    }

    pub(super) fn reload_configs(&mut self, cx: &mut Cx) -> Result<(), String> {
        let home = profiles::home();
        if let Some(cfg_path) = config::config_path(home.as_deref()) {
            self.config = Config::load(&cfg_path)?;
        }
        let (terminal_settings, diagnostics) = ghostty_config::load(
            home.as_deref().map(std::path::Path::new),
            makepad_widgets::makepad_draw::host_dark().unwrap_or(true),
        );
        for diagnostic in &diagnostics {
            eprintln!("[koma:ghostty] {diagnostic}");
        }
        if let Some(mut strip) = self
            .ui
            .widget(cx, ids!(sheet_strip))
            .borrow_mut::<SheetStrip>()
        {
            strip.set_terminal_settings(terminal_settings);
            strip.set_terminal_shell(self.config.terminal.shell.clone());
            strip.set_center_focused(self.config.center_focused);
            strip.set_koma_gap(self.config.koma_gap);
        }
        if let Some(first_err) = diagnostics.into_iter().next() {
            return Err(first_err);
        }
        Ok(())
    }

    pub(super) fn shutdown_runtime(&mut self, cx: &mut Cx, event: &Event) {
        if !self.is_private {
            self.save_state(cx);
        }
        self.ipc = None;
        self.ui.handle_event(cx, event, &mut Scope::empty());
        if let Some(runtime) = self.web_runtime.take() {
            runtime.borrow_mut().shutdown(cx);
        }
        if let Some(dir) = self.private_cef_dir.take() {
            let _ = std::fs::remove_dir_all(dir);
        }
        if let Some(dir) = &self.state_dir {
            let _ = std::fs::remove_file(dir.join("endpoint"));
        }
    }
}
