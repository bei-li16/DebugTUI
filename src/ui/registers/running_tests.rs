use super::tests::{engine, sample};
use super::*;
use crate::{
    config::MemoryAccess,
    registers::{Component, Reason, SampleView},
};

fn app() -> App {
    let mut project = Project::default();
    project.registers.cpu = "cortex-m4".into();
    project.memory_access = vec![MemoryAccess {
        id: "ppb".into(),
        label: "ppb".into(),
        tcl_endpoint: "localhost:6666".into(),
        target: "cpu0".into(),
        cores: vec!["default".into()],
        while_running: true,
    }];
    project
        .registers
        .targets
        .insert("default".into(), "cpu0".into());
    project.registers.component_owners.insert(
        "ppb".into(),
        BTreeMap::from([(
            "core:default".into(),
            Component {
                base: 0,
                channel: "ppb".into(),
                little_endian: true,
            },
        )]),
    );
    let mut app = App::new(project, false);
    app.snapshot.state = "RUNNING".into();
    app.snapshot.register_session = 17;
    app.snapshot.generation = 3;
    app.select_pane(3);
    app.view_rects[3] = Rect::new(0, 0, 80, 4);
    app.register_view.rows = ["scb.cpuid", "scb.ccr", "r0", "dcb.dhcsr"]
        .into_iter()
        .map(|id| Row::Register(index(&app, id), 1))
        .collect();
    app
}
fn index(app: &App, id: &str) -> usize {
    app.register_view
        .catalogue
        .as_ref()
        .unwrap()
        .registers
        .iter()
        .position(|r| r.id == id)
        .unwrap()
}
fn running_sample(app: &App, id: &str) -> Sample {
    let mut s = sample(app, id, "0x410fc241");
    s.view = SampleView::RunningMemory;
    s.provenance=Some(serde_json::from_value(json!({"acquisition":"catalogue","catalogue_reader":{"kind":"core_private","address":0xe000ed00u32},
        "access":{"route":{"kind":"tcl_memory","endpoint":"localhost:6666","target":"cpu0","channel":"ppb","configuration_source":"test","address":"0xe000ed00","bits":32,"bus_width":32,"count":1,"byte_order":"little","atomic":true},
            "phase":"responded","command":"cpu0 read_memory 0xe000ed00 32 1","context":s.context,"timestamp_ms":20,"completed_ms":23}})).unwrap());
    s
}

#[test]
fn running_real_ui_scheduler_reads_only_visible_safe_rows_once_and_renews_after_runtime_boundary() {
    let mut app = app();
    let (engine, requests) = engine();
    app.snapshot.state = "STOPPED".into();
    app.sync_register_sample_validity();
    app.register_view
        .attempts
        .insert((17, 3, "default".into(), 0, "scb.cpuid".into()));
    app.register_view
        .attempts
        .insert((17, 3, "default".into(), 0, "scb.ccr".into()));
    app.snapshot.state = "RUNNING".into();
    assert!(app.ensure_visible_data(Some(&engine)));
    let request = requests.try_recv().unwrap();
    assert_eq!(request.method, "registers_read");
    assert_eq!(request.params["ids"], json!(["scb.ccr", "scb.cpuid"]));
    let context = app.register_context();
    let values = vec![
        running_sample(&app, "scb.ccr"),
        running_sample(&app, "scb.cpuid"),
    ];
    app.register_response(
        request.id,
        &json!({"context":context,"samples":values}),
        None,
    );
    for _ in 0..50 {
        assert!(!app.ensure_visible_data(Some(&engine)));
    }
    assert!(requests.try_recv().is_err());
    assert!(app.action_enabled("register-refresh"));
    let pos = index(&app, "scb.cpuid");
    assert_eq!(
        app.register_view
            .category(&app.project, &context, pos, false),
        status::Category::Valid
    );
    app.snapshot.state = "STOPPED".into();
    app.sync_register_sample_validity();
    let stored = app
        .register_view
        .sample(&app.project, &context, pos)
        .unwrap();
    assert_eq!(stored.state, State::Stale);
    assert!(stored.value.is_some());
    app.snapshot.state = "FAULT".into();
    assert!(!app.ensure_visible_data(Some(&engine)));
    assert!(requests.try_recv().is_err());
}

