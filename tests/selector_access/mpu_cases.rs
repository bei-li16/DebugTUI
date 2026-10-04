use super::*;
#[test]
fn deferred_mpu_driver_runs_actual_binary_and_preserves_both_cores_without_board_claims() {
    let mut fixture = fixture("");
    fixture.project.cores = (0..2)
        .map(|i| Core {
            name: format!("core{i}"),
            endpoint: format!("localhost:{}", 24900 + i),
            ..Default::default()
        })
        .collect();
    fixture.project.registers.targets.remove("default");
    for index in 0..2 {
        fixture
            .project
            .registers
            .targets
            .insert(format!("core{index}"), format!("cpu{index}"));
    }
    let directory = fixture.transcript.parent().unwrap();
    let project = directory.join("mpu-driver.toml");
    fs::write(&project, toml::to_string(&fixture.project).unwrap()).unwrap();
    let original = fs::read(&project).unwrap();
    let case = directory.join("mpu-driver.json");
    let mut spec: Value =
        serde_json::from_str(include_str!("../fixtures/mpu-regions-board.example.json")).unwrap();
    spec["frame_function"] = json!("main");
    fs::write(&case, serde_json::to_vec_pretty(&spec).unwrap()).unwrap();
    let output = Command::new("node")
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/test-mpu-regions-hardware.cjs"),
        )
        .args(["--run", "--software-fixture", "--core", "core0", "--binary"])
        .arg(env!("CARGO_BIN_EXE_debugtui"))
        .arg("--project")
        .arg(&project)
        .arg("--case")
        .arg(&case)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("\"passed\":5,\"failed\":0,\"skipped\":0"),
        "{stdout}"
    );
    assert_eq!(fs::read(&project).unwrap(), original);
    let state = fixture.state.lock().unwrap();
    assert_eq!(state["current"], "outside");
    assert!(selector_writes(&state).is_empty());
    for core in ["cpu0", "cpu1"] {
        for (selector, value) in [("prselr", 1), ("hprselr", 2), ("pmselr", 31)] {
            assert_eq!(state["targets"][core][selector], value);
        }
    }
}
fn read(engine: &session::EngineHandle, id: u64, context: &Value, bank: &str) -> Value {
    ok(
        engine,
        id,
        "registers_mpu",
        json!({"context":context,"bank":bank,"read":true}),
    )
}
fn clear_trace(fixture: &Fixture) {
    fixture.state.lock().unwrap()["trace"] = json!([]);
}
#[test]
fn direct_mpu_discards_a_complete_batch_when_actual_mode_changes_at_final_validation() {
    let mut fixture = fixture("");
    let values = fixture.transcript.with_extension("register-values.json");
    fixture.project.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTER_VALUES_FILE".into(),
        values.to_string_lossy().into_owned(),
    );
    let engine = session::spawn(fixture.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let context = probe(&engine, 2);
    let first = read(&engine, 4, &context, "el1");
    {
        let mut state = fixture.state.lock().unwrap();
        state["fault"] = json!("final_mode_change");
        state["register_values_file"] = json!(values);
    }
    let error = request(
        &engine,
        5,
        "registers_mpu",
        json!({"context":context,"bank":"el1","read":true}),
    )
    .unwrap_err();
    assert!(error.contains("samples discarded"));
    let status = ok(&engine, 6, "status", json!({}));
    assert!(status["register_probe"].is_null());
    let samples = status["register_samples"].as_array().unwrap();
    for prior in first["samples"].as_array().unwrap() {
        let stored = samples.iter().find(|s| s["id"] == prior["id"]).unwrap();
        assert_eq!(stored["state"], "stale");
        assert_eq!(stored["timestamp_ms"], prior["timestamp_ms"]);
        assert_eq!(stored["value"], prior["value"]);
    }
    assert!(selector_writes(&fixture.state.lock().unwrap()).is_empty());
    ok(&engine, 7, "quit", json!({}));
}
#[test]
fn direct_mpu_rejects_redirected_last_catalogue_entry_before_any_debugger_read() {
    let mut fixture = fixture("");
    let directory = fixture.transcript.parent().unwrap();
    let path = directory.join("redirected-mpu.toml");
    let mut catalogue = debugtui::registers::Catalogue::builtin("cortex-r52").unwrap();
    catalogue
        .registers
        .iter_mut()
        .find(|r| r.id == "prlar23")
        .unwrap()
        .read_side_effect = true;
    fs::write(&path, toml::to_string(&catalogue).unwrap()).unwrap();
    fixture.project.registers.catalogue = path;
    let engine = session::spawn(fixture.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let context = probe(&engine, 2);
    clear_trace(&fixture);
    let transcript = fs::read(&fixture.transcript).unwrap();
    let error = request(
        &engine,
        4,
        "registers_mpu",
        json!({"context":context,"bank":"el1","read":true}),
    )
    .unwrap_err();
    assert!(error.contains("prlar23"));
    assert!(
        fixture.state.lock().unwrap()["trace"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(fs::read(&fixture.transcript).unwrap(), transcript);
    ok(&engine, 5, "quit", json!({}));
}
#[test]
fn direct_mpu_absent_el2_bank_reads_no_regions_or_optional_mair() {
    let mut fixture = fixture("");
    let mut values: Value =
        serde_json::from_str(&fixture.project.gdb.env["DEBUGTUI_TEST_REGISTER_VALUES"]).unwrap();
    values["hmpuir"] = json!("0");
    fixture
        .project
        .gdb
        .env
        .insert("DEBUGTUI_TEST_REGISTER_VALUES".into(), values.to_string());
    let engine = session::spawn(fixture.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let context = probe(&engine, 2);
    fixture.state.lock().unwrap()["targets"]["cpu0"]["el2_count"] = json!(0);
    clear_trace(&fixture);
    let result = read(&engine, 4, &context, "el2");
    assert_eq!(result["view"]["count"], 0);
    assert!(result["view"]["regions"].as_array().unwrap().is_empty());
    assert!(
        result["view"]["mair"]
            .as_array()
            .unwrap()
            .iter()
            .all(Value::is_null)
    );
    let state = fixture.state.lock().unwrap();
    let reads: Vec<_> = state["trace"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e[1] == "mrc")
        .collect();
    assert_eq!(reads.len(), 2);
    assert!(selector_writes(&state).is_empty());
    drop(state);
    ok(&engine, 5, "quit", json!({}));
}
#[test]
fn direct_mpu_overview_reads_implemented_regions_and_current_mair_without_mcr() {
    let fixture = fixture("");
    let engine = session::spawn(fixture.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let context = probe(&engine, 2);
    for (bank, count, samples) in [("el1", 24, 54), ("el2", 20, 48)] {
        let response = read(&engine, 5, &context, bank);
        assert_eq!(response["samples"].as_array().unwrap().len(), samples);
        assert_eq!(response["view"]["regions"].as_array().unwrap().len(), count);
        assert_eq!(
            response["view"]["regions"][count - 1]["decoded"]["limit_inclusive"],
            if bank == "el1" {
                "0x2017ffff"
            } else {
                "0x2013ffff"
            }
        );
        assert_eq!(
            response["view"]["regions"][0]["attribute"]["memory"]["kind"],
            "normal"
        );
        assert_eq!(response["view"]["owner"], "core:default");
        assert_eq!(response["view"]["global_enabled"], false);
        let transcript = fs::read(&fixture.transcript).unwrap();
        clear_trace(&fixture);
        let cached = ok(
            &engine,
            6,
            "registers_mpu",
            json!({"context":context,"bank":bank}),
        );
        assert_eq!(cached["view"], response["view"]);
        assert_eq!(fs::read(&fixture.transcript).unwrap(), transcript);
        assert!(
            fixture.state.lock().unwrap()["trace"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    let state = fixture.state.lock().unwrap().clone();
    assert_eq!(state["current"], "outside");
    assert!(selector_writes(&state).is_empty());
    for (name, value) in [("prselr", 1), ("hprselr", 2), ("pmselr", 31)] {
        assert_eq!(state["targets"]["cpu0"][name], value);
    }
    assert!(
        !fs::read_to_string(&fixture.transcript)
            .unwrap()
            .contains("-data-write")
    );
    ok(&engine, 9, "quit", json!({}));
}
#[test]
fn direct_mpu_partial_failures_isolate_regions_and_keep_last_raw_values_unavailable() {
    let fixture = fixture("");
    let engine = session::spawn(fixture.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let context = probe(&engine, 2);
    read(&engine, 4, &context, "el1");
    fixture.state.lock().unwrap()["fault"] = json!("mair0_error");
    let mair = read(&engine, 5, &context, "el1");
    assert_eq!(mair["view"]["mair"][0]["state"], "unavailable");
    assert_eq!(mair["view"]["regions"][0]["attribute"]["raw"], "0xff");
    fixture.state.lock().unwrap()["fault"] = json!("region_error");
    let region = read(&engine, 6, &context, "el1");
    assert!(region["view"]["regions"][5]["decoded"].is_null());
    assert!(region["view"]["regions"][6]["decoded"].is_object());
    let cached = ok(
        &engine,
        7,
        "registers_mpu",
        json!({"context":context,"bank":"el1"}),
    );
    assert_eq!(cached["view"]["regions"][5]["base"]["state"], "unavailable");
    assert_eq!(
        cached["view"]["regions"][5]["base"]["value"]["hex"],
        "0x2005001f"
    );
    assert!(cached["view"]["regions"][5]["attribute"].is_null());
    assert_eq!(ok(&engine, 8, "status", json!({}))["state"], "STOPPED");
    assert!(selector_writes(&fixture.state.lock().unwrap()).is_empty());
    ok(&engine, 9, "quit", json!({}));
}
#[test]
fn direct_mpu_identity_count_mode_and_request_guards_stop_before_unsafe_indices() {
    for fault in ["identity_changed", "count_changed", "non_hyp"] {
        let mut fixture = fixture("");
        let values = fixture.transcript.with_extension("register-values.json");
        fixture.project.gdb.env.insert(
            "DEBUGTUI_TEST_REGISTER_VALUES_FILE".into(),
            values.to_string_lossy().into_owned(),
        );
        let engine = session::spawn(fixture.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let context = probe(&engine, 2);
        clear_trace(&fixture);
        let bank = if fault == "non_hyp" {
            fs::write(&values, "{\"cpsr\":\"0x13\"}").unwrap();
            "el2"
        } else {
            fixture.state.lock().unwrap()["fault"] = json!(fault);
            "el1"
        };
        assert!(
            request(
                &engine,
                4,
                "registers_mpu",
                json!({"context":context,"bank":bank,"read":true})
            )
            .is_err()
        );
        let state = fixture.state.lock().unwrap();
        let reads: Vec<_> = state["trace"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e[1] == "mrc")
            .collect();
        assert_eq!(
            reads.len(),
            if fault == "non_hyp" {
                0
            } else if fault == "identity_changed" {
                1
            } else {
                2
            }
        );
        assert!(selector_writes(&state).is_empty());
        drop(state);
        ok(&engine, 5, "quit", json!({}));
    }
    let fixture = fixture("");
    let engine = session::spawn(fixture.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let context = probe(&engine, 2);
    clear_trace(&fixture);
    let transcript = fs::read(&fixture.transcript).unwrap();
    let mut expired = context.clone();
    expired["generation"] = json!(900);
    for params in [
        json!({"context":expired,"bank":"el1","read":true}),
        json!({"context":context,"bank":"el1","read":true,"execute":"continue"}),
        json!({"context":context,"bank":"pmu","read":true}),
    ] {
        assert!(request(&engine, 4, "registers_mpu", params).is_err());
    }
    assert!(
        fixture.state.lock().unwrap()["trace"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(fs::read(&fixture.transcript).unwrap(), transcript);
    ok(&engine, 9, "quit", json!({}));
}
#[test]
fn multicore_scope_all_mpu_overview_reads_only_the_selected_physical_core() {
    let mut fixture = fixture("");
    fixture.project.cores = (0..2)
        .map(|i| Core {
            name: format!("core{i}"),
            endpoint: format!("localhost:{}", 24800 + i),
            ..Default::default()
        })
        .collect();
    fixture.project.registers.targets.remove("default");
    for index in 0..2 {
        fixture
            .project
            .registers
            .targets
            .insert(format!("core{index}"), format!("cpu{index}"));
    }
    let engine = debugtui::coordinator::spawn(fixture.project.clone());
    ok(&engine, 1, "connect", json!({}));
    ok(&engine, 2, "control_scope", json!({"scope":"all"}));
    ok(&engine, 3, "select_core", json!({"index":1}));
    let context = probe(&engine, 4);
    clear_trace(&fixture);
    let response = read(&engine, 6, &context, "el1");
    assert_eq!(response["view"]["owner"], "core:core1");
    assert!(
        response["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["context"]["core"] == "core1")
    );
    let state = fixture.state.lock().unwrap();
    assert!(
        state["trace"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e[1] == "mrc")
            .all(|e| e[0] == "cpu1")
    );
    assert!(selector_writes(&state).is_empty());
    assert_eq!(state["current"], "outside");
    drop(state);
    ok(&engine, 7, "quit", json!({}));
}
#[test]
fn direct_mpu_target_restore_failure_quarantines_the_service_without_retry() {
    let fixture = fixture("");
    let engine = session::spawn(fixture.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let context = probe(&engine, 2);
    fixture.state.lock().unwrap()["fault"] = json!("target_restore");
    clear_trace(&fixture);
    let error = request(
        &engine,
        4,
        "registers_mpu",
        json!({"context":context,"bank":"el1","read":true}),
    )
    .unwrap_err();
    assert!(error.contains("reconnect"));
    assert_eq!(ok(&engine, 5, "status", json!({}))["state"], "FAULT");
    let trace = fixture.state.lock().unwrap()["trace"].clone();
    assert!(
        request(
            &engine,
            6,
            "registers_mpu",
            json!({"context":context,"bank":"el1","read":true})
        )
        .is_err()
    );
    assert_eq!(fixture.state.lock().unwrap()["trace"], trace);
    ok(&engine, 7, "quit", json!({}));
}
