use super::tests::{app, engine, sample};
use super::*;
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn structured_conditions_and_unknown_owner_do_not_queue_automatic_ui_reads() {
    use crate::registers::{
        Field, SampleView, Scope, Segment,
        policy::{Compare, FieldCondition},
    };
    let mut app = app();
    let (engine, requests) = engine();
    let positions: Vec<_> = ["r0", "r1", "r2", "r3", "r4"]
        .iter()
        .map(|id| index(&app, id))
        .collect();
    let c = app.register_view.catalogue.as_mut().unwrap();
    c.registers[positions[0]].fields = vec![Field {
        name: "FLAG".into(),
        description: "Enable flag".into(),
        segments: vec![Segment {
            offset: 7,
            width: 1,
        }],
        access: None,
        enums: vec![],
    }];
    let condition = FieldCondition {
        reg: "r0".into(),
        field: "FLAG".into(),
        op: Compare::Eq,
        value: 1,
    };
    c.registers[positions[1]].present_if = Some(condition.clone());
    c.registers[positions[2]].access_rule.need_enable = Some(condition);
    c.registers[positions[3]].scope = Scope::Unknown;
    c.registers[positions[3]].writer = None;
    c.registers[positions[3]].write = None;
    c.registers[positions[4]].access_rule.min_el = Some(2);
    c.validate().unwrap();
    app.project.registers.facts.insert("cpu.debug_el".into(), 2);
    app.sync_register_capabilities();
    app.register_view.rows = positions[1..]
        .iter()
        .map(|i| Row::Register(*i, 1))
        .collect();
    assert!(!app.ensure_registers(Some(&engine)));
    assert!(requests.try_recv().is_err());
    let mut observed = sample(&app, "r0", "0x80");
    observed.view = SampleView::PhysicalCore;
    observed.provenance=Some(serde_json::from_value(json!({"acquisition":"catalogue","catalogue_reader":{"kind":"gdb","name":"r0"},
        "access":{"route":{"kind":"gdb_register","endpoint":"localhost:3333","configured_endpoint":"localhost:3333","name":"r0","index":0},
            "phase":"responded","command":"-data-list-register-values r 0","context":observed.context,"timestamp_ms":20,"completed_ms":22}})).unwrap());
    app.snapshot.register_samples = vec![observed];
    app.sync_register_capabilities();
    app.register_view.rows = positions[1..]
        .iter()
        .map(|i| Row::Register(*i, 1))
        .collect();
    assert!(app.ensure_registers(Some(&engine)));
    let request = requests.try_recv().unwrap();
    assert_eq!(request.params["ids"], json!(["r1", "r2"]));
    assert!(requests.try_recv().is_err());
}

