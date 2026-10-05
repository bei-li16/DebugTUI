use super::*;

fn configured(fault: &'static str) -> Fixture {
    let mut f = fixture(fault);
    f.project.registers.timer_command = "aarch64 timer".into();
    f.project.registers.cp15_command.clear();
    f.project.registers.cp15_64_command.clear();
    f.project.registers.selector_command.clear();
    f.project.registers.facts.insert("timer.present".into(), 1);
    f
}
fn timer_reads(state: &Value) -> Vec<String> {
    state["trace"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row[1] == "timer")
        .map(|row| row[2].as_str().unwrap().into())
        .collect()
}
#[test]
fn timer_adapter_reads_all_native_widths_with_current_debug_evidence_and_single_owner() {
    let entries = [
        ("cntfrq", "0x05f5e100", 32),
        ("cntkctl", "0x000003ad", 32),
        ("cntp_tval", "0xfffffff0", 32),
        ("cntp_ctl", "0x00000007", 32),
        ("cntv_tval", "0x80000001", 32),
        ("cntv_ctl", "0x00000004", 32),
        ("cnthctl", "0x000000ab", 32),
        ("cnthp_tval", "0x7ffffffe", 32),
        ("cnthp_ctl", "0x00000001", 32),
        ("cntpct", "0x89abcdef01234567", 64),
        ("cntvct", "0x89abcdee01234567", 64),
        ("cntp_cval", "0x0123456789abcdef", 64),
        ("cntv_cval", "0xfedcba9876543210", 64),
        ("cntvoff", "0x0000000100000000", 64),
        ("cnthp_cval", "0x8000000000000001", 64),
    ];
    let mut f = configured("");
    f.project.cores = (0..2)
        .map(|i| Core {
            name: format!("core{i}"),
            endpoint: format!("localhost:{}", 27700 + i),
            ..Default::default()
        })
        .collect();
    f.project.registers.targets = [
        ("core0".into(), "cpu0".into()),
        ("core1".into(), "cpu1".into()),
    ]
    .into();
    let mut values = json!({});
    for (name, raw, _) in entries {
        values[name] = json!(raw);
    }
    *f.state.lock().unwrap() = json!({"targets":{"cpu1":{"timer_values":values}},"trace":[]});
    let engine = debugtui::coordinator::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    ok(&engine, 2, "control_scope", json!({"scope":"all"}));
    ok(&engine, 3, "select_core", json!({"index":1}));
    let context = ok(&engine, 4, "registers_list", json!({}))["context"].clone();
    let read = ok(
        &engine,
        5,
        "registers_read",
        json!({"context":context,"ids":entries.map(|e|e.0)}),
    );
    for (i, (name, raw, bits)) in entries.iter().enumerate() {
        let sample = &read["samples"][i];
        assert_eq!(sample["id"], *name);
        assert_eq!(sample["state"], "valid", "{sample}");
        assert_eq!(sample["value"], json!({"bits":bits,"hex":raw}));
        assert_eq!(sample["view"], "physical_core");
        assert_eq!(sample["owner"], "core:core1");
        assert_eq!(sample["source"], "openocd:aarch64 timer");
        let access = &sample["provenance"]["access"];
        assert_eq!(access["route"]["target"], "cpu1");
        assert_eq!(access["timer"]["dscr"]["hex"], "0x01000200");
        assert_eq!(access["timer"]["dspsr"]["hex"], "0xa2000410");
        assert_eq!(access["timer"]["dlr"]["hex"], "0x81234568");
        assert!(
            access["command"]
                .as_str()
                .unwrap()
                .contains(&format!("aarch64 timer \"{name}\""))
        );
    }
    let state = f.state.lock().unwrap().clone();
    assert_eq!(timer_reads(&state), entries.map(|e| e.0));
    assert_eq!(state["current"], "outside");
    assert!(
        state["trace"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r[1] == "timer")
            .all(|r| r[0] == "cpu1")
    );
    assert!(!state["trace"].to_string().contains("mrrc"));
    assert!(selector_writes(&state).is_empty());
    ok(&engine, 6, "select_core", json!({"index":0}));
    assert!(
        request(
            &engine,
            7,
            "registers_read",
            json!({"context":context,"ids":["cntpct"]})
        )
        .is_err()
    );
    assert_eq!(timer_reads(&f.state.lock().unwrap()).len(), 15);
    ok(&engine, 8, "quit", json!({}));
}
#[test]
fn timer_adapter_keeps_guest_permission_unknown_separate_from_retained_physical_evidence() {
    let f = configured("");
    f.state.lock().unwrap()["targets"] = json!({"cpu0":{"timer_dscr":"0x01010100",
        "timer_values":{"cntvct":"0xfedcba9876543210"},
        "timer_errors":{"cntpct":"access-unknown","cntvoff":"access-restricted"}}});
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let read = ok(
        &engine,
        2,
        "registers_read",
        json!({"ids":["cntvct","cntpct","cntvoff"]}),
    );
    assert_eq!(read["samples"][0]["state"], "valid");
    assert_eq!(
        read["samples"][0]["provenance"]["access"]["timer"]["dscr"]["hex"],
        "0x01010100"
    );
    assert_eq!(read["samples"][1]["reason"], "unknown");
    assert_eq!(read["samples"][2]["reason"], "access_restricted");
    assert!(
        read["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["implementation"] == "yes")
    );
    f.state.lock().unwrap()["targets"]["cpu0"]["timer_errors"]["cntvct"] = json!("access-unknown");
    let failed = ok(&engine, 3, "registers_read", json!({"ids":["cntvct"]}));
    assert!(failed["samples"][0]["value"].is_null());
    let status = ok(&engine, 4, "status", json!({}));
    let retained = status["register_samples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "cntvct")
        .unwrap();
    assert_eq!(retained["value"]["hex"], "0xfedcba9876543210");
    assert_eq!(
        retained["last_value_provenance"]["provenance"]["access"]["timer"]["dscr"]["hex"],
        "0x01010100"
    );
    assert_eq!(status["state"], "STOPPED");
    assert!(selector_writes(&f.state.lock().unwrap()).is_empty());
    ok(&engine, 5, "quit", json!({}));
}
#[test]
fn timer_adapter_rejects_protocol_identity_permissions_and_forged_or_truncated_evidence_without_fallback()
 {
    for (fault, reason) in [
        ("timer_protocol", "reader_unsupported"),
        ("timer_identity", "reader_unsupported"),
        ("timer_restricted", "access_restricted"),
        ("timer_unknown", "unknown"),
        ("timer_short", "reader_unsupported"),
        ("timer_forged_el", "reader_unsupported"),
    ] {
        let f = configured(fault);
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let read = ok(&engine, 2, "registers_read", json!({"ids":["cntpct"]}));
        assert_eq!(read["samples"][0]["reason"], reason, "{fault}: {read}");
        assert!(read["samples"][0]["value"].is_null());
        assert_eq!(read["samples"][0]["implementation"], "yes");
        assert_eq!(ok(&engine, 3, "status", json!({}))["state"], "STOPPED");
        assert_eq!(
            timer_reads(&f.state.lock().unwrap()).len(),
            usize::from(fault != "timer_protocol")
        );
        assert!(
            !f.state.lock().unwrap()["trace"]
                .to_string()
                .contains("mrrc")
        );
        ok(&engine, 4, "quit", json!({}));
    }
    let f = configured("timer_fault");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let read = ok(
        &engine,
        2,
        "registers_read",
        json!({"ids":["cntpct","cntvct"]}),
    );
    assert!(
        read["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["value"].is_null())
    );
    assert_eq!(ok(&engine, 3, "status", json!({}))["state"], "FAULT");
    assert_eq!(timer_reads(&f.state.lock().unwrap()).len(), 1);
    assert!(request(&engine, 4, "continue", json!({})).is_err());
    ok(&engine, 5, "quit", json!({}));
}
#[test]
fn cancelled_timer_transaction_restores_target_and_discards_samples_without_retry() {
    let f = configured("");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let before = ok(&engine, 2, "status", json!({}));
    let read = Request::new(3, "registers_read", json!({"ids":["cntpct","cntvct"]}));
    *f.cancel_on_transaction.lock().unwrap() = Some(read.clone());
    engine.send(read).unwrap();
    loop {
        if let Event::Response {
            id: 3, ok, error, ..
        } = engine.events.recv_timeout(Duration::from_secs(15)).unwrap()
        {
            assert!(!ok && error.unwrap().contains("cancelled"));
            break;
        }
    }
    let after = ok(&engine, 4, "status", json!({}));
    assert_eq!(after["register_samples"], before["register_samples"]);
    assert_eq!(after["state"], "STOPPED");
    let state = f.state.lock().unwrap().clone();
    assert_eq!(state["current"], "outside");
    assert_eq!(timer_reads(&state).len(), 1);
    ok(&engine, 5, "quit", json!({}));
}

#[test]
fn timer_context_change_discards_physical_value_and_stops_the_batch() {
    let mut f = configured("timer_context_change");
    let path = f.transcript.parent().unwrap().join("physical-context.json");
    fs::write(&path, r#"{"thread":"1","frame":0}"#).unwrap();
    f.project.gdb.env.insert(
        "DEBUGTUI_TEST_CONTEXT_FILE".into(),
        path.to_string_lossy().into_owned(),
    );
    f.state.lock().unwrap()["context_file"] = json!(path);
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let result = ok(
        &engine,
        2,
        "registers_read",
        json!({"ids":["cntpct","cntvct"]}),
    );
    assert_eq!(result["samples"][0]["reason"], "access_restricted");
    assert!(
        result["samples"][0]["detail"]
            .as_str()
            .unwrap()
            .contains("discarded")
    );
    assert!(
        result["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["value"].is_null())
    );
    assert_eq!(timer_reads(&f.state.lock().unwrap()).len(), 1);
    assert_eq!(f.state.lock().unwrap()["current"], "outside");
    assert!(ok(&engine, 3, "registers_list", json!({}))["probe"].is_null());
    ok(&engine, 4, "quit", json!({}));
}
