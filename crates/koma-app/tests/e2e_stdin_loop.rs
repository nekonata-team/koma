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
                break;
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
        let terminal = saved.koma_by_id(&koma_id).unwrap();
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
            assert_eq!(saved.koma_by_id(&koma_id).unwrap().width, 900.0);
            break;
        }
        assert!(
            Instant::now() < reopen_deadline,
            "u did not reopen and focus the closed Koma"
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    send_batch(&sender, vec![StudioToApp::Kill]);
    drop(child);
    let _ = std::fs::remove_dir_all(&tmp);
}
