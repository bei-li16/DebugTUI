use super::super::framework_tests::key;
use super::super::tests::{app, engine, sample};
use super::*;
use ratatui::{Terminal, backend::TestBackend};

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
fn counts(app: &App) -> Counts {
    app.register_view.counts(
        &app.project,
        &app.register_context(),
        app.snapshot.state == "STOPPED",
    )
}
fn put(app: &mut App, id: &str, state: State, reason: Reason) {
    let mut value = sample(app, id, "0x80000001");
    value.state = state;
    value.reason = reason;
    app.register_view
        .values
        .insert(("core:default".into(), id.into(), "default".into()), value);
}
fn text(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        let mut x = 0;
        while x < buffer.area.width {
            let symbol = buffer[(x, y)].symbol();
            text.push_str(symbol);
            x += unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
        }
    }
    text
}

#[test]
fn register_status_reports_manual_sources_inheritance_and_unknown_reset_without_io() {
    let mut app = app();
    app.register_view.catalogue = Some(
        crate::registers::Catalogue::load(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("profiles/registers/examples/r52-source-sample.toml"),
        )
        .unwrap(),
    );
    let selected = index(&app, "edprsr");
    app.register_view.rows = vec![Row::Register(selected, 0)];
    app.selections[3] = 0;
    let before = serde_json::to_value(&app.register_view.catalogue).unwrap();
    let (engine, requests) = engine();
    for (width, height) in [(45, 12), (80, 24), (120, 36)] {
        app.open_register_status();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut rendered = String::new();
        loop {
            terminal.draw(|f| draw(f, &mut app)).unwrap();
            rendered.push_str(&text(&terminal));
            let popup = app.register_view.status_popup.as_ref().unwrap();
            if popup.scroll >= popup.max_scroll {
                break;
            }
            app.register_status_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        let compact: String = rendered
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '│')
            .collect();
        for required in [
            "Definitionconfidence:medium",
            "Reset:Unknown",
            "Definitionfile:file:",
            "Inheritance:",
            "r52-source-common.toml",
            "DDI0487",
            "M.b",
            "PDFpage:15261",
            "Fields:incomplete",
        ] {
            assert!(compact.contains(required), "{width}x{height}: {required}");
        }
        assert!(!app.ensure_registers(Some(&engine)));
        assert!(requests.try_recv().is_err());
        app.register_status_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    }
    assert_eq!(
        serde_json::to_value(&app.register_view.catalogue).unwrap(),
        before
    );
}

#[test]
fn m_cpacr_field_status_shows_permission_enums_and_manual_pages_without_io() {
    for (cpu, page) in [("cortex-m4", 264), ("cortex-m7", 287)] {
        for (width, height) in [(35, 12), (80, 24)] {
            for field in 0..2 {
                let mut app = app();
                app.project.registers.cpu = cpu.into();
                app.register_view.catalogue =
                    Some(crate::registers::Catalogue::builtin(cpu).unwrap());
                let selected = index(&app, "scb.cpacr");
                app.register_view.rows = vec![Row::Field(selected, field, 2)];
                app.selections[3] = 0;
                let mut value = sample(&app, "scb.cpacr", "0x00900000");
                value.view = crate::registers::SampleView::PhysicalCore;
                app.register_view.values.insert(
                    ("core:default".into(), "scb.cpacr".into(), "default".into()),
                    value,
                );
                let (engine, requests) = engine();
                app.open_register_status();
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                let mut rendered = String::new();
                loop {
                    terminal.draw(|f| draw(f, &mut app)).unwrap();
                    rendered.push_str(&text(&terminal));
                    let popup = app.register_view.status_popup.as_ref().unwrap();
                    if popup.scroll >= popup.max_scroll {
                        break;
                    }
                    app.register_status_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
                }
                let compact: String = rendered
                    .chars()
                    .filter(|c| !c.is_whitespace() && *c != '│')
                    .collect();
                for expected in [
                    format!("CP{}", field + 10),
                    "Denied".into(),
                    "PrivilegedOnly".into(),
                    "ReservedUnpredictable".into(),
                    "FullAccess".into(),
                    format!("PDFpage:{page}"),
                ] {
                    assert!(
                        compact.contains(&expected),
                        "{cpu} {width}x{height}: {expected}"
                    );
                }
                assert!(!app.ensure_registers(Some(&engine)));
                assert!(requests.try_recv().is_err());
            }
        }
    }
}

