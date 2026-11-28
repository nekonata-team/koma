//! Headless E2E tests driving `koma-app` over Makepad's `--stdin-loop` protocol + UDS IPC.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use koma_ipc::protocol::{
    DrawerCommand, IpcCommand, IpcPayload, IpcRequest, KomaCommand, KomaListArgs, KomaPlacement,
    KomaTextArgs, SheetCommand, TerminalCommand, TerminalNewArgs, TerminalTextArgs, WebCommand,
    WebEvalArgs, WebNewArgs, WorkspaceCommand,
};
use koma_ipc::transport::send_request;
use makepad_app_host::hub::{send_to_app, AppHub, HubEvent};
use makepad_app_host::launch;
use makepad_platform::Cx;
use makepad_studio_protocol::{KeyCode, KeyEvent, KeyModifiers, StudioToApp, TextInputEvent};

const POLL_INTERVAL: Duration = Duration::from_millis(15);

struct TestApp {
    _cx: Cx,
    hub: AppHub,
    sender: Sender<Vec<u8>>,
    child: Child,
    tmp: PathBuf,
    sock_str: String,
}

impl Drop for TestApp {
    fn drop(&mut self) {
        send_to_app(&self.sender, vec![StudioToApp::Kill]);
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.tmp);
    }
}

impl TestApp {
    fn spawn(name: &str, prepare: impl FnOnce(&Path)) -> Self {
        let cx = Cx::new(Box::new(|_, _| {}));
        let hub = AppHub::start(cx.thread_spawner()).expect("start AppHub");

        let tmp = std::env::temp_dir().join(format!("koma-e2e-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        prepare(&tmp);

        let sock_path = tmp.join("koma.sock");
        let sock_str = sock_path.display().to_string();

        let mut cmd = Command::new(env!("CARGO_BIN_EXE_koma-app"));
        cmd.arg("--stdin-loop")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .env("MAKEPAD_HIDE_WINDOWS", "1")
            .env("KOMA_GHOSTTY_CONFIG", tmp.join("config.ghostty"))
            .env("KOMA_CONFIG", tmp.join("config.toml"))
            .env("KOMA_STATE_DIR", &tmp)
            .env("KOMA_SOCKET", &sock_str);
        launch::configure(&mut cmd, hub.port, 1, "koma-app");

        let child = cmd.spawn().expect("spawn koma-app");
        let deadline = Instant::now() + Duration::from_secs(15);
        let sender = loop {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for --stdin-loop connect"
            );
            if let Ok(HubEvent::Connected { sender, .. }) =
                hub.rx.recv_timeout(Duration::from_millis(200))
            {
                break sender;
            }
        };

        let app = Self {
            _cx: cx,
            hub,
            sender,
            child,
            tmp,
            sock_str,
        };

        app.send_batch(vec![
            StudioToApp::WindowGeomChange {
                screen_origin: None,
                dpi_factor: 1.0,
                window_id: 0,
                left: 0.0,
                top: 0.0,
                width: 1280.0,
                height: 760.0,
            },
            StudioToApp::Tick,
        ]);

        app.wait_until(
            Duration::from_secs(15),
            "IPC server did not become ready in time",
            || {
                sock_path.exists()
                    && send_request(
                        &app.sock_str,
                        &IpcRequest::new(IpcCommand::Workspace(WorkspaceCommand::Health)),
                    )
                    .is_ok_and(|r| r.is_ok())
            },
        );

        app
    }

    fn profile_dir(&self) -> PathBuf {
        self.tmp.join("profiles/default")
    }

    fn send_batch(&self, msgs: Vec<StudioToApp>) {
        send_to_app(&self.sender, msgs);
    }

    fn drain_hub(&self) {
        while self.hub.rx.try_recv().is_ok() {}
    }

    fn tick(&self) {
        self.send_batch(vec![StudioToApp::Tick]);
        self.drain_hub();
    }

    fn ipc(&self, command: IpcCommand) -> Result<IpcPayload, String> {
        send_request(&self.sock_str, &IpcRequest::new(command))
            .unwrap()
            .into_result()
    }

    fn send_primary_key(&self, key_code: KeyCode, time: f64) {
        self.send_batch(vec![
            StudioToApp::KeyDown(KeyEvent {
                key_code,
                modifiers: KeyModifiers {
                    logo: cfg!(target_os = "macos"),
                    control: !cfg!(target_os = "macos"),
                    ..Default::default()
                },
                time,
                ..Default::default()
            }),
            StudioToApp::Tick,
        ]);
    }

    fn send_koma_key(&self, key_code: KeyCode, time: f64) {
        self.send_batch(vec![
            StudioToApp::KeyDown(KeyEvent {
                key_code: KeyCode::Comma,
                modifiers: KeyModifiers {
                    control: true,
                    ..Default::default()
                },
                is_repeat: false,
                time,
            }),
            StudioToApp::KeyDown(KeyEvent {
                key_code,
                modifiers: KeyModifiers::default(),
                is_repeat: false,
                time: time + 0.05,
            }),
            StudioToApp::KeyUp(KeyEvent {
                key_code,
                modifiers: KeyModifiers::default(),
                is_repeat: false,
                time: time + 0.06,
            }),
            StudioToApp::Tick,
        ]);
    }

    fn wait_until(&self, timeout: Duration, message: &str, mut predicate: impl FnMut() -> bool) {
        let deadline = Instant::now() + timeout;
        loop {
            self.tick();
            if predicate() {
                return;
            }
            assert!(Instant::now() < deadline, "{message}");
            std::thread::sleep(POLL_INTERVAL);
        }
    }

    fn wait_for_text(&self, koma_id: Option<&str>, expected: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.tick();
            let outcome = self.ipc(IpcCommand::Terminal(TerminalCommand::Text(
                TerminalTextArgs {
                    koma_id: koma_id.map(str::to_owned),
                    lines: Some(100),
                },
            )));
            if let Ok(IpcPayload::Text { text, .. }) = outcome {
                if text.contains(expected) {
                    return text;
                }
                assert!(
                    Instant::now() < deadline,
                    "missing terminal output {expected:?}: {text}"
                );
            } else {
                assert!(
                    Instant::now() < deadline,
                    "missing terminal output {expected:?}: {outcome:?}"
                );
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }
}

fn write_test_ghostty_config(tmp: &Path) {
    std::fs::write(
        tmp.join("config.ghostty"),
        "font-size = 13\nfont-variation-bold = wght=700\nadjust-cell-height = 25%\nbackground = 223344",
    )
    .unwrap();
}

#[test]
fn startup_profile_isolation_and_keyboard_flows() {
    let app = TestApp::spawn("startup-keys", |tmp| {
        let mut legacy = koma_core::model::WorkspaceState::new();
        legacy.add_sheet(false);
        koma_app::persistence::save(tmp, &legacy).unwrap();
        write_test_ghostty_config(tmp);
    });

    let saved = koma_app::persistence::load(&app.profile_dir())
        .unwrap()
        .unwrap();
    assert_eq!(saved.profile_id, "default");
    assert_eq!(saved.sheets().len(), 1);
    assert_eq!(
        koma_app::persistence::load(&app.tmp)
            .unwrap()
            .unwrap()
            .sheets()
            .len(),
        2
    );

    assert_eq!(
        app.ipc(IpcCommand::Workspace(WorkspaceCommand::ProfileList))
            .unwrap(),
        IpcPayload::ProfileList {
            profiles: vec!["default".into()]
        }
    );
    assert!(app
        .ipc(IpcCommand::Workspace(WorkspaceCommand::ProfileActivate {
            id: "other".into(),
        }))
        .is_err());

    let IpcPayload::Overview { sheets } =
        app.ipc(IpcCommand::Sheet(SheetCommand::Overview)).unwrap()
    else {
        panic!("expected overview payload");
    };
    assert!(sheets[0].komas.is_empty());

    // Drive UI via StudioToApp::KeyDown: Ctrl+, (Koma Mode) then 't' (CreateTerminal).
    app.send_koma_key(KeyCode::KeyT, 1.0);
    app.wait_until(
        Duration::from_secs(5),
        "Ctrl+, then 't' via StudioToApp did not create a Terminal Koma",
        || {
            matches!(
                app.ipc(IpcCommand::Sheet(SheetCommand::Overview)).unwrap(),
                IpcPayload::Overview { sheets } if sheets[0].komas.len() == 1
            )
        },
    );

    // Send IME preedit (replace_last: true) followed by committed Japanese text (replace_last: false).
    app.send_batch(vec![
        StudioToApp::TextInput(TextInputEvent {
            input: "にほんご".into(),
            replace_last: true,
            was_paste: false,
            ..Default::default()
        }),
        StudioToApp::TextInput(TextInputEvent {
            input: "日本語テスト".into(),
            replace_last: false,
            was_paste: false,
            ..Default::default()
        }),
        StudioToApp::Tick,
    ]);
    app.wait_for_text(None, "日本語テスト");

    // Rename the focused Koma via Palette using two consecutive IME commits, verifying
    // TextInput exits composition on commit instead of overwriting the previous commit.
    app.send_koma_key(KeyCode::KeyR, 1.10);
    for _ in 0..3 {
        std::thread::sleep(POLL_INTERVAL);
        app.tick();
    }
    app.send_batch(vec![
        StudioToApp::TextInput(TextInputEvent {
            input: "にほんご".into(),
            replace_last: true,
            was_paste: false,
            ..Default::default()
        }),
        StudioToApp::TextInput(TextInputEvent {
            input: "日本語".into(),
            replace_last: false,
            was_paste: false,
            ..Default::default()
        }),
        StudioToApp::TextInput(TextInputEvent {
            input: "こま".into(),
            replace_last: true,
            was_paste: false,
            ..Default::default()
        }),
        StudioToApp::TextInput(TextInputEvent {
            input: "コマ".into(),
            replace_last: false,
            was_paste: false,
            ..Default::default()
        }),
        StudioToApp::KeyDown(KeyEvent {
            key_code: KeyCode::ReturnKey,
            modifiers: KeyModifiers::default(),
            is_repeat: false,
            time: 1.20,
        }),
        StudioToApp::Tick,
    ]);
    app.wait_until(
        Duration::from_secs(5),
        "Consecutive IME commits in Palette did not rename focused Koma",
        || {
            matches!(
                app.ipc(IpcCommand::Koma(KomaCommand::List(KomaListArgs {
                    sheet_id: None,
                }))),
                Ok(IpcPayload::KomaList { komas, .. })
                    if komas.iter().any(|k| k.label == "日本語コマ")
            )
        },
    );

    // Stow the new Terminal Koma to Drawer via Ctrl+, then 'a'.
    app.send_koma_key(KeyCode::KeyA, 1.3);
    app.wait_until(
        Duration::from_secs(5),
        "Ctrl+, then 'a' via StudioToApp did not stow Koma to Drawer",
        || {
            matches!(
                app.ipc(IpcCommand::Drawer(DrawerCommand::List)).unwrap(),
                IpcPayload::DrawerList { items } if items.len() == 1
            )
        },
    );
}

#[test]
fn ipc_sheet_koma_and_pty_lifecycle() {
    let app = TestApp::spawn("ipc-pty", write_test_ghostty_config);

    let IpcPayload::SheetId { sheet_id } = app
        .ipc(IpcCommand::Sheet(SheetCommand::New {
            label: "Checks".into(),
            focus: false,
        }))
        .unwrap()
    else {
        panic!("expected new Sheet ID");
    };
    let IpcPayload::KomaId { koma_id } = app
        .ipc(IpcCommand::Terminal(TerminalCommand::New(
            TerminalNewArgs {
                cwd: None,
                run: Some("printf '%s-%s\\n' FIRST CHECK".into()),
                ..Default::default()
            },
        )))
        .unwrap()
    else {
        panic!("expected new Koma ID");
    };
    assert_eq!(
        app.ipc(IpcCommand::Koma(KomaCommand::Move {
            koma_id: koma_id.clone(),
            placement: KomaPlacement::EndOf(koma_id.clone()),
        }))
        .unwrap_err(),
        format!("Sheet not found: {koma_id}")
    );
    app.ipc(IpcCommand::Koma(KomaCommand::Move {
        koma_id: koma_id.clone(),
        placement: KomaPlacement::EndOf(sheet_id.clone()),
    }))
    .unwrap();
    let IpcPayload::SheetList { sheets } = app.ipc(IpcCommand::Sheet(SheetCommand::List)).unwrap()
    else {
        panic!("expected Sheet list");
    };
    assert!(sheets[0].is_active);
    assert_eq!(sheets[1].koma_count, 1);
    let IpcPayload::KomaList { komas, .. } = app
        .ipc(IpcCommand::Koma(KomaCommand::List(KomaListArgs {
            sheet_id: Some(sheet_id.clone()),
        })))
        .unwrap()
    else {
        panic!("expected Koma list");
    };
    assert_eq!(komas[0].id, koma_id);
    assert!(matches!(
        app.ipc(IpcCommand::Koma(KomaCommand::Resize {
            koma_id: koma_id.clone(),
            width: 900.0,
        }))
        .unwrap(),
        IpcPayload::KomaResized { width: 900.0, .. }
    ));
    app.ipc(IpcCommand::Koma(KomaCommand::Focus {
        koma_id: koma_id.clone(),
    }))
    .unwrap();
    let IpcPayload::KomaFocused(focused) = app.ipc(IpcCommand::Koma(KomaCommand::Focused)).unwrap()
    else {
        panic!("expected focused Koma");
    };
    assert_eq!(focused.sheet_id, sheet_id);
    assert_eq!(focused.koma.unwrap().id, koma_id);

    let input = |text: &str| KomaTextArgs {
        koma_id: Some(koma_id.clone()),
        text: text.into(),
    };
    app.wait_for_text(Some(&koma_id), "FIRST-CHECK");
    app.ipc(IpcCommand::Terminal(TerminalCommand::Send(input(
        "printf '%s-%s\\n' SECOND CHECK",
    ))))
    .unwrap();
    app.ipc(IpcCommand::Terminal(TerminalCommand::Press(input("Enter"))))
        .unwrap();
    app.wait_for_text(Some(&koma_id), "SECOND-CHECK");
    app.ipc(IpcCommand::Terminal(TerminalCommand::Run(input(
        "printf '%s-%s\\n' THIRD CHECK",
    ))))
    .unwrap();
    app.wait_for_text(Some(&koma_id), "THIRD-CHECK");

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let reported_cwd = app.tmp.join("terminal cwd");
        std::fs::create_dir(&reported_cwd).unwrap();
        let reported_uri =
            format!("file://localhost{}", reported_cwd.display()).replace(' ', "%20");
        app.ipc(IpcCommand::Terminal(TerminalCommand::Run(input(&format!(
            "/bin/sh -c 'trap \"echo FOREGROUND\"\"-DONE; exit 0\" TERM; printf \"\\033]2;Working label\\007\\033]7;%s\\007\" \"{reported_uri}\"; printf \"%s-%s\\n\" FOREGROUND READY; sleep 30'",
        )))))
        .unwrap();
        app.wait_for_text(Some(&koma_id), "FOREGROUND-READY");
        app.wait_until(
            Duration::from_secs(5),
            "terminal metadata did not persist",
            || {
                let saved = koma_app::persistence::load(&app.profile_dir())
                    .unwrap()
                    .unwrap();
                let terminal = saved.koma_by_id(&koma_id.as_str().into()).unwrap();
                terminal.label == "Working label"
                    && terminal.cwd().map(Path::new) == Some(reported_cwd.as_path())
            },
        );
        app.ipc(IpcCommand::Terminal(TerminalCommand::Kill(input("TERM"))))
            .unwrap();
        app.wait_for_text(Some(&koma_id), "FOREGROUND-DONE");
    }
}

#[test]
fn terminal_zoom_and_reopen() {
    let app = TestApp::spawn("zoom-reopen", write_test_ghostty_config);

    let IpcPayload::KomaId { koma_id } = app
        .ipc(IpcCommand::Terminal(TerminalCommand::New(
            TerminalNewArgs::default(),
        )))
        .unwrap()
    else {
        panic!("expected new Koma ID");
    };
    app.ipc(IpcCommand::Koma(KomaCommand::Resize {
        koma_id: koma_id.clone(),
        width: 900.0,
    }))
    .unwrap();
    app.ipc(IpcCommand::Koma(KomaCommand::Focus {
        koma_id: koma_id.clone(),
    }))
    .unwrap();
    let IpcPayload::KomaFocused(initial_focus) =
        app.ipc(IpcCommand::Koma(KomaCommand::Focused)).unwrap()
    else {
        panic!("expected focused Koma");
    };
    let origin_sheet_id = initial_focus.sheet_id;

    let input = |text: &str| KomaTextArgs {
        koma_id: Some(koma_id.clone()),
        text: text.into(),
    };

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use makepad_studio_protocol::{PinchPhase, RemotePinch};

        let grid = |mark: &str| -> Vec<usize> {
            app.ipc(IpcCommand::Terminal(TerminalCommand::Run(input(&format!(
                "printf '%s%s ' GRID- {mark}; stty size"
            )))))
            .unwrap();
            let prefix = format!("GRID-{mark} ");
            app.wait_for_text(Some(&koma_id), &prefix)
                .lines()
                .find_map(|line| line.trim_start().strip_prefix(&prefix))
                .unwrap()
                .split_whitespace()
                .map(|n| n.parse().unwrap())
                .collect()
        };
        let base = grid("BASE");
        assert_eq!(base.len(), 2);

        let pinch = |phase, x, scale, time| {
            StudioToApp::Pinch(RemotePinch {
                phase,
                x,
                y: 300.0,
                scale,
                time,
                ..Default::default()
            })
        };
        app.send_batch(vec![
            pinch(PinchPhase::Begin, 450.0, 1.0, 2.0),
            // The pointer leaving the terminal does not retarget an active pinch.
            pinch(PinchPhase::Update, 1270.0, 1.5, 2.1),
            pinch(PinchPhase::End, 1270.0, 1.0, 2.2),
            StudioToApp::Tick,
        ]);
        let large = grid("LARGE");
        assert_eq!(large.len(), 2);
        assert!(
            large[0] < base[0] && large[1] < base[1],
            "{large:?} vs {base:?}"
        );
        app.send_batch(vec![
            pinch(PinchPhase::Begin, 450.0, 1.0, 2.3),
            pinch(PinchPhase::Update, 450.0, 2.0 / 3.0, 2.4),
            pinch(PinchPhase::End, 450.0, 1.0, 2.5),
            StudioToApp::Tick,
        ]);
        assert_eq!(grid("SHRUNK"), base);
        app.send_primary_key(KeyCode::Minus, 2.6);
        let small = grid("KEYBOARD");
        assert!(
            small[0] > base[0] && small[1] > base[1],
            "{small:?} vs {base:?}"
        );
        app.send_primary_key(KeyCode::Key0, 2.7);
        assert_eq!(grid("DEFAULT"), base);
    }

