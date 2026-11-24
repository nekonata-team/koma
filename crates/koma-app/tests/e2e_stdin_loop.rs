//! Headless E2E test driving `koma-app` over Makepad's `--stdin-loop` protocol + UDS IPC.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use koma_ipc::protocol::{
    IpcCommand, IpcPayload, IpcRequest, KomaListArgs, KomaPlacement, KomaTextArgs, TerminalNewArgs,
    TerminalTextArgs,
};
use koma_ipc::transport::send_request;
use makepad_app_host::hub::{send_to_app, AppHub, HubEvent};
use makepad_app_host::launch;
use makepad_platform::Cx;
use makepad_studio_protocol::{KeyCode, KeyEvent, KeyModifiers, StudioToApp, TextInputEvent};

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn send_batch(sender: &std::sync::mpsc::Sender<Vec<u8>>, msgs: Vec<StudioToApp>) {
    send_to_app(sender, msgs);
}

#[test]
fn headless_stdin_loop_handles_keys_and_ipc() {
    let cx = Cx::new(Box::new(|_, _| {}));
    let hub = AppHub::start(cx.thread_spawner()).expect("start AppHub");

    let tmp = std::env::temp_dir().join(format!("koma-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let mut legacy = koma_core::model::WorkspaceState::new();
    legacy.add_sheet(false);
    koma_app::persistence::save(&tmp, &legacy).unwrap();
    let ghostty_config = tmp.join("config.ghostty");
    std::fs::write(
        &ghostty_config,
        "font-size = 13\nfont-variation-bold = wght=700\nadjust-cell-height = 25%\nbackground = 223344",
    )
    .unwrap();
    let sock_path: PathBuf = tmp.join("koma.sock");
    let sock_str = sock_path.display().to_string();

    let bin = env!("CARGO_BIN_EXE_koma-app");
    let mut cmd = Command::new(bin);
    cmd.arg("--stdin-loop")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .env("MAKEPAD_HIDE_WINDOWS", "1")
        .env("KOMA_GHOSTTY_CONFIG", &ghostty_config)
        .env("KOMA_CONFIG", tmp.join("config.toml"))
        .env("KOMA_STATE_DIR", &tmp)
        .env("KOMA_SOCKET", &sock_str);
    launch::configure(&mut cmd, hub.port, 1, "koma-app");

    let child = ChildGuard(cmd.spawn().expect("spawn koma-app"));

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

    // Give the window geometry and pump ticks until the app initializes its IPC server.
    send_batch(
        &sender,
        vec![
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
        ],
    );

    let mut ready = false;
    while Instant::now() < deadline {
        send_batch(&sender, vec![StudioToApp::Tick]);
        while hub.rx.try_recv().is_ok() {}
        if sock_path.exists()
            && send_request(&sock_str, &IpcRequest::new(IpcCommand::Health))
                .is_ok_and(|r| r.is_ok())
        {
            ready = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(ready, "IPC server did not become ready in time");
    let profile_dir = tmp.join("profiles/default");
    let saved = koma_app::persistence::load(&profile_dir).unwrap().unwrap();
    assert_eq!(saved.profile_id, "default");
    assert_eq!(saved.sheets().len(), 1);
    assert_eq!(
        koma_app::persistence::load(&tmp)
            .unwrap()
            .unwrap()
            .sheets()
            .len(),
        2
    );
    let profiles = send_request(&sock_str, &IpcRequest::new(IpcCommand::ProfileList))
        .unwrap()
        .into_result()
        .unwrap();
    assert_eq!(
        profiles,
        IpcPayload::ProfileList {
            profiles: vec!["default".into()]
        }
    );
    let other_profile = IpcRequest::new(IpcCommand::ProfileActivate { id: "other".into() });
    assert!(!send_request(&sock_str, &other_profile).unwrap().is_ok());

    // Initially Sheet 1 is empty.
    let ov = send_request(&sock_str, &IpcRequest::new(IpcCommand::Overview))
        .unwrap()
        .into_result()
        .unwrap();
    let IpcPayload::Overview { sheets } = ov else {
        panic!("expected overview payload, got {ov:?}");
    };
    assert!(sheets[0].komas.is_empty());

    // Drive UI via StudioToApp::KeyDown: Ctrl+, (Koma Mode) then 't' (CreateTerminal).
    send_batch(
        &sender,
        vec![
            StudioToApp::KeyDown(KeyEvent {
                key_code: KeyCode::Comma,
                modifiers: KeyModifiers {
                    control: true,
                    ..Default::default()
                },
                is_repeat: false,
                time: 1.0,
            }),
            StudioToApp::KeyDown(KeyEvent {
                key_code: KeyCode::KeyT,
                modifiers: KeyModifiers::default(),
                is_repeat: false,
                time: 1.1,
            }),
            StudioToApp::Tick,
        ],
    );

    let mut created = false;
    while Instant::now() < deadline {
        send_batch(&sender, vec![StudioToApp::Tick]);
        while hub.rx.try_recv().is_ok() {}
        let ov = send_request(&sock_str, &IpcRequest::new(IpcCommand::Overview))
            .unwrap()
            .into_result()
            .unwrap();
        if let IpcPayload::Overview { sheets } = ov {
            if sheets[0].komas.len() == 1 {
                created = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        created,
        "Ctrl+, then 't' via StudioToApp did not create a Terminal Koma"
    );

    // Send IME preedit (replace_last: true) followed by committed Japanese text (replace_last: false).
    send_batch(
        &sender,
        vec![
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
        ],
    );

    let mut got_ime_text = false;
    while Instant::now() < deadline {
        send_batch(&sender, vec![StudioToApp::Tick]);
        while hub.rx.try_recv().is_ok() {}
        if let Ok(res) = send_request(
            &sock_str,
            &IpcRequest::new(IpcCommand::TerminalText(TerminalTextArgs::default())),
        ) {
            if let Ok(IpcPayload::Text { text, .. }) = res.into_result() {
                if text.contains("日本語テスト") {
                    got_ime_text = true;
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(got_ime_text, "Committed IME text did not reach MpTerm PTY");

    // Stow the new Terminal Koma to Drawer via Ctrl+, then 'a'.
    send_batch(
        &sender,
        vec![
            StudioToApp::KeyDown(KeyEvent {
                key_code: KeyCode::Comma,
                modifiers: KeyModifiers {
                    control: true,
                    ..Default::default()
                },
                is_repeat: false,
                time: 1.2,
            }),
            StudioToApp::KeyDown(KeyEvent {
                key_code: KeyCode::KeyA,
                modifiers: KeyModifiers::default(),
                is_repeat: false,
                time: 1.3,
            }),
            StudioToApp::Tick,
        ],
    );

    let mut stowed = false;
    while Instant::now() < deadline {
        send_batch(&sender, vec![StudioToApp::Tick]);
        while hub.rx.try_recv().is_ok() {}
        let dl = send_request(&sock_str, &IpcRequest::new(IpcCommand::DrawerList))
            .unwrap()
            .into_result()
            .unwrap();
        if let IpcPayload::DrawerList { items } = dl {
            if items.len() == 1 {
                stowed = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        stowed,
        "Ctrl+, then 'a' via StudioToApp did not stow Koma to Drawer"
    );

    let ipc = |command| {
        send_request(&sock_str, &IpcRequest::new(command))
            .unwrap()
            .into_result()
    };
    let IpcPayload::SheetId { sheet_id } = ipc(IpcCommand::SheetNew {
        label: "Checks".into(),
        focus: false,
    })
    .unwrap() else {
        panic!("expected new Sheet ID")
    };
    let IpcPayload::KomaId { koma_id } = ipc(IpcCommand::TerminalNew(TerminalNewArgs {
        cwd: None,
        run: Some("printf '%s-%s\\n' FIRST CHECK".into()),
    }))
    .unwrap() else {
        panic!("expected new Koma ID")
    };
    assert_eq!(
        ipc(IpcCommand::KomaMove {
            koma_id: koma_id.clone(),
            placement: KomaPlacement::EndOf(koma_id.clone()),
        })
        .unwrap_err(),
        format!("Sheet not found: {koma_id}")
    );
    ipc(IpcCommand::KomaMove {
        koma_id: koma_id.clone(),
        placement: KomaPlacement::EndOf(sheet_id.clone()),
    })
    .unwrap();
    let IpcPayload::SheetList { sheets } = ipc(IpcCommand::SheetList).unwrap() else {
        panic!("expected Sheet list");
    };
    assert!(sheets[0].is_active);
    assert_eq!(sheets[1].koma_count, 1);
    let IpcPayload::KomaList { komas, .. } = ipc(IpcCommand::KomaList(KomaListArgs {
        sheet_id: Some(sheet_id.clone()),
    }))
    .unwrap() else {
        panic!("expected Koma list")
    };
    assert_eq!(komas[0].id, koma_id);
    assert!(matches!(
        ipc(IpcCommand::KomaResize {
            koma_id: koma_id.clone(),
            width: 900.0,
        })
        .unwrap(),
        IpcPayload::KomaResized { width: 900.0, .. }
    ));
    ipc(IpcCommand::KomaFocus {
        koma_id: koma_id.clone(),
    })
    .unwrap();
    let IpcPayload::KomaFocused(focused) = ipc(IpcCommand::KomaFocused).unwrap() else {
        panic!("expected focused Koma");
    };
    assert_eq!(focused.sheet_id, sheet_id);
    assert_eq!(focused.koma.unwrap().id, koma_id);

    let wait_for_text = |expected: &str| {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            send_batch(&sender, vec![StudioToApp::Tick]);
            while hub.rx.try_recv().is_ok() {}
            let IpcPayload::Text { text, .. } = ipc(IpcCommand::TerminalText(TerminalTextArgs {
                koma_id: Some(koma_id.clone()),
                lines: Some(100),
            }))
            .unwrap() else {
                panic!("expected terminal text")
            };
            if text.contains(expected) {
                break text;
            }
            assert!(
                Instant::now() < deadline,
                "missing terminal output {expected:?}: {text}"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    wait_for_text("FIRST-CHECK");
    let input = |text: &str| KomaTextArgs {
        koma_id: Some(koma_id.clone()),
        text: text.into(),
    };
    ipc(IpcCommand::TerminalSend(input(
        "printf '%s-%s\\n' SECOND CHECK",
    )))
    .unwrap();
    ipc(IpcCommand::TerminalPress(input("Enter"))).unwrap();
    wait_for_text("SECOND-CHECK");
    ipc(IpcCommand::TerminalRun(input(
        "printf '%s-%s\\n' THIRD CHECK",
    )))
    .unwrap();
    wait_for_text("THIRD-CHECK");
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use makepad_studio_protocol::{PinchPhase, RemotePinch};

        let grid = |mark: &str| -> Vec<usize> {
            ipc(IpcCommand::TerminalRun(input(&format!(
                "printf '%s%s ' GRID- {mark}; stty size"
            ))))
            .unwrap();
            let prefix = format!("GRID-{mark} ");
            wait_for_text(&prefix)
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
        send_batch(
            &sender,
            vec![
                pinch(PinchPhase::Begin, 450.0, 1.0, 2.0),
                // The pointer leaving the terminal does not retarget an active pinch.
                pinch(PinchPhase::Update, 1270.0, 1.5, 2.1),
                pinch(PinchPhase::End, 1270.0, 1.0, 2.2),
                StudioToApp::Tick,
            ],
        );
        let large = grid("LARGE");
        assert_eq!(large.len(), 2);
        assert!(
            large[0] < base[0] && large[1] < base[1],
            "{large:?} vs {base:?}"
        );
        send_batch(
            &sender,
            vec![
                pinch(PinchPhase::Begin, 450.0, 1.0, 2.3),
                pinch(PinchPhase::Update, 450.0, 2.0 / 3.0, 2.4),
                pinch(PinchPhase::End, 450.0, 1.0, 2.5),
                StudioToApp::Tick,
            ],
        );
        assert_eq!(grid("SHRUNK"), base);

        let reported_cwd = tmp.join("terminal cwd");
        std::fs::create_dir(&reported_cwd).unwrap();
        let reported_uri =
            format!("file://localhost{}", reported_cwd.display()).replace(' ', "%20");
        ipc(IpcCommand::TerminalRun(input(&format!(
            "/bin/sh -c 'trap \"echo FOREGROUND\"\"-DONE; exit 0\" TERM; printf \"\\033]2;Working label\\007\\033]7;%s\\007\" \"{reported_uri}\"; printf \"%s-%s\\n\" FOREGROUND READY; sleep 30'",
        ))))
        .unwrap();
        wait_for_text("FOREGROUND-READY");
        let saved = koma_app::persistence::load(&profile_dir).unwrap().unwrap();
        let terminal = saved.koma_by_id(&koma_id.as_str().into()).unwrap();
        assert_eq!(terminal.label, "Working label");
        assert_eq!(
            terminal.cwd.as_deref().map(std::path::Path::new),
            Some(reported_cwd.as_path())
        );
        ipc(IpcCommand::TerminalKill(input("TERM"))).unwrap();
        wait_for_text("FOREGROUND-DONE");
    }
    ipc(IpcCommand::KomaClose {
        koma_id: koma_id.clone(),
    })
    .unwrap();
    assert!(matches!(
        ipc(IpcCommand::KomaExists {
            koma_id: koma_id.clone()
        })
        .unwrap(),
        IpcPayload::KomaExists { exists: false, .. }
    ));
    ipc(IpcCommand::SheetFocus { sheet_index: 1 }).unwrap();

    send_batch(
        &sender,
        vec![
            StudioToApp::KeyDown(KeyEvent {
                key_code: KeyCode::Comma,
                modifiers: KeyModifiers {
                    control: true,
                    ..Default::default()
                },
                is_repeat: false,
                time: 4.0,
            }),
            StudioToApp::KeyDown(KeyEvent {
                key_code: KeyCode::KeyU,
                modifiers: KeyModifiers::default(),
                is_repeat: false,
                time: 4.1,
            }),
            StudioToApp::Tick,
        ],
    );
    let reopen_deadline = Instant::now() + Duration::from_secs(5);
    loop {
        send_batch(&sender, vec![StudioToApp::Tick]);
        while hub.rx.try_recv().is_ok() {}
        let IpcPayload::KomaFocused(focused) = ipc(IpcCommand::KomaFocused).unwrap() else {
            panic!("expected focused Koma");
        };
        if focused.koma.as_ref().is_some_and(|koma| koma.id == koma_id) {
            assert_eq!(focused.sheet_id, sheet_id);
            let saved = koma_app::persistence::load(&profile_dir).unwrap().unwrap();
            assert_eq!(
                saved.koma_by_id(&koma_id.as_str().into()).unwrap().width,
                900.0
            );
            break;
        }
        assert!(
            Instant::now() < reopen_deadline,
            "u did not reopen and focus the closed Koma"
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    if std::env::var_os("KOMA_E2E_WEB_ZOOM").is_some() {
        std::thread::scope(|scope| {
            let (stop, stopped) = std::sync::mpsc::channel::<()>();
            let tick_sender = sender.clone();
            scope.spawn(move || {
                while stopped.recv_timeout(Duration::from_millis(16))
                    == Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                {
                    send_batch(&tick_sender, vec![StudioToApp::Tick]);
                }
            });
            verify_web_zoom(&sender, &ipc, &koma_id, &tmp);
            drop(stop);
        });
    }

    send_batch(&sender, vec![StudioToApp::Kill]);
    drop(child);
    let _ = std::fs::remove_dir_all(&tmp);
}

// With the CEF distribution and browser host installed: KOMA_E2E_WEB_ZOOM=1 just test.
fn verify_web_zoom(
    sender: &std::sync::mpsc::Sender<Vec<u8>>,
    ipc: &impl Fn(IpcCommand) -> Result<IpcPayload, String>,
    terminal: &str,
    tmp: &std::path::Path,
) {
    use koma_ipc::protocol::WebEvalArgs;
    use makepad_studio_protocol::{PinchPhase, RemotePinch};

    ipc(IpcCommand::KomaClose {
        koma_id: terminal.into(),
    })
    .unwrap();
    let html = tmp.join("zoom.html");
    std::fs::write(&html, "<!doctype html><title>Zoom test</title>").unwrap();
    let new_page = || {
        let IpcPayload::WebKoma(web) = ipc(IpcCommand::WebNew {
            url: html.display().to_string(),
            koma_api: false,
        })
        .unwrap() else {
            panic!("expected Web Koma ID")
        };
        web.koma_id
    };
    let a = new_page();
    let b = new_page();
    let wait_number = |id: &str, expr: &str, accept: &dyn Fn(f64) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let outcome = ipc(IpcCommand::WebEval(WebEvalArgs {
                koma_id: Some(id.into()),
                script: format!("document.title === 'Zoom test' ? ({expr}) : null"),
            }));
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
        send_batch(sender, vec![event(PinchPhase::Begin, 1.0)]);
        for _ in 0..7 {
            send_batch(sender, vec![event(PinchPhase::Update, scale)]);
            std::thread::sleep(Duration::from_millis(16));
        }
        send_batch(sender, vec![event(PinchPhase::End, 1.0)]);
    };
    pinch(1.13);
    let enlarged = wait_number(&a, "visualViewport.scale", &|scale| scale > 1.1);
    assert_eq!(wait_number(&a, "innerWidth", &|_| true), width);
    assert_eq!(wait_number(&b, "visualViewport.scale", &|_| true), base);
    assert!(wait_number(&a, "visualViewport.offsetLeft", &|_| true) > 20.0);
    assert!(wait_number(&a, "visualViewport.offsetTop", &|_| true) > 20.0);
    pinch(0.86);
    wait_number(&a, "visualViewport.scale", &|scale| scale < enlarged - 0.1);
}
