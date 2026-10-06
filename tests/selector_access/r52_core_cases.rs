use super::*;

pub(super) fn configured(fault: &'static str) -> Fixture {
    let mut f = fixture(fault);
    f.project.registers.cp15_command = debugtui::registers::r52_core::COMMAND.into();
    f.project.registers.selector_command.clear();
    f.project.registers.isb_command.clear();
    f.project.gdb.env.insert("DEBUGTUI_TEST_REGISTER_VALUES".into(), json!({
        "cpsr":"0xa2000410", "midr":"0x411fd134", "id_pfr1":"0x10111001", "id_dfr0":"0x03010066",
        "mpuir":"0x1000", "hmpuir":"0x10", "cpacr":"0xf00000", "pmcr":"0x41132000", "icc_ctlr":"0x400", "ich_vtr":"0x90180003"
    }).to_string());
    f
}
pub(super) fn native_reads(state: &Value) -> Vec<&Value> {
    state["trace"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r[1] == "r52_read")
        .collect()
}

#[test]
fn r52_native_deferred_driver_runs_actual_binary_and_rejects_wrong_independent_baseline() {
    let mut f = configured("");
    f.project.cores = (0..2)
        .map(|i| Core {
            name: format!("core{i}"),
            endpoint: format!("localhost:{}", 28300 + i),
            ..Default::default()
        })
        .collect();
    f.project.registers.targets = [
        ("core0".into(), "cpu0".into()),
        ("core1".into(), "cpu1".into()),
    ]
    .into();
    let dir = f.transcript.parent().unwrap();
    let project = dir.join("r52-core-driver.toml");
    let case = dir.join("r52-core-driver.json");
    fs::write(&project, toml::to_string(&f.project).unwrap()).unwrap();
    let original = fs::read(&project).unwrap();
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts/test-register-r52-core-hardware.cjs");
    let default = Command::new("node").arg(&script).output().unwrap();
    assert!(default.status.success());
    assert!(
        String::from_utf8_lossy(&default.stdout)
            .contains("\"passed\":0,\"failed\":0,\"skipped\":4")
    );
    assert_eq!(*f.state.lock().unwrap(), json!({}));
    for negative in [false, true] {
        let mut spec: Value = serde_json::from_str(include_str!(
            "../fixtures/register-r52-core-board.example.json"
        ))
        .unwrap();
        spec["endpoint"] = json!(f.project.registers.tcl_endpoint);
        if negative {
            spec["values"]["sctlr"] = json!("0x00c50079");
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
                "\"passed\":2,\"failed\":1,\"skipped\":2"
            } else {
                "\"passed\":5,\"failed\":0,\"skipped\":0"
            }),
            "{text}"
        );
        assert_eq!(fs::read(&project).unwrap(), original);
    }
}

