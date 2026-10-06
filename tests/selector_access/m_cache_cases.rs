//! Production worker and actual Tcl interpreter. Independent raw PPB expectations.
use super::m_profile_mpu_cases::m_fixture;

#[test]
fn m7_cache_deferred_driver_runs_actual_binary_and_rejects_wrong_independent_baseline() {
    let f = m_fixture("cortex-m7", 16);
    let out = f.transcript.parent().unwrap();
    let project = out.join("m7-cache-driver.toml");
    fs::write(&project, toml::to_string(&f.project).unwrap()).unwrap();
    let original = fs::read(&project).unwrap();
    let spec = out.join("m7-cache-driver.json");
    fs::write(
        &spec,
        include_str!("../fixtures/m7-cache-board.example.json"),
    )
    .unwrap();
    let run = || {
        Command::new("node")
            .arg(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("scripts/test-m7-cache-hardware.cjs"),
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
            .arg(&spec)
            .output()
            .unwrap()
    };
    let result = run();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        String::from_utf8_lossy(&result.stdout).contains("\"passed\":5,\"failed\":0,\"skipped\":0")
    );
    let mut wrong: Value =
        serde_json::from_str(include_str!("../fixtures/m7-cache-board.example.json")).unwrap();
    wrong["caches"][0]["ccsidr"] = json!("0xf003e019");
    fs::write(&spec, serde_json::to_vec(&wrong).unwrap()).unwrap();
    assert!(
        !run().status.success(),
        "Wrong independent capacity must fail"
    );
    assert_eq!(fs::read(&project).unwrap(), original);
    assert_eq!(f.state.lock().unwrap()["targets"]["cpu0"]["c_sel"], 1);
}
use super::*;

fn read(engine: &session::EngineHandle, id: u64, context: &Value) -> Result<Value, String> {
    request(
        engine,
        id,
        "registers_cache",
        json!({"context":context,"read":true}),
    )
}
fn trace(f: &Fixture) -> Vec<Value> {
    f.state.lock().unwrap()["trace"].as_array().unwrap().clone()
}
fn arm_fault(f: &Fixture, fault: &str) {
    let mut state = f.state.lock().unwrap();
    state["fault"] = json!(fault);
    state["trace"] = json!([]);
}
fn selector_writes(f: &Fixture) -> Vec<Value> {
    trace(f)
        .into_iter()
        .filter(|v| v[1] == "write_memory")
        .collect()
}

#[test]
fn m7_cache_gdb_only_refuses_indexed_reads_before_data_io_and_reports_unsupported() {
    let mut f = m_fixture("cortex-m7", 16);
    f.project
        .registers
        .component_owners
        .get_mut("ppb")
        .unwrap()
        .get_mut("core:default")
        .unwrap()
        .channel
        .clear();
    let memory = f.transcript.with_extension("memory.json");
    let raw = json!({"0xe000ed00":"71c20f41","0xe000e004":"02000000","0xe000ed90":"00100000","0xe000edfc":"00000000","0xe0002000":"60000010","0xe000ed78":"03000009","0xe000ed7c":"03c00383"});
    fs::write(
        &memory,
        serde_json::to_vec(&std::collections::BTreeMap::from([(
            f.project.target.endpoint.clone(),
            raw,
        )]))
        .unwrap(),
    )
    .unwrap();
    f.project.gdb.env.insert(
        "DEBUGTUI_TEST_OWNER_MEMORY_FILE".into(),
        memory.to_string_lossy().into_owned(),
    );
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let context = probe(&engine, 2);
    let before = trace(&f);
    let mi_before = fs::read(&f.transcript).unwrap();
    let error = read(&engine, 4, &context).unwrap_err();
    assert!(error.contains("Tcl/AP channel"), "{error}");
    assert_eq!(trace(&f), before);
    assert_eq!(fs::read(&f.transcript).unwrap(), mi_before);
    let result = ok(
        &engine,
        5,
        "registers_read",
        json!({"context":context,"ids":["scb.ccsidr"],"manual":true}),
    );
    assert_eq!(result["samples"][0]["reason"], "reader_unsupported");
    assert_eq!(trace(&f), before);
    let delta = fs::read_to_string(&f.transcript).unwrap();
    let old = String::from_utf8(mi_before).unwrap();
    assert!(!delta[old.len()..].contains("-data-read-memory-bytes"));
    ok(&engine, 6, "quit", json!({}));
}

