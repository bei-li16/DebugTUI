use super::*;
use debugtui::registers::{Catalogue, Reader, Sample, SampleView};

#[test]
fn running_deferred_driver_uses_actual_exe_and_rejects_wrong_independent_value() {
    let f = running_fixture("cortex-m4");
    let out = f.transcript.parent().unwrap();
    let project = out.join("running-driver.toml");
    fs::write(&project, toml::to_string(&f.project).unwrap()).unwrap();
    let original = fs::read(&project).unwrap();
    let mut expected: Value = serde_json::from_str(include_str!(
        "../fixtures/register-running-board.example.json"
    ))
    .unwrap();
    expected["endpoint"] = json!(f.project.memory_access[0].tcl_endpoint);
    let spec = out.join("running-driver.json");
    fs::write(&spec, serde_json::to_vec(&expected).unwrap()).unwrap();
    let run = || {
        Command::new("node")
            .arg(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("scripts/test-register-running-hardware.cjs"),
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
    fs::write(
        out.join("running-driver-positive.log"),
        [result.stdout.clone(), result.stderr.clone()].concat(),
    )
    .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        String::from_utf8_lossy(&result.stdout).contains("\"passed\":5,\"failed\":0,\"skipped\":0")
    );
    expected["values"]["scb.cpuid"] = json!("0x410fc231");
    fs::write(&spec, serde_json::to_vec(&expected).unwrap()).unwrap();
    let wrong = run();
    fs::write(
        out.join("running-driver-wrong.log"),
        [wrong.stdout.clone(), wrong.stderr.clone()].concat(),
    )
    .unwrap();
    assert!(!wrong.status.success(), "Wrong independent CPUID must fail");
    assert!(String::from_utf8_lossy(&wrong.stderr).contains("REG-RUN-H02-AP"));
    assert_eq!(fs::read(&project).unwrap(), original);
    assert!(trace(&f).iter().all(|r| r[1] == "read_memory"));
}

fn running_fixture(cpu: &str) -> Fixture {
    let mut f = m_profile_mpu_cases::m_fixture(cpu, 8);
    f.project.memory_access[0].while_running = true;
    f.project
        .gdb
        .env
        .insert("DEBUGTUI_TEST_PAUSE".into(), "query-rejected".into());
    f.state.lock().unwrap()["allow_running_memory"] = json!(true);
    f
}
fn start(f: &Fixture, engine: &session::EngineHandle) -> Value {
    ok(engine, 1, "connect", json!({}));
    ok(engine, 2, "continue", json!({}));
    assert_eq!(ok(engine, 3, "status", json!({}))["state"], "RUNNING");
    f.state.lock().unwrap()["targets"]["cpu0"]["status"] = json!("running");
    f.state.lock().unwrap()["trace"] = json!([]);
    fs::write(&f.transcript, "").unwrap();
    ok(engine, 4, "registers_list", json!({}))["context"].clone()
}
fn read(
    engine: &session::EngineHandle,
    id: u64,
    context: &Value,
    ids: &[&str],
    manual: bool,
) -> Value {
    ok(
        engine,
        id,
        "registers_read",
        json!({"context":context,"ids":ids,"manual":manual}),
    )
}
fn trace(f: &Fixture) -> Vec<Value> {
    f.state.lock().unwrap()["trace"].as_array().unwrap().clone()
}
fn valid(sample: &Value, context: &Value, hex: &str) {
    assert_eq!(sample["state"], "valid", "{sample}");
    assert_eq!(sample["value"]["hex"], hex);
    assert_eq!(sample["view"], "running_memory");
    let access = &sample["provenance"]["access"];
    assert_eq!(access["phase"], "responded");
    assert_eq!(access["route"]["kind"], "tcl_memory");
    assert_eq!(access["route"]["target"], "cpu0");
    assert_eq!(access["context"], *context);
    assert!(access["completed_ms"].as_u64().unwrap() >= access["timestamp_ms"].as_u64().unwrap());
    let model: Sample = serde_json::from_value(sample.clone()).unwrap();
    assert!(model.runtime_matches(false));
    assert!(!model.runtime_matches(true));
    assert_eq!(model.view, SampleView::RunningMemory);
}

#[test]
fn running_m_profiles_read_actual_ap_values_without_stopped_mi_queries_and_stale_on_pause() {
    for (cpu, hex) in [
        ("cortex-m3", "0x410fc231"),
        ("cortex-m4", "0x410fc241"),
        ("cortex-m7", "0x410fc271"),
    ] {
        let f = running_fixture(cpu);
        let engine = session::spawn(f.project.clone());
        let context = start(&f, &engine);
        let result = read(&engine, 5, &context, &["scb.cpuid", "scb.ccr", "r0"], true);
        valid(&result["samples"][0], &context, hex);
        valid(&result["samples"][1], &context, "0x00030000");
        assert_eq!(result["samples"][2]["state"], "unavailable");
        assert!(
            result["samples"][2]["detail"]
                .as_str()
                .unwrap()
                .contains("NeedHalt")
        );
        assert_eq!(trace(&f).len(), 2);
        assert!(trace(&f).iter().all(|r| r[1] == "read_memory"));
        assert!(fs::read_to_string(&f.transcript).unwrap().is_empty());
        assert!(request(&engine, 6, "registers_probe", json!({"context":context})).is_err());
        assert!(
            request(
                &engine,
                7,
                "registers_mpu",
                json!({"context":context,"read":true,"bank":"m"})
            )
            .is_err()
        );
        if cpu == "cortex-m7" {
            let denied = read(&engine, 8, &context, &["scb.ccsidr"], true);
            assert!(
                denied["samples"][0]["detail"]
                    .as_str()
                    .unwrap()
                    .contains("NeedHalt")
            );
            assert!(
                request(
                    &engine,
                    9,
                    "registers_cache",
                    json!({"context":context,"read":true})
                )
                .is_err()
            );
        }
        assert_eq!(trace(&f).len(), 2);
        assert_eq!(
            ok(&engine, 10, "status", json!({}))["register_samples"][0]["state"],
            "valid"
        );
        ok(&engine, 11, "pause", json!({}));
        let status = ok(&engine, 12, "status", json!({}));
        assert_eq!(status["state"], "STOPPED");
        let stored = status["register_samples"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "scb.cpuid")
            .unwrap();
        assert_eq!(stored["state"], "stale");
        for key in ["value", "timestamp_ms", "provenance"] {
            assert_eq!(stored[key], result["samples"][0][key]);
        }
        ok(&engine, 13, "quit", json!({}));
    }
}

#[test]
fn running_declared_metadata_cannot_authorize_stopped_missing_wrong_core_or_gdb_routes() {
    for kind in [
        "stopped-channel",
        "gdb",
        "missing",
        "wrong-core",
        "wrong-target",
    ] {
        let mut f = running_fixture("cortex-m4");
        match kind {
            "stopped-channel" => f.project.memory_access[0].while_running = false,
            "gdb" => f
                .project
                .registers
                .component_owners
                .get_mut("ppb")
                .unwrap()
                .get_mut("core:default")
                .unwrap()
                .channel
                .clear(),
            "missing" => f.project.memory_access.clear(),
            "wrong-core" => {
                f.project.cores = vec![
                    Core {
                        name: "default".into(),
                        endpoint: f.project.target.endpoint.clone(),
                        ..Default::default()
                    },
                    Core {
                        name: "other".into(),
                        endpoint: "localhost:36001".into(),
                        ..Default::default()
                    },
                ];
                f.project.memory_access[0].cores = vec!["other".into()];
            }
            "wrong-target" => f.project.memory_access[0].target = "cpu1".into(),
            _ => unreachable!(),
        }
        let engine = session::spawn(f.project.clone());
        let context = start(&f, &engine);
        let result = read(&engine, 5, &context, &["scb.cpuid"], true);
        let sample = &result["samples"][0];
        assert_ne!(sample["state"], "valid", "{kind}: {sample}");
        assert!(sample["value"].is_null());
        assert!(sample["provenance"]["access"].is_null());
        assert!(trace(&f).is_empty());
        assert!(fs::read_to_string(&f.transcript).unwrap().is_empty());
        ok(&engine, 6, "quit", json!({}));
    }
}

#[test]
fn running_read_side_effects_stay_manual_once_and_wo_cache_commands_never_execute() {
    let f = running_fixture("cortex-m7");
    let engine = session::spawn(f.project.clone());
    let context = start(&f, &engine);
    let ids = ["dcb.dhcsr", "systick.ctrl", "scb.iciallu"];
    let automatic = read(&engine, 5, &context, &ids, false);
    assert!(
        automatic["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["state"] != "valid")
    );
    assert!(trace(&f).is_empty());
    let manual = read(&engine, 6, &context, &ids, true);
    valid(&manual["samples"][0], &context, "0x01010001");
    valid(&manual["samples"][1], &context, "0x00010005");
    assert_eq!(manual["samples"][2]["reason"], "write_only");
    assert_eq!(trace(&f).len(), 2);
    assert!(trace(&f).iter().all(|r| r[1] == "read_memory"));
    ok(&engine, 7, "quit", json!({}));
}

#[test]
fn running_mmio64_and_aliases_share_actual_nonatomic_ap_interval_without_cp15_or_gdb_io() {
    let mut f = running_fixture("cortex-m4");
    let mut c = Catalogue::builtin("cortex-r52").unwrap();
    let mut memory = c.register("r0").unwrap().clone();
    memory.id = "memory64".into();
    memory.bits = 64;
    memory.fields.clear();
    memory.writer = None;
    memory.write = None;
    memory.reader = Reader::Mmio {
        component: "memory".into(),
        offset: 0,
        require_owner_mapping: false,
    };
    memory.access_rule.need_halt = Some(false);
    let mut alias = memory.clone();
    alias.id = "high".into();
    alias.bits = 32;
    alias.reader = Reader::Alias {
        source: "memory64".into(),
        offset: 32,
    };
    c.registers = vec![
        memory,
        alias,
        c.register("midr").unwrap().clone(),
        c.register("r0").unwrap().clone(),
    ];
    c.validate().unwrap();
    let file = f.transcript.with_extension("catalogue.toml");
    fs::write(&file, toml::to_string(&c).unwrap()).unwrap();
    f.project.registers.catalogue = file;
    f.project.registers.components.insert(
        "memory".into(),
        debugtui::registers::Component {
            base: 0x20000000,
            channel: "ppb0".into(),
            little_endian: true,
        },
    );
    f.state.lock().unwrap()["memory_words"] =
        json!({"536870912":0x01234567u32,"536870916":0x89abcdefu32});
    let engine = session::spawn(f.project.clone());
    let context = start(&f, &engine);
    let result = read(
        &engine,
        5,
        &context,
        &["memory64", "high", "midr", "r0"],
        true,
    );
    valid(&result["samples"][0], &context, "0x89abcdef01234567");
    valid(&result["samples"][1], &context, "0x89abcdef");
    for s in &result["samples"].as_array().unwrap()[2..] {
        assert!(s["detail"].as_str().unwrap().contains("NeedHalt"));
    }
    assert_eq!(trace(&f).len(), 1);
    assert_eq!(
        trace(&f)[0],
        json!(["cpu0", "read_memory", "0x20000000", "32", "2"])
    );
    assert_eq!(
        result["samples"][0]["provenance"]["access"]["route"]["atomic"],
        false
    );
    assert_eq!(
        result["samples"][0]["provenance"]["access"],
        result["samples"][1]["provenance"]["access"]
    );
    assert!(fs::read_to_string(&f.transcript).unwrap().is_empty());
    ok(&engine, 6, "quit", json!({}));
}

#[test]
fn running_transport_errors_keep_old_raw_origin_and_never_retry_or_publish_zero() {
    for fault in ["running_memory_error", "running_memory_incomplete"] {
        let f = running_fixture("cortex-m4");
        let engine = session::spawn(f.project.clone());
        let context = start(&f, &engine);
        let old = read(&engine, 5, &context, &["scb.cpuid"], true)["samples"][0].clone();
        f.state.lock().unwrap()["fault"] = json!(fault);
        let result = read(&engine, 6, &context, &["scb.cpuid"], true);
        let failed = &result["samples"][0];
        assert_eq!(failed["state"], "error");
        assert!(failed["value"].is_null());
        let status = ok(&engine, 7, "status", json!({}));
        let retained = status["register_samples"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "scb.cpuid")
            .unwrap();
        assert_eq!(retained["state"], "error");
        assert_eq!(retained["value"], old["value"]);
        assert_eq!(
            retained["last_value_provenance"]["provenance"],
            old["provenance"]
        );
        assert_eq!(trace(&f).len(), 2);
        assert!(fs::read_to_string(&f.transcript).unwrap().is_empty());
        ok(&engine, 8, "quit", json!({}));
    }
}

#[test]
fn running_inflight_stop_and_stop_run_burst_discard_response_and_stop_remaining_batch_io() {
    for notices in [
        json!(["stopped"]),
        json!(["stopped", "running"]),
        json!(["thread-selected"]),
        json!(["disconnect"]),
    ] {
        let mut f = running_fixture("cortex-m4");
        let notify = f.transcript.with_extension("notify.json");
        fs::write(&notify, "{}").unwrap();
        f.project.gdb.env.insert(
            "DEBUGTUI_TEST_NOTIFY_FILE".into(),
            notify.to_string_lossy().into_owned(),
        );
        {
            let mut s = f.state.lock().unwrap();
            s["notify_file"] = json!(notify);
            s["gdb_endpoint"] = json!(f.project.target.endpoint);
        }
        let engine = session::spawn(f.project.clone());
        let context = start(&f, &engine);
        let disconnected = notices == json!(["disconnect"]);
        f.state.lock().unwrap()["memory_notices"] = notices.clone();
        let result = read(&engine, 5, &context, &["scb.cpuid", "scb.ccr"], true);
        assert_eq!(trace(&f).len(), 1);
        assert!(
            result["samples"]
                .as_array()
                .unwrap()
                .iter()
                .all(|s| s["state"] != "valid" && s["value"].is_null())
        );
        assert_eq!(result["samples"].as_array().unwrap().len(), 1);
        // Stop injecting notices before cleanup; a persistent "running" notice
        // would deliberately resume the MI model after its cleanup interrupt.
        fs::write(&notify, "{}").unwrap();
        if disconnected {
            assert_eq!(ok(&engine, 6, "status", json!({}))["state"], "FAULT");
            let error = request(&engine, 7, "quit", json!({})).unwrap_err();
            assert!(error.contains("Disconnected with errors"), "{error}");
        } else {
            request(&engine, 6, "quit", json!({})).unwrap_or_else(|e| panic!("{notices}: {e}"));
        }
    }
}

#[test]
fn running_cancel_discards_completed_ap_reply_without_followup_batch_io() {
    let f = running_fixture("cortex-m4");
    let engine = session::spawn(f.project.clone());
    let context = start(&f, &engine);
    let req = Request::new(
        5,
        "registers_read",
        json!({"context":context,"ids":["scb.cpuid","scb.ccr"],"manual":true}),
    );
    *f.cancel_on_transaction.lock().unwrap() = Some(req.clone());
    engine.send(req).unwrap();
    loop {
        if let Event::Response {
            id: 5, ok, error, ..
        } = engine.events.recv_timeout(Duration::from_secs(15)).unwrap()
        {
            assert!(!ok);
            assert!(error.unwrap().contains("cancelled"));
            break;
        }
    }
    assert_eq!(trace(&f).len(), 1);
    assert!(
        ok(&engine, 6, "status", json!({}))["register_samples"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    ok(&engine, 7, "quit", json!({}));
}