#[test]
fn r52_native_metadata_delegates_min_el2_but_never_unknown_owner_or_other_encodings() {
    use debugtui::registers::{Catalogue, Reader, Scope};
    for native in [true, false] {
        let mut f = configured("");
        let mut c = Catalogue::builtin("cortex-r52").unwrap();
        let mut allowed = c.register("sctlr").unwrap().clone();
        allowed.id = "requiresel2".into();
        allowed.access_rule.min_el = Some(2);
        let mut alias = allowed.clone();
        alias.id = "alias".into();
        alias.bits = 8;
        alias.fields.clear();
        alias.reader = Reader::Alias {
            source: allowed.id.clone(),
            offset: 0,
        };
        let mut higher = allowed.clone();
        higher.id = "requiresel3".into();
        higher.access_rule.min_el = Some(3);
        let mut unknown = allowed.clone();
        unknown.id = "unknownowner".into();
        unknown.scope = Scope::Unknown;
        let mut unsupported = allowed.clone();
        unsupported.id = "unreviewed".into();
        unsupported.access_rule.min_el = None;
        unsupported.reader = Reader::Cp15 {
            cp: 15,
            op1: 0,
            crn: 1,
            crm: 0,
            op2: 1,
        };
        let mut disabled = allowed.clone();
        disabled.id = "requiresenable".into();
        disabled.access_rule.need_enable = Some(
            serde_json::from_value(json!({
                "reg":allowed.id.clone(),"field":"M","op":"eq","value":1
            }))
            .unwrap(),
        );
        let mut side_effect = allowed.clone();
        side_effect.id = "manualonly".into();
        side_effect.read_side_effect = true;
        c.registers = vec![
            allowed,
            alias,
            higher,
            unknown,
            unsupported,
            disabled,
            side_effect,
        ];
        c.validate().unwrap();
        let path = f.transcript.parent().unwrap().join("bounded-custom.toml");
        fs::write(&path, toml::to_string(&c).unwrap()).unwrap();
        f.project.registers.catalogue = path;
        if !native {
            f.project.registers.cp15_command = "arm mrc".into();
        }
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let read = ok(
            &engine,
            2,
            "registers_read",
            json!({"ids":["requiresel2","alias","requiresel3","unknownowner","unreviewed","manualonly"]}),
        );
        assert_eq!(
            read["samples"][0]["state"],
            if native { "valid" } else { "unavailable" },
            "{read}"
        );
        assert_eq!(
            read["samples"][1]["state"],
            if native { "valid" } else { "unavailable" }
        );
        assert_eq!(
            read["samples"][2]["reason"],
            if native {
                "access_restricted"
            } else {
                "unknown"
            }
        );
        assert_eq!(read["samples"][3]["reason"], "unknown");
        if native {
            assert_eq!(read["samples"][4]["reason"], "reader_unsupported");
            let state = f.state.lock().unwrap().clone();
            assert_eq!(native_reads(&state).len(), 1);
            assert_eq!(read["samples"][1]["value"]["hex"], "0x78");
            assert_eq!(
                read["samples"][1]["provenance"]["access"],
                read["samples"][0]["provenance"]["access"]
            );
            assert!(read["samples"][5]["provenance"]["access"].is_null());
            let denied = ok(
                &engine,
                4,
                "registers_read",
                json!({"ids":["requiresenable"]}),
            );
            assert_eq!(
                denied["samples"][0]["reason"], "feature_disabled",
                "{denied}"
            );
            assert_eq!(native_reads(&f.state.lock().unwrap()).len(), 1);
        }
        ok(&engine, 3, "quit", json!({}));
    }
}
#[test]
fn r52_native_reads_reviewed_subset_at_current_el2_despite_saved_user_and_keeps_multicore_proof() {
    let mut f = configured("");
    f.project.cores = (0..2)
        .map(|i| Core {
            name: format!("core{i}"),
            endpoint: format!("localhost:{}", 28000 + i),
            ..Default::default()
        })
        .collect();
    f.project.registers.targets = [
        ("core0".into(), "cpu0".into()),
        ("core1".into(), "cpu1".into()),
    ]
    .into();
    *f.state.lock().unwrap() =
        json!({"targets":{"cpu1":{"el1_count":24,"el2_count":20}},"trace":[]});
    let engine = debugtui::coordinator::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    ok(&engine, 2, "control_scope", json!({"scope":"all"}));
    ok(&engine, 3, "select_core", json!({"index":1}));
    let context = probe(&engine, 4);
    let status = ok(&engine, 6, "status", json!({}));
    assert_eq!(
        status["register_probe"]["facts"]["mpu.el1.regions"]["value"],
        24
    );
    assert_eq!(
        status["register_probe"]["facts"]["mpu.el2.regions"]["value"],
        20
    );
    for id in ["midr", "mpuir", "hmpuir", "cpacr"] {
        let sample = status["register_probe"]["samples"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == id)
            .unwrap();
        assert_eq!(sample["source"], "openocd:aarch64 r52_read");
        assert_eq!(
            sample["provenance"]["access"]["r52_core"]["dspsr"]["hex"],
            "0xa2000410"
        );
    }
    let ids = [
        "midr", "mpidr", "sctlr", "hsctlr", "cpacr", "hcr", "mpuir", "hmpuir", "prselr", "hprselr",
        "hprenr", "mair0", "mair1", "hmair0", "hmair1", "prbar0", "prlar23", "hprbar19", "hprlar0",
    ];
    let read = ok(
        &engine,
        7,
        "registers_read",
        json!({"context":context,"ids":ids}),
    );
    for (i, id) in ids.iter().enumerate() {
        let sample = &read["samples"][i];
        assert_eq!(sample["id"], *id);
        assert_eq!(sample["state"], "valid", "{sample}");
        assert_eq!(sample["owner"], "core:core1");
        assert_eq!(sample["view"], "physical_core");
        let access = &sample["provenance"]["access"];
        assert_eq!(access["route"]["target"], "cpu1");
        assert_eq!(access["r52_core"]["dscr"]["hex"], "0x01050213");
        assert_eq!(access["r52_core"]["dspsr"]["hex"], "0xa2000410");
        assert_eq!(access["phase"], "responded");
        assert_eq!(access["context"], context);
    }
    assert_eq!(read["samples"][1]["value"]["hex"], "0x80000001");
    assert_eq!(
        read["samples"][16]["provenance"]["access"]["r52_core"]["capacity"]["hex"],
        "0x00001800"
    );
    assert_eq!(
        read["samples"][17]["provenance"]["access"]["r52_core"]["capacity"]["hex"],
        "0x00000014"
    );
    let state = f.state.lock().unwrap().clone();
    assert!(native_reads(&state).iter().all(|r| r[0] == "cpu1"));
    assert!(selector_writes(&state).is_empty());
    assert_eq!(state["current"], "outside");
    assert!(!state["trace"].to_string().contains("mrc"));
    ok(&engine, 8, "select_core", json!({"index":0}));
    assert!(
        request(
            &engine,
            9,
            "registers_read",
            json!({"context":context,"ids":["sctlr"]})
        )
        .is_err()
    );
    assert_eq!(
        native_reads(&f.state.lock().unwrap()).len(),
        native_reads(&state).len()
    );
    ok(&engine, 10, "quit", json!({}));
}
#[test]
fn r52_native_permission_refusal_keeps_original_success_without_data_or_fallback() {
    let f = configured("");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let read = ok(&engine, 2, "registers_read", json!({"ids":["sctlr"]}));
    assert_eq!(read["samples"][0]["state"], "valid");
    let original = read["samples"][0]["provenance"].clone();
    for (id, dscr, reason) in [
        ("sctlr", "0x01050113", "unknown"),
        ("hsctlr", "0x01050113", "access_restricted"),
        ("sctlr", "0x01058213", "access_restricted"),
    ] {
        f.state.lock().unwrap()["targets"]["cpu0"]["r52_dscr"] = json!(dscr);
        let denied = ok(&engine, 3, "registers_read", json!({"ids":[id]}));
        assert_eq!(denied["samples"][0]["reason"], reason, "{denied}");
        assert!(denied["samples"][0]["value"].is_null());
    }
    let status = ok(&engine, 4, "status", json!({}));
    let old = status["register_samples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "sctlr")
        .unwrap();
    assert_eq!(old["last_value_provenance"]["status"], "known");
    assert_eq!(old["last_value_provenance"]["provenance"], original);
    assert_eq!(status["state"], "STOPPED");
    let state = f.state.lock().unwrap().clone();
    assert_eq!(state["targets"]["cpu0"]["r52_data_reads"], 1);
    assert!(selector_writes(&state).is_empty());
    assert!(!state["trace"].to_string().contains("mrc"));
    ok(&engine, 5, "quit", json!({}));
}
#[test]
fn r52_native_fresh_capacity_refuses_changed_region_and_zero_hyp_mpu_without_selector_write() {
    let f = configured("");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let context = probe(&engine, 2);
    f.state.lock().unwrap()["targets"]["cpu0"]["el2_count"] = json!(16);
    let out = ok(
        &engine,
        4,
        "registers_read",
        json!({"context":context,"ids":["hprbar19"]}),
    );
    assert_eq!(
        out["samples"][0]["reason"], "hardware_not_implemented",
        "{out}"
    );
    f.state.lock().unwrap()["targets"]["cpu0"]["el2_count"] = json!(0);
    let out = ok(
        &engine,
        5,
        "registers_read",
        json!({"ids":["hprselr","hmair0"]}),
    );
    assert!(
        out["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["reason"] == "hardware_not_implemented" && s["value"].is_null()),
        "{out}"
    );
    let state = f.state.lock().unwrap().clone();
    assert!(selector_writes(&state).is_empty());
    assert_eq!(state["targets"]["cpu0"]["hprselr"], 2);
    ok(&engine, 6, "quit", json!({}));
}
#[test]
fn r52_native_bad_protocol_or_forged_response_never_falls_back_to_raw_mrc_or_gdb() {
    for fault in [
        "r52_protocol",
        "r52_short",
        "r52_bare",
        "r52_forged_el",
        "r52_forged_bank",
        "r52_forged_identity",
    ] {
        let f = configured(fault);
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let out = ok(&engine, 2, "registers_read", json!({"ids":["sctlr"]}));
        assert_eq!(
            out["samples"][0]["reason"], "reader_unsupported",
            "{fault}: {out}"
        );
        assert!(out["samples"][0]["value"].is_null());
        let state = f.state.lock().unwrap().clone();
        assert_eq!(state["current"], "outside");
        assert_eq!(
            native_reads(&state).len(),
            usize::from(fault != "r52_protocol")
        );
        assert!(!state["trace"].to_string().contains("mrc"));
        assert_eq!(ok(&engine, 3, "status", json!({}))["state"], "STOPPED");
        ok(&engine, 4, "quit", json!({}));
    }
}
#[test]
fn r52_native_context_change_and_cancellation_discard_values_after_transaction() {
    for fault in ["r52_context_change", ""] {
        let mut f = configured(fault);
        if !fault.is_empty() {
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
        let read = Request::new(2, "registers_read", json!({"ids":["sctlr","hcr"]}));
        if fault.is_empty() {
            *f.cancel_on_transaction.lock().unwrap() = Some(read.clone());
        }
        engine.send(read).unwrap();
        let response = loop {
            if let Event::Response {
                id: 2,
                ok,
                result,
                error,
            } = engine.events.recv_timeout(Duration::from_secs(15)).unwrap()
            {
                break (ok, result, error);
            }
        };
        if fault.is_empty() {
            assert!(!response.0);
            assert!(response.2.unwrap().contains("cancelled"));
        } else {
            assert!(response.0);
            assert!(
                response.1["samples"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|s| s["state"] != "valid")
            );
        }
        let state = f.state.lock().unwrap().clone();
        assert_eq!(native_reads(&state).len(), 1);
        assert_eq!(state["current"], "outside");
        assert_eq!(ok(&engine, 3, "status", json!({}))["state"], "STOPPED");
        ok(&engine, 4, "quit", json!({}));
    }
}
#[test]
fn r52_native_restoration_failure_quarantines_service_and_prevents_followup_reads() {
    let f = configured("r52_fault");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let out = ok(&engine, 2, "registers_read", json!({"ids":["sctlr","hcr"]}));
    assert!(
        out["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["state"] != "valid")
    );
    assert_eq!(ok(&engine, 3, "status", json!({}))["state"], "FAULT");
    assert!(request(&engine, 4, "registers_read", json!({"ids":["sctlr"]})).is_err());
    assert_eq!(native_reads(&f.state.lock().unwrap()).len(), 1);
    ok(&engine, 5, "quit", json!({}));
}