#[test]
fn m7_cache_unimplemented_current_bank_is_explicit_and_unknown_geometry_stays_raw() {
    for clidr in [0x09000002u32, 0x09000003] {
        let f = m_fixture("cortex-m7", 16);
        {
            let mut state = f.state.lock().unwrap();
            state["targets"]["cpu0"]["c_clidr"] = json!(clidr);
            state["targets"]["cpu0"]["c_data"] = json!(0xf003e011u32);
        }
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let context = probe(&engine, 2);
        let result = ok(
            &engine,
            4,
            "registers_read",
            json!({"context":context,"ids":["scb.ccsidr"],"manual":true}),
        );
        if clidr == 0x09000002 {
            assert_eq!(result["samples"][0]["reason"], "hardware_not_implemented");
            assert!(result["samples"][0]["value"].is_null());
        } else {
            assert_eq!(result["samples"][0]["value"]["hex"], "0xf007e009");
        }
        let view = read(&engine, 5, &context).unwrap()["view"].clone();
        assert_eq!(view["state"], "valid");
        assert_eq!(view["caches"][0]["size_id"]["value"]["hex"], "0xf003e011");
        assert_eq!(f.state.lock().unwrap()["targets"]["cpu0"]["c_sel"], 1);
        ok(&engine, 6, "quit", json!({}));
    }
}

#[test]
fn m7_cache_real_tcl_returns_distinct_implemented_banks_and_restores_without_enabling() {
    for (clidr, selectors) in [
        (0, vec![]),
        (0x09000001u32, vec![1]),
        (0x09000002, vec![0]),
        (0x09000003, vec![0, 1]),
    ] {
        let f = m_fixture("cortex-m7", 16);
        f.state.lock().unwrap()["targets"]["cpu0"]["c_clidr"] = json!(clidr);
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let context = probe(&engine, 2);
        arm_fault(&f, "");
        let view = read(&engine, 4, &context).unwrap()["view"].clone();
        assert_eq!(view["state"], "valid");
        assert_eq!(view["owner"], "core:default");
        let caches = view["caches"].as_array().unwrap();
        assert_eq!(caches.len(), selectors.len());
        for (cache, selector) in caches.iter().zip(&selectors) {
            assert_eq!(cache["selector"], *selector);
            assert_eq!(
                cache["kind"],
                if *selector == 0 {
                    "data"
                } else {
                    "instruction"
                }
            );
            let s = &cache["size_id"];
            assert_eq!(
                s["value"]["hex"],
                if *selector == 0 {
                    "0xf00fe019"
                } else {
                    "0xf007e009"
                }
            );
            assert_eq!(s["provenance"]["access"]["route"]["target"], "cpu0");
            assert_eq!(s["provenance"]["access"]["phase"], "responded");
            assert_eq!(s["provenance"]["access"]["context"], context);
            assert!(
                s["provenance"]["access"]["completed_ms"].as_u64().unwrap()
                    >= s["provenance"]["access"]["timestamp_ms"].as_u64().unwrap()
            );
        }
        let writes = selector_writes(&f);
        if selectors.is_empty() {
            assert!(view["original_selector"].is_null());
            assert!(view["restored_selector"].is_null());
            assert!(writes.is_empty());
            assert!(
                trace(&f)
                    .iter()
                    .all(|v| !matches!(v[2].as_str(), Some("0xe000ed84" | "0xe000ed80")))
            );
        } else {
            assert_eq!(view["original_selector"]["hex"], "0x00000001");
            assert_eq!(view["restored_selector"]["hex"], "0x00000001");
            assert_eq!(writes.len(), selectors.len() + 1);
            assert!(
                writes
                    .iter()
                    .all(|w| w[0] == "cpu0" && w[2] == "0xe000ed84" && w[3] == "32")
            );
        }
        assert_eq!(f.state.lock().unwrap()["targets"]["cpu0"]["c_sel"], 1);
        let before = trace(&f);
        assert_eq!(
            ok(&engine, 5, "registers_cache", json!({"context":context}))["view"],
            view
        );
        assert_eq!(trace(&f), before);
        ok(&engine, 6, "quit", json!({}));
    }
}

