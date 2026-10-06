use super::*;
#[test]
fn gic_deferred_driver_checks_full_firmware_baselines_and_rejects_unready_or_mismatched_fixtures() {
    for scenario in ["positive", "not_ready", "reference_mismatch"] {
        let mut f = configured("");
        let spec: Value =
            serde_json::from_str(include_str!("../fixtures/register-gic-board.example.json"))
                .unwrap();
        let mut values = json!({});
        let mut expressions = json!({});
        for entry in spec["registers"].as_array().unwrap() {
            values[entry["id"].as_str().unwrap()] = entry["expected"].clone();
            expressions[format!(
                "(unsigned long long){}",
                entry["reference"].as_str().unwrap()
            )] = entry["expected"].clone();
        }
        expressions["(unsigned long long)debugtui_gic_reference[0].ready"] =
            json!(if scenario == "not_ready" { "0" } else { "1" });
        if scenario == "reference_mismatch" {
            expressions["(unsigned long long)debugtui_gic_reference[0].raw[3]"] =
                json!("0x76543210");
        }
        f.project.gdb.env.insert(
            "DEBUGTUI_TEST_EXPRESSION_VALUES".into(),
            expressions.to_string(),
        );
        *f.state.lock().unwrap() = json!({"current":"outside","targets":{"cpu0":{"pmselr":3,"gic_dspsr":"0xa200041a","gic_values":values}},"trace":[]});
        let directory = f.transcript.parent().unwrap();
        let project = directory.join("gic-driver.toml");
        fs::write(&project, toml::to_string(&f.project).unwrap()).unwrap();
        let original = fs::read(&project).unwrap();
        let mut spec = spec;
        spec["frame_function"] = json!("main");
        let case = directory.join("gic-driver.json");
        fs::write(&case, serde_json::to_vec_pretty(&spec).unwrap()).unwrap();
        let output = Command::new("node")
            .arg(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("scripts/test-register-gic-hardware.cjs"),
            )
            .arg("--run")
            .arg("--software-fixture")
            .arg("--binary")
            .arg(env!("CARGO_BIN_EXE_debugtui"))
            .arg("--project")
            .arg(&project)
            .arg("--core")
            .arg("default")
            .arg("--case")
            .arg(&case)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(
            output.status.success(),
            scenario == "positive",
            "{scenario}: {stdout} / {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report_dir = stdout
            .lines()
            .find(|line| line.starts_with("RESULT "))
            .unwrap()
            .split_whitespace()
            .last()
            .unwrap();
        let report: Value = serde_json::from_slice(
            &fs::read(PathBuf::from(report_dir).join("report.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(report["board_tests_executed"], false);
        if scenario == "positive" {
            assert_eq!(report["counts"], json!({"passed":5,"failed":0,"skipped":0}));
        } else {
            assert_eq!(report["counts"]["failed"], 1);
            let failure = report["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["status"] == "failed")
                .unwrap()["error"]
                .as_str()
                .unwrap();
            let expected = match scenario {
                "not_ready" => "baseline not ready",
                "reference_mismatch" => "baseline differs",
                _ => unreachable!(),
            };
            assert!(failure.contains(expected), "{scenario}: {failure}");
            if scenario == "not_ready" {
                assert!(gic_reads(&f.state.lock().unwrap()).is_empty());
            }
        }
        assert_eq!(fs::read(&project).unwrap(), original);
        let state = f.state.lock().unwrap().clone();
        assert_eq!(state["current"], "outside");
        assert!(selector_writes(&state).is_empty());
        assert_eq!(state["targets"]["cpu0"]["pmselr"], 3);
        assert!(!state["trace"].to_string().contains("resume"));
    }
}

fn configured(fault: &'static str) -> Fixture {
    let mut f = fixture(fault);
    f.project.registers.gic_command = "aarch64 gic".into();
    f.project.registers.cp15_command.clear();
    f.project.registers.cp15_64_command.clear();
    f.project.registers.selector_command.clear();
    for (key, value) in [
        ("gic.system_interface", 1),
        ("icc.physical.prebits", 7),
        ("icv.virtual.prebits", 7),
        ("ich.list_registers", 16),
    ] {
        f.project.registers.facts.insert(key.into(), value);
    }
    f
}
fn gic_reads(state: &Value) -> Vec<String> {
    state["trace"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r[1] == "gic")
        .map(|r| r[2].as_str().unwrap().into())
        .collect()
}
#[test]
fn gic_adapter_probes_current_el2_filters_ap_and_reads_physical_and_virtual_backing_on_one_core() {
    let mut f = configured("");
    f.project.cores = (0..2)
        .map(|i| Core {
            name: format!("core{i}"),
            endpoint: format!("localhost:{}", 27900 + i),
            ..Default::default()
        })
        .collect();
    f.project.registers.targets = [
        ("core0".into(), "cpu0".into()),
        ("core1".into(), "cpu1".into()),
    ]
    .into();
    let mut gdb: Value =
        serde_json::from_str(&f.project.gdb.env["DEBUGTUI_TEST_REGISTER_VALUES"]).unwrap();
    gdb["cpsr"] = json!("0x10");
    gdb["icc_ctlr"] = json!("0x600");
    gdb["ich_vtr"] = json!("0xd418000f");
    f.project
        .gdb
        .env
        .insert("DEBUGTUI_TEST_REGISTER_VALUES".into(), gdb.to_string());
    let engine = debugtui::coordinator::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    ok(&engine, 2, "control_scope", json!({"scope":"all"}));
    ok(&engine, 3, "select_core", json!({"index":1}));
    let context = ok(&engine, 4, "registers_list", json!({}))["context"].clone();
    let probe = ok(&engine, 5, "registers_probe", json!({"context":context}));
    assert_eq!(probe["facts"]["cpu.mode"], 16);
    for key in [
        "icc.physical.pribits",
        "icc.physical.prebits",
        "icv.virtual.pribits",
        "icv.virtual.prebits",
    ] {
        assert_eq!(probe["facts"][key], 5);
    }
    assert_eq!(probe["facts"]["ich.list_registers"], 4);
    for key in ["icc.physical.prebits", "icv.virtual.prebits"] {
        assert_eq!(
            probe["probe"]["facts"][key]["source"],
            "openocd:aarch64 gic"
        );
    }
    let ids: Vec<_> = debugtui::registers::gic::SCALARS
        .iter()
        .filter(|(name, ..)| {
            !name.starts_with("icc_iar") && (!name.contains("_ap") || name.ends_with('0'))
        })
        .map(|(name, ..)| *name)
        .chain(["icv_ap0r0", "icv_ap1r0"])
        .collect();
    let read = ok(
        &engine,
        6,
        "registers_read",
        json!({"context":context,"ids":ids}),
    );
    assert_eq!(ids.len(), 31);
    for sample in read["samples"].as_array().unwrap() {
        assert_eq!(sample["state"], "valid", "{sample}");
        assert_eq!(sample["implementation"], "yes");
        assert_eq!(sample["owner"], "core:core1");
        assert_eq!(sample["value"]["bits"], 32);
        let name = sample["id"].as_str().unwrap();
        if name.starts_with("icv_") {
            let parent = name.replacen("icv_", "ich_", 1);
            assert_eq!(
                sample["source"],
                format!("alias:{parent}@0 <- openocd:aarch64 gic")
            );
            assert_eq!(sample["provenance"]["aliases"][0]["source"], parent);
        } else {
            assert_eq!(sample["source"], "openocd:aarch64 gic");
        }
        assert_eq!(sample["view"], "physical_core");
        let access = &sample["provenance"]["access"];
        assert_eq!(access["route"]["target"], "cpu1");
        assert_eq!(access["gic"]["read_method"], "mrc32");
        assert_eq!(
            access["gic"]["view"],
            if sample["id"].as_str().unwrap().starts_with("icc_") {
                "physical_icc"
            } else {
                "hypervisor_ich"
            }
        );
        assert_eq!(access["gic"]["dscr"]["hex"], "0x01000200");
        assert_eq!(access["gic"]["dspsr"]["hex"], "0xa2000410");
        assert!(
            access["completed_ms"].as_u64().unwrap() >= access["timestamp_ms"].as_u64().unwrap()
        );
    }
    let mut absent = vec![];
    for kind in ["icc", "ich", "icv"] {
        for bank in 0..2 {
            for n in 1..4 {
                absent.push(format!("{kind}_ap{bank}r{n}"));
            }
        }
    }
    let read = ok(
        &engine,
        7,
        "registers_read",
        json!({"context":context,"ids":absent,"manual":true}),
    );
    assert!(
        read["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["implementation"] == "no" && s["reason"] == "hardware_not_implemented")
    );
    for sample in read["samples"].as_array().unwrap() {
        let virtual_view = !sample["id"].as_str().unwrap().starts_with("icc_");
        let basis = sample["eligibility"]["probe"]["observations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["id"] == if virtual_view { "ich_vtr" } else { "icc_ctlr" })
            .unwrap();
        let access = &basis["provenance"]["access"];
        assert_eq!(access["context"], context);
        assert_eq!(access["route"]["target"], "cpu1");
        assert_eq!(
            access["gic"]["view"],
            if virtual_view {
                "hypervisor_ich"
            } else {
                "physical_icc"
            }
        );
        assert_eq!(access["gic"]["icc_ctlr"]["hex"], "0x00000403");
        assert_eq!(access["gic"]["ich_vtr"]["hex"], "0x90180003");
        assert!(
            access["completed_ms"].as_u64().unwrap() >= access["timestamp_ms"].as_u64().unwrap()
        );
    }
    let state = f.state.lock().unwrap().clone();
    assert_eq!(gic_reads(&state).len(), 31);
    assert_eq!(state["current"], "outside");
    assert!(
        state["trace"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r[1] == "gic")
            .all(|r| r[0] == "cpu1")
    );
    assert!(selector_writes(&state).is_empty());
    assert!(!state["trace"].to_string().contains("mrrc"));
    ok(&engine, 8, "select_core", json!({"index":0}));
    assert!(
        request(
            &engine,
            9,
            "registers_read",
            json!({"context":context,"ids":["icc_ap0r0"]})
        )
        .is_err()
    );
    assert_eq!(gic_reads(&f.state.lock().unwrap()).len(), 31);
    ok(&engine, 10, "quit", json!({}));
}
#[test]
fn gic_acknowledgement_and_write_only_commands_never_become_observational_reads() {
    let f = configured("");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let ids = [
        "icc_iar0",
        "icc_iar1",
        "icc_eoir0",
        "icc_eoir1",
        "icc_dir",
        "icc_sgi0r",
        "icc_sgi1r",
        "icc_asgi1r",
    ];
    let automatic = ok(&engine, 2, "registers_read", json!({"ids":ids}));
    assert!(
        automatic["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["value"].is_null())
    );
    assert!(gic_reads(&f.state.lock().unwrap()).is_empty());
    let manual = ok(
        &engine,
        3,
        "registers_read",
        json!({"ids":ids,"manual":true}),
    );
    for sample in &manual["samples"].as_array().unwrap()[..2] {
        assert_eq!(sample["reason"], "access_restricted");
        assert!(sample["value"].is_null());
    }
    for sample in &manual["samples"].as_array().unwrap()[2..] {
        assert_eq!(sample["reason"], "write_only");
    }
    assert_eq!(
        gic_reads(&f.state.lock().unwrap()),
        ["icc_iar0", "icc_iar1"]
    );
    assert!(selector_writes(&f.state.lock().unwrap()).is_empty());
    ok(&engine, 4, "quit", json!({}));
}
#[test]
fn gic_lower_el_unknown_and_restricted_requests_preserve_old_value_proof() {
    let f = configured("");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let read = ok(&engine, 2, "registers_read", json!({"ids":["icc_ap0r0"]}));
    let original = read["samples"][0]["provenance"]["access"].clone();
    for (i, dscr, reason) in [
        (3, "0x01010100", "unknown"),
        (5, "0x01008200", "access_restricted"),
    ] {
        f.state.lock().unwrap()["targets"]["cpu0"]["gic_dscr"] = json!(dscr);
        let read = ok(&engine, i, "registers_read", json!({"ids":["icc_ap0r0"]}));
        assert_eq!(read["samples"][0]["reason"], reason);
        assert!(read["samples"][0]["value"].is_null());
        let status = ok(&engine, i + 1, "status", json!({}));
        let retained = status["register_samples"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "icc_ap0r0")
            .unwrap();
        assert_eq!(retained["value"]["hex"], "0xf1234567");
        assert_eq!(
            retained["last_value_provenance"]["provenance"]["access"],
            original
        );
        assert_eq!(status["state"], "STOPPED");
    }
    ok(&engine, 7, "quit", json!({}));
}
#[test]
fn gic_protocol_identity_and_forged_capacity_or_view_are_rejected_without_fallback() {
    for (fault, reason) in [
        ("gic_protocol", "reader_unsupported"),
        ("gic_identity", "reader_unsupported"),
        ("gic_unknown", "unknown"),
        ("gic_short", "reader_unsupported"),
        ("gic_forged_el", "reader_unsupported"),
        ("gic_forged_capacity", "reader_unsupported"),
        ("gic_forged_view", "reader_unsupported"),
    ] {
        let f = configured(fault);
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let read = ok(&engine, 2, "registers_read", json!({"ids":["icc_ap0r0"]}));
        assert_eq!(read["samples"][0]["reason"], reason, "{fault}: {read}");
        assert!(read["samples"][0]["value"].is_null());
        let state = f.state.lock().unwrap().clone();
        assert_eq!(
            gic_reads(&state).len(),
            usize::from(fault != "gic_protocol")
        );
        assert!(selector_writes(&state).is_empty());
        assert!(!state["trace"].to_string().contains("mrc"));
        assert_eq!(ok(&engine, 3, "status", json!({}))["state"], "STOPPED");
        ok(&engine, 4, "quit", json!({}));
    }
    let f = configured("gic_fault");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let read = ok(
        &engine,
        2,
        "registers_read",
        json!({"ids":["icc_ap0r0","ich_ap0r0"]}),
    );
    assert!(
        read["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["value"].is_null())
    );
    assert_eq!(gic_reads(&f.state.lock().unwrap()).len(), 1);
    assert_eq!(ok(&engine, 3, "status", json!({}))["state"], "FAULT");
    assert!(request(&engine, 4, "continue", json!({})).is_err());
    ok(&engine, 5, "quit", json!({}));
}
#[test]
fn cancelled_gic_transaction_restores_target_and_discards_samples_without_retry() {
    let f = configured("");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let before = ok(&engine, 2, "status", json!({}));
    let read = Request::new(
        3,
        "registers_read",
        json!({"ids":["icc_ap0r0","ich_ap0r0"]}),
    );
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
    assert_eq!(gic_reads(&state).len(), 1);
    ok(&engine, 5, "quit", json!({}));
}

#[test]
fn gic_context_change_discards_physical_value_and_stops_the_batch() {
    let mut f = configured("gic_context_change");
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
        json!({"ids":["icc_ap0r0","ich_ap0r0"]}),
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
    assert_eq!(gic_reads(&f.state.lock().unwrap()).len(), 1);
    assert_eq!(f.state.lock().unwrap()["current"], "outside");
    assert!(ok(&engine, 3, "registers_list", json!({}))["probe"].is_null());
    ok(&engine, 4, "quit", json!({}));
}
