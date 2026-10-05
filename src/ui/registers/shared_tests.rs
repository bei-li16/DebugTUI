use super::framework_tests::key;
use super::tests::{app, engine, sample};
use super::*;
use crate::registers::{Reader, Scope};
use ratatui::{Terminal, backend::TestBackend};

fn status_evidence(app: &mut App, engine: &EngineHandle, expected: &[&str]) {
    app.selection = 0;
    for (width, height) in [(100, 24), (35, 12)] {
        key(app, KeyCode::Char('t'), engine);
        assert!(app.register_view.status_popup.is_some());
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut evidence = String::new();
        for _ in 0..64 {
            terminal.draw(|f| draw_status(f, app)).unwrap();
            let buffer = terminal.backend().buffer();
            for y in 0..height {
                for x in 0..width {
                    evidence.push_str(buffer[(x, y)].symbol());
                }
            }
            key(app, KeyCode::Down, engine);
        }
        let compact: String = evidence
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '│')
            .collect();
        for expected in expected {
            assert!(
                compact.contains(expected),
                "{width}x{height}: missing {expected}: {compact}"
            );
        }
        key(app, KeyCode::Esc, engine);
        assert!(app.register_view.status_popup.is_none());
    }
}

fn switch(app: &mut App, name: &str, index: usize, session: u64) {
    let mut snapshot = app.snapshot.clone();
    snapshot.core = Some(crate::session::CoreStatus {
        index,
        name: name.into(),
        endpoint: format!("localhost:{}", 5330 + index),
        state: "STOPPED".into(),
    });
    snapshot.register_session = session;
    app.update(Event::Snapshot {
        snapshot: Box::new(snapshot),
    });
    app.register_view.rows = (0..3).map(|i| Row::Register(i, 1)).collect();
}
#[test]
fn shared_ui_keeps_each_route_ages_only_changed_owners_and_rejects_late_owner_values() {
    let mut app = app();
    let (engine, requests) = engine();
    app.project.registers.topology.chip = "board".into();
    app.project.registers.topology.clusters = [
        ("default".into(), "A".into()),
        ("core1".into(), "A".into()),
        ("core2".into(), "B".into()),
    ]
    .into();
    let catalogue = app.register_view.catalogue.as_mut().unwrap();
    for (index, scope) in [(0, Scope::Cluster), (1, Scope::Chip), (2, Scope::Core)] {
        catalogue.registers[index].scope = scope;
        catalogue.registers[index].reader = Reader::Mmio {
            require_owner_mapping: false,
            component: "board".into(),
            offset: index as u64 * 4,
        };
    }
    app.snapshot.register_owner_generations = [
        ("cluster:A".into(), 4),
        ("cluster:B".into(), 6),
        ("chip:board".into(), 8),
    ]
    .into();
    for (name, index, session, raw) in [
        ("default", 0, 17, "0x11111111"),
        ("core1", 1, 18, "0x22222222"),
        ("core2", 2, 19, "0x33333333"),
    ] {
        switch(&mut app, name, index, session);
        assert!(app.request_registers(
            Some(&engine),
            vec!["r0".into(), "r1".into(), "r2".into()],
            true
        ));
        let request = requests.try_recv().unwrap();
        let mut values = vec![];
        for (id, scope) in [
            ("r0", Scope::Cluster),
            ("r1", Scope::Chip),
            ("r2", Scope::Core),
        ] {
            let mut value = sample(&app, id, raw);
            value.owner = app.project.registers.topology.owner(scope, name);
            value.view = crate::registers::SampleView::PhysicalCore;
            value.source = "mmio:board".into();
            value.owner_generation = value
                .owner
                .as_ref()
                .and_then(|owner| app.snapshot.register_owner_generations.get(owner))
                .copied();
            values.push(value);
        }
        app.register_response(request.id, &json!({"samples":values}), None);
    }
    assert_eq!(
        app.register_view.values.len(),
        9,
        "shared owner + id must not evict another worker route"
    );
    switch(&mut app, "default", 0, 17);
    for index in 0..3 {
        let value = app
            .register_view
            .sample(&app.project, &app.register_context(), index)
            .unwrap();
        assert_eq!(value.value.as_ref().unwrap().hex, "0x11111111");
        assert_eq!(value.context.core, "default");
        assert_eq!(value.state, State::Valid);
    }
    assert!(!app.ensure_registers(Some(&engine)));
    assert!(app.request_registers(Some(&engine), vec!["r2".into()], false));
    let failed_request = requests.try_recv().unwrap();
    let mut failed = sample(&app, "r2", "0x11111111");
    failed.state = State::Error;
    failed.reason = crate::registers::Reason::TransportError;
    failed.value = None;
    app.register_response(failed_request.id, &json!({"samples":[failed]}), None);
    let mut snapshot = app.snapshot.clone();
    snapshot
        .register_owner_generations
        .insert("cluster:A".into(), 5);
    snapshot
        .register_owner_generations
        .insert("chip:board".into(), 9);
    app.update(Event::Snapshot {
        snapshot: Box::new(snapshot),
    });
    for (index, state) in [(0, State::Stale), (1, State::Stale), (2, State::Error)] {
        assert_eq!(
            app.register_view
                .sample(&app.project, &app.register_context(), index)
                .unwrap()
                .state,
            state
        );
    }
    assert!(app.ensure_registers(Some(&engine)));
    let request = requests.try_recv().unwrap();
    assert_eq!(
        request.params["ids"],
        json!(["r0", "r1"]),
        "private failure backoff must survive unrelated owner changes"
    );
    let mut late = sample(&app, "r0", "0xdeadbeef");
    late.owner = Some("cluster:A".into());
    late.owner_generation = Some(5);
    late.view = crate::registers::SampleView::PhysicalCore;
    let mut snapshot = app.snapshot.clone();
    snapshot
        .register_owner_generations
        .insert("cluster:A".into(), 6);
    app.update(Event::Snapshot {
        snapshot: Box::new(snapshot),
    });
    app.register_response(request.id, &json!({"samples":[late]}), None);
    let preserved = app
        .register_view
        .sample(&app.project, &app.register_context(), 0)
        .unwrap();
    assert_eq!(preserved.value.as_ref().unwrap().hex, "0x11111111");
    assert_eq!(preserved.state, State::Stale);
    assert!(app.ensure_registers(Some(&engine)));
    let next = requests.try_recv().unwrap();
    assert_eq!(
        next.params["ids"],
        json!(["r0"]),
        "changed owner must be refreshable without retrying an unchanged failed owner"
    );
    status_evidence(
        &mut app,
        &engine,
        &[
            "Owner:cluster:A·Cluster",
            "Samplecore:default",
            "Ownerlifetime:4·current6",
            "Samplingview:physicalcorestate",
            "0x11111111",
            "mmio:board",
            "Stale",
        ],
    );
    switch(&mut app, "core3", 3, 20);
    status_evidence(&mut app, &engine, &["Owner:unknown·Cluster", "Notread"]);
    assert!(
        requests.try_recv().is_err(),
        "status and unknown-owner views must not read a target"
    );
}