#[test]
fn nvic_status_shows_priority_source_irq_names_conflict_and_unknown_without_io() {
    let mut app = app();
    app.register_view.catalogue = Some(crate::registers::Catalogue::builtin("cortex-m4").unwrap());
    let selected = index(&app, "nvic.ipr31");
    app.register_view.rows = vec![Row::Register(selected, 0)];
    app.selections[3] = 0;
    let context = app.register_context();
    let nvic = crate::registers::m_profile::Nvic {
        catalogue_cpu: "cortex-m4".into(),
        priority_bits: Some(crate::registers::m_profile::Priority {
            value: 4,
            source: "SVD G:\\board.svd: /device/cpu/nvicPrioBits".into(),
        }),
        configured_priority_bits: Some(5),
        svd_priority_bits: Some(4),
        svd_source: Some("G:\\board.svd".into()),
        svd_cpu: Some("CM4".into()),
        interrupts: Some(vec![crate::svd::Interrupt {
            peripheral: "UART".into(),
            name: "UART_RX".into(),
            description: "接收中断".into(),
            value: 31,
        }]),
        notes: vec!["NVIC priority conflict: SVD=4 configuration=5".into()],
    };
    app.snapshot.register_probe = Some(crate::registers::capabilities::Probe {
        context: context.clone(),
        thread: "1".into(),
        identity: None,
        facts: Default::default(),
        samples: vec![],
        nvic: Some(nvic),
        gdb_names: vec![],
        notes: vec![],
    });
    let (engine, requests) = engine();
    for stale in [false, true] {
        if stale {
            app.snapshot
                .register_probe
                .as_mut()
                .unwrap()
                .context
                .generation += 1;
        }
        app.open_register_status();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let mut rendered = String::new();
        loop {
            terminal.draw(|f| draw(f, &mut app)).unwrap();
            rendered.push_str(&text(&terminal));
            let popup = app.register_view.status_popup.as_ref().unwrap();
            if popup.scroll >= popup.max_scroll {
                break;
            }
            app.register_status_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        let compact: String = rendered
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '│')
            .collect();
        if stale {
            assert!(compact.contains("NVICprioritybits:Unknown"));
            assert!(!compact.contains("UART_RX"));
        } else {
            for required in [
                "NVICprioritybits:4",
                "/device/cpu/nvicPrioBits",
                "IRQ31:UART/UART_RX",
                "接收中断",
                "NVICpriorityconflict",
            ] {
                assert!(compact.contains(required), "missing {required}");
            }
        }
        app.register_status_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        app.probe_registers(None);
        assert!(requests.try_recv().is_err());
    }
    drop(engine);
}

#[test]
fn register_status_preserves_current_latest_and_retained_condition_sources_without_io() {
    let mut app = app();
    let selected = index(&app, "r0");
    app.register_view.catalogue.as_mut().unwrap().registers[selected].conditions =
        vec![crate::registers::Condition {
            fact: "fixture.count".into(),
            min: 1,
            max: Some(2),
        }];
    app.project
        .registers
        .facts
        .insert("fixture.count".into(), 1);
    app.register_view.facts = app.project.registers.facts.clone();
    app.register_view.rows = vec![Row::Register(selected, 0)];
    app.selections[3] = 0;
    let context = app.register_context();
    let catalogue = app.register_view.catalogue.as_ref().unwrap();
    let register = &catalogue.registers[selected];
    let mut value = sample(&app, "r0", "0x80000001");
    value.state = State::Unavailable;
    value.reason = Reason::FeatureDisabled;
    value.eligibility =
        Some(catalogue.eligibility(register, &app.project.registers.facts, None, &context));
    let mut old = context.clone();
    old.generation -= 1;
    value.last_value_eligibility = Some(crate::registers::eligibility::Retained::Known(Box::new(
        catalogue.eligibility(
            register,
            &BTreeMap::from([("fixture.count".into(), 2)]),
            None,
            &old,
        ),
    )));
    app.register_view.values.insert(
        ("core:default".into(), "r0".into(), "default".into()),
        value,
    );
    let before =
        serde_json::to_value(app.register_view.values.values().collect::<Vec<_>>()).unwrap();
    let (engine, requests) = engine();
    for (width, height) in [(45, 12), (80, 24), (120, 36)] {
        app.open_register_status();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut rendered = String::new();
        loop {
            terminal.draw(|f| draw(f, &mut app)).unwrap();
            rendered.push_str(&text(&terminal));
            let popup = app.register_view.status_popup.as_ref().unwrap();
            if popup.scroll >= popup.max_scroll {
                break;
            }
            app.register_status_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        let compact: String = rendered
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '│')
            .collect();
        assert!(
            compact.contains("Currentconditionevaluation:Yes"),
            "{width}x{height}"
        );
        assert!(compact.contains("Latestattemptconditionevaluation:Yes"));
        assert!(compact.contains("Retainedrawvalueconditionevaluation:Yes"));
        assert!(compact.contains("fixture.count=1required1..2"));
        assert!(compact.contains("fixture.count=2required1..2"));
        assert!(compact.contains("Configuration"));
        assert!(!app.ensure_registers(Some(&engine)));
        assert!(requests.try_recv().is_err());
        app.register_status_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    }
    assert_eq!(
        serde_json::to_value(app.register_view.values.values().collect::<Vec<_>>()).unwrap(),
        before
    );
}

