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
        .insert(("core:default".into(), id.into()), value);
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
        .get_mut(&("core:default".into(), "r0".into()))
        .unwrap();
    value.owner = Some("core:other".into());
    assert_eq!(counts(&app).get(Category::Stale), 1);
    put(&mut app, "r0", State::Valid, Reason::Unknown);
    // A matching owner does not turn a missing raw value into success.
    app.register_view
        .values
        .get_mut(&("core:default".into(), "r0".into()))
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
    app.register_view.query = "AP0R".into();
    app.register_view.rebuild();
    assert_eq!(counts(&app).shown, 1);
    app.register_view.all_definitions = true;
    app.register_view.rebuild();
    assert_eq!(counts(&app).shown, 4);
    assert_eq!(counts(&app).get(Category::NotImplemented), 3);
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
        .get_mut(&("core:default".into(), "r0".into()))
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
    app.register_view
        .values
        .insert(("core:default".into(), "q15".into()), value);
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
            app.register_view
                .values
                .insert(("core:default".into(), "cpsr".into()), value);
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
                    &app.register_view.values[&("core:default".into(), "cpsr".into())]
                )
                .unwrap(),
                before
            );
            assert!(requests.try_recv().is_err());
        }
    }
}
