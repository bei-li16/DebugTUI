use super::r52_core_cases::{configured, native_reads};
use super::*;

fn mpu(engine: &session::EngineHandle, id: u64, context: &Value, bank: &str) -> Value {
    ok(
        engine,
        id,
        "registers_mpu",
        json!({"context":context,"bank":bank,"read":true}),
    )
}
fn clear_trace(f: &Fixture) {
    f.state.lock().unwrap()["trace"] = json!([]);
}
fn no_raw_fallback(state: &Value) {
    assert!(selector_writes(state).is_empty());
    assert!(!state["trace"].to_string().contains("mrc"));
    assert_eq!(state["current"], "outside");
}

#[test]
fn r52_native_mpu_deferred_driver_checks_actual_exe_and_rejects_independent_wrong_region() {
    let mut f = configured("");
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
    let directory = f.transcript.parent().unwrap();
    let project = directory.join("native-mpu-driver.toml");
    let case = directory.join("native-mpu-driver.json");
    fs::write(&project, toml::to_string(&f.project).unwrap()).unwrap();
    let original = fs::read(&project).unwrap();
    let script =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/test-mpu-regions-hardware.cjs");
    let default = Command::new("node").arg(&script).output().unwrap();
    assert!(default.status.success());
    assert!(
        String::from_utf8_lossy(&default.stdout)
            .contains("\"passed\":0,\"failed\":0,\"skipped\":4")
    );
    assert_eq!(*f.state.lock().unwrap(), json!({}));
    for negative in [false, true] {
        let mut spec: Value = serde_json::from_str(include_str!(
            "../fixtures/r52-native-mpu-board.example.json"
        ))
        .unwrap();
        spec["current_debug"]["endpoint"] = json!(f.project.registers.tcl_endpoint);
        if negative {
            spec["views"][0]["regions"][3][0] = json!("0x2003001a");
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
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(
            output.status.success(),
            !negative,
            "{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            stdout.contains(if negative {
                "\"passed\":3,\"failed\":1,\"skipped\":1"
            } else {
                "\"passed\":5,\"failed\":0,\"skipped\":0"
            }),
            "{stdout}"
        );
        assert_eq!(fs::read(&project).unwrap(), original);
    }
    no_raw_fallback(&f.state.lock().unwrap());
}