#[test]
fn register_status_warns_on_current_observed_cpu_mismatch_without_probing_or_cross_core_reuse() {
    let mut app = app();
    app.register_view.catalogue = Some(crate::registers::Catalogue::builtin("cortex-m4").unwrap());
    app.register_view.rows = vec![Row::Register(0, 1)];
    let mut probe = crate::registers::capabilities::Probe {
        context: app.register_context(),
        thread: "1".into(),
        identity: None,
        facts: Default::default(),
        samples: vec![sample(&app, "midr", "0x411fd134")],
        nvic: None,
        gdb_names: vec![],
        notes: vec![],
    };
    probe.decode();
    let (engine, requests) = engine();
    for change in [
        "current",
        "run",
        "session",
        "stop",
        "core",
        "frame",
        "unadapted",
    ] {
        let mut evidence = probe.clone();
        app.snapshot.state = "STOPPED".into();
        match change {
            "run" => app.snapshot.state = "RUNNING".into(),
            "session" => evidence.context.session += 1,
            "stop" => evidence.context.generation += 1,
            "core" => evidence.context.core = "other".into(),
            "frame" => evidence.context.frame += 1,
            "unadapted" => evidence.identity.as_mut().unwrap().model = None,
            _ => {}
        }
        app.snapshot.register_probe = Some(evidence);
        key(&mut app, KeyCode::Char('t'), &engine);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        key(&mut app, KeyCode::End, &engine);
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let compact: String = text(&terminal)
            .chars()
            .filter(|ch| !ch.is_whitespace() && *ch != '│')
            .collect();
        assert_eq!(
            compact.contains("Warning:observedCPU"),
            change == "current",
            "{change}: {compact}"
        );
        if change == "current" {
            assert!(compact.contains("Cortex-R52differsfromcatalogueCPUcortex-m4"));
        }
        key(&mut app, KeyCode::Esc, &engine);
    }
    assert!(requests.try_recv().is_err());
}

