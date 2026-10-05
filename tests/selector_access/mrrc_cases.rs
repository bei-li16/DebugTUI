use super::*;

#[test]
fn deferred_timer_driver_runs_actual_binary_with_independent_fixture_baseline_and_peer() {
    let mut fixture = fixture("");
    adapter(&mut fixture);
    fixture.project.cores = (0..2)
        .map(|i| Core {
            name: format!("core{i}"),
            endpoint: format!("localhost:{}", 26900 + i),
            ..Default::default()
        })
        .collect();
    fixture.project.registers.targets = [
        ("core0".into(), "cpu0".into()),
        ("core1".into(), "cpu1".into()),
    ]
    .into();
    fixture.project.gdb.env.insert(
        "DEBUGTUI_TEST_EXPRESSION_VALUES".into(),
        json!({
            "(unsigned long long)debugtui_timer_reference_cntpct":"0xfedcba9876543210",
            "(unsigned long long)debugtui_timer_reference_cntvct":"0xfedcba9876543210",
            "(unsigned long long)debugtui_timer_ready":"1",
            "(unsigned long long)debugtui_timer_reference_cntp_cval":"0xfedcba9876543210",
            "(unsigned long long)debugtui_timer_reference_cntv_cval":"0x0123456789abcdef",
            "(unsigned long long)debugtui_timer_reference_cntvoff":"0x0000000100000000",
            "(unsigned long long)debugtui_timer_reference_cnthp_cval":"0x8000000000000001"
        })
        .to_string(),
    );
    fixture.state.lock().unwrap()["targets"] = json!({"cpu0":{"timer64":{
        "15 2 14":"0xfedcba9876543210", "15 3 14":"0x0123456789abcdef",
        "15 4 14":"0x0000000100000000", "15 6 14":"0x8000000000000001"
    }}});
    let directory = fixture.transcript.parent().unwrap();
    let project = directory.join("timer-driver.toml");
    fs::write(&project, toml::to_string(&fixture.project).unwrap()).unwrap();
    let original = fs::read(&project).unwrap();
    let case = directory.join("timer-driver.json");
    let mut spec: Value = serde_json::from_str(include_str!(
        "../fixtures/register-timer-board.example.json"
    ))
    .unwrap();
    spec["frame_function"] = json!("main");
    for counter in spec["counters"].as_array_mut().unwrap() {
        counter["require_progress"] = json!(false);
    }
    fs::write(&case, serde_json::to_vec_pretty(&spec).unwrap()).unwrap();
    let output = Command::new("node")
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("scripts/test-register-timer-hardware.cjs"),
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
        stdout.contains("\"passed\":6,\"failed\":0,\"skipped\":0"),
        "{stdout}"
    );
    assert_eq!(fs::read(&project).unwrap(), original);
    let state = fixture.state.lock().unwrap();
    assert_eq!(state["current"], "outside");
    assert!(selector_writes(&state).is_empty());
    assert!(
        state["trace"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s[1] == "mrrc")
            .all(|s| s[0] == "cpu0")
    );
    for cpu in ["cpu0", "cpu1"] {
        for (id, value) in [("prselr", 1), ("hprselr", 2), ("pmselr", 31)] {
            assert_eq!(state["targets"][cpu][id], value);
        }
    }
}

fn adapter(fixture: &mut Fixture) {
    fixture.project.registers.cp15_command = "aarch64 mrc".into();
    fixture.project.registers.selector_command = "aarch64 mcr".into();
    fixture.project.registers.cp15_64_command = "aarch64 mrrc".into();
    fixture.project.registers.isb_command = "aarch64 isb".into();
}

