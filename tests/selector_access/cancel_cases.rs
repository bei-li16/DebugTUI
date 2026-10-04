use super::*;

#[test]
fn cancelled_selector_finishes_restoration_and_does_not_publish_a_new_sample() {
    for fault in ["", "restore_mismatch"] {
        let fixture = fixture(fault);
        let engine = session::spawn(fixture.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let context = probe(&engine, 2);
        let before = ok(&engine, 3, "status", json!({}));
        let read = Request::new(
            4,
            "registers_select",
            json!({"context":context,"kind":"mpu_el2","index":19}),
        );
        *fixture.cancel_on_transaction.lock().unwrap() = Some(read.clone());
        engine.send(read).unwrap();
        let error = loop {
            if let Event::Response {
                id: 4, ok, error, ..
            } = engine.events.recv_timeout(Duration::from_secs(15)).unwrap()
            {
                assert!(!ok);
                break error.unwrap();
            }
        };
        let state = fixture.state.lock().unwrap().clone();
        assert_eq!(
            selector_writes(&state).len(),
            2,
            "selected and restored: {state}"
        );
        let after = ok(&engine, 5, "status", json!({}));
        if fault.is_empty() {
            assert!(error.contains("cancelled"), "{error}");
            assert_eq!(state["targets"]["cpu0"]["hprselr"], 2);
            assert_eq!(after["state"], "STOPPED");
            assert_eq!(after["register_samples"], before["register_samples"]);
            assert_eq!(after["register_probe"], before["register_probe"]);
            assert!(!engine.cancellation.load(Ordering::Relaxed));
            // Cancellation is confined to the old request.
            ok(
                &engine,
                6,
                "registers_select",
                json!({"context":context,"kind":"mpu_el2","index":18}),
            );
        } else {
            assert!(error.contains("outcome unknown"), "{error}");
            assert_eq!(after["state"], "FAULT");
            assert!(request(&engine, 6, "registers_read", json!({"ids":["r0"]})).is_err());
        }
        ok(&engine, 7, "quit", json!({}));
    }
}

#[test]
fn multicore_scope_all_propagates_read_cancellation_only_to_the_selected_worker() {
    let mut fixture = fixture("");
    fixture.project.cores = (0..2)
        .map(|i| Core {
            name: format!("core{i}"),
            endpoint: format!("localhost:{}", 29000 + i),
            ..Default::default()
        })
        .collect();
    fixture.project.registers.targets.remove("default");
    for i in 0..2 {
        fixture
            .project
            .registers
            .targets
            .insert(format!("core{i}"), format!("cpu{i}"));
    }
    let engine = debugtui::coordinator::spawn(fixture.project.clone());
    ok(&engine, 1, "connect", json!({}));
    ok(&engine, 2, "control_scope", json!({"scope":"all"}));
    ok(&engine, 3, "select_core", json!({"index":1}));
    let context = probe(&engine, 4);
    let read = Request::new(
        5,
        "registers_select",
        json!({"context":context,"kind":"mpu_el2","index":19}),
    );
    *fixture.cancel_on_transaction.lock().unwrap() = Some(read.clone());
    engine.send(read).unwrap();
    loop {
        if let Event::Response {
            id: 5, ok, error, ..
        } = engine.events.recv_timeout(Duration::from_secs(15)).unwrap()
        {
            assert!(!ok);
            assert!(error.unwrap().contains("cancelled"));
            break;
        }
    }
    let state = fixture.state.lock().unwrap().clone();
    assert_eq!(state["targets"]["cpu0"]["hprselr"], 2);
    assert_eq!(state["targets"]["cpu1"]["hprselr"], 2);
    assert!(
        selector_writes(&state)
            .iter()
            .all(|write| write[0] == "cpu1")
    );
    assert_eq!(selector_writes(&state).len(), 2);
    let status = ok(&engine, 6, "status", json!({}));
    assert!(
        status["cores"]
            .as_array()
            .unwrap()
            .iter()
            .all(|core| core["state"] == "STOPPED")
    );
    assert!(!engine.cancellation.load(Ordering::Relaxed));
    ok(&engine, 7, "select_core", json!({"index":0}));
    let other = probe(&engine, 8);
    ok(
        &engine,
        9,
        "registers_select",
        json!({"context":other,"kind":"mpu_el2","index":18}),
    );
    ok(&engine, 10, "quit", json!({}));
}

#[test]
fn cancelled_direct_mpu_discards_the_batch_keeps_previous_probe_and_leaves_selectors_unchanged() {
    let fixture = fixture("");
    let engine = session::spawn(fixture.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let context = probe(&engine, 2);
    let before = ok(&engine, 3, "status", json!({}));
    let read = Request::new(
        4,
        "registers_mpu",
        json!({"context":context,"bank":"el2","read":true}),
    );
    *fixture.cancel_on_transaction.lock().unwrap() = Some(read.clone());
    engine.send(read).unwrap();
    loop {
        if let Event::Response {
            id: 4, ok, error, ..
        } = engine.events.recv_timeout(Duration::from_secs(15)).unwrap()
        {
            assert!(!ok);
            assert!(error.unwrap().contains("cancelled"));
            break;
        }
    }
    let after = ok(&engine, 5, "status", json!({}));
    assert_eq!(after["register_samples"], before["register_samples"]);
    assert_eq!(after["register_probe"], before["register_probe"]);
    assert_eq!(after["state"], "STOPPED");
    let state = fixture.state.lock().unwrap().clone();
    assert_eq!(state["current"], "outside");
    assert_eq!(state["targets"]["cpu0"]["hprselr"], 2);
    assert_eq!(selector_writes(&state).len(), 0);
    assert_eq!(
        state["trace"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r[1] == "mrc")
            .count(),
        1
    );
    ok(&engine, 6, "quit", json!({}));
}

#[test]
fn cancelled_vfp_pair_completes_the_adapter_transaction_then_discards_all_aliases() {
    let mut fixture = fixture("");
    fixture.project.registers.vfp_command = "aarch64 vfp".into();
    let engine = session::spawn(fixture.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let before = ok(&engine, 2, "status", json!({}));
    let read = Request::new(
        3,
        "registers_read",
        json!({"ids":["d0","d1","q0"],"manual":true}),
    );
    *fixture.cancel_on_transaction.lock().unwrap() = Some(read.clone());
    engine.send(read).unwrap();
    loop {
        if let Event::Response {
            id: 3, ok, error, ..
        } = engine.events.recv_timeout(Duration::from_secs(15)).unwrap()
        {
            assert!(!ok);
            assert!(error.unwrap().contains("cancelled"));
            break;
        }
    }
    let after = ok(&engine, 4, "status", json!({}));
    assert_eq!(after["register_samples"], before["register_samples"]);
    assert_eq!(after["state"], "STOPPED");
    let state = fixture.state.lock().unwrap().clone();
    assert_eq!(state["current"], "outside");
    assert_eq!(
        state["trace"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r[1] == "vfp")
            .count(),
        1
    );
    assert!(!state["trace"].to_string().contains("resume"));
    assert!(!engine.cancellation.load(Ordering::Relaxed));
    ok(&engine, 5, "quit", json!({}));
}
