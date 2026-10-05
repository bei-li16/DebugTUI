use super::*;
use crate::registers::{RawValue, Reason, capabilities::Probe};
use std::sync::{Arc, atomic::AtomicBool, mpsc};
fn sample(app: &App, id: &str, value: &str) -> Sample {
    Sample {
        id: id.into(),
        state: State::Valid,
        implementation: Implementation::Yes,
        reason: Reason::Unknown,
        detail: String::new(),
        value: Some(RawValue::parse(value, 32).unwrap()),
        owner: Some(format!("core:{}", app.register_context().core)),
        context: app.register_context(),
        view: crate::registers::SampleView::PhysicalCore,
        owner_generation: None,
        provenance: None,
        last_value_provenance: None,
        eligibility: None,
        last_value_eligibility: None,
        timestamp_ms: 10,
        source: "openocd:cp15".into(),
    }
}
fn app() -> App {
    let mut project = Project::default();
    project.registers.cpu = "cortex-r52".into();
    let mut app = App::new(project, false);
    app.snapshot.state = "STOPPED".into();
    app.snapshot.register_session = 1;
    app.snapshot.generation = 2;
    let mut probe = Probe {
        context: app.register_context(),
        thread: "1".into(),
        identity: None,
        facts: Default::default(),
        samples: vec![
            sample(&app, "cpsr", "0x1a"),
            sample(&app, "midr", "0x411fd134"),
            sample(&app, "mpuir", "0x1800"),
            sample(&app, "hmpuir", "0x14"),
        ],
        gdb_names: vec![],
        notes: vec![],
    };
    probe.decode();
    app.snapshot.register_probe = Some(probe);
    for (id, value) in [
        ("sctlr", "0x20001"),
        ("mair0", "0xff440400"),
        ("mair1", "0xff440400"),
    ] {
        let s = sample(&app, id, value);
        app.register_view.values.insert(
            (s.owner.clone().unwrap(), id.into(), s.context.core.clone()),
            s,
        );
    }
    for index in 0..24 {
        for (id, value) in [
            (
                format!("prbar{index}"),
                format!("0x{:08x}", 0x2000001fu32 + index * 0x10000),
            ),
            (
                format!("prlar{index}"),
                format!("0x{:08x}", 0x2000ffcfu32 + index * 0x10000),
            ),
        ] {
            let s = sample(&app, &id, &value);
            app.register_view
                .values
                .insert((s.owner.clone().unwrap(), id, s.context.core.clone()), s);
        }
    }
    app
}
fn engine() -> (EngineHandle, mpsc::Receiver<Request>) {
    let (commands, requests) = mpsc::channel();
    let (_, events) = mpsc::sync_channel(512);
    (
        EngineHandle {
            commands,
            events,
            cancellation: Arc::new(AtomicBool::new(false)),
        },
        requests,
    )
}
fn render(app: &mut App, w: u16, h: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| draw(f, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect()
}
#[test]
fn mpu_overview_keyboard_mouse_and_narrow_layout_read_only_on_explicit_action() {
    let mut app = app();
    let (engine, requests) = engine();
    app.command(Some(&engine), ":mpu el1");
    assert!(requests.try_recv().is_err());
    assert!(!app.ensure_registers(Some(&engine)));
    let text = render(&mut app, 112, 30);
    assert!(text.contains("MAIR0"));
    assert!(text.contains("core:default"));
    assert!(text.contains("Normal"));
    app.key(
        KeyEvent::new(KeyCode::End, KeyModifiers::NONE),
        Some(&engine),
    );
    let text = render(&mut app, 112, 30);
    assert!(text.contains("#23"));
    let text = render(&mut app, 44, 12);
    assert!(text.contains("Read"));
    assert!(app.register_view.mpu_popup.as_ref().unwrap().max_scroll > 0);
    app.key(
        KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
        Some(&engine),
    );
    assert!(requests.try_recv().is_err());
    let read = app
        .register_view
        .mpu_popup
        .as_ref()
        .unwrap()
        .hits
        .iter()
        .find(|(_, button)| *button == 2)
        .unwrap()
        .0;
    app.mouse(
        MouseEvent {
            kind: MouseEventKind::Down(event::MouseButton::Left),
            column: read.x,
            row: read.y,
            modifiers: KeyModifiers::NONE,
        },
        Some(&engine),
    );
    let request = requests.try_recv().unwrap();
    assert_eq!(request.method, "registers_mpu");
    assert_eq!(request.params["bank"], "el1");
    assert_eq!(request.params["read"], true);
    app.key(
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
        Some(&engine),
    );
    assert!(requests.try_recv().is_err());
    app.key(
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        Some(&engine),
    );
    assert!(app.register_view.mpu_popup.is_none());
    app.register_response(request.id, &json!({"samples":[]}), None);
    assert!(app.register_view.pending.is_none());
}
#[test]
fn mpu_switch_bank_and_stale_mair_do_not_mix_data_or_show_old_values_as_current() {
    let mut app = app();
    let (engine, requests) = engine();
    app.open_mpu_view("el1");
    render(&mut app, 112, 30);
    app.snapshot.state = "RUNNING".into();
    let text = render(&mut app, 112, 30);
    assert!(text.contains("last-known"));
    assert!(text.contains("Stale"));
    assert!(!text.contains("#00 0x20000000"));
    app.key(
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
        Some(&engine),
    );
    assert!(requests.try_recv().is_err());
    app.snapshot.state = "STOPPED".into();
    app.key(
        KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE),
        Some(&engine),
    );
    let text = render(&mut app, 112, 30);
    assert!(text.contains("El2"));
    assert!(text.contains("pair unavailable"));
    assert!(!text.contains("#00 0x20000000"));
    assert!(requests.try_recv().is_err());
    app.snapshot.core = Some(crate::session::CoreStatus {
        index: 1,
        name: "core1".into(),
        endpoint: "localhost:3334".into(),
        state: "STOPPED".into(),
    });
    app.sync_register_preferences();
    assert!(app.register_view.mpu_popup.is_none());
}