#[test]
fn m7_cache_selected_reader_delegates_and_tcm_configs_preserve_raw_while_wo_reads_send_nothing() {
    let f = m_fixture("cortex-m7", 16);
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let context = probe(&engine, 2);
    arm_fault(&f, "");
    let result = ok(
        &engine,
        4,
        "registers_read",
        json!({"context":context,"ids":["scb.ccsidr","scb.cacr","scb.itcmcr","scb.dtcmcr","scb.ahbpcr","scb.ahbscr"],"manual":true}),
    );
    for (sample, expected) in result["samples"].as_array().unwrap().iter().zip([
        "0xf007e009",
        "0x00000005",
        "0x00000033",
        "0x00000043",
        "0x00000001",
        "0x00ab1001",
    ]) {
        assert_eq!(sample["state"], "valid");
        assert_eq!(sample["value"]["hex"], expected);
    }
    assert_eq!(selector_writes(&f).len(), 3);
    assert!(
        trace(&f)
            .iter()
            .filter(|v| v[1] == "write_memory")
            .all(|v| v[2] == "0xe000ed84")
    );
    let before = trace(&f);
    let result = ok(
        &engine,
        5,
        "registers_read",
        json!({"context":context,"ids":["scb.iciallu","scb.icimvau","scb.dcimvac","scb.dcisw","scb.dccmvau","scb.dccmvac","scb.dccsw","scb.dccimvac","scb.dccisw"],"manual":true}),
    );
    assert!(
        result["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["state"] != "valid" && s["value"].is_null())
    );
    assert_eq!(trace(&f), before);
    ok(&engine, 6, "quit", json!({}));
}

#[test]
fn m7_cache_known_failure_restores_and_keeps_original_samples_stale() {
    for fault in [
        "c_select_after_write",
        "c_select_readback",
        "c_data_read",
        "c_final_identity",
    ] {
        let f = m_fixture("cortex-m7", 16);
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let context = probe(&engine, 2);
        let old = read(&engine, 4, &context).unwrap()["view"].clone();
        arm_fault(&f, fault);
        let error = read(&engine, 5, &context).unwrap_err();
        assert!(error.contains("known restoration"), "{fault}: {error}");
        assert_eq!(f.state.lock().unwrap()["targets"]["cpu0"]["c_sel"], 1);
        let status = ok(&engine, 6, "status", json!({}));
        assert_eq!(status["state"], "STOPPED");
        assert!(status["register_probe"].is_null());
        assert_eq!(status["register_cache"]["state"], "stale");
        for key in ["value", "timestamp_ms", "provenance"] {
            assert_eq!(
                status["register_cache"]["caches"][0]["size_id"][key],
                old["caches"][0]["size_id"][key]
            );
        }
        ok(&engine, 7, "quit", json!({}));
    }
}

#[test]
fn m7_cache_uncertain_restore_or_reply_quarantines_and_never_retries() {
    for fault in [
        "c_restore_write",
        "c_restore_readback",
        "c_disconnect_reply",
        "c_incomplete_reply",
    ] {
        let f = m_fixture("cortex-m7", 16);
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let context = probe(&engine, 2);
        arm_fault(&f, fault);
        assert!(
            read(&engine, 4, &context)
                .unwrap_err()
                .contains("outcome unknown")
        );
        let status = ok(&engine, 5, "status", json!({}));
        assert_eq!(status["state"], "FAULT");
        assert!(status["register_cache"].is_null());
        let before = trace(&f);
        assert!(read(&engine, 6, &context).is_err());
        assert_eq!(trace(&f), before);
        ok(&engine, 7, "quit", json!({}));
    }
}

#[test]
fn m7_cache_cancel_finishes_restoration_and_actual_context_change_discards_batch() {
    for cancel in [true, false] {
        let mut f = m_fixture("cortex-m7", 16);
        let context_file = f.transcript.with_extension("context.json");
        fs::write(&context_file, "{}").unwrap();
        f.project.gdb.env.insert(
            "DEBUGTUI_TEST_CONTEXT_FILE".into(),
            context_file.to_string_lossy().into_owned(),
        );
        f.state.lock().unwrap()["context_file"] = json!(context_file);
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let context = probe(&engine, 2);
        if cancel {
            let req = Request::new(4, "registers_cache", json!({"context":context,"read":true}));
            *f.cancel_on_transaction.lock().unwrap() = Some(req.clone());
            engine.send(req).unwrap();
            let mut done = false;
            while let Ok(event) = engine.events.recv_timeout(Duration::from_secs(10)) {
                if let Event::Response {
                    id: 4, ok, error, ..
                } = event
                {
                    assert!(!ok);
                    assert!(error.unwrap().contains("cancelled"));
                    done = true;
                    break;
                }
            }
            assert!(done);
        } else {
            arm_fault(&f, "c_context_change");
            assert!(
                read(&engine, 4, &context)
                    .unwrap_err()
                    .contains("context changed")
            );
        }
        assert_eq!(f.state.lock().unwrap()["targets"]["cpu0"]["c_sel"], 1);
        assert!(ok(&engine, 5, "status", json!({}))["register_cache"].is_null());
        ok(&engine, 6, "quit", json!({}));
    }
}

#[test]
fn m7_cache_new_invalid_clidr_revokes_old_bank_and_denies_new_data_io() {
    let f = m_fixture("cortex-m7", 16);
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let context = probe(&engine, 2);
    let old = read(&engine, 4, &context).unwrap()["view"].clone();
    f.state.lock().unwrap()["targets"]["cpu0"]["c_clidr"] = json!(0x09000004u32);
    ok(
        &engine,
        5,
        "registers_read",
        json!({"context":context,"ids":["scb.clidr"],"manual":true}),
    );
    let cached = ok(&engine, 6, "registers_cache", json!({"context":context}))["view"].clone();
    assert_eq!(cached["state"], "stale");
    assert_eq!(
        cached["caches"][0]["size_id"]["value"],
        old["caches"][0]["size_id"]["value"]
    );
    let before = trace(&f);
    assert!(read(&engine, 7, &context).is_err());
    assert_eq!(trace(&f), before);
    ok(&engine, 8, "quit", json!({}));
}

#[test]
fn m7_cache_running_frame_and_session_boundaries_keep_only_original_stale_samples() {
    for action in ["continue", "frame", "register_boundary"] {
        let f = m_fixture("cortex-m7", 16);
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let context = probe(&engine, 2);
        let old = read(&engine, 4, &context).unwrap()["view"].clone();
        ok(
            &engine,
            5,
            action,
            if action == "frame" {
                json!({"level":1})
            } else {
                json!({})
            },
        );
        let view = ok(&engine, 6, "status", json!({}))["register_cache"].clone();
        assert_eq!(view["state"], "stale");
        assert_eq!(view["caches"][0]["size_id"]["state"], "stale");
        for key in ["value", "timestamp_ms", "provenance", "context"] {
            assert_eq!(
                view["caches"][0]["size_id"][key],
                old["caches"][0]["size_id"][key]
            );
        }
        let before = trace(&f);
        assert!(read(&engine, 7, &context).is_err());
        assert_eq!(trace(&f), before);
        ok(&engine, 8, "quit", json!({}));
    }
}

#[test]
fn m7_cache_invalid_identity_config_selector_and_changed_definitions_fail_without_selector_writes()
{
    for kind in ["c_config_change", "c_invalid_original", "definition", "m4"] {
        let mut f = m_fixture(
            if kind == "m4" {
                "cortex-m4"
            } else {
                "cortex-m7"
            },
            8,
        );
        if kind == "definition" {
            let mut c = debugtui::registers::Catalogue::builtin("cortex-m7").unwrap();
            c.extends.clear();
            c.registers
                .iter_mut()
                .find(|r| r.id == "scb.ccsidr")
                .unwrap()
                .reader = debugtui::registers::Reader::CorePrivate {
                address: 0xe000ed88,
            };
            let file = f.transcript.with_extension("catalogue.toml");
            fs::write(&file, toml::to_string(&c).unwrap()).unwrap();
            f.project.registers.catalogue = file;
        }
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let context = probe(&engine, 2);
        arm_fault(&f, if kind.starts_with("c_") { kind } else { "" });
        assert!(read(&engine, 4, &context).is_err());
        assert!(selector_writes(&f).is_empty());
        assert!(ok(&engine, 5, "status", json!({}))["register_cache"].is_null());
        ok(&engine, 6, "quit", json!({}));
    }
}

#[test]
fn m7_cache_two_workers_keep_same_selector_address_values_and_owners_separate() {
    use debugtui::{
        config::MemoryAccess,
        coordinator,
        registers::{Component, CoreConfig},
    };
    use std::collections::BTreeMap;
    let mut f = m_fixture("cortex-m7", 16);
    let endpoint = f.project.memory_access[0].tcl_endpoint.clone();
    f.project.memory_access.clear();
    f.project.registers.targets = BTreeMap::from([
        ("core0".into(), "cpu0".into()),
        ("core2".into(), "cpu1".into()),
    ]);
    f.project.cores = [("core0", "cpu0", "ppb0"), ("core2", "cpu1", "ppb2")]
        .into_iter()
        .enumerate()
        .map(|(i, (name, target, channel))| {
            f.project.memory_access.push(MemoryAccess {
                id: channel.into(),
                label: channel.into(),
                tcl_endpoint: endpoint.clone(),
                target: target.into(),
                cores: vec![name.into()],
                while_running: false,
            });
            Core {
                name: name.into(),
                endpoint: format!("localhost:{}", 34000 + i),
                registers: Some(CoreConfig {
                    cpu: Some("cortex-m7".into()),
                    component_owners: Some(BTreeMap::from([(
                        "ppb".into(),
                        BTreeMap::from([(
                            format!("core:{name}"),
                            Component {
                                base: 0,
                                channel: channel.into(),
                                little_endian: true,
                            },
                        )]),
                    )])),
                    ..Default::default()
                }),
                ..Default::default()
            }
        })
        .collect();
    f.state.lock().unwrap()["targets"]["cpu1"] = json!({"m_cpuid":0x411fc271u32,"c_data":0xf03fe019u32,"c_instruction":0xf07fe009u32,"c_sel":0});
    let engine = coordinator::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    ok(&engine, 2, "select_core", json!({"name":"core0"}));
    ok(&engine, 3, "control_scope", json!({"scope":"all"}));
    let c0 = probe(&engine, 4);
    let first = read(&engine, 6, &c0).unwrap()["view"].clone();
    ok(&engine, 7, "select_core", json!({"name":"core2"}));
    assert!(ok(&engine, 8, "status", json!({}))["register_cache"].is_null());
    let c2 = probe(&engine, 9);
    let second = read(&engine, 11, &c2).unwrap()["view"].clone();
    assert_eq!(second["owner"], "core:core2");
    assert_eq!(second["caches"][0]["size_id"]["value"]["hex"], "0xf03fe019");
    assert_eq!(
        second["caches"][0]["size_id"]["provenance"]["access"]["route"]["target"],
        "cpu1"
    );
    assert_eq!(second["original_selector"]["hex"], "0x00000000");
    ok(&engine, 12, "select_core", json!({"name":"core0"}));
    let before = trace(&f);
    assert_eq!(
        ok(&engine, 13, "registers_cache", json!({"context":c0}))["view"],
        first
    );
    assert_eq!(trace(&f), before);
    assert_eq!(f.state.lock().unwrap()["targets"]["cpu0"]["c_sel"], 1);
    assert_eq!(f.state.lock().unwrap()["targets"]["cpu1"]["c_sel"], 0);
    ok(&engine, 14, "quit", json!({}));
}
