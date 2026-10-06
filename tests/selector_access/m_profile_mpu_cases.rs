use super::*;
use debugtui::{config::MemoryAccess, registers::Component};

pub(super) fn m_fixture(cpu: &str, count: u8) -> Fixture {
    let mut f = fixture("");
    f.project.registers.cpu = cpu.into();
    f.project.registers.cp15_command.clear();
    f.project.registers.selector_command.clear();
    let endpoint = f.project.registers.tcl_endpoint.clone();
    f.project.memory_access = vec![MemoryAccess {
        id: "ppb0".into(),
        label: "M PPB".into(),
        tcl_endpoint: endpoint,
        target: "cpu0".into(),
        cores: vec!["default".into()],
        while_running: false,
    }];
    f.project.registers.component_owners.insert(
        "ppb".into(),
        std::collections::BTreeMap::from([(
            "core:default".into(),
            Component {
                base: 0,
                channel: "ppb0".into(),
                little_endian: true,
            },
        )]),
    );
    f.state.lock().unwrap()["targets"] = json!({"cpu0":{"m_count":count,"m_cpuid": match cpu {
        "cortex-m3" => 0x410fc231u32, "cortex-m4" => 0x410fc241u32, _ => 0x410fc271u32 }}});
    f.state.lock().unwrap()["trace"] = json!([]);
    f
}
fn read(engine: &session::EngineHandle, id: u64, context: &Value) -> Result<Value, String> {
    request(
        engine,
        id,
        "registers_mpu",
        json!({"context":context,"bank":"m","read":true}),
    )
}
fn trace(f: &Fixture) -> Vec<Value> {
    f.state.lock().unwrap()["trace"].as_array().unwrap().clone()
}
fn writes(f: &Fixture) -> Vec<Value> {
    trace(f)
        .into_iter()
        .filter(|v| v[1] == "write_memory")
        .collect()
}
fn arm_fault(f: &Fixture, fault: &str) {
    let mut state = f.state.lock().unwrap();
    state["fault"] = json!(fault);
    state["trace"] = json!([]);
    let cpu = &mut state["targets"]["cpu0"];
    cpu["m_pairs"] = json!(0);
    cpu["m_writes"] = json!(0);
    cpu["m_last_restore"] = json!(false);
    cpu["m_failed"] = json!(false);
}

#[test]
fn m_mpu_real_tcl_reads_all_adapted_regions_restores_rnr_and_preserves_configuration() {
    for (cpu, count) in [("cortex-m3", 8), ("cortex-m4", 8), ("cortex-m7", 16)] {
        let f = m_fixture(cpu, count);
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let context = probe(&engine, 2);
        arm_fault(&f, "");
        let result = read(&engine, 4, &context).unwrap();
        let view = &result["view"];
        assert_eq!(view["count"], count);
        assert_eq!(view["state"], "valid");
        assert_eq!(view["owner"], "core:default");
        assert_eq!(
            view["regions"].as_array().unwrap().len(),
            usize::from(count)
        );
        for index in 0..count {
            let region = &view["regions"][usize::from(index)];
            assert_eq!(region["index"], index);
            assert_eq!(
                region["base"]["value"]["hex"],
                format!("0x{:08x}", 0x20000000u32 + u32::from(index) * 0x10001)
            );
            assert_eq!(
                region["attributes"]["value"]["hex"],
                format!("0x{:08x}", 0x0307001fu32 + u32::from(index) * 0x100)
            );
            let access = &region["base"]["provenance"]["access"];
            assert_eq!(access["phase"], "responded");
            assert_eq!(access["context"], context);
            assert_eq!(access["route"]["target"], "cpu0");
            assert!(
                access["completed_ms"].as_u64().unwrap()
                    >= access["timestamp_ms"].as_u64().unwrap()
            );
        }
        assert_eq!(view["original_selector"]["hex"], "0x00000003");
        assert_eq!(view["restored_selector"]["hex"], "0x00000003");
        assert_eq!(f.state.lock().unwrap()["targets"]["cpu0"]["m_rnr"], 3);
        let writes = writes(&f);
        assert_eq!(writes.len(), usize::from(count) + 1);
        assert!(
            writes
                .iter()
                .all(|w| w[0] == "cpu0" && w[2] == "0xe000ed98" && w[3] == "32")
        );
        assert!(!trace(&f).iter().any(|v| v[0] == "targets"));
        let before = trace(&f);
        let cached = ok(
            &engine,
            5,
            "registers_mpu",
            json!({"context":context,"bank":"m"}),
        );
        assert_eq!(cached["view"], *view);
        assert_eq!(trace(&f), before, "cached view must produce zero Tcl I/O");
        ok(&engine, 6, "quit", json!({}));
    }
}

