use super::*;
use crate::registers::{RawValue, Reason, capabilities::Probe};
use std::sync::{Arc, atomic::AtomicBool, mpsc};
#[test]
fn m7_cache_popup_open_scroll_render_and_close_are_zero_io_and_read_is_explicit() {
    let mut project = Project::default();
    project.registers.cpu = "cortex-m7".into();
    let mut app = App::new(project, false);
    app.snapshot.state = "STOPPED".into();
    app.snapshot.register_session = 1;
    app.snapshot.generation = 2;
    let mut proof = super::tests::app().snapshot.register_probe.unwrap();
    proof.identity.as_mut().unwrap().model = Some("Cortex-M7".into());
    for (id, value) in [("mcache.clidr", 0x09000003), ("mcache.ctr", 0x8303c003)] {
        let mut fact = proof.facts["mpu.el1.regions"].clone();
        fact.value = value;
        proof.facts.insert(id.into(), fact);
    }
    app.snapshot.register_probe = Some(proof);
    let mut view = crate::registers::m_cache::View {
        context: app.register_context(),
        owner: "core:default".into(),
        state: State::Valid,
        identity: sample(&app, "scb.cpuid", "0x411fc271"),
        clidr: sample(&app, "scb.clidr", "0x09000003"),
        ctr: sample(&app, "scb.ctr", "0x8303c003"),
        original_selector: Some(RawValue::parse("0x1", 32).unwrap()),
        restored_selector: Some(RawValue::parse("0x1", 32).unwrap()),
        caches: vec![
            crate::registers::m_cache::Cache {
                selector: 0,
                kind: "data".into(),
                size_id: sample(&app, "scb.ccsidr", "0xf00fe019"),
            },
            crate::registers::m_cache::Cache {
                selector: 1,
                kind: "instruction".into(),
                size_id: sample(&app, "scb.ccsidr", "0xf007e009"),
            },
        ],
    };
    assert_eq!(view.caches[0].size_bytes(), Some(16384));
    view.caches[0].size_id.stale();
    assert_eq!(view.caches[0].size_bytes(), None);
    view.caches[0].size_id.state = State::Valid;
    app.snapshot.register_cache = Some(view);
    let (engine, requests) = engine();
    app.open_cache_view("");
    assert!(app.register_view.mpu_popup.as_ref().unwrap().cache);
    let text = render(&mut app, 112, 30);
    assert!(text.contains("16 KiB"));
    assert!(text.contains("4 KiB"));
    assert!(text.contains("ICache="));
    for (w, h) in [(35, 12), (80, 24)] {
        render(&mut app, w, h);
        app.key(
            KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
            Some(&engine),
        );
    }
    assert!(requests.try_recv().is_err());
    app.key(
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
        Some(&engine),
    );
    let req = requests.try_recv().unwrap();
    assert_eq!(req.method, "registers_cache");
    assert!(req.params.get("bank").is_none());
    app.register_response(req.id, &json!({}), None);
    app.snapshot.state = "RUNNING".into();
    app.register_view.mpu_popup.as_mut().unwrap().scroll = 0;
    let text = render(&mut app, 112, 30);
    assert!(text.contains("Stale"));
    assert!(!text.contains("KiB"));
    assert!(!text.contains("ICache="));
    assert!(text.contains("0xf00fe019"));
    app.key(
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
        Some(&engine),
    );
    app.key(
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        Some(&engine),
    );
    assert!(requests.try_recv().is_err());
}

#[test]
fn m7_cache_popup_refuses_other_cpus_and_unknown_identity_without_requests() {
    let mut app = app();
    app.open_cache_view("");
    assert!(app.register_view.mpu_popup.is_none());
    let mut project = Project::default();
    project.registers.cpu = "cortex-m7".into();
    let mut app = App::new(project, false);
    app.snapshot.state = "STOPPED".into();
    let (engine, requests) = engine();
    app.open_cache_view("");
    app.key(
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
        Some(&engine),
    );
    assert!(requests.try_recv().is_err());
    assert!(
        app.register_view
            .mpu_popup
            .as_ref()
            .unwrap()
            .error
            .is_some()
    );
}

#[test]
fn m_mpu_overview_uses_explicit_read_and_shows_stale_raw_without_field_derivation() {
    let mut project = Project::default();
    project.registers.cpu = "cortex-m3".into();
    let mut app = App::new(project, false);
    app.snapshot.state = "STOPPED".into();
    app.snapshot.register_session = 1;
    app.snapshot.generation = 2;
    let mut proof = super::tests::app().snapshot.register_probe.unwrap();
    proof.identity.as_mut().unwrap().model = Some("Cortex-M3".into());
    let mut count = proof.facts["mpu.el1.regions"].clone();
    count.value = 8;
    proof.facts.insert("mpu.regions".into(), count);
    app.snapshot.register_probe = Some(proof);
    app.snapshot.register_mpu = Some(crate::registers::mpu::m_profile::View {
        context: app.register_context(),
        owner: "core:default".into(),
        cpu: "cortex-m3".into(),
        count: 8,
        state: State::Valid,
        identity: sample(&app, "scb.cpuid", "0x410fc231"),
        mpu_type: sample(&app, "mpu.type", "0x800"),
        control: Some(sample(&app, "mpu.ctrl", "0x5")),
        original_selector: Some(RawValue::parse("0x3", 32).unwrap()),
        restored_selector: Some(RawValue::parse("0x3", 32).unwrap()),
        regions: vec![crate::registers::mpu::m_profile::Region {
            index: 0,
            base: sample(&app, "mpu.rbar", "0x20000000"),
            attributes: sample(&app, "mpu.rasr", "0x0307001f"),
        }],
    });
    let (engine, requests) = engine();
    app.open_mpu_view("");
    let text = render(&mut app, 112, 30);
    assert!(text.contains("RBAR"));
    assert!(text.contains("ENABLE="));
    assert!(requests.try_recv().is_err());
    app.key(
        KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE),
        Some(&engine),
    );
    assert!(app.register_view.mpu_popup.as_ref().unwrap().m_profile);
    assert!(render(&mut app, 44, 12).contains("Read"));
    assert!(requests.try_recv().is_err());
    app.key(
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
        Some(&engine),
    );
    let request = requests.try_recv().unwrap();
    assert_eq!(request.method, "registers_mpu");
    assert_eq!(request.params["bank"], "m");
    app.register_response(request.id, &json!({}), None);
    app.snapshot.state = "RUNNING".into();
    let text = render(&mut app, 112, 30);
    assert!(text.contains("Stale"));
    assert!(!text.contains("ENABLE="));
    app.key(
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
        Some(&engine),
    );
    assert!(requests.try_recv().is_err());
}
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
        nvic: None,
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