#[test]
fn r52_native_mpu_uses_current_el2_with_saved_user_and_isolates_selected_core_and_cache() {
    let mut f = configured("");
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
    clear_trace(&f);
    for (bank, count, samples, reads, capacity) in [
        ("el1", 24, 54, 55, "0x00001800"),
        ("el2", 20, 48, 49, "0x00000014"),
    ] {
        let result = mpu(&engine, 6, &context, bank);
        assert_eq!(result["view"]["count"], count);
        assert_eq!(result["samples"].as_array().unwrap().len(), samples);
        assert_eq!(result["view"]["owner"], "core:core1");
        assert_eq!(
            result["view"]["regions"][0]["decoded"]["base"],
            "0x30000000"
        );
        for sample in result["samples"].as_array().unwrap() {
            assert_eq!(sample["state"], "valid", "{sample}");
            assert_eq!(sample["owner"], "core:core1");
            let access = &sample["provenance"]["access"];
            assert_eq!(sample["provenance"]["acquisition"], "mpu_regions");
            assert_eq!(access["context"], context);
            if sample["id"] == "cpsr" {
                assert_eq!(sample["value"]["hex"], "0xa2000410");
                assert_eq!(access["route"]["kind"], "gdb_register");
                assert!(access["r52_core"].is_null());
            } else {
                assert_eq!(sample["source"], "openocd:aarch64 r52_read");
                assert_eq!(access["route"]["target"], "cpu1");
                assert_eq!(access["r52_core"]["dspsr"]["hex"], "0xa2000410");
                assert_eq!(access["r52_core"]["dscr"]["hex"], "0x01050213");
                if access["r52_core"]["bank"] != "none" {
                    assert_eq!(access["r52_core"]["capacity"]["hex"], capacity);
                }
            }
        }
        assert_eq!(native_reads(&f.state.lock().unwrap()).len(), reads);
        assert!(
            native_reads(&f.state.lock().unwrap())
                .iter()
                .all(|r| r[0] == "cpu1")
        );
        let transcript = fs::read(&f.transcript).unwrap();
        clear_trace(&f);
        let cached = ok(
            &engine,
            7,
            "registers_mpu",
            json!({"context":context,"bank":bank}),
        );
        assert_eq!(cached["view"], result["view"]);
        assert_eq!(fs::read(&f.transcript).unwrap(), transcript);
        assert!(
            f.state.lock().unwrap()["trace"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    ok(&engine, 8, "select_core", json!({"index":0}));
    assert!(
        request(
            &engine,
            9,
            "registers_mpu",
            json!({"context":context,"bank":"el1","read":true})
        )
        .is_err()
    );
    assert!(native_reads(&f.state.lock().unwrap()).is_empty());
    let peer = probe(&engine, 10);
    clear_trace(&f);
    let peer_result = mpu(&engine, 12, &peer, "el1");
    assert_eq!(
        peer_result["view"]["regions"][0]["decoded"]["base"],
        "0x20000000"
    );
    assert!(
        native_reads(&f.state.lock().unwrap())
            .iter()
            .all(|r| r[0] == "cpu0")
    );
    no_raw_fallback(&f.state.lock().unwrap());
    ok(&engine, 13, "quit", json!({}));
}

#[test]
fn r52_native_mpu_rechecks_capacity_before_indices_and_preserves_zero_el2_bank() {
    for zero in [false, true] {
        let f = configured("");
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        if zero {
            f.state.lock().unwrap()["targets"]["cpu0"]["el2_count"] = json!(0);
        }
        let context = probe(&engine, 2);
        clear_trace(&f);
        if zero {
            let result = mpu(&engine, 4, &context, "el2");
            assert_eq!(result["view"]["count"], 0);
            assert!(result["view"]["regions"].as_array().unwrap().is_empty());
            assert!(
                result["view"]["mair"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(Value::is_null)
            );
            assert_eq!(
                native_reads(&f.state.lock().unwrap())
                    .iter()
                    .map(|r| r[2].as_str().unwrap())
                    .collect::<Vec<_>>(),
                ["midr", "hmpuir", "midr", "hmpuir"]
            );
        } else {
            f.state.lock().unwrap()["targets"]["cpu0"]["el1_count"] = json!(16);
            let error = request(
                &engine,
                4,
                "registers_mpu",
                json!({"context":context,"bank":"el1","read":true}),
            )
            .unwrap_err();
            assert!(error.contains("count changed"), "{error}");
            assert_eq!(
                native_reads(&f.state.lock().unwrap())
                    .iter()
                    .map(|r| r[2].as_str().unwrap())
                    .collect::<Vec<_>>(),
                ["midr", "mpuir"]
            );
            assert!(ok(&engine, 5, "status", json!({}))["register_probe"].is_null());
        }
        no_raw_fallback(&f.state.lock().unwrap());
        ok(&engine, 6, "quit", json!({}));
    }
}

#[test]
fn r52_native_mpu_rejects_between_transaction_proof_or_capacity_drift_before_publication() {
    for (trigger, mutation, allowed, error_part) in [
        (
            "prbar2",
            json!({"r52_dspsr":"0xa3000410"}),
            false,
            "Debug evidence changed",
        ),
        (
            "prbar2",
            json!({"r52_dlr":"0x8123456c"}),
            false,
            "Debug evidence changed",
        ),
        (
            "prbar2",
            json!({"r52_midr":"0x411fd135"}),
            false,
            "Debug evidence changed",
        ),
        (
            "prbar2",
            json!({"el1_count":20}),
            false,
            "bank capacity changed",
        ),
        (
            "prlar23",
            json!({"el1_count":20}),
            false,
            "final validation",
        ),
        ("prbar2", json!({"r52_dscr":"0x21050213"}), true, ""),
    ] {
        let f = configured("");
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let context = probe(&engine, 2);
        f.state.lock().unwrap()["targets"]["cpu0"]["r52_mutate_on"] = json!(trigger);
        f.state.lock().unwrap()["targets"]["cpu0"]["r52_mutations"] = mutation;
        clear_trace(&f);
        let result = request(
            &engine,
            4,
            "registers_mpu",
            json!({"context":context,"bank":"el1","read":true}),
        );
        if allowed {
            assert!(result.is_ok(), "{result:?}");
        } else {
            let error = result.unwrap_err();
            assert!(error.contains(error_part), "{trigger}: {error}");
            let status = ok(&engine, 5, "status", json!({}));
            assert!(status["register_probe"].is_null());
            assert!(
                !status["register_samples"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|s| s["id"] == "prbar0" && s["state"] == "valid")
            );
            if trigger == "prbar2" {
                assert_eq!(
                    native_reads(&f.state.lock().unwrap()).last().unwrap()[2],
                    "prlar2"
                );
            }
        }
        no_raw_fallback(&f.state.lock().unwrap());
        ok(&engine, 6, "quit", json!({}));
    }
}

#[test]
fn r52_native_mpu_backend_refusal_or_fault_stops_batch_and_preserves_old_provenance() {
    for uncertain in [false, true] {
        let f = configured("");
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let context = probe(&engine, 2);
        let first = mpu(&engine, 4, &context, "el1");
        if uncertain {
            f.state.lock().unwrap()["fault"] = json!("r52_fault");
        } else {
            f.state.lock().unwrap()["targets"]["cpu0"]["r52_errors"] =
                json!({"prlar2":"access-unknown"});
        }
        clear_trace(&f);
        assert!(
            request(
                &engine,
                5,
                "registers_mpu",
                json!({"context":context,"bank":"el1","read":true})
            )
            .is_err()
        );
        let status = ok(&engine, 6, "status", json!({}));
        assert_eq!(status["state"], if uncertain { "FAULT" } else { "STOPPED" });
        for previous in first["samples"].as_array().unwrap() {
            let stored = status["register_samples"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["id"] == previous["id"])
                .unwrap();
            assert_eq!(stored["state"], "stale");
            assert_eq!(stored["value"], previous["value"]);
            assert_eq!(stored["provenance"], previous["provenance"]);
            assert_eq!(stored["timestamp_ms"], previous["timestamp_ms"]);
        }
        let state = f.state.lock().unwrap().clone();
        assert_eq!(native_reads(&state).len(), if uncertain { 1 } else { 11 });
        assert!(
            request(
                &engine,
                7,
                "registers_mpu",
                json!({"context":context,"bank":"el1","read":true})
            )
            .is_err()
        );
        assert_eq!(
            native_reads(&f.state.lock().unwrap()).len(),
            native_reads(&state).len()
        );
        no_raw_fallback(&state);
        ok(&engine, 8, "quit", json!({}));
    }
}

#[test]
fn r52_native_mpu_refuses_lower_current_el_hdd_or_stricter_catalogue_without_fallback() {
    for dscr in ["0x01050113", "0x01058213"] {
        let f = configured("");
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let context = probe(&engine, 2);
        let reads = f.state.lock().unwrap()["targets"]["cpu0"]["r52_data_reads"].clone();
        f.state.lock().unwrap()["targets"]["cpu0"]["r52_dscr"] = json!(dscr);
        clear_trace(&f);
        assert!(
            request(
                &engine,
                4,
                "registers_mpu",
                json!({"context":context,"bank":"el1","read":true})
            )
            .is_err()
        );
        assert_eq!(native_reads(&f.state.lock().unwrap()).len(), 1);
        assert_eq!(
            f.state.lock().unwrap()["targets"]["cpu0"]["r52_data_reads"],
            reads
        );
        no_raw_fallback(&f.state.lock().unwrap());
        ok(&engine, 5, "quit", json!({}));
    }
    let mut f = configured("");
    let mut catalogue = debugtui::registers::Catalogue::builtin("cortex-r52").unwrap();
    catalogue
        .registers
        .iter_mut()
        .find(|r| r.id == "prlar23")
        .unwrap()
        .access_rule
        .min_el = Some(3);
    let path = f.transcript.parent().unwrap().join("strict-mpu.toml");
    fs::write(&path, toml::to_string(&catalogue).unwrap()).unwrap();
    f.project.registers.catalogue = path;
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let context = probe(&engine, 2);
    clear_trace(&f);
    let transcript = fs::read(&f.transcript).unwrap();
    let error = request(
        &engine,
        4,
        "registers_mpu",
        json!({"context":context,"bank":"el1","read":true}),
    )
    .unwrap_err();
    assert!(error.contains("NeedEl(3)"), "{error}");
    assert_eq!(fs::read(&f.transcript).unwrap(), transcript);
    assert!(
        f.state.lock().unwrap()["trace"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    ok(&engine, 5, "quit", json!({}));
}

#[test]
fn r52_native_mpu_cancellation_or_physical_frame_change_discards_batch_and_restores_target() {
    for context_change in [false, true] {
        let mut f = configured("");
        if context_change {
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
        clear_trace(&f);
        let action = Request::new(
            4,
            "registers_mpu",
            json!({"context":context,"bank":"el1","read":true}),
        );
        if context_change {
            f.state.lock().unwrap()["targets"]["cpu0"]["r52_context_on"] = json!("prbar2");
        } else {
            *f.cancel_on_transaction.lock().unwrap() = Some(action.clone());
        }
        engine.send(action).unwrap();
        let error = loop {
            if let Event::Response {
                id: 4, ok, error, ..
            } = engine.events.recv_timeout(Duration::from_secs(15)).unwrap()
            {
                assert!(!ok);
                break error.unwrap();
            }
        };
        assert!(
            error.contains(if context_change {
                "Physical context changed"
            } else {
                "cancelled"
            }),
            "{error}"
        );
        let state = f.state.lock().unwrap().clone();
        assert_eq!(
            native_reads(&state).len(),
            if context_change { 10 } else { 1 }
        );
        no_raw_fallback(&state);
        let status = ok(&engine, 5, "status", json!({}));
        assert_eq!(status["state"], "STOPPED");
        assert!(
            !status["register_samples"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s["id"] == "prbar0" && s["state"] == "valid")
        );
        ok(&engine, 6, "quit", json!({}));
    }
}
