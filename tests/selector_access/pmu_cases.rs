use super::*;
#[test]
fn pmu_deferred_driver_checks_full_firmware_baselines_and_rejects_unready_or_enabled_fixtures() {
    for scenario in ["positive", "not_ready", "reference_mismatch", "enabled"] {
        let mut f = configured("");
        let spec: Value =
            serde_json::from_str(include_str!("../fixtures/register-pmu-board.example.json"))
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
        expressions["(unsigned long long)debugtui_pmu_reference[0].ready"] =
            json!(if scenario == "not_ready" { "0" } else { "1" });
        if scenario == "reference_mismatch" {
            expressions["(unsigned long long)debugtui_pmu_reference[0].cycle"] =
                json!("0x76543210");
        }
        f.project.gdb.env.insert(
            "DEBUGTUI_TEST_EXPRESSION_VALUES".into(),
            expressions.to_string(),
        );
        *f.state.lock().unwrap() = json!({"current":"outside","targets":{"cpu0":{"pmselr":3,"pmu_dspsr":"0xa200041a","pmu_values":values,"pmu_pmcr":if scenario=="enabled" {"0x41132049"} else {"0x41132048"}}},"trace":[]});
        let directory = f.transcript.parent().unwrap();
        let project = directory.join("pmu-driver.toml");
        fs::write(&project, toml::to_string(&f.project).unwrap()).unwrap();
        let original = fs::read(&project).unwrap();
        let mut spec = spec;
        spec["frame_function"] = json!("main");
        let case = directory.join("pmu-driver.json");
        fs::write(&case, serde_json::to_vec_pretty(&spec).unwrap()).unwrap();
        let output = Command::new("node")
            .arg(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("scripts/test-register-pmu-hardware.cjs"),
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
                "enabled" => "counters already disabled",
                _ => unreachable!(),
            };
            assert!(failure.contains(expected), "{scenario}: {failure}");
            if scenario == "not_ready" {
                assert!(pmu_reads(&f.state.lock().unwrap()).is_empty());
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
    f.project.registers.pmu_command = "aarch64 pmu".into();
    f.project.registers.cp15_command.clear();
    f.project.registers.cp15_64_command.clear();
    f.project.registers.selector_command.clear();
    f.project.registers.facts.insert("pmu.present".into(), 1);
    f
}
fn pmu_reads(state: &Value) -> Vec<String> {
    state["trace"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row[1] == "pmu")
        .map(|row| row[2].as_str().unwrap().into())
        .collect()
}
#[test]
fn pmu_adapter_reads_native_widths_on_selected_core_and_probes_capacity_from_current_debug_el() {
    let mut f = configured("");
    f.project.cores = (0..2)
        .map(|i| Core {
            name: format!("core{i}"),
            endpoint: format!("localhost:{}", 27800 + i),
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
    gdb["pmcr"] = json!("0x41130800");
    f.project
        .gdb
        .env
        .insert("DEBUGTUI_TEST_REGISTER_VALUES".into(), gdb.to_string());
    let mut entries: Vec<(&str, String, u16)> = debugtui::registers::pmu::SCALARS
        .iter()
        .enumerate()
        .map(|(i, (name, ..))| (*name, format!("0x{:08x}", 0xf1234500u32 + i as u32), 32))
        .collect();
    for (name, raw, _) in &mut entries {
        if *name == "pmcr" {
            *raw = "0x41132048".into();
        }
        if *name == "pmselr" {
            *raw = "0x00000003".into();
        }
    }
    entries.push(("pmccntr", "0xfedcba9876543210".into(), 64));
    let mut values = json!({});
    for (name, raw, _) in &entries {
        values[*name] = json!(raw);
    }
    *f.state.lock().unwrap() =
        json!({"targets":{"cpu1":{"pmselr":3,"pmu_values":values}},"trace":[]});
    let engine = debugtui::coordinator::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    ok(&engine, 2, "control_scope", json!({"scope":"all"}));
    ok(&engine, 3, "select_core", json!({"index":1}));
    let initial = ok(&engine, 40, "registers_list", json!({}))["context"].clone();
    let probe = ok(&engine, 4, "registers_probe", json!({"context":initial}));
    assert_eq!(probe["facts"]["cpu.mode"], 16);
    assert_eq!(probe["facts"]["pmu.counters"], 4);
    assert_eq!(probe["facts"]["pmu.guest_partition"], 2);
    assert_eq!(
        probe["probe"]["facts"]["pmu.counters"]["source"],
        "openocd:aarch64 pmu"
    );
    let context = probe["probe"]["context"].clone();
    let read = ok(
        &engine,
        5,
        "registers_read",
        json!({"context":context,"ids":entries.iter().map(|e|e.0).collect::<Vec<_>>() }),
    );
    for (i, (name, raw, bits)) in entries.iter().enumerate() {
        let sample = &read["samples"][i];
        assert_eq!(sample["id"], *name);
        assert_eq!(sample["state"], "valid", "{sample}");
        assert_eq!(sample["implementation"], "yes");
        assert_eq!(sample["value"], json!({"bits":bits,"hex":raw}));
        assert_eq!(sample["owner"], "core:core1");
        assert_eq!(sample["source"], "openocd:aarch64 pmu");
        assert_eq!(sample["view"], "physical_core");
        let access = &sample["provenance"]["access"];
        assert_eq!(access["route"]["target"], "cpu1");
        assert_eq!(access["pmu"]["dscr"]["hex"], "0x01000200");
        assert_eq!(access["pmu"]["dspsr"]["hex"], "0xa2000410");
        assert_eq!(access["pmu"]["pmselr"]["hex"], "0x00000003");
        assert_eq!(
            access["pmu"]["read_method"],
            if *bits == 64 { "mrrc64" } else { "mrc32" }
        );
        assert_eq!(access["phase"], "responded");
        assert!(
            access["completed_ms"].as_u64().unwrap() >= access["timestamp_ms"].as_u64().unwrap()
        );
    }
    let state = f.state.lock().unwrap().clone();
    assert_eq!(pmu_reads(&state).len(), 24);
    assert!(
        state["trace"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r[1] == "pmu")
            .all(|r| r[0] == "cpu1")
    );
    assert_eq!(state["current"], "outside");
    assert_eq!(state["targets"]["cpu1"]["pmselr"], 3);
    assert_eq!(state["targets"]["cpu0"]["pmselr"], 31);
    assert!(selector_writes(&state).is_empty());
    assert!(!state["trace"].to_string().contains("mrrc"));
    ok(&engine, 6, "select_core", json!({"index":0}));
    assert!(
        request(
            &engine,
            7,
            "registers_read",
            json!({"context":context,"ids":["pmccntr"]})
        )
        .is_err()
    );
    assert_eq!(pmu_reads(&f.state.lock().unwrap()).len(), 24);
    ok(&engine, 8, "quit", json!({}));
}
#[test]
fn pmu_adapter_keeps_guest_permissions_and_unimplemented_selection_separate_from_retained_evidence()
{
    let f = configured("");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let read = ok(
        &engine,
        2,
        "registers_read",
        json!({"ids":["pmccntr","pmxevtyper","pmxevcntr"]}),
    );
    assert_eq!(read["samples"][0]["state"], "valid");
    assert_eq!(read["samples"][1]["state"], "valid"); // SEL=31 is cycle filter.
    assert_eq!(read["samples"][2]["reason"], "access_restricted");
    let original = read["samples"][0]["provenance"]["access"].clone();
    f.state.lock().unwrap()["targets"]["cpu0"]["pmu_dscr"] = json!("0x01010100");
    let failed = ok(
        &engine,
        3,
        "registers_read",
        json!({"ids":["pmccntr","pmevcntr0"]}),
    );
    assert!(
        failed["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["reason"] == "unknown" && s["value"].is_null())
    );
    let status = ok(&engine, 4, "status", json!({}));
    let retained = status["register_samples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "pmccntr")
        .unwrap();
    assert_eq!(retained["value"]["hex"], "0xfedcba9876543210");
    assert_eq!(
        retained["last_value_provenance"]["provenance"]["access"],
        original
    );
    assert_eq!(status["state"], "STOPPED");
    assert!(selector_writes(&f.state.lock().unwrap()).is_empty());
    ok(&engine, 5, "quit", json!({}));
}
#[test]
fn pmu_adapter_rejects_protocol_identity_permissions_and_forged_or_truncated_evidence_without_fallback()
 {
    for (fault, reason) in [
        ("pmu_protocol", "reader_unsupported"),
        ("pmu_identity", "reader_unsupported"),
        ("pmu_unknown", "unknown"),
        ("pmu_short", "reader_unsupported"),
        ("pmu_forged_el", "reader_unsupported"),
        ("pmu_forged_count", "reader_unsupported"),
    ] {
        let f = configured(fault);
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let read = ok(&engine, 2, "registers_read", json!({"ids":["pmccntr"]}));
        assert_eq!(read["samples"][0]["reason"], reason, "{fault}: {read}");
        assert!(read["samples"][0]["value"].is_null());
        assert_eq!(read["samples"][0]["implementation"], "yes");
        assert_eq!(ok(&engine, 3, "status", json!({}))["state"], "STOPPED");
        assert_eq!(
            pmu_reads(&f.state.lock().unwrap()).len(),
            usize::from(fault != "pmu_protocol")
        );
        assert!(
            !f.state.lock().unwrap()["trace"]
                .to_string()
                .contains("mrrc")
        );
        ok(&engine, 4, "quit", json!({}));
    }
    let f = configured("pmu_fault");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let read = ok(
        &engine,
        2,
        "registers_read",
        json!({"ids":["pmccntr","pmceid0"]}),
    );
    assert!(
        read["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["value"].is_null())
    );
    assert_eq!(ok(&engine, 3, "status", json!({}))["state"], "FAULT");
    assert_eq!(pmu_reads(&f.state.lock().unwrap()).len(), 1);
    assert!(request(&engine, 4, "continue", json!({})).is_err());
    ok(&engine, 5, "quit", json!({}));
}
#[test]
fn cancelled_pmu_transaction_restores_target_and_discards_samples_without_retry() {
    let f = configured("");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let before = ok(&engine, 2, "status", json!({}));
    let read = Request::new(3, "registers_read", json!({"ids":["pmccntr","pmceid0"]}));
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
    assert_eq!(pmu_reads(&state).len(), 1);
    ok(&engine, 5, "quit", json!({}));
}

#[test]
fn pmu_context_change_discards_physical_value_and_stops_the_batch() {
    let mut f = configured("pmu_context_change");
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
        json!({"ids":["pmccntr","pmceid0"]}),
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
    assert_eq!(pmu_reads(&f.state.lock().unwrap()).len(), 1);
    assert_eq!(f.state.lock().unwrap()["current"], "outside");
    assert!(ok(&engine, 3, "registers_list", json!({}))["probe"].is_null());
    ok(&engine, 4, "quit", json!({}));
}
