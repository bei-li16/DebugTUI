use super::*;

#[test]
fn deferred_bank_driver_checks_independent_baselines_peer_and_user_refusals() {
    for user in [false, true] {
        let mut f = configured(if user { "bank_unavailable" } else { "" });
        f.project.cores = (0..2)
            .map(|i| Core {
                name: format!("core{i}"),
                endpoint: format!("localhost:{}", 26700 + i),
                ..Default::default()
            })
            .collect();
        f.project.registers.targets = [
            ("core0".into(), "cpu0".into()),
            ("core1".into(), "cpu1".into()),
        ]
        .into();
        let mut spec: Value = serde_json::from_str(include_str!(
            "../fixtures/register-banked-board.example.json"
        ))
        .unwrap();
        spec["frame_function"] = json!("main");
        let mut names: Vec<Value> =
            serde_json::from_str(&f.project.gdb.env["DEBUGTUI_TEST_REGISTERS"]).unwrap();
        let mut values: Value =
            serde_json::from_str(&f.project.gdb.env["DEBUGTUI_TEST_REGISTER_VALUES"]).unwrap();
        for name in spec["stable_registers"].as_array().unwrap() {
            if !names.contains(name) {
                names.push(name.clone());
                values[name.as_str().unwrap()] = json!("0x11223344");
            }
        }
        if user {
            spec["expected_mode"] = json!("0x10");
            values["cpsr"] = json!("0x10");
            for bank in spec["banks"].as_array_mut().unwrap() {
                bank["unavailable"] = json!(true);
            }
        }
        f.project.gdb.env.insert(
            "DEBUGTUI_TEST_REGISTERS".into(),
            serde_json::to_string(&names).unwrap(),
        );
        f.project
            .gdb
            .env
            .insert("DEBUGTUI_TEST_REGISTER_VALUES".into(), values.to_string());
        let mut references = serde_json::Map::new();
        for index in 0..spec["banks"].as_array().unwrap().len() {
            references.insert(
                format!("((unsigned int *)&debugtui_banked_reference)[{index}]"),
                json!(format!("0x{:08x}", 0x51000000u32 + index as u32)),
            );
        }
        f.project.gdb.env.insert(
            "DEBUGTUI_TEST_EXPRESSION_VALUES".into(),
            Value::Object(references).to_string(),
        );
        let directory = f.transcript.parent().unwrap();
        let project = directory.join("banked-driver.toml");
        let case = directory.join("banked-driver.json");
        fs::write(&project, toml::to_string(&f.project).unwrap()).unwrap();
        let original = fs::read(&project).unwrap();
        fs::write(&case, serde_json::to_vec_pretty(&spec).unwrap()).unwrap();
        let output = Command::new("node")
            .arg(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("scripts/test-register-banked-hardware.cjs"),
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
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .contains("\"passed\":5,\"failed\":0,\"skipped\":0")
        );
        assert_eq!(fs::read(&project).unwrap(), original);
        let state = f.state.lock().unwrap();
        assert_eq!(state["current"], "outside");
        assert!(selector_writes(&state).is_empty());
        assert!(
            state["trace"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| row[1] == "banked")
                .all(|row| row[0] == "cpu0")
        );
    }
}

fn configured(fault: &'static str) -> Fixture {
    let mut f = fixture(fault);
    f.project.registers.banked_command = "aarch64 banked".into();
    let mut names: Vec<Value> =
        serde_json::from_str(&f.project.gdb.env["DEBUGTUI_TEST_REGISTERS"]).unwrap();
    names.push(json!("r0"));
    f.project.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTERS".into(),
        serde_json::to_string(&names).unwrap(),
    );
    let mut values: Value =
        serde_json::from_str(&f.project.gdb.env["DEBUGTUI_TEST_REGISTER_VALUES"]).unwrap();
    values["r0"] = json!("0x11223344");
    f.project
        .gdb
        .env
        .insert("DEBUGTUI_TEST_REGISTER_VALUES".into(), values.to_string());
    f
}