#[test]
fn setup_render_imports_current_identity_and_clears_it_on_run_or_draft_target_change_without_io() {
    let mut app = app();
    app.document.set(
        "registers",
        "catalogue",
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("profiles/registers/cortex-m4.toml")
            .to_string_lossy()
            .into_owned()
            .into(),
    );
    app.document
        .set("target", "endpoint", "localhost:3333".into());
    app.project = app.document.project().unwrap();
    let mut probe = crate::registers::capabilities::Probe {
        context: app.register_context(),
        thread: "1".into(),
        identity: None,
        facts: Default::default(),
        samples: vec![sample(&app, "midr", "0x411fd134")],
        nvic: None,
        gdb_names: vec![],
        notes: vec![],
    };
    probe.decode();
    app.snapshot.register_probe = Some(probe);
    let (engine, requests) = engine();
    app.open_setup();
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    for _ in 0..18 {
        app.key(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            Some(&engine),
        );
        app.key(
            KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE),
            Some(&engine),
        );
        terminal
            .draw(|f| crate::ui::render::draw(f, &mut app))
            .unwrap();
        if text(&terminal).contains("Register catalogue / preview") {
            break;
        }
    }
    assert!(text(&terminal).contains("Register catalogue / preview"));
    for state in ["current", "run", "target-change"] {
        if state == "run" {
            app.snapshot.state = "RUNNING".into();
        }
        if state == "target-change" {
            app.snapshot.state = "STOPPED".into();
            app.setup
                .as_mut()
                .unwrap()
                .document
                .set("target", "endpoint", "localhost:9999".into());
        }
        app.key(
            KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
            Some(&engine),
        );
        let mut seen = String::new();
        for _ in 0..45 {
            terminal
                .draw(|f| crate::ui::render::draw(f, &mut app))
                .unwrap();
            seen.push_str(&text(&terminal));
            app.key(
                KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
                Some(&engine),
            );
        }
        let compact: String = seen
            .chars()
            .filter(|ch| !ch.is_whitespace() && *ch != '│')
            .collect();
        assert_eq!(
            compact.contains("Warning:observedCPU"),
            state == "current",
            "{state}: {compact}"
        );
        assert_eq!(
            compact.contains("ObservedCPU[default]:Cortex-R52"),
            state == "current"
        );
        if state != "current" {
            assert!(compact.contains("ObservedCPU[default]:Unknown"));
        }
    }
    assert!(requests.try_recv().is_err());
}
#[test]
fn register_status_counts_expanded_rows_once_and_separates_failures_from_last_values() {
    let mut app = app();
    app.register_view.rows = (0..7)
        .map(|i| Row::Register(index(&app, &format!("r{i}")), 1))
        .collect();
    put(&mut app, "r0", State::Valid, Reason::Unknown);
    put(
        &mut app,
        "r1",
        State::Unsupported,
        Reason::HardwareNotImplemented,
    );
    put(&mut app, "r2", State::Unavailable, Reason::FeatureDisabled);
    put(
        &mut app,
        "r3",
        State::Unsupported,
        Reason::ReaderUnsupported,
    );
    put(&mut app, "r4", State::Error, Reason::TransportError);
    put(&mut app, "r5", State::Stale, Reason::Unknown);
    let wo = index(&app, "r6");
    app.register_view.catalogue.as_mut().unwrap().registers[wo].access =
        crate::registers::Access::Wo;
    // Fields do not create extra successful register values or definitions.
    app.register_view
        .rows
        .push(Row::Field(index(&app, "r0"), 0, 2));
    let result = counts(&app);
    assert_eq!(result.shown, 7);
    assert!(result.total > result.shown);
    assert_eq!(result.readable, 5);
    for category in CATEGORIES {
        assert_eq!(
            result.get(category),
            usize::from(category != Category::NotRead),
            "{category:?}"
        );
    }
    assert_eq!(result.categories.iter().sum::<usize>(), result.shown);
    let alias = index(&app, "r7");
    app.register_view.catalogue.as_mut().unwrap().registers[alias].reader =
        crate::registers::Reader::Alias {
            source: "r6".into(),
            offset: 0,
        };
    app.register_view.rows.push(Row::Register(alias, 1));
    assert_eq!(counts(&app).shown, 8);
    assert_eq!(counts(&app).get(Category::WriteOnly), 2);
    assert_eq!(counts(&app).readable, 5, "Alias cannot bypass parent WO");
    app.register_view.rows.clear();
    assert_eq!(counts(&app).shown, 0);
    assert_eq!(counts(&app).get(Category::Valid), 0);
    assert_eq!(counts(&app).total, result.total);
}
#[test]
fn register_status_counts_apply_owner_stop_session_and_gdb_frame_rules() {
    let mut app = app();
    app.register_view.query = "r0 in the core register group.".into();
    app.register_view.rebuild();
    put(&mut app, "r0", State::Valid, Reason::Unknown);
    assert_eq!(counts(&app).get(Category::Valid), 1);
    app.snapshot.frame.level = 1;
    assert_eq!(counts(&app).get(Category::Stale), 1);
    app.snapshot.frame.level = 0;
    app.snapshot.state = "RUNNING".into();
    assert_eq!(counts(&app).get(Category::Valid), 0);
    app.snapshot.state = "STOPPED".into();
    app.snapshot.generation += 1;
    assert_eq!(counts(&app).get(Category::Stale), 1);
    app.snapshot.generation -= 1;
    app.snapshot.register_session += 1;
    assert_eq!(counts(&app).get(Category::Stale), 1);
    app.snapshot.register_session -= 1;
    let value = app
        .register_view
        .values
        .get_mut(&("core:default".into(), "r0".into(), "default".into()))
        .unwrap();
    value.owner = Some("core:other".into());
    assert_eq!(counts(&app).get(Category::Stale), 1);
    put(&mut app, "r0", State::Valid, Reason::Unknown);
    // A matching owner does not turn a missing raw value into success.
    app.register_view
        .values
        .get_mut(&("core:default".into(), "r0".into(), "default".into()))
        .unwrap()
        .value = None;
    assert_eq!(counts(&app).get(Category::Valid), 0);
    assert_eq!(counts(&app).get(Category::Error), 1);
}
#[test]
fn register_status_search_filter_and_target_view_update_definition_counts_without_reads() {
    let mut app = app();
    let total = counts(&app).total;
    app.register_view
        .facts
        .insert("icc.physical.prebits".into(), 5);
    app.register_view
        .facts
        .insert("gic.system_interface".into(), 1);
    app.register_view
        .facts
        .insert("icv.virtual.prebits".into(), 5);
    app.register_view.query = "AP0R".into();
    app.register_view.rebuild();
    assert_eq!(counts(&app).shown, 3);
    app.register_view.all_definitions = true;
    app.register_view.rebuild();
    assert_eq!(counts(&app).shown, 12);
    assert_eq!(counts(&app).get(Category::NotImplemented), 9);
    app.register_view.filter = 1;
    app.register_view.rebuild();
    assert_eq!(counts(&app).shown, 0);
    assert_eq!(counts(&app).total, total);
    let (engine, requests) = engine();
    app.command(Some(&engine), ":register-status");
    assert!(!app.ensure_registers(Some(&engine)));
    assert!(requests.try_recv().is_err());
}
#[test]
fn register_status_popup_scrolls_in_narrow_layout_and_closes_by_keyboard_and_mouse() {
    let mut app = app();
    app.register_view.source = "自定义目录/核心寄存器.toml".into();
    for (width, height) in [(100, 24), (35, 12)] {
        app.open_register_status();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        assert!(text(&terminal).contains("Catalogue total"));
        assert!(text(&terminal).contains("Shown registers"));
        let max = app.register_view.status_popup.as_ref().unwrap().max_scroll;
        if width == 35 {
            assert!(max > 0);
        }
        app.register_status_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let rendered = text(&terminal);
        let chinese: String = rendered
            .chars()
            .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c))
            .collect();
        assert!(
            chinese.contains("自定义目录"),
            "{width}x{height}: {rendered}"
        );
        let close = app.register_view.status_popup.as_ref().unwrap().close;
        app.register_status_mouse(MouseEvent {
            kind: MouseEventKind::Down(event::MouseButton::Left),
            column: close.x,
            row: close.y,
            modifiers: KeyModifiers::NONE,
        });
        assert!(app.register_view.status_popup.is_none());
    }
    app.open_register_status();
    app.register_status_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.register_view.status_popup.is_none());
}
#[test]
fn register_read_cancel_keeps_inflight_limit_old_values_and_ignores_late_response() {
    let mut app = app();
    put(&mut app, "r0", State::Valid, Reason::Unknown);
    let old = serde_json::to_value(app.register_view.values.values().collect::<Vec<_>>()).unwrap();
    let (engine, requests) = engine();
    assert!(app.ensure_registers(Some(&engine)));
    let request = requests.try_recv().unwrap();
    app.key(
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        Some(&engine),
    );
    assert!(
        request
            .read_cancel
            .load(std::sync::atomic::Ordering::Relaxed)
    );
    assert!(
        !engine
            .cancellation
            .load(std::sync::atomic::Ordering::Relaxed)
    );
    assert!(!app.ensure_registers(Some(&engine)));
    assert!(app.register_read_pending());
    app.register_response(
        request.id,
        &json!({"samples":[sample(&app,"r0","0x22")]}),
        None,
    );
    assert_eq!(
        serde_json::to_value(app.register_view.values.values().collect::<Vec<_>>()).unwrap(),
        old
    );
    assert!(!app.register_read_pending());
    assert!(!app.ensure_registers(Some(&engine)));
    assert!(requests.try_recv().is_err());
    assert!(app.notice.contains("cancelled"));
    assert!(app.request_registers(Some(&engine), vec!["r0".into()], true));
    assert!(
        !requests
            .try_recv()
            .unwrap()
            .read_cancel
            .load(std::sync::atomic::Ordering::Relaxed)
    );
}
#[test]
fn register_probe_cancel_does_not_import_late_capabilities_and_search_escape_is_separate() {
    let mut app = app();
    let (engine, requests) = engine();
    app.probe_registers(Some(&engine));
    let request = requests.try_recv().unwrap();
    app.start_register_search();
    app.key(
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        Some(&engine),
    );
    assert!(
        !request
            .read_cancel
            .load(std::sync::atomic::Ordering::Relaxed)
    );
    app.command(Some(&engine), ":register-cancel");
    let facts = app.register_view.facts.clone();
    app.register_probe_response(request.id, &json!({"probe":{}}), None);
    assert_eq!(app.register_view.facts, facts);
    assert!(!app.register_read_pending());
    assert!(app.snapshot.register_probe.is_none());
}