#[test]
fn per_core_register_ui_switches_catalogue_facts_and_drops_other_model_values() {
    let mut project = Project::default();
    project.registers.cpu = "cortex-m4".into();
    project.cores = vec![
        crate::config::Core {
            name: "m4".into(),
            endpoint: "localhost:3333".into(),
            ..Default::default()
        },
        crate::config::Core {
            name: "r52".into(),
            endpoint: "localhost:3334".into(),
            registers: Some(
                toml::from_str("cpu='cortex-r52'\n[facts]\n'customer.capacity'=3\n").unwrap(),
            ),
            ..Default::default()
        },
    ];
    let mut app = App::new(project, false);
    assert_eq!(
        app.register_view.catalogue.as_ref().unwrap().cpu,
        "cortex-m4"
    );
    app.register_view.query = "xpsr".into();
    let old_sample = sample(&app, "r0", "0x1234");
    app.register_view
        .values
        .insert(("core:m4".into(), "r0".into(), "m4".into()), old_sample);
    let mut snapshot = app.snapshot.clone();
    snapshot.core = Some(crate::session::CoreStatus {
        index: 1,
        name: "r52".into(),
        endpoint: "localhost:3334".into(),
        state: "STOPPED".into(),
    });
    app.update(Event::Snapshot {
        snapshot: Box::new(snapshot.clone()),
    });
    assert_eq!(
        app.register_view.catalogue.as_ref().unwrap().cpu,
        "cortex-r52"
    );
    assert!(
        app.register_view
            .catalogue
            .as_ref()
            .unwrap()
            .register("cpsr")
            .is_some()
    );
    assert!(
        app.register_view
            .catalogue
            .as_ref()
            .unwrap()
            .register("xpsr")
            .is_none()
    );
    assert!(app.register_view.query.is_empty() && app.register_view.values.is_empty());
    assert_eq!(app.register_view.facts["customer.capacity"], 3);
    let r52_scope = app.register_view.preference_scope.clone();
    snapshot.core = Some(crate::session::CoreStatus {
        index: 0,
        name: "m4".into(),
        endpoint: "localhost:3333".into(),
        state: "STOPPED".into(),
    });
    app.update(Event::Snapshot {
        snapshot: Box::new(snapshot),
    });
    assert_eq!(
        app.register_view.catalogue.as_ref().unwrap().cpu,
        "cortex-m4"
    );
    assert!(!app.register_view.facts.contains_key("customer.capacity"));
    assert_ne!(app.register_view.preference_scope, r52_scope);
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

#[test]
fn m_multicore_ui_switch_rejects_late_same_ppb_value_from_the_other_builtin_model() {
    let mut project = Project::default();
    project.registers.cpu = "cortex-m7".into();
    project.cores = [("m7", "cortex-m7"), ("m4", "cortex-m4")]
        .into_iter()
        .enumerate()
        .map(|(i, (name, cpu))| crate::config::Core {
            name: name.into(),
            endpoint: format!("localhost:{}", 3333 + i),
            registers: Some(crate::registers::CoreConfig {
                cpu: Some(cpu.into()),
                ..Default::default()
            }),
            ..Default::default()
        })
        .collect();
    let mut app = App::new(project, false);
    let (engine, requests) = engine();
    let mut snapshot = app.snapshot.clone();
    snapshot.core = Some(crate::session::CoreStatus {
        index: 0,
        name: "m7".into(),
        endpoint: "localhost:3333".into(),
        state: "STOPPED".into(),
    });
    snapshot.state = "STOPPED".into();
    snapshot.register_session = 17;
    app.update(Event::Snapshot {
        snapshot: Box::new(snapshot),
    });
    let mut late = sample(&app, "scb.cpuid", "0x410fc271");
    late.view = crate::registers::SampleView::PhysicalCore;
    assert!(app.request_registers(Some(&engine), vec!["scb.cpuid".into()], true));
    let request = requests.try_recv().unwrap();
    let mut snapshot = app.snapshot.clone();
    snapshot.core = Some(crate::session::CoreStatus {
        index: 1,
        name: "m4".into(),
        endpoint: "localhost:3334".into(),
        state: "STOPPED".into(),
    });
    snapshot.register_session = 18;
    app.update(Event::Snapshot {
        snapshot: Box::new(snapshot),
    });
    assert_eq!(
        app.register_view.catalogue.as_ref().unwrap().cpu,
        "cortex-m4"
    );
    assert!(
        app.register_view
            .catalogue
            .as_ref()
            .unwrap()
            .register("scb.ccsidr")
            .is_none()
    );
    let mut current = sample(&app, "scb.cpuid", "0x411fc241");
    current.view = crate::registers::SampleView::PhysicalCore;
    let key = ("core:m4".into(), "scb.cpuid".into(), "m4".into());
    app.register_view
        .values
        .insert(key.clone(), current.clone());
    app.register_response(request.id, &json!({"samples":[late]}), None);
    assert_eq!(app.register_view.values[&key].value, current.value);
    assert_eq!(app.register_view.values[&key].context, current.context);
    assert_eq!(app.register_view.values.len(), 1);
    assert!(
        requests.try_recv().is_err(),
        "Switching models must not poll PPB"
    );
}
fn group_row(app: &App, id: &str) -> usize {
    app.register_view
        .rows
        .iter()
        .position(|row| {
            matches!(row, Row::Group(i, _)
        if app.register_view.catalogue.as_ref().unwrap().groups[*i].id == id)
        })
        .unwrap()
}
fn row_text(terminal: &Terminal<TestBackend>, y: u16) -> String {
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    let mut x = 0;
    while x < buffer.area.width {
        let symbol = buffer[(x, y)].symbol();
        text.push_str(symbol);
        x += unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
    }
    text
}
fn text(terminal: &Terminal<TestBackend>) -> String {
    (0..terminal.backend().buffer().area.height)
        .map(|y| row_text(terminal, y))
        .collect()
}
pub(super) fn key(app: &mut App, code: KeyCode, engine: &EngineHandle) {
    app.key(KeyEvent::new(code, KeyModifiers::NONE), Some(engine));
}

#[test]
fn register_columns_keep_size_access_and_value_hits_visible_for_long_names_and_128_bits() {
    for (id, bits, raw) in [
        ("r0", 32, "0x12345678"),
        ("d31", 64, "0xfedcba9876543210"),
        ("q15", 128, "0xfedcba98765432100123456789abcdef"),
    ] {
        for width in [35, 50, 80, 120] {
            let mut app = app();
            let index = index(&app, id);
            app.register_view.catalogue.as_mut().unwrap().registers[index].name =
                "客户寄存器 VeryLongCustomerRegisterNameBeyondTheAvailableWidth".into();
            app.register_view.rows = vec![Row::Register(index, 1)];
            app.selection = 0;
            let mut value = sample(&app, id, "0x0");
            value.value = Some(crate::registers::RawValue::parse(raw, bits).unwrap());
            app.register_view
                .values
                .insert(("core:default".into(), id.into(), "default".into()), value);
            let mut terminal = Terminal::new(TestBackend::new(width, 12)).unwrap();
            terminal.draw(|f| app.draw_registers(f, f.area())).unwrap();
            let y = app.view_rects[3].y;
            let rendered = row_text(&terminal, y);
            assert!(rendered.contains("客户"), "{rendered}");
            assert!(rendered.contains(" = 0x"), "{rendered}");
            assert!(rendered.contains('…'), "{rendered}");
            let hit = &app.formats.hits.last().unwrap().rect;
            assert_eq!(hit.y, y);
            assert_eq!(terminal.backend().buffer()[(hit.x, y)].symbol(), "0");
            if width >= 50 {
                let header = row_text(&terminal, 0);
                // These are terminal cell positions; Chinese continuation cells are excluded.
                let cells = terminal.backend().buffer();
                let size_x = (0..width).find(|x| cells[(*x, 0)].symbol() == "S").unwrap();
                let access_x = (size_x + 1..width)
                    .find(|x| cells[(*x, 0)].symbol() == "A")
                    .unwrap();
                assert!(header.contains("Size") && header.contains("Access"));
                assert_eq!(cells[(size_x, y)].symbol(), &bits.to_string()[..1]);
                assert_eq!(cells[(access_x, y)].symbol(), "R");
                assert!(hit.right() <= size_x);
            } else {
                assert!(text(&terminal).contains(&format!("{bits} bits RW")));
            }
            if width == 120 {
                assert!(rendered.contains(raw));
            }
        }
    }
}

#[test]
fn register_field_columns_and_details_use_the_field_width_and_access_override() {
    for width in [35, 80] {
        let mut app = app();
        let cpsr = index(&app, "cpsr");
        let register = &mut app.register_view.catalogue.as_mut().unwrap().registers[cpsr];
        let field = register.fields.iter().position(|f| f.name == "M").unwrap();
        register.fields[field].access = Some(crate::registers::Access::Ro);
        app.register_view.rows = vec![Row::Field(cpsr, field, 2)];
        let value = sample(&app, "cpsr", "0x00000013");
        app.register_view.values.insert(
            ("core:default".into(), "cpsr".into(), "default".into()),
            value,
        );
        let mut terminal = Terminal::new(TestBackend::new(width, 12)).unwrap();
        terminal.draw(|f| app.draw_registers(f, f.area())).unwrap();
        let y = app.view_rects[3].y;
        assert!(row_text(&terminal, y).contains("0x13"));
        assert!(text(&terminal).contains("5 bits RO"));
        assert!(!text(&terminal).contains("32 bits RW"));
        if width == 80 {
            assert!(row_text(&terminal, y).contains("5     RO"));
        }
    }
}

#[test]
fn register_frame_cache_never_resurrects_on_return_or_accepts_late_frame_responses() {
    let mut app = app();
    let (engine, requests) = engine();
    let r0 = index(&app, "r0");
    let r1 = index(&app, "r1");
    app.register_view.rows = vec![Row::Register(r0, 1), Row::Register(r1, 1)];
    let frame_value = sample(&app, "r0", "0x11111111");
    let mut physical = sample(&app, "r1", "0x22222222");
    physical.view = crate::registers::SampleView::PhysicalCore;
    app.register_view.values.insert(
        ("core:default".into(), "r0".into(), "default".into()),
        frame_value,
    );
    app.register_view.values.insert(
        ("core:default".into(), "r1".into(), "default".into()),
        physical,
    );
    assert!(!app.ensure_registers(Some(&engine)));
    let mut snapshot = app.snapshot.clone();
    snapshot.frame.level = 1;
    app.update(Event::Snapshot {
        snapshot: Box::new(snapshot),
    });
    assert_eq!(
        app.register_view.values[&("core:default".into(), "r0".into(), "default".into())].state,
        State::Stale
    );
    assert_eq!(
        app.register_view.values[&("core:default".into(), "r1".into(), "default".into())].state,
        State::Valid
    );
    assert!(app.ensure_registers(Some(&engine)));
    let request = requests.try_recv().unwrap();
    assert_eq!(request.params["ids"], json!(["r0"]));
    let mut late = sample(&app, "r0", "0xffffffff");
    let mut snapshot = app.snapshot.clone();
    snapshot.frame.level = 0;
    app.update(Event::Snapshot {
        snapshot: Box::new(snapshot),
    });
    assert!(app.register_response(request.id, &json!({"samples":[late]}), None));
    assert_eq!(
        app.register_view.values[&("core:default".into(), "r0".into(), "default".into())]
            .value
            .as_ref()
            .unwrap()
            .hex,
        "0x11111111"
    );
    assert_eq!(
        app.register_view.values[&("core:default".into(), "r0".into(), "default".into())].state,
        State::Stale
    );
    assert!(app.ensure_registers(Some(&engine)));
    let request = requests.try_recv().unwrap();
    assert_eq!(request.params["context"]["frame"], 0);
    late = sample(&app, "r0", "0x33333333");
    app.register_response(request.id, &json!({"samples":[late]}), None);
    assert!(!app.ensure_registers(Some(&engine)));
    assert!(requests.try_recv().is_err());
}

#[test]
fn register_change_highlight_compares_same_owner_and_field_bits_only_after_valid_reads() {
    let mut app = app();
    let (engine, requests) = engine();
    let cpsr = index(&app, "cpsr");
    let fields = &app.register_view.catalogue.as_ref().unwrap().registers[cpsr].fields;
    let n = fields.iter().position(|f| f.name == "N").unwrap();
    let c = fields.iter().position(|f| f.name == "C").unwrap();
    let mut terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
    for (step, raw, expected) in [
        (0, "0x00000013", theme::TEXT),
        (1, "0x80000013", theme::AMBER),
        (2, "0x80000013", theme::TEXT),
        (3, "0x00000013", theme::TEXT),
        (4, "0x80000013", theme::TEXT),
        (5, "0x00000013", theme::TEXT),
    ] {
        match step {
            1 => app.snapshot.generation += 1,
            3 => app.snapshot.register_session += 1,
            4 => app.snapshot.frame.level += 1,
            5 => {
                app.snapshot.core = Some(crate::session::CoreStatus {
                    index: 1,
                    name: "core1".into(),
                    endpoint: "localhost:3334".into(),
                    state: "STOPPED".into(),
                })
            }
            _ => {}
        }
        app.sync_register_preferences();
        app.register_view.rows = vec![
            Row::Register(cpsr, 1),
            Row::Field(cpsr, n, 2),
            Row::Field(cpsr, c, 2),
        ];
        app.selection = 0;
        app.view_tops[3] = 0;
        assert!(app.request_registers(Some(&engine), vec!["cpsr".into()], true));
        let request = requests.try_recv().unwrap();
        assert_eq!(request.method, "registers_read");
        let mut next = sample(&app, "cpsr", raw);
        next.owner = Some(format!("core:{}", app.register_context().core));
        assert!(app.register_response(request.id, &json!({"samples":[next]}), None));
        terminal.draw(|f| app.draw_registers(f, f.area())).unwrap();
        let buffer = terminal.backend().buffer();
        let y = app.view_rects[3].y;
        assert_eq!(buffer[(6, y)].fg, expected, "step {step}");
        assert_eq!(buffer[(6, y + 1)].fg, expected, "N at step {step}");
        assert_eq!(
            buffer[(6, y + 2)].fg,
            theme::TEXT,
            "unchanged C at step {step}"
        );
        assert!(requests.try_recv().is_err());
    }
    // A previous value remains accessible, but a failed/currently stale read never claims a change.
    app.snapshot.state = "RUNNING".into();
    terminal.draw(|f| app.draw_registers(f, f.area())).unwrap();
    assert_eq!(
        terminal.backend().buffer()[(6, app.view_rects[3].y)].fg,
        theme::MUTED
    );
    assert_eq!(
        app.register_view.values[&("core:default".into(), "cpsr".into(), "default".into())]
            .value
            .as_ref()
            .unwrap()
            .hex,
        "0x80000013"
    );
}

#[test]
fn register_tree_keyboard_mouse_scroll_and_description_filters_only_save_preferences() {
    let mut app = app();
    let (engine, requests) = engine();
    let mut terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
    app.selection = group_row(&app, "simd");
    key(&mut app, KeyCode::Right, &engine);
    assert!(app.register_view.open.contains("simd"));
    app.selection = group_row(&app, "quad");
    key(&mut app, KeyCode::Enter, &engine);
    assert!(app.register_view.open.contains("quad"));
    assert!(!app.register_view.open.contains("double"));
    assert!(!app.register_view.open.contains("single"));
    let q0 = index(&app, "q0");
    assert!(
        app.register_view
            .rows
            .iter()
            .any(|r| matches!(r, Row::Register(i, 2) if *i == q0))
    );
    terminal.draw(|f| app.draw_registers(f, f.area())).unwrap();
    let before = app.selection;
    key(&mut app, KeyCode::PageDown, &engine);
    assert!(app.selection > before && app.view_tops[3] > 0);
    app.mouse(
        MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 1,
            row: app.view_rects[3].y,
            modifiers: KeyModifiers::NONE,
        },
        Some(&engine),
    );
    assert!(app.view_tops[3] < app.selection);
    let quad = group_row(&app, "quad");
    app.view_tops[3] = quad;
    terminal.draw(|f| app.draw_registers(f, f.area())).unwrap();
    app.mouse(
        MouseEvent {
            kind: MouseEventKind::Down(event::MouseButton::Left),
            column: 2,
            row: app.view_rects[3].y,
            modifiers: KeyModifiers::NONE,
        },
        Some(&engine),
    );
    assert_eq!(app.selection, quad);
    assert!(!app.register_view.open.contains("quad"));
    assert!(
        !app.register_view
            .rows
            .iter()
            .any(|r| matches!(r, Row::Register(i, _) if *i == q0))
    );
    for group in &app.register_view.catalogue.as_ref().unwrap().groups {
        app.register_view.open.insert(group.id.clone());
    }
    for (filter, root) in [(1, "core"), (2, "simd"), (3, "system"), (0, "core")] {
        key(&mut app, KeyCode::Char('v'), &engine);
        assert_eq!(app.register_view.filter, filter);
        assert!(
            app.register_view
                .rows
                .iter()
                .any(|r| matches!(r, Row::Group(i, 0)
            if app.register_view.catalogue.as_ref().unwrap().groups[*i].id == root))
        );
        if filter != 0 {
            assert_eq!(
                app.register_view
                    .rows
                    .iter()
                    .filter(|r| matches!(r, Row::Group(_, 0)))
                    .count(),
                1
            );
        }
    }
    let r0 = index(&app, "r0");
    app.register_view.catalogue.as_mut().unwrap().registers[r0].description =
        "芯片定制的测量样本".into();
    key(&mut app, KeyCode::Char('s'), &engine);
    for ch in "测量样本".chars() {
        key(&mut app, KeyCode::Char(ch), &engine);
    }
    key(&mut app, KeyCode::Enter, &engine);
    assert_eq!(app.register_view.query, "测量样本");
    assert_eq!(
        app.register_view
            .rows
            .iter()
            .filter(|r| matches!(r, Row::Register(_, _)))
            .count(),
        1
    );
    assert!(
        app.register_view
            .rows
            .iter()
            .any(|r| matches!(r, Row::Register(i, _) if *i == r0))
    );
    assert!(
        requests
            .try_iter()
            .all(|request| request.method == "register_preferences")
    );
}