#[test]
fn timer_baseline_driver_rejects_wrong_mode_unready_and_independent_reference_mismatch() {
    for failure in ["non_hyp", "not_ready", "reference_mismatch"] {
        let mut fixture = fixture("");
        adapter(&mut fixture);
        let mut registers: Value =
            serde_json::from_str(&fixture.project.gdb.env["DEBUGTUI_TEST_REGISTER_VALUES"])
                .unwrap();
        registers["cpsr"] = json!(if failure == "non_hyp" { "0x13" } else { "0x1a" });
        fixture.project.gdb.env.insert(
            "DEBUGTUI_TEST_REGISTER_VALUES".into(),
            registers.to_string(),
        );
        fixture.project.gdb.env.insert("DEBUGTUI_TEST_EXPRESSION_VALUES".into(),json!({
            "(unsigned long long)debugtui_timer_ready":if failure == "not_ready" { "0" } else { "1" },
            "(unsigned long long)debugtui_timer_reference_cntp_cval":"0x0000000000000001"
        }).to_string());
        fixture.state.lock().unwrap()["targets"] = json!({"cpu0":{"timer64":{
            "15 2 14":"0xfedcba9876543210", "15 3 14":"0x0123456789abcdef",
            "15 4 14":"0x0000000100000000", "15 6 14":"0x8000000000000001"
        }}});
        let directory = fixture.transcript.parent().unwrap();
        let project = directory.join("timer-negative.toml");
        fs::write(&project, toml::to_string(&fixture.project).unwrap()).unwrap();
        let original = fs::read(&project).unwrap();
        let case = directory.join("timer-negative.json");
        let mut spec: Value = serde_json::from_str(include_str!(
            "../fixtures/register-timer-board.example.json"
        ))
        .unwrap();
        spec["frame_function"] = json!("main");
        spec.as_object_mut().unwrap().remove("control_scope");
        spec.as_object_mut().unwrap().remove("peer_core");
        fs::write(&case, serde_json::to_vec_pretty(&spec).unwrap()).unwrap();
        let output = Command::new("node")
            .arg(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("scripts/test-register-timer-hardware.cjs"),
            )
            .args([
                "--run",
                "--software-fixture",
                "--core",
                "default",
                "--binary",
            ])
            .arg(env!("CARGO_BIN_EXE_debugtui"))
            .arg("--project")
            .arg(&project)
            .arg("--case")
            .arg(&case)
            .output()
            .unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(!output.status.success(), "{failure}: {stdout}");
        let report_directory = stdout
            .lines()
            .find(|line| line.starts_with("RESULT "))
            .unwrap()
            .rsplit_once("} ")
            .unwrap()
            .1;
        let report: Value = serde_json::from_slice(
            &fs::read(PathBuf::from(report_directory).join("report.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(report["board_tests_executed"], false);
        assert_eq!(report["counts"]["failed"], 1);
        let failed = report["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["status"] == "failed")
            .unwrap();
        assert_eq!(
            failed["id"],
            if failure == "reference_mismatch" {
                "REG-H05-WIDTH"
            } else {
                "REG-H05-STOP"
            }
        );
        assert!(
            report["cases"]
                .as_array()
                .unwrap()
                .iter()
                .any(|case| case["id"] == "REG-H05-CLEANUP" && case["status"] == "passed")
        );
        assert_eq!(fs::read(&project).unwrap(), original);
        let state = fixture.state.lock().unwrap();
        if failure != "reference_mismatch" {
            assert!(
                state
                    .get("trace")
                    .is_none_or(|trace| trace.as_array().unwrap().is_empty()),
                "No Timer/control/Probe TCL before a ready Hyp baseline"
            );
        } else {
            assert_eq!(state["current"], "outside");
            assert!(selector_writes(&state).is_empty());
            assert_eq!(
                state["trace"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|entry| entry[1] == "mrrc")
                    .count(),
                4
            );
        }
    }
}

#[test]
fn genuine_isb_selector_transaction_works_with_cp15ben_clear_and_restores_all_selectors() {
    let mut fixture = fixture("");
    adapter(&mut fixture);
    fixture.state.lock().unwrap()["targets"]["cpu0"]["sync"] = json!(0);
    let engine = session::spawn(fixture.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let context = probe(&engine, 2);
    for (id, kind, index) in [(4, "mpu_el1", 23), (5, "mpu_el2", 19), (6, "pmu", 3)] {
        let result = ok(
            &engine,
            id,
            "registers_select",
            json!({"context":context,"kind":kind,"index":index}),
        );
        assert!(
            result["evidence"]["synchronization"]
                .as_str()
                .unwrap()
                .contains("Genuine ISB")
        );
        assert_eq!(
            result["evidence"]["original"],
            result["evidence"]["restored"]
        );
        assert!(
            result["samples"]
                .as_array()
                .unwrap()
                .iter()
                .all(|s| s["state"] == "valid")
        );
    }
    let state = fixture.state.lock().unwrap().clone();
    assert_eq!(state["current"], "outside");
    assert_eq!(state["targets"]["cpu0"]["sync"], 0);
    for (selector, old) in [("prselr", 1), ("hprselr", 2), ("pmselr", 31)] {
        assert_eq!(state["targets"]["cpu0"][selector], old);
    }
    let trace = state["trace"].as_array().unwrap();
    assert_eq!(trace.iter().filter(|s| s[1] == "isb").count(), 9);
    assert!(trace.iter().all(|s| !(s[1] == "mcr" && s[4] == "7")));
    assert_eq!(selector_writes(&state).len(), 6);
    ok(&engine, 7, "quit", json!({}));
}

#[test]
fn adapter_protocol_mismatch_blocks_mrrc_and_selector_writes_before_access() {
    let mut fixture = fixture("adapter_mismatch");
    adapter(&mut fixture);
    let engine = session::spawn(fixture.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let context = probe(&engine, 2);
    let read = ok(
        &engine,
        4,
        "registers_read",
        json!({"context":context,"ids":["cntpct"],"manual":true}),
    );
    assert_eq!(read["samples"][0]["state"], "unsupported");
    assert_eq!(read["samples"][0]["reason"], "reader_unsupported");
    assert!(
        request(
            &engine,
            5,
            "registers_select",
            json!({"context":context,"kind":"pmu","index":3})
        )
        .is_err()
    );
    let state = fixture.state.lock().unwrap().clone();
    assert!(selector_writes(&state).is_empty());
    assert!(
        state["trace"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s[1] != "mrrc" && s[1] != "isb")
    );
    assert_eq!(state["current"], "outside");
    assert_eq!(ok(&engine, 6, "status", json!({}))["state"], "STOPPED");
    ok(&engine, 7, "quit", json!({}));
}

#[test]
fn mrrc_scope_all_reads_only_selected_core_with_full_width_and_rejects_old_context() {
    let mut fixture = fixture("");
    adapter(&mut fixture);
    fixture.project.cores = (0..2)
        .map(|i| Core {
            name: format!("core{i}"),
            endpoint: format!("localhost:{}", 26000 + i),
            ..Default::default()
        })
        .collect();
    fixture.project.registers.targets = [
        ("core0".into(), "cpu0".into()),
        ("core1".into(), "cpu1".into()),
    ]
    .into();
    let engine = debugtui::coordinator::spawn(fixture.project.clone());
    ok(&engine, 1, "connect", json!({}));
    ok(&engine, 2, "control_scope", json!({"scope":"all"}));
    ok(&engine, 3, "select_core", json!({"index":1}));
    let context = probe(&engine, 4);
    fixture.state.lock().unwrap()["trace"] = json!([]);
    let result = ok(
        &engine,
        6,
        "registers_read",
        json!({"context":context,"ids":["cntpct","cntvct","cntp_cval"],"manual":true}),
    );
    assert_eq!(result["samples"].as_array().unwrap().len(), 3);
    for sample in result["samples"].as_array().unwrap() {
        assert_eq!(sample["state"], "valid");
        assert_eq!(sample["value"]["hex"], "0x81234567abcdef01");
        assert_eq!(sample["owner"], "core:core1");
        assert_eq!(sample["source"], "openocd:aarch64 mrrc");
        let access = &sample["provenance"]["access"];
        assert_eq!(access["route"]["target"], "cpu1");
        assert_eq!(
            access["route"]["endpoint"],
            fixture.project.registers.tcl_endpoint
        );
        assert_eq!(access["context"], context);
        assert_eq!(access["phase"], "responded");
        assert!(
            access["route"]["operation"]
                .as_str()
                .unwrap()
                .starts_with("aarch64 mrrc ")
        );
        assert!(access["command"].as_str().unwrap().contains("aarch64 mrrc"));
    }
    let state = fixture.state.lock().unwrap().clone();
    let reads: Vec<_> = state["trace"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s[1] == "mrrc")
        .collect();
    assert_eq!(reads.len(), 3);
    assert!(reads.iter().all(|r| r[0] == "cpu1"));
    assert!(selector_writes(&state).is_empty());
    ok(&engine, 7, "select_core", json!({"index":0}));
    assert!(
        request(
            &engine,
            8,
            "registers_read",
            json!({"context":context,"ids":["cntpct"],"manual":true})
        )
        .unwrap_err()
        .contains("expired")
    );
    assert_eq!(*fixture.state.lock().unwrap(), state);
    ok(&engine, 9, "quit", json!({}));
}

#[test]
fn genuine_isb_fault_quarantines_core_and_all_later_reads_without_retry() {
    let mut fixture = fixture("genuine_isb_fault");
    adapter(&mut fixture);
    let engine = session::spawn(fixture.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let context = probe(&engine, 2);
    assert!(
        request(
            &engine,
            4,
            "registers_select",
            json!({"context":context,"kind":"pmu","index":3})
        )
        .unwrap_err()
        .contains("unknown")
    );
    assert_eq!(ok(&engine, 5, "status", json!({}))["state"], "FAULT");
    let state = fixture.state.lock().unwrap().clone();
    assert!(
        request(
            &engine,
            6,
            "registers_read",
            json!({"context":context,"ids":["cntpct"],"manual":true})
        )
        .is_err()
    );
    assert!(request(&engine, 7, "continue", json!({})).is_err());
    assert_eq!(*fixture.state.lock().unwrap(), state);
    ok(&engine, 8, "quit", json!({}));
}
