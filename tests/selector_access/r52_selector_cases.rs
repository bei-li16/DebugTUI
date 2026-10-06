use super::*;

fn configured() -> Fixture {
    let mut f = super::r52_core_cases::configured("");
    f.project.registers.selector_command = debugtui::registers::selector::NATIVE_COMMAND.into();
    f
}
fn select(
    engine: &session::EngineHandle,
    context: &Value,
    kind: &str,
    index: u8,
) -> Result<Value, String> {
    request(
        engine,
        30,
        "registers_select",
        json!({"context":context,"kind":kind,"index":index}),
    )
}
fn selects(state: &Value) -> Vec<&Value> {
    state["trace"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r[1] == "r52_select")
        .collect()
}
fn no_fallback(state: &Value) {
    assert!(!state["trace"].to_string().contains("mrc"));
    assert!(!state["trace"].to_string().contains("mcr"));
    assert_eq!(state["current"], "outside");
}

#[test]
fn r52_native_selector_deferred_driver_checks_actual_exe_and_independent_pair() {
    let mut f = configured();
    f.project.cores = (0..2)
        .map(|i| Core {
            name: format!("core{i}"),
            endpoint: format!("localhost:{}", 28500 + i),
            ..Default::default()
        })
        .collect();
    f.project.registers.targets = [
        ("core0".into(), "cpu0".into()),
        ("core1".into(), "cpu1".into()),
    ]
    .into();
    let dir = f.transcript.parent().unwrap();
    let project = dir.join("native-selector-driver.toml");
    let case = dir.join("native-selector-driver.json");
    fs::write(&project, toml::to_string(&f.project).unwrap()).unwrap();
    let original = fs::read(&project).unwrap();
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts/test-register-selectors-hardware.cjs");
    let default = Command::new("node").arg(&script).output().unwrap();
    assert!(default.status.success());
    assert!(
        String::from_utf8_lossy(&default.stdout)
            .contains("\"passed\":0,\"failed\":0,\"skipped\":4")
    );
    assert_eq!(*f.state.lock().unwrap(), json!({}));
    for negative in [false, true] {
        let mut spec: Value = serde_json::from_str(include_str!(
            "../fixtures/r52-native-selector-board.example.json"
        ))
        .unwrap();
        spec["current_debug"]["endpoint"] = json!(f.project.registers.tcl_endpoint);
        spec["frame_function"] = json!("main");
        if negative {
            spec["requests"][0]["expected"][0] = json!("0x2017001a");
        }
        fs::write(&case, serde_json::to_vec_pretty(&spec).unwrap()).unwrap();
        let output = Command::new("node")
            .arg(&script)
            .args([
                "--run",
                "--software-fixture",
                "--core",
                "core0",
                "--binary",
                env!("CARGO_BIN_EXE_debugtui"),
                "--project",
            ])
            .arg(&project)
            .arg("--case")
            .arg(&case)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&output.stdout);
        assert_eq!(
            output.status.success(),
            !negative,
            "{text}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains(if negative {
                "\"passed\":3,\"failed\":1,\"skipped\":1"
            } else {
                "\"passed\":5,\"failed\":0,\"skipped\":0"
            }),
            "{text}"
        );
        assert_eq!(fs::read(&project).unwrap(), original);
    }
}