#[test]
fn m_mpu_known_failures_restore_selector_and_keep_old_samples_stale_without_partial_results() {
    for fault in [
        "m_select_after_write",
        "m_select_readback",
        "m_base_read",
        "m_rasr_read",
        "m_final_identity",
    ] {
        let f = m_fixture("cortex-m3", 8);
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let context = probe(&engine, 2);
        let old = read(&engine, 4, &context).unwrap()["view"].clone();
        arm_fault(&f, fault);
        let error = read(&engine, 5, &context).unwrap_err();
        assert!(error.contains("known restoration"), "{fault}: {error}");
        assert_eq!(
            f.state.lock().unwrap()["targets"]["cpu0"]["m_rnr"],
            3,
            "{fault}"
        );
        let status = ok(&engine, 6, "status", json!({}));
        assert_eq!(status["state"], "STOPPED");
        assert!(status["register_probe"].is_null());
        assert_eq!(status["register_mpu"]["state"], "stale");
        assert_eq!(
            status["register_mpu"]["regions"][0]["base"]["value"],
            old["regions"][0]["base"]["value"]
        );
        assert_eq!(
            status["register_mpu"]["regions"][0]["base"]["timestamp_ms"],
            old["regions"][0]["base"]["timestamp_ms"]
        );
        assert_eq!(
            status["register_mpu"]["regions"][0]["base"]["provenance"],
            old["regions"][0]["base"]["provenance"]
        );
        ok(&engine, 7, "quit", json!({}));
    }
}

#[test]
fn m_mpu_restore_failures_quarantine_service_and_never_retry_data_reads() {
    for fault in [
        "m_restore_write",
        "m_restore_readback",
        "m_incomplete_reply",
        "m_disconnect_reply",
    ] {
        let f = m_fixture("cortex-m3", 8);
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
        assert!(status["register_mpu"].is_null());
        let before = trace(&f);
        assert!(read(&engine, 6, &context).is_err());
        assert_eq!(trace(&f), before);
        ok(&engine, 7, "quit", json!({}));
    }
}

#[test]
fn m_mpu_invalid_capacity_or_original_selector_never_writes_a_selector() {
    for fault in ["m_capacity", "m_invalid_original"] {
        let f = m_fixture("cortex-m3", 8);
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let context = probe(&engine, 2);
        arm_fault(&f, fault);
        assert!(read(&engine, 4, &context).is_err());
        assert!(writes(&f).is_empty());
        assert!(ok(&engine, 5, "status", json!({}))["register_mpu"].is_null());
        ok(&engine, 6, "quit", json!({}));
    }
}

#[test]
fn m_mpu_cancel_completes_restoration_and_discards_entire_batch() {
    let f = m_fixture("cortex-m3", 8);
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let context = probe(&engine, 2);
    let request = Request::new(
        4,
        "registers_mpu",
        json!({"context":context,"bank":"m","read":true}),
    );
    *f.cancel_on_transaction.lock().unwrap() = Some(request.clone());
    engine.send(request).unwrap();
    let mut cancelled = false;
    while let Ok(event) = engine.events.recv_timeout(Duration::from_secs(10)) {
        if let Event::Response {
            id: 4, ok, error, ..
        } = event
        {
            assert!(!ok);
            assert!(error.unwrap().contains("cancelled"));
            cancelled = true;
            break;
        }
    }
    assert!(cancelled);
    assert_eq!(f.state.lock().unwrap()["targets"]["cpu0"]["m_rnr"], 3);
    assert!(ok(&engine, 5, "status", json!({}))["register_mpu"].is_null());
    ok(&engine, 6, "quit", json!({}));
}