#[test]
fn running_ui_has_zero_automatic_requests_for_stopped_or_gdb_memory_routes_and_disconnected_values()
{
    for kind in ["stopped", "gdb", "missing"] {
        let mut app = app();
        match kind {
            "stopped" => app.project.memory_access[0].while_running = false,
            "gdb" => app
                .project
                .registers
                .component_owners
                .get_mut("ppb")
                .unwrap()
                .get_mut("core:default")
                .unwrap()
                .channel
                .clear(),
            "missing" => app.project.memory_access.clear(),
            _ => unreachable!(),
        }
        let (engine, requests) = engine();
        for _ in 0..10 {
            assert!(!app.ensure_visible_data(Some(&engine)));
        }
        assert!(requests.try_recv().is_err());
    }
    let mut app = app();
    let context = app.register_context();
    let pos = index(&app, "scb.cpuid");
    let s = running_sample(&app, "scb.cpuid");
    app.register_view
        .values
        .insert(("core:default".into(), s.id.clone(), "default".into()), s);
    app.snapshot.state = "DISCONNECTED".into();
    app.sync_register_sample_validity();
    assert_eq!(
        app.register_view
            .category(&app.project, &context, pos, false),
        status::Category::Stale
    );
}

#[test]
fn running_ui_rejects_stopped_samples_and_shows_needhalt_as_unavailable_without_target_io() {
    let mut app = app();
    let (engine, requests) = engine();
    assert!(app.request_registers(Some(&engine), vec!["scb.cpuid".into()], true));
    let request = requests.try_recv().unwrap();
    let context = app.register_context();
    let old = sample(&app, "scb.cpuid", "0x12345678");
    app.register_response(
        request.id,
        &json!({"context":context,"samples":[old]}),
        None,
    );
    assert!(app.register_view.values.is_empty());
    let mut denied = sample(&app, "r0", "0x12345678");
    denied.state = State::Unavailable;
    denied.reason = Reason::AccessRestricted;
    denied.detail = "NeedHalt: GDB regfile requires stopped target".into();
    app.register_view.values.insert(
        ("core:default".into(), "r0".into(), "default".into()),
        denied,
    );
    assert_eq!(
        app.register_view
            .category(&app.project, &context, index(&app, "r0"), false),
        status::Category::Unavailable
    );
    assert!(requests.try_recv().is_err());
}

#[test]
fn running_status_displays_recorded_ap_interval_raw_and_view_in_narrow_and_wide_windows_without_io()
{
    use ratatui::{Terminal, backend::TestBackend};
    for (width, height) in [(35, 12), (80, 24)] {
        let mut app = app();
        let s = running_sample(&app, "scb.cpuid");
        app.register_view
            .values
            .insert(("core:default".into(), s.id.clone(), "default".into()), s);
        app.register_view.rows = vec![Row::Register(index(&app, "scb.cpuid"), 1)];
        app.selections[3] = 0;
        let (engine, requests) = engine();
        app.open_register_status();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut rendered = String::new();
        let mut previous = String::new();
        let mut ended = false;
        for _ in 0..256 {
            terminal.draw(|f| status::draw(f, &mut app)).unwrap();
            let buffer = terminal.backend().buffer();
            let mut screen = String::new();
            for y in 0..height {
                for x in 0..width {
                    screen.push_str(buffer[(x, y)].symbol());
                }
            }
            if screen == previous {
                ended = true;
                break;
            }
            rendered.push_str(&screen);
            previous = screen;
            app.register_status_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        assert!(ended, "status scrolling must settle at the end");
        let compact: String = rendered
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '│')
            .collect();
        for text in [
            "Samplingview:runningAPmemory",
            "0x410fc241",
            "cpu0",
            "20",
            "23",
        ] {
            assert!(compact.contains(text), "{width}x{height}: {text}");
        }
        assert!(!app.ensure_visible_data(Some(&engine)));
        assert!(requests.try_recv().is_err());
    }
}