#[test]
fn r52_native_selector_current_el2_saved_user_both_banks_and_multicore_isolation() {
    let mut f = configured();
    f.project.cores = (0..2)
        .map(|i| Core {
            name: format!("core{i}"),
            endpoint: format!("localhost:{}", 28400 + i),
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
    let context = probe(&engine, 4);
    f.state.lock().unwrap()["trace"] = json!([]);
    for (kind, index, selector, original, count, base) in [
        ("mpu_el1", 23, "prselr", 1, 24, "0x30170000"),
        ("mpu_el2", 19, "hprselr", 2, 20, "0x30130000"),
    ] {
        let result = select(&engine, &context, kind, index).unwrap();
        assert_eq!(result["count"], count);
        assert_eq!(result["target"], "cpu1");
        assert_eq!(result["region"]["base"], base);
        assert_eq!(
            result["evidence"]["original"],
            result["evidence"]["restored"]
        );
        assert!(
            result["evidence"]["synchronization"]
                .as_str()
                .unwrap()
                .contains("native R52")
        );
        for sample in result["samples"].as_array().unwrap() {
            assert_eq!(sample["state"], "valid");
            assert_eq!(sample["owner"], "core:core1");
            let access = &sample["provenance"]["access"];
            assert_eq!(access["context"], context);
            assert_eq!(access["route"]["target"], "cpu1");
            assert_eq!(access["r52_core"]["dscr"]["hex"], "0x01050213");
            assert_eq!(access["r52_core"]["dspsr"]["hex"], "0xa2000410");
        }
        let state = f.state.lock().unwrap().clone();
        assert_eq!(state["targets"]["cpu1"][selector], original);
        assert!(selects(&state).iter().all(|r| r[0] == "cpu1"));
        no_fallback(&state);
    }
    assert_eq!(selects(&f.state.lock().unwrap()).len(), 2);
    ok(&engine, 5, "select_core", json!({"index":0}));
    assert!(select(&engine, &context, "mpu_el1", 23).is_err());
    let peer = probe(&engine, 6);
    f.state.lock().unwrap()["trace"] = json!([]);
    let result = select(&engine, &peer, "mpu_el1", 23).unwrap();
    assert_eq!(result["region"]["base"], "0x20170000");
    assert_eq!(selects(&f.state.lock().unwrap())[0][0], "cpu0");
    ok(&engine, 9, "quit", json!({}));
}

#[test]
fn r52_native_selector_capacity_permission_and_invalid_original_refuse_without_writes() {
    for (field, value, kind, error) in [
        ("el2_count", json!(0), "mpu_el2", "HardwareNotImplemented"),
        ("el2_count", json!(16), "mpu_el2", "capacity-changed"),
        ("hprselr", json!(20), "mpu_el2", "selector-invalid"),
        (
            "r52_dscr",
            json!("0x01050113"),
            "mpu_el2",
            "AccessRestricted",
        ),
        (
            "r52_dscr",
            json!("0x01058213"),
            "mpu_el1",
            "AccessRestricted",
        ),
        ("r52_dscr", json!("0x01050113"), "mpu_el1", "Unknown"),
    ] {
        let f = configured();
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let context = probe(&engine, 2);
        f.state.lock().unwrap()["trace"] = json!([]);
        f.state.lock().unwrap()["targets"]["cpu0"][field] = value;
        let failure = select(&engine, &context, kind, 0).unwrap_err();
        assert!(failure.contains(error), "{failure}");
        let state = f.state.lock().unwrap().clone();
        assert_eq!(selects(&state).len(), 1);
        assert!(state["targets"]["cpu0"]["native_selector_writes"].is_null());
        no_fallback(&state);
        assert_eq!(ok(&engine, 3, "status", json!({}))["state"], "STOPPED");
        ok(&engine, 4, "quit", json!({}));
    }
}

#[test]
fn r52_native_selector_protocol_refusal_preserves_old_origin_and_uncertain_receipt_quarantines() {
    for fault in [
        "r52_selector_protocol",
        "r52_selector_forged_restore",
        "r52_selector_fault",
        "r52_selector_bad_context_restore",
    ] {
        let mut f = configured();
        if fault == "r52_selector_bad_context_restore" {
            let path = f.transcript.parent().unwrap().join("physical-context.json");
            fs::write(&path, r#"{"thread":"1","frame":0}"#).unwrap();
            f.project.gdb.env.insert(
                "DEBUGTUI_TEST_CONTEXT_FILE".into(),
                path.to_string_lossy().into_owned(),
            );
            f.state.lock().unwrap()["context_file"] = json!(path);
        }
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let context = probe(&engine, 2);
        let first = select(&engine, &context, "mpu_el2", 19).unwrap();
        f.state.lock().unwrap()["trace"] = json!([]);
        f.state.lock().unwrap()["fault"] = json!(fault);
        assert!(select(&engine, &context, "mpu_el2", 19).is_err());
        let status = ok(&engine, 3, "status", json!({}));
        assert_eq!(
            status["state"],
            if fault == "r52_selector_protocol" {
                "STOPPED"
            } else {
                "FAULT"
            }
        );
        if fault == "r52_selector_protocol" {
            for old in first["samples"].as_array().unwrap() {
                let stored = status["register_samples"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|s| s["id"] == old["id"])
                    .unwrap();
                assert_eq!(
                    stored["last_value_provenance"]["provenance"],
                    old["provenance"]
                );
            }
        } else {
            assert!(select(&engine, &context, "mpu_el2", 19).is_err());
            assert_eq!(selects(&f.state.lock().unwrap()).len(), 1);
        }
        no_fallback(&f.state.lock().unwrap());
        ok(&engine, 4, "quit", json!({}));
    }
}

#[test]
fn r52_native_selector_custom_catalogue_and_unadapted_kind_have_zero_target_io() {
    for strict in [false, true] {
        let mut f = configured();
        if strict {
            let mut catalogue = debugtui::registers::Catalogue::builtin("cortex-r52").unwrap();
            catalogue
                .registers
                .iter_mut()
                .find(|r| r.id == "hprlar19")
                .unwrap()
                .access_rule
                .min_el = Some(3);
            let path = f.transcript.parent().unwrap().join("strict-selector.toml");
            fs::write(&path, toml::to_string(&catalogue).unwrap()).unwrap();
            f.project.registers.catalogue = path;
        }
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let context = probe(&engine, 2);
        f.state.lock().unwrap()["trace"] = json!([]);
        let transcript = fs::read(&f.transcript).unwrap();
        let error = select(
            &engine,
            &context,
            if strict { "mpu_el2" } else { "pmu" },
            if strict { 19 } else { 0 },
        )
        .unwrap_err();
        assert!(
            error.contains(if strict {
                "NeedEl(3)"
            } else {
                "Native selector"
            }),
            "{error}"
        );
        assert_eq!(fs::read(&f.transcript).unwrap(), transcript);
        assert!(
            f.state.lock().unwrap()["trace"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        ok(&engine, 4, "quit", json!({}));
    }
}

#[test]
fn r52_native_selector_cancel_and_context_change_finish_restore_but_discard_samples() {
    for changed in [false, true] {
        let mut f = configured();
        if changed {
            let path = f.transcript.parent().unwrap().join("physical-context.json");
            fs::write(&path, r#"{"thread":"1","frame":0}"#).unwrap();
            f.project.gdb.env.insert(
                "DEBUGTUI_TEST_CONTEXT_FILE".into(),
                path.to_string_lossy().into_owned(),
            );
            f.state.lock().unwrap()["context_file"] = json!(path);
        }
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let context = probe(&engine, 2);
        let before = ok(&engine, 3, "status", json!({}));
        f.state.lock().unwrap()["trace"] = json!([]);
        if changed {
            f.state.lock().unwrap()["fault"] = json!("r52_selector_context_change");
        }
        let read = Request::new(
            30,
            "registers_select",
            json!({"context":context,"kind":"mpu_el2","index":19}),
        );
        if !changed {
            *f.cancel_on_transaction.lock().unwrap() = Some(read.clone());
        }
        engine.send(read).unwrap();
        loop {
            if let Event::Response {
                id: 30, ok, error, ..
            } = engine.events.recv_timeout(Duration::from_secs(15)).unwrap()
            {
                assert!(!ok);
                assert!(error.unwrap().contains(if changed {
                    "context changed"
                } else {
                    "cancelled"
                }));
                break;
            }
        }
        let state = f.state.lock().unwrap().clone();
        assert_eq!(state["targets"]["cpu0"]["hprselr"], 2);
        assert_eq!(state["targets"]["cpu0"]["native_selector_writes"], 2);
        assert_eq!(selects(&state).len(), 1);
        no_fallback(&state);
        let after = ok(&engine, 4, "status", json!({}));
        assert_eq!(after["state"], "STOPPED");
        if changed {
            for old in before["register_samples"].as_array().unwrap() {
                let retained = after["register_samples"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|s| s["id"] == old["id"])
                    .unwrap();
                for field in ["value", "provenance", "timestamp_ms", "context"] {
                    assert_eq!(retained[field], old[field], "{} {field}", old["id"]);
                }
            }
            assert!(
                after["register_samples"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|s| s["id"] == "hprbar19" || s["id"] == "hprlar19")
                    .all(|s| s["value"].is_null() && s["state"] != "valid")
            );
        } else {
            assert_eq!(after["register_samples"], before["register_samples"]);
        }
        ok(&engine, 5, "quit", json!({}));
    }
}