#[test]
fn all_builtin_banks_use_exact_words_preserve_target_and_avoid_legacy_get_reg() {
    let f = configured("");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let listed = ok(&engine, 2, "registers_list", json!({}));
    let ids: Vec<_> = listed["catalogue"]["registers"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|reg| reg["reader"]["kind"] == "banked")
        .map(|reg| reg["id"].clone())
        .collect();
    assert_eq!(ids.len(), 23);
    let result = ok(
        &engine,
        3,
        "registers_read",
        json!({"context":listed["context"],"ids":ids}),
    );
    for (index, sample) in result["samples"].as_array().unwrap().iter().enumerate() {
        assert_eq!(sample["state"], "valid", "{sample}");
        assert_eq!(
            sample["value"]["hex"],
            format!("0x{:08x}", 0x51000000u32 + index as u32)
        );
        assert_eq!(
            sample["source"],
            format!("openocd:aarch64 banked:{}", sample["id"].as_str().unwrap())
        );
    }
    let state = f.state.lock().unwrap().clone();
    assert_eq!(state["current"], "outside");
    assert!(selector_writes(&state).is_empty());
    assert_eq!(
        state["trace"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row[1] == "banked")
            .count(),
        23
    );
    assert!(
        !fs::read_to_string(&f.transcript)
            .unwrap()
            .contains("get_reg")
    );
    ok(&engine, 4, "quit", json!({}));
}

#[test]
fn missing_or_mismatched_bank_protocol_never_falls_back_to_legacy_access() {
    for missing in [true, false] {
        let mut f = configured(if missing { "" } else { "bank_protocol" });
        if missing {
            f.project.registers.banked_command.clear();
        }
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let result = ok(&engine, 2, "registers_read", json!({"ids":["sp_irq"]}));
        assert_eq!(result["samples"][0]["reason"], "reader_unsupported");
        assert!(
            !f.state.lock().unwrap()["trace"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| row[1] == "banked"))
        );
        assert!(
            !fs::read_to_string(&f.transcript)
                .unwrap()
                .contains("get_reg")
        );
        ok(&engine, 3, "quit", json!({}));
    }
}

#[test]
fn safe_bank_refusals_keep_core_register_reads_and_debug_control_available() {
    for (fault, reason) in [
        ("bank_unavailable", "access_restricted"),
        ("bank_identity", "reader_unsupported"),
        ("bank_short", "reader_unsupported"),
    ] {
        let f = configured(fault);
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let result = ok(&engine, 2, "registers_read", json!({"ids":["sp_irq","r0"]}));
        assert_eq!(result["samples"][0]["reason"], reason, "{result}");
        assert_eq!(result["samples"][1]["state"], "valid");
        assert_eq!(ok(&engine, 3, "status", json!({}))["state"], "STOPPED");
        ok(&engine, 4, "step", json!({}));
        ok(&engine, 5, "quit", json!({}));
    }
}

#[test]
fn uncertain_bank_transfer_stops_shared_channel_and_never_retries() {
    let f = configured("bank_fault");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let result = ok(
        &engine,
        2,
        "registers_read",
        json!({"ids":["sp_irq","lr_irq"]}),
    );
    assert_eq!(result["samples"][0]["state"], "error");
    assert_eq!(ok(&engine, 3, "status", json!({}))["state"], "FAULT");
    let state = f.state.lock().unwrap().clone();
    assert_eq!(
        state["trace"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row[1] == "banked")
            .count(),
        1
    );
    assert!(request(&engine, 4, "continue", json!({})).is_err());
    assert!(request(&engine, 5, "registers_read", json!({"ids":["sp_irq"]})).is_err());
    assert_eq!(*f.state.lock().unwrap(), state);
    ok(&engine, 6, "quit", json!({}));
}