    // Create a second Sheet so reopen verifies restoring focus across Sheets.
    app.ipc(IpcCommand::Sheet(SheetCommand::New {
        label: "Other".into(),
        focus: false,
    }))
    .unwrap();
    app.ipc(IpcCommand::Koma(KomaCommand::Close {
        koma_id: koma_id.clone(),
    }))
    .unwrap();
    assert!(matches!(
        app.ipc(IpcCommand::Koma(KomaCommand::Exists {
            koma_id: koma_id.clone()
        }))
        .unwrap(),
        IpcPayload::KomaExists { exists: false, .. }
    ));
    app.ipc(IpcCommand::Sheet(SheetCommand::Focus { sheet_index: 2 }))
        .unwrap();

    let reopen_koma = |time_base: f64| {
        app.send_koma_key(KeyCode::KeyU, time_base);
        app.wait_until(
            Duration::from_secs(5),
            "u did not reopen and focus the closed Koma",
            || {
                let IpcPayload::KomaFocused(focused) =
                    app.ipc(IpcCommand::Koma(KomaCommand::Focused)).unwrap()
                else {
                    return false;
                };
                if focused.koma.as_ref().is_some_and(|koma| koma.id == koma_id) {
                    assert_eq!(focused.sheet_id, origin_sheet_id);
                    let saved = koma_app::persistence::load(&app.profile_dir())
                        .unwrap()
                        .unwrap();
                    assert_eq!(
                        saved.koma_by_id(&koma_id.as_str().into()).unwrap().width,
                        900.0
                    );
                    true
                } else {
                    false
                }
            },
        );
    };
    reopen_koma(4.0);