#[test]
fn register_status_runtime_absence_is_context_scoped_and_all_view_does_not_auto_probe_it() {
    let mut app = app();
    app.register_view.query = "r0 in the core register group.".into();
    put(
        &mut app,
        "r0",
        State::Unsupported,
        Reason::HardwareNotImplemented,
    );
    app.sync_register_absence();
    assert_eq!(counts(&app).shown, 0);
    app.toggle_register_definitions(None);
    assert_eq!(counts(&app).shown, 1);
    assert_eq!(counts(&app).get(Category::NotImplemented), 1);
    let (engine, requests) = engine();
    assert!(!app.ensure_registers(Some(&engine)));
    assert!(requests.try_recv().is_err());
    app.snapshot.generation += 1;
    app.sync_register_absence();
    assert_eq!(counts(&app).get(Category::NotImplemented), 0);
    assert_eq!(counts(&app).get(Category::Stale), 1);
    assert!(app.ensure_registers(Some(&engine)));
}

#[test]
fn register_status_row_explains_reader_unsupported_and_retains_precise_old_value() {
    let mut app = app();
    app.register_view.query = "r0".into();
    app.register_view.rebuild();
    put(
        &mut app,
        "r0",
        State::Unsupported,
        Reason::ReaderUnsupported,
    );
    let mut terminal = Terminal::new(TestBackend::new(100, 15)).unwrap();
    terminal.draw(|f| app.draw_registers(f, f.area())).unwrap();
    let rendered = text(&terminal);
    assert!(
        rendered.contains("0x80000001 [Reader unsupported]"),
        "{rendered}"
    );
    assert!(rendered.contains("Valid 0"));
    assert_eq!(counts(&app).get(Category::ReaderUnsupported), 1);
}