#[test]
fn banked_scope_all_reads_only_selected_core_and_rejects_old_context() {
    let mut f = configured("");
    f.project.cores = (0..2)
        .map(|i| Core {
            name: format!("core{i}"),
            endpoint: format!("localhost:{}", 28100 + i),
            ..Default::default()
        })
        .collect();
    f.project.registers.targets = [
        ("core0".into(), "cpu0".into()),
        ("core1".into(), "cpu1".into()),
    ]
    .into();
    let engine = debugtui::coordinator::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    ok(&engine, 2, "control_scope", json!({"scope":"all"}));
    ok(&engine, 3, "select_core", json!({"index":1}));
    let listed = ok(&engine, 4, "registers_list", json!({}));
    let result = ok(
        &engine,
        5,
        "registers_read",
        json!({"context":listed["context"],"ids":["sp_irq","sp_hyp"]}),
    );
    for sample in result["samples"].as_array().unwrap() {
        assert_eq!(sample["state"], "valid");
        assert_eq!(sample["owner"], "core:core1");
        assert!(sample["value"]["hex"].as_str().unwrap().starts_with("0x61"));
    }
    let state = f.state.lock().unwrap().clone();
    assert!(
        state["trace"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row[1] == "banked")
            .all(|row| row[0] == "cpu1")
    );
    ok(&engine, 6, "select_core", json!({"index":0}));
    assert!(
        request(
            &engine,
            7,
            "registers_read",
            json!({"context":listed["context"],"ids":["sp_irq"]})
        )
        .unwrap_err()
        .contains("expired")
    );
    assert_eq!(*f.state.lock().unwrap(), state);
    ok(&engine, 8, "quit", json!({}));
}

#[test]
fn nonphysical_gdb_frame_is_rejected_before_any_bank_protocol_or_instruction() {
    let mut f = configured("");
    f.project
        .gdb
        .env
        .insert("DEBUGTUI_TEST_CAPABILITY_FRAME_CHANGE".into(), "1".into());
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let error = request(&engine, 2, "registers_read", json!({"ids":["r0"]})).unwrap_err();
    assert!(error.contains("thread/frame changed"), "{error}");
    let snapshot = ok(&engine, 20, "status", json!({}));
    assert!(
        snapshot["register_samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|sample| sample["id"] != "r0" || sample["state"] != "valid")
    );
    let result = ok(&engine, 3, "registers_read", json!({"ids":["sp_irq"]}));
    assert_eq!(
        result["samples"][0]["reason"], "access_restricted",
        "{result}"
    );
    assert!(
        f.state.lock().unwrap()["trace"]
            .as_array()
            .is_none_or(|rows| rows
                .iter()
                .all(|row| row[1] != "banked" && row[1] != "debugtui_banked_protocol"))
    );
    ok(&engine, 4, "quit", json!({}));
}

#[test]
fn physical_thread_and_frame_change_discards_bank_value_and_invalidates_probe() {
    for fault in ["bank_context_change", "bank_refusal_context_change"] {
        let mut f = configured(fault);
        let path = f.transcript.parent().unwrap().join("physical-context.json");
        fs::write(&path, r#"{"thread":"1","frame":0}"#).unwrap();
        f.project.gdb.env.insert(
            "DEBUGTUI_TEST_CONTEXT_FILE".into(),
            path.to_string_lossy().into_owned(),
        );
        f.state.lock().unwrap()["context_file"] = json!(path);
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let context = probe(&engine, 2);
        let result = ok(
            &engine,
            3,
            "registers_read",
            json!({"context":context,"ids":["sp_irq"]}),
        );
        assert_eq!(result["samples"][0]["reason"], "access_restricted");
        assert!(result["samples"][0]["value"].is_null());
        assert!(
            result["samples"][0]["detail"]
                .as_str()
                .unwrap()
                .contains("discarded")
        );
        assert!(ok(&engine, 4, "registers_list", json!({}))["probe"].is_null());
        assert_eq!(f.state.lock().unwrap()["current"], "outside");
        ok(&engine, 5, "quit", json!({}));
    }
}