#[test]
fn m_mpu_absent_module_reads_only_identity_and_type_without_fabricated_controls() {
    let f = m_fixture("cortex-m3", 0);
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let context = probe(&engine, 2);
    arm_fault(&f, "");
    let view = read(&engine, 4, &context).unwrap()["view"].clone();
    assert_eq!(view["count"], 0);
    assert!(view["control"].is_null());
    assert!(view["original_selector"].is_null());
    assert!(view["restored_selector"].is_null());
    assert!(view["regions"].as_array().unwrap().is_empty());
    assert!(writes(&f).is_empty());
    assert_eq!(trace(&f).len(), 4);
    assert!(
        trace(&f)
            .iter()
            .all(|v| v[2] == "0xe000ed00" || v[2] == "0xe000ed90")
    );
    ok(&engine, 5, "quit", json!({}));
}

#[test]
fn m_mpu_deferred_driver_checks_actual_binary_and_rejects_wrong_independent_baseline() {
    let f = m_fixture("cortex-m3", 8);
    let out = f.transcript.parent().unwrap();
    let project = out.join("m-mpu-driver.toml");
    fs::write(&project, toml::to_string(&f.project).unwrap()).unwrap();
    let original = fs::read(&project).unwrap();
    let spec = out.join("m-mpu-driver.json");
    fs::write(
        &spec,
        include_str!("../fixtures/m-profile-mpu-board.example.json"),
    )
    .unwrap();
    let run = || {
        Command::new("node")
            .arg(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("scripts/test-m-profile-mpu-hardware.cjs"),
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
        String::from_utf8_lossy(&result.stdout).contains("\"passed\":4,\"failed\":0,\"skipped\":0")
    );
    let mut wrong: Value =
        serde_json::from_str(include_str!("../fixtures/m-profile-mpu-board.example.json")).unwrap();
    wrong["regions"][0][0] = json!("0x40000000");
    fs::write(&spec, serde_json::to_vec(&wrong).unwrap()).unwrap();
    arm_fault(&f, "");
    assert!(
        !run().status.success(),
        "A wrong independent region baseline must fail"
    );
    assert_eq!(fs::read(&project).unwrap(), original);
    assert_eq!(f.state.lock().unwrap()["targets"]["cpu0"]["m_rnr"], 3);
}

#[test]
fn m_mpu_heterogeneous_workers_keep_same_ppb_address_and_selector_caches_separate() {
    use debugtui::{coordinator, registers::CoreConfig};
    use std::collections::BTreeMap;
    let mut f = m_fixture("cortex-m4", 8);
    let endpoint = f.project.memory_access[0].tcl_endpoint.clone();
    f.project.registers.targets = BTreeMap::from([
        ("core0".into(), "cpu0".into()),
        ("core2".into(), "cpu1".into()),
    ]);
    f.project.memory_access.clear();
    f.project.cores = [
        ("core0", "cortex-m4", "cpu0", "ppb0"),
        ("core2", "cortex-m7", "cpu1", "ppb2"),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (name, cpu, target, channel))| {
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
                cpu: Some(cpu.into()),
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
    f.state.lock().unwrap()["targets"]["cpu1"] = json!({"m_count":16,"m_cpuid":0x410fc271u32});
    let engine = coordinator::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    ok(&engine, 20, "select_core", json!({"name":"core0"}));
    ok(&engine, 2, "control_scope", json!({"scope":"all"}));
    let c0 = probe(&engine, 3);
    let first = read(&engine, 5, &c0).unwrap()["view"].clone();
    assert_eq!(first["count"], 8);
    ok(&engine, 6, "select_core", json!({"name":"core2"}));
    assert!(ok(&engine, 7, "status", json!({}))["register_mpu"].is_null());
    let c2 = probe(&engine, 8);
    let second = read(&engine, 10, &c2).unwrap()["view"].clone();
    assert_eq!(second["count"], 16);
    assert_eq!(second["owner"], "core:core2");
    assert_eq!(second["regions"][0]["base"]["value"]["hex"], "0x30000000");
    assert_eq!(
        second["regions"][0]["base"]["provenance"]["access"]["route"]["target"],
        "cpu1"
    );
    ok(&engine, 11, "select_core", json!({"name":"core0"}));
    let cached = ok(
        &engine,
        12,
        "registers_mpu",
        json!({"context":c0,"bank":"m"}),
    );
    assert_eq!(cached["view"], first);
    let state = f.state.lock().unwrap();
    assert_eq!(state["targets"]["cpu0"]["m_rnr"], 3);
    assert_eq!(state["targets"]["cpu1"]["m_rnr"], 3);
    drop(state);
    ok(&engine, 13, "quit", json!({}));
}

#[test]
fn m_mpu_late_batch_is_discarded_when_actual_thread_or_frame_changes() {
    let mut f = m_fixture("cortex-m3", 8);
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
    arm_fault(&f, "m_context_change");
    assert!(
        read(&engine, 4, &context)
            .unwrap_err()
            .contains("context changed")
    );
    let status = ok(&engine, 5, "status", json!({}));
    assert!(status["register_mpu"].is_null());
    assert!(status["register_probe"].is_null());
    assert_eq!(f.state.lock().unwrap()["targets"]["cpu0"]["m_rnr"], 3);
    ok(&engine, 6, "quit", json!({}));
}

#[test]
fn m_mpu_rejects_modified_or_unbound_definitions_before_any_target_command() {
    for kind in ["side_effect", "permission", "gdb_only"] {
        let mut f = m_fixture("cortex-m3", 8);
        if kind == "gdb_only" {
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
            let raw = json!({"0xe000ed00":"31c20f41","0xe000e004":"02000000","0xe000ed90":"00080000","0xe000edfc":"00000000","0xe0002000":"60000010"});
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
        } else {
            let mut catalogue = debugtui::registers::Catalogue::builtin("cortex-m3").unwrap();
            catalogue.extends.clear();
            let register = catalogue
                .registers
                .iter_mut()
                .find(|r| r.id == "mpu.rasr")
                .unwrap();
            if kind == "side_effect" {
                register.read_side_effect = true;
            } else {
                register.access_rule.min_el = Some(3);
            }
            let path = f.transcript.with_extension("catalogue.toml");
            fs::write(&path, toml::to_string(&catalogue).unwrap()).unwrap();
            f.project.registers.catalogue = path;
        }
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        probe(&engine, 2);
        let context = ok(&engine, 4, "registers_list", json!({}))["context"].clone();
        let before = trace(&f);
        let mi_before = fs::read(&f.transcript).unwrap();
        let error = read(&engine, 5, &context).unwrap_err();
        assert!(
            error.contains(if kind == "gdb_only" {
                "Tcl/AP channel"
            } else {
                "mpu.rasr"
            }),
            "{kind}: {error}"
        );
        assert_eq!(trace(&f), before);
        assert_eq!(fs::read(&f.transcript).unwrap(), mi_before);
        ok(&engine, 6, "quit", json!({}));
    }
}

#[test]
fn m_mpu_new_invalid_type_revokes_old_bank_validity_and_denies_new_region_io() {
    let f = m_fixture("cortex-m3", 8);
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let context = probe(&engine, 2);
    let old = read(&engine, 4, &context).unwrap()["view"].clone();
    arm_fault(&f, "m_capacity");
    ok(
        &engine,
        5,
        "registers_read",
        json!({"context":context,"ids":["mpu.type"],"manual":true}),
    );
    let cached = ok(
        &engine,
        6,
        "registers_mpu",
        json!({"context":context,"bank":"m"}),
    )["view"]
        .clone();
    assert_eq!(cached["state"], "stale");
    assert_eq!(
        cached["regions"][0]["base"]["value"],
        old["regions"][0]["base"]["value"]
    );
    let before = trace(&f);
    assert!(read(&engine, 7, &context).is_err());
    assert_eq!(trace(&f), before);
    ok(&engine, 8, "quit", json!({}));
}