#[test]
fn register_status_details_keep_long_reader_reason_source_and_sample_time_accessible() {
    let mut app = app();
    app.register_view.query = "r0 in the core register group.".into();
    app.register_view.rebuild();
    app.selection = app
        .register_view
        .rows
        .iter()
        .position(|row| matches!(row, Row::Register(_, _)))
        .unwrap();
    put(&mut app, "r0", State::Unavailable, Reason::AccessRestricted);
    app.register_view
        .values
        .get_mut(&("core:default".into(), "r0".into(), "default".into()))
        .unwrap()
        .detail = format!(
        "{}physical permission denied at the selected core",
        "long backend diagnostic; ".repeat(20)
    );
    app.open_register_status();
    let mut terminal = Terminal::new(TestBackend::new(35, 12)).unwrap();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let mut evidence = text(&terminal);
    let max = app.register_view.status_popup.as_ref().unwrap().max_scroll;
    for _ in 0..max {
        app.register_status_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        evidence.push_str(&text(&terminal));
    }
    assert!(evidence.contains("gdb:r0"));
    assert!(evidence.contains("sample at 23 ms"));
    let compact: String = evidence
        .chars()
        .filter(|c| c.is_ascii() && !c.is_whitespace())
        .collect();
    assert!(compact.contains("physicalpermissiondenied"));
    assert!(evidence.contains("0x80000001"));
}