    // Return from KomaMode to ContentInput before typing `exit` in the reopened terminal.
    app.send_batch(vec![
        StudioToApp::KeyDown(KeyEvent {
            key_code: KeyCode::Escape,
            modifiers: KeyModifiers::default(),
            is_repeat: false,
            time: 4.2,
        }),
        StudioToApp::Tick,
    ]);

    // Exited terminal (e.g. via C-d / exit) is also pushed to the reopen stack.
    app.ipc(IpcCommand::Terminal(TerminalCommand::Send(input("exit"))))
        .unwrap();
    app.ipc(IpcCommand::Terminal(TerminalCommand::Press(input("Enter"))))
        .unwrap();
    app.wait_until(
        Duration::from_secs(5),
        "exited terminal Koma was not removed",
        || {
            matches!(
                app.ipc(IpcCommand::Koma(KomaCommand::Exists {
                    koma_id: koma_id.clone()
                }))
                .unwrap(),
                IpcPayload::KomaExists { exists: false, .. }
            )
        },
    );
    reopen_koma(4.5);
}

#[test]
#[ignore = "requires CEF distribution and browser host (run via `just test::e2e-web`)"]
fn web_zoom() {
    use makepad_studio_protocol::{PinchPhase, RemotePinch};

    let app = TestApp::spawn("web-zoom", |_| {});

    std::thread::scope(|scope| {
        let (stop, stopped) = std::sync::mpsc::channel::<()>();
        let tick_sender = app.sender.clone();
        scope.spawn(move || {
            while stopped.recv_timeout(Duration::from_millis(16))
                == Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            {
                send_to_app(&tick_sender, vec![StudioToApp::Tick]);
            }
        });

        let html = app.tmp.join("zoom.html");
        std::fs::write(&html, "<!doctype html><title>Zoom test</title>").unwrap();
        let new_page = || {
            let IpcPayload::WebKoma(web) = app
                .ipc(IpcCommand::Web(WebCommand::New(WebNewArgs {
                    url: html.display().to_string(),
                    ..Default::default()
                })))
                .unwrap()
            else {
                panic!("expected Web Koma ID");
            };
            web.koma_id
        };
        let a = new_page();
        let b = new_page();
        let wait_number = |id: &str, expr: &str, accept: &dyn Fn(f64) -> bool| {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let outcome = app.ipc(IpcCommand::Web(WebCommand::Eval(WebEvalArgs {
                    koma_id: Some(id.into()),
                    script: format!("document.title === 'Zoom test' ? ({expr}) : null"),
                })));
                if let Ok(IpcPayload::WebEval { value, .. }) = &outcome {
                    if let Ok(number) = value.parse::<f64>() {
                        if accept(number) {
                            return number;
                        }
                    }
                }
                assert!(Instant::now() < deadline, "{id} {expr}: {outcome:?}");
                std::thread::sleep(Duration::from_millis(50));
            }
        };
        let width = wait_number(&a, "innerWidth", &|_| true);
        let base = wait_number(&b, "visualViewport.scale", &|_| true);
        let pinch = |scale| {
            let event = |phase, scale| {
                StudioToApp::Pinch(RemotePinch {
                    phase,
                    scale,
                    x: 280.0,
                    y: 300.0,
                    time: 5.0,
                    ..Default::default()
                })
            };
            app.send_batch(vec![event(PinchPhase::Begin, 1.0)]);
            for _ in 0..7 {
                app.send_batch(vec![event(PinchPhase::Update, scale)]);
                std::thread::sleep(Duration::from_millis(16));
            }
            app.send_batch(vec![event(PinchPhase::End, 1.0)]);
        };
        pinch(1.13);
        let enlarged = wait_number(&a, "visualViewport.scale", &|scale| scale > 1.1);
        assert_eq!(wait_number(&a, "innerWidth", &|_| true), width);
        assert_eq!(wait_number(&b, "visualViewport.scale", &|_| true), base);
        assert!(wait_number(&a, "visualViewport.offsetLeft", &|_| true) > 20.0);
        assert!(wait_number(&a, "visualViewport.offsetTop", &|_| true) > 20.0);
        pinch(0.86);
        wait_number(&a, "visualViewport.scale", &|scale| scale < enlarged - 0.1);

        pinch(1.13);
        let visual = wait_number(&a, "visualViewport.scale", &|scale| scale > 1.1);
        let other_width = wait_number(&b, "innerWidth", &|_| true);
        app.ipc(IpcCommand::Koma(KomaCommand::Focus { koma_id: a.clone() }))
            .unwrap();
        app.send_primary_key(KeyCode::Equals, 6.0);
        let zoomed_width = |value: f64| (value - width / 1.1).abs() <= 1.0;
        wait_number(&a, "innerWidth", &zoomed_width);
        assert_eq!(wait_number(&b, "innerWidth", &|_| true), other_width);
        assert!((wait_number(&a, "visualViewport.scale", &|_| true) - visual).abs() < 0.01);

        app.send_primary_key(KeyCode::Minus, 6.1);
        wait_number(&a, "innerWidth", &|value| value == width);
        app.send_primary_key(KeyCode::Equals, 6.2);
        wait_number(&a, "innerWidth", &zoomed_width);
        app.send_primary_key(KeyCode::Key0, 6.3);
        wait_number(&a, "innerWidth", &|value| value == width);
        wait_number(&a, "visualViewport.scale", &|scale| scale == base);
        pinch(1.13);
        wait_number(&a, "visualViewport.scale", &|scale| scale > 1.1);

        drop(stop);
    });
}