#[test]
fn register_help_preserves_the_full_128_bit_raw_value_when_the_main_row_is_clipped() {
    let mut app = app();
    let (engine, requests) = engine();
    let q15 = index(&app, "q15");
    app.register_view.rows = vec![Row::Register(q15, 2)];
    let raw = "0xfedcba98765432100123456789abcdef";
    let mut value = sample(&app, "q15", "0x0");
    value.value = Some(crate::registers::RawValue::parse(raw, 128).unwrap());
    app.register_view.values.insert(
        ("core:default".into(), "q15".into(), "default".into()),
        value,
    );
    let mut terminal = Terminal::new(TestBackend::new(35, 12)).unwrap();
    terminal.draw(|f| app.draw_registers(f, f.area())).unwrap();
    assert!(!text(&terminal).contains(raw));
    key(&mut app, KeyCode::Char('t'), &engine);
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let mut evidence = text(&terminal);
    let max = app.register_view.status_popup.as_ref().unwrap().max_scroll;
    for _ in 0..max {
        key(&mut app, KeyCode::Down, &engine);
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        evidence.push_str(&text(&terminal));
    }
    let compact = evidence
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '│')
        .collect::<String>();
    assert!(compact.contains(raw));
    assert!(compact.contains("128bitsRW"));
    assert!(compact.contains("Owner:core:default"));
    assert!(requests.try_recv().is_err());
}

#[test]
fn register_field_help_keeps_all_enums_conditions_bits_and_chinese_description_scrollable() {
    for (width, height) in [(35, 12), (80, 24)] {
        for field_name in ["M", "IT"] {
            let mut app = app();
            let (engine, requests) = engine();
            let cpsr = index(&app, "cpsr");
            let register = &mut app.register_view.catalogue.as_mut().unwrap().registers[cpsr];
            register.description = "父寄存器描述".into();
            register.access_condition = "需要当前物理核心暂停，不改变处理器模式".into();
            let field = register
                .fields
                .iter()
                .position(|f| f.name == field_name)
                .unwrap();
            register.fields[field].description =
                format!("{}字段说明末尾", "中文字段说明；".repeat(12));
            let enums = register.fields[field]
                .enums
                .iter()
                .map(|e| e.name.clone())
                .collect::<Vec<_>>();
            app.register_view.rows = vec![Row::Field(cpsr, field, 2)];
            let mut value = sample(&app, "cpsr", "0x0400ac13");
            value.state = State::Unavailable;
            value.reason = Reason::AccessRestricted;
            value.detail = "physical access denied for this stopped core".into();
            let before = serde_json::to_value(&value).unwrap();
            app.register_view.values.insert(
                ("core:default".into(), "cpsr".into(), "default".into()),
                value,
            );
            key(&mut app, KeyCode::Char('t'), &engine);
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| draw(f, &mut app)).unwrap();
            let mut evidence = text(&terminal);
            let max = app.register_view.status_popup.as_ref().unwrap().max_scroll;
            for _ in 0..max {
                key(&mut app, KeyCode::Down, &engine);
                terminal.draw(|f| draw(f, &mut app)).unwrap();
                evidence.push_str(&text(&terminal));
            }
            assert!(evidence.contains("gdb:cpsr"));
            assert!(evidence.contains("AccessRestricted"));
            let compact = evidence
                .chars()
                .filter(|c| !c.is_whitespace() && *c != '│')
                .collect::<String>();
            assert!(
                compact.contains("父寄存器描述"),
                "{width}x{height} {field_name}: {compact}"
            );
            assert!(compact.contains("字段说明末尾"));
            assert!(compact.contains("不改变处理器模式"));
            if field_name == "M" {
                assert!(compact.contains("Fieldwidth:5bits"));
                assert!(compact.contains("[4:0]"));
                assert!(evidence.contains("0x13"));
                for name in enums {
                    assert!(evidence.contains(&name), "missing {name}: {evidence}");
                }
            } else {
                assert!(compact.contains("Fieldwidth:8bits"));
                assert!(compact.contains("[26:25],[15:10]"));
                assert!(evidence.contains("0xae"));
            }
            key(&mut app, KeyCode::Home, &engine);
            assert_eq!(app.register_view.status_popup.as_ref().unwrap().scroll, 0);
            key(&mut app, KeyCode::Esc, &engine);
            assert!(app.register_view.status_popup.is_none());
            assert_eq!(
                serde_json::to_value(
                    &app.register_view.values
                        [&("core:default".into(), "cpsr".into(), "default".into())]
                )
                .unwrap(),
                before
            );
            assert!(requests.try_recv().is_err());
        }
    }
}
