#![cfg(windows)]
//! Current-target evidence through the real worker/MI pipe. No board is used.
use debugtui::{
    config::{Core, Project},
    session::{self, Event, Request},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

static NEXT: AtomicU64 = AtomicU64::new(1);
const NAMES: &[&str] = &[
    "cpsr", "midr", "id_pfr1", "id_dfr0", "mpuir", "hmpuir", "cpacr", "pmcr", "icc_ctlr", "ich_vtr",
];

fn fixture(flags: &[(&str, &str)]) -> (Project, PathBuf) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
    let output =
        root.join("artifacts")
            .join(format!("capabilities-{}-{}", std::process::id(), sequence));
    fs::create_dir_all(&output).unwrap();
    let transcript = output.join("commands.txt");
    fs::write(&transcript, "").unwrap();
    let mut project = Project::default();
    project.gdb.executable = "node".into();
    project.gdb.args = vec![
        root.join("tests/mock-gdb.cjs")
            .to_string_lossy()
            .into_owned(),
    ];
    project
        .gdb
        .env
        .insert("DEBUGTUI_TEST_REGISTERS".into(), json!(NAMES).to_string());
    project.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTER_VALUES".into(),
        json!({
            "cpsr":"0x1a", "midr":"0x411fd134", "id_pfr1":"0x10111001", "id_dfr0":"0x03010066",
            "mpuir":"0x1800", "hmpuir":"0x14", "cpacr":"0xf00000", "pmcr":"0x41003000",
            "icc_ctlr":"0x400", "ich_vtr":"0xd4180003"
        })
        .to_string(),
    );
    project.gdb.env.insert(
        "DEBUGTUI_TEST_TRANSCRIPT".into(),
        transcript.to_string_lossy().into_owned(),
    );
    for &(key, value) in flags {
        project.gdb.env.insert(key.into(), value.into());
    }
    project.registers.catalogue = root.join("profiles/registers/cortex-r52.toml");
    project
        .registers
        .facts
        .insert("icc.physical.prebits".into(), 7);
    project.target.endpoint = format!("localhost:{}", 20000 + sequence);
    project.session.on_exit = "disconnect".into();
    (project, transcript)
}

fn request(
    engine: &session::EngineHandle,
    id: u64,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    engine.send(Request::new(id, method, params)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Event::Response {
            id: found,
            ok,
            result,
            error,
        } = engine
            .events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap()
            && found == id
        {
            return if ok {
                Ok(result)
            } else {
                Err(error.unwrap_or_default())
            };
        }
    }
}
fn ok(engine: &session::EngineHandle, id: u64, method: &str, params: Value) -> Value {
    request(engine, id, method, params).unwrap_or_else(|e| panic!("{method}: {e}"))
}
fn reads(path: &PathBuf) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|s| s.starts_with("-data-list-register-values r "))
        .map(str::to_owned)
        .collect()
}

#[test]
fn explicit_probe_decodes_evidence_filters_ap_registers_and_expires_at_next_stop() {
    let (project, transcript) = fixture(&[]);
    let engine = session::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    let listed = ok(&engine, 2, "registers_list", json!({}));
    assert!(
        reads(&transcript).is_empty(),
        "Connecting/listing must not probe"
    );
    let result = ok(
        &engine,
        3,
        "registers_probe",
        json!({"context":listed["context"]}),
    );
    assert_eq!(result["probe"]["identity"]["revision_name"], "r1p4");
    assert_eq!(result["probe"]["identity"]["model"], "Cortex-R52");
    assert_eq!(result["facts"]["mpu.el1.regions"], 24);
    assert_eq!(result["facts"]["mpu.el2.regions"], 20);
    assert_eq!(result["facts"]["icc.physical.prebits"], 5);
    assert_eq!(result["facts"]["icv.virtual.prebits"], 6);
    assert_eq!(result["facts"]["icv.virtual.pribits"], 7);
    assert_eq!(result["facts"]["pmu.counters"], 6);
    assert!(result["facts"]["vfp.present"].is_null());
    assert!(result["facts"]["vfp.enabled"].is_null());
    assert_eq!(
        result["probe"]["facts"]["icc.physical.prebits"]["source"],
        "gdb:icc_ctlr"
    );
    assert!(
        result["probe"]["notes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap().contains("observation wins"))
    );
    assert!(
        !result["probe"]["notes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap().contains("Actual MIDR is unreadable"))
    );
    for sample in result["probe"]["samples"].as_array().unwrap() {
        assert_eq!(sample["state"], "valid");
        assert_eq!(sample["context"], listed["context"]);
        assert_eq!(sample["owner"], "core:default");
    }
    assert_eq!(
        reads(&transcript),
        (0..10)
            .map(|i| format!("-data-list-register-values r {i}"))
            .collect::<Vec<_>>()
    );
    let read = ok(
        &engine,
        4,
        "registers_read",
        json!({"ids":["icc_ap0r1"], "context":listed["context"]}),
    );
    assert_eq!(read["samples"][0]["implementation"], "no");
    assert_eq!(read["samples"][0]["reason"], "hardware_not_implemented");
    assert_eq!(
        reads(&transcript).len(),
        10,
        "Physically absent AP entries must not be read"
    );
    let listed_after = ok(&engine, 5, "registers_list", json!({}));
    assert_eq!(listed_after["facts"]["icc.physical.prebits"], 5);
    let commands = fs::read_to_string(&transcript).unwrap();
    assert!(!commands.contains("-data-write-") && !commands.contains("-exec-"));
    assert!(!commands.contains("mcr ") && !commands.contains("FPEXC"));
    ok(&engine, 6, "step", json!({}));
    ok(&engine, 7, "wait_stopped", json!({}));
    let listed_after = ok(&engine, 8, "registers_list", json!({}));
    assert!(listed_after["probe"].is_null());
    assert_eq!(
        listed_after["facts"]["icc.physical.prebits"], 7,
        "Expired observation must not overwrite declared configuration"
    );
    assert_eq!(
        reads(&transcript).len(),
        10,
        "Next stop must not auto-probe"
    );
    assert!(
        request(
            &engine,
            9,
            "registers_probe",
            json!({"context":listed["context"]})
        )
        .is_err()
    );
    ok(&engine, 10, "quit", json!({}));
}

#[test]
fn inaccessible_identity_and_optional_features_remain_unknown_without_speculative_reads() {
    let (project, transcript) = fixture(&[(
        "DEBUGTUI_TEST_REGISTER_ERRORS",
        "[\"midr\",\"id_pfr1\",\"id_dfr0\",\"mpuir\"]",
    )]);
    let engine = session::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    let listed = ok(&engine, 2, "registers_list", json!({}));
    let result = ok(
        &engine,
        3,
        "registers_probe",
        json!({"context":listed["context"]}),
    );
    assert!(result["probe"]["identity"].is_null());
    for key in [
        "el2.present",
        "timer.present",
        "gic.system_interface",
        "pmu.present",
        "mpu.el1.regions",
        "icv.virtual.prebits",
    ] {
        assert!(result["probe"]["facts"][key].is_null(), "{key}");
    }
    for id in [
        "midr", "id_pfr1", "id_dfr0", "mpuir", "pmcr", "icc_ctlr", "ich_vtr",
    ] {
        let sample = result["probe"]["samples"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == id)
            .unwrap();
        assert_eq!(sample["state"], "unavailable");
        assert_eq!(sample["implementation"], "unknown");
        assert!(sample["value"].is_null());
    }
    let actual_reads = reads(&transcript);
    for index in [7, 8, 9] {
        assert!(!actual_reads.contains(&format!("-data-list-register-values r {index}")));
    }
    assert_eq!(
        actual_reads.len(),
        2,
        "Unidentified CPU must not probe optional R52 registers"
    );
    ok(&engine, 4, "quit", json!({}));
}

#[test]
fn unadapted_midr_preserves_raw_identity_and_never_runs_selected_catalogue_probes() {
    let (mut project, transcript) = fixture(&[]);
    let mut values: Value =
        serde_json::from_str(&project.gdb.env["DEBUGTUI_TEST_REGISTER_VALUES"]).unwrap();
    values["midr"] = json!("0x411fd164");
    project
        .gdb
        .env
        .insert("DEBUGTUI_TEST_REGISTER_VALUES".into(), values.to_string());
    let engine = session::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    let listed = ok(&engine, 2, "registers_list", json!({}));
    let result = ok(
        &engine,
        3,
        "registers_probe",
        json!({"context":listed["context"]}),
    );
    assert!(result["probe"]["identity"]["model"].is_null());
    assert_eq!(result["probe"]["samples"][1]["value"]["hex"], "0x411fd164");
    assert!(
        result["probe"]["samples"].as_array().unwrap()[2..]
            .iter()
            .all(|s| s["state"] == "unsupported" && s["implementation"] == "unknown")
    );
    assert_eq!(reads(&transcript).len(), 2);
    ok(&engine, 4, "quit", json!({}));
}

#[test]
fn unreadable_optional_id_fields_do_not_infer_absence_or_poll_dependents() {
    let (project, transcript) = fixture(&[(
        "DEBUGTUI_TEST_REGISTER_ERRORS",
        "[\"id_pfr1\",\"id_dfr0\",\"mpuir\"]",
    )]);
    let engine = session::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    let listed = ok(&engine, 2, "registers_list", json!({}));
    let result = ok(
        &engine,
        3,
        "registers_probe",
        json!({"context":listed["context"]}),
    );
    assert_eq!(result["probe"]["identity"]["model"], "Cortex-R52");
    for key in [
        "el2.present",
        "timer.present",
        "gic.system_interface",
        "pmu.present",
        "mpu.el1.regions",
    ] {
        assert!(result["probe"]["facts"][key].is_null(), "{key}");
    }
    let actual_reads = reads(&transcript);
    assert_eq!(actual_reads.len(), 7);
    for index in [7, 8, 9] {
        assert!(!actual_reads.contains(&format!("-data-list-register-values r {index}")));
    }
    ok(&engine, 4, "quit", json!({}));
}

#[test]
fn non_hyp_probe_never_reads_el2_or_attributes_virtual_icc_as_physical() {
    let (mut project, transcript) = fixture(&[]);
    let mut values: Value =
        serde_json::from_str(&project.gdb.env["DEBUGTUI_TEST_REGISTER_VALUES"]).unwrap();
    values["cpsr"] = json!("0x13");
    project
        .gdb
        .env
        .insert("DEBUGTUI_TEST_REGISTER_VALUES".into(), values.to_string());
    let engine = session::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    let listed = ok(&engine, 2, "registers_list", json!({}));
    let result = ok(
        &engine,
        3,
        "registers_probe",
        json!({"context":listed["context"]}),
    );
    for key in [
        "mpu.el2.regions",
        "icc.physical.prebits",
        "icv.virtual.prebits",
    ] {
        assert!(result["probe"]["facts"][key].is_null());
    }
    let actual_reads = reads(&transcript);
    for index in [5, 8, 9] {
        assert!(!actual_reads.contains(&format!("-data-list-register-values r {index}")));
    }
    assert_eq!(actual_reads.len(), 7);
    ok(&engine, 4, "frame", json!({"level":1}));
    let listed = ok(&engine, 5, "registers_list", json!({}));
    assert!(listed["probe"].is_null());
    assert!(
        request(
            &engine,
            6,
            "registers_probe",
            json!({"context":listed["context"]})
        )
        .unwrap_err()
        .contains("frame 0")
    );
    assert_eq!(reads(&transcript), actual_reads);
    ok(&engine, 7, "quit", json!({}));
}

#[test]
fn running_during_probe_discards_all_evidence_and_stops_the_batch() {
    let (project, transcript) = fixture(&[("DEBUGTUI_TEST_REGISTER_RUN_ON_READ", "1")]);
    let engine = session::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    let listed = ok(&engine, 2, "registers_list", json!({}));
    let error = request(
        &engine,
        3,
        "registers_probe",
        json!({"context":listed["context"]}),
    )
    .unwrap_err();
    assert!(error.contains("discarded"), "{error}");
    assert_eq!(reads(&transcript), ["-data-list-register-values r 0"]);
    let state = ok(&engine, 4, "status", json!({}));
    assert!(state["register_probe"].is_null());
    assert_eq!(state["state"], "RUNNING");
    ok(&engine, 5, "quit", json!({}));
}

#[test]
fn actual_gdb_thread_or_frame_change_discards_an_otherwise_complete_probe() {
    for flag in [
        "DEBUGTUI_TEST_CAPABILITY_THREAD_CHANGE",
        "DEBUGTUI_TEST_CAPABILITY_FRAME_CHANGE",
    ] {
        let (project, transcript) = fixture(&[(flag, "1")]);
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        let listed = ok(&engine, 2, "registers_list", json!({}));
        let error = request(
            &engine,
            3,
            "registers_probe",
            json!({"context":listed["context"]}),
        )
        .unwrap_err();
        assert!(error.contains("thread/frame changed"), "{error}");
        assert_eq!(reads(&transcript).len(), 10);
        assert!(ok(&engine, 4, "status", json!({}))["register_probe"].is_null());
        ok(&engine, 5, "quit", json!({}));
    }
}

#[test]
fn deferred_capability_driver_runs_actual_binary_and_preserves_fixture_without_board_claims() {
    let (mut project, transcript) = fixture(&[]);
    project.version = 2;
    let directory = transcript.parent().unwrap();
    let config = directory.join("project.toml");
    // This fixture independently declares named GDB readers for stable controls;
    // board profiles can instead use their verified target-specific MRC route.
    let mut catalogue = debugtui::registers::Catalogue::builtin("cortex-r52").unwrap();
    for id in ["cpacr", "pmcr"] {
        catalogue
            .registers
            .iter_mut()
            .find(|r| r.id == id)
            .unwrap()
            .reader = debugtui::registers::Reader::Gdb { name: id.into() };
    }
    project.registers.catalogue = directory.join("gdb-control-catalogue.toml");
    fs::write(
        &project.registers.catalogue,
        toml::to_string(&catalogue).unwrap(),
    )
    .unwrap();
    fs::write(&config, toml::to_string(&project).unwrap()).unwrap();
    let case_file = directory.join("capability-case.json");
    fs::write(&case_file, serde_json::to_string(&json!({
        "frame_function":"main", "stable_registers":["cpsr","cpacr","pmcr"],
        "expected_raw":{"midr":"0x411fd134"},
        "expected_facts":{"mpu.el1.regions":24,"mpu.el2.regions":20,"icc.physical.prebits":5,"icv.virtual.prebits":6},
        "expected_unavailable":[], "expected_unknown_facts":["vfp.present","vfp.enabled"]
    })).unwrap()).unwrap();
    let result = std::process::Command::new("node")
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("scripts/test-register-capabilities-hardware.cjs"),
        )
        .args([
            "--run",
            "--software-fixture",
            "--binary",
            env!("CARGO_BIN_EXE_debugtui"),
            "--project",
        ])
        .arg(&config)
        .args(["--core", "default", "--case"])
        .arg(case_file)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stdout.contains("\"passed\":5,\"failed\":0,\"skipped\":0"),
        "{stdout}"
    );
    let report_path = stdout
        .lines()
        .find_map(|line| {
            line.strip_prefix("RESULT ")
                .and_then(|line| line.split_once("} ").map(|(_, directory)| directory))
        })
        .unwrap();
    let report: Value = serde_json::from_str(
        &fs::read_to_string(PathBuf::from(report_path).join("report.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(report["board_tests_executed"], false);
    assert_eq!(report["tools"]["gdb"]["status"], "unknown");
    assert_eq!(reads(&transcript).len(), 16);
    assert_eq!(
        fs::read_to_string(&config).unwrap(),
        toml::to_string(&project).unwrap()
    );
}

#[test]
fn target_scoped_tcl_probe_uses_exact_mrcs_and_quarantines_failed_restoration() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{Arc, Mutex},
    };
    for restore_failure in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap().to_string();
        listener.set_nonblocking(true).unwrap();
        let scripts = Arc::new(Mutex::new(Vec::new()));
        let captured = scripts.clone();
        let encodings = [
            "15 0 0 0 0",
            "15 0 0 1 1",
            "15 0 0 1 2",
            "15 0 0 0 4",
            "15 4 0 0 4",
            "15 0 1 0 2",
            "15 0 9 12 0",
            "15 0 12 12 4",
            "15 4 12 11 1",
        ];
        let values = [
            "0x411fd134",
            "0x10111001",
            "0x03010066",
            "0x1800",
            "0x14",
            "0xf00000",
            "0x41003000",
            "0x400",
            "0xd4180003",
        ];
        let server = std::thread::spawn(move || {
            for index in 0..if restore_failure { 1 } else { 9 } {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(e)
                            if e.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            std::thread::park_timeout(Duration::from_millis(5))
                        }
                        Err(e) => panic!("TCL fixture accept: {e}"),
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut packet = Vec::new();
                let mut byte = [0];
                loop {
                    stream.read_exact(&mut byte).unwrap();
                    if byte[0] == 0x1a {
                        break;
                    }
                    packet.push(byte[0]);
                }
                let packet = String::from_utf8(packet).unwrap();
                let quoted = packet.strip_prefix("set __dt_code [catch \"").unwrap();
                let mut script = String::new();
                let mut chars = quoted.chars();
                while let Some(c) = chars.next() {
                    if c == '"' {
                        break;
                    }
                    script.push(if c == '\\' { chars.next().unwrap() } else { c });
                }
                assert!(script.starts_with("set __dt_old [target current];"));
                assert!(
                    script.contains("targets \"cpu0\"; if {[\"cpu0\" curstate] ne \"halted\"}")
                );
                assert!(script.contains("catch {targets $__dt_old}"));
                assert!(
                    script.contains(&format!("arm mrc {}", encodings[index])),
                    "{script}"
                );
                assert!(
                    !script.contains("mcr ")
                        && !script.contains("write_memory")
                        && !script.contains("resume")
                );
                captured.lock().unwrap().push(script);
                let response = if restore_failure {
                    "__DEBUGTUI_RPC__1:Target restoration failed: fixture refused".into()
                } else {
                    format!("__DEBUGTUI_RPC__0:{}", values[index])
                };
                stream.write_all(response.as_bytes()).unwrap();
                stream.write_all(&[0x1a]).unwrap();
            }
        });
        let (mut project, transcript) = fixture(&[]);
        project.gdb.env.insert(
            "DEBUGTUI_TEST_REGISTERS".into(),
            json!(["cpsr"]).to_string(),
        );
        project.registers.cp15_command = "arm mrc".into();
        project.registers.tcl_endpoint = endpoint;
        project
            .registers
            .targets
            .insert("default".into(), "cpu0".into());
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        let listed = ok(&engine, 2, "registers_list", json!({}));
        let result = request(
            &engine,
            3,
            "registers_probe",
            json!({"context":listed["context"]}),
        );
        if restore_failure {
            assert!(result.unwrap_err().contains("restoration failed"));
            let state = ok(&engine, 4, "status", json!({}));
            assert_eq!(state["state"], "FAULT");
            assert!(state["register_probe"].is_null());
            let before = fs::read_to_string(&transcript).unwrap();
            assert!(
                request(
                    &engine,
                    5,
                    "registers_probe",
                    json!({"context":listed["context"]})
                )
                .is_err()
            );
            assert_eq!(before, fs::read_to_string(&transcript).unwrap());
            assert_eq!(scripts.lock().unwrap().len(), 1);
        } else {
            let result = result.unwrap();
            assert_eq!(result["facts"]["icc.physical.prebits"], 5);
            assert_eq!(
                result["probe"]["facts"]["mpu.el1.regions"]["source"],
                "openocd:cp15"
            );
            assert_eq!(scripts.lock().unwrap().len(), 9);
            assert_eq!(reads(&transcript), ["-data-list-register-values r 0"]);
        }
        server.join().unwrap();
        let _ = request(&engine, 6, "quit", json!({}));
    }
}

#[test]
fn multicore_probe_uses_worker_generation_and_reads_one_owner_under_scope_all() {
    let (mut project, transcript) = fixture(&[]);
    project.cores = (0..2)
        .map(|i| Core {
            name: format!("core{i}"),
            endpoint: format!("localhost:{}", 3333 + i),
            ..Default::default()
        })
        .collect();
    let engine = debugtui::coordinator::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    ok(&engine, 2, "select_core", json!({"index":0}));
    ok(&engine, 3, "control_scope", json!({"scope":"all"}));
    let first = ok(&engine, 4, "registers_list", json!({}));
    let state = ok(&engine, 5, "status", json!({}));
    assert_eq!(state["register_generation"], first["context"]["generation"]);
    assert_ne!(
        state["register_generation"], state["generation"],
        "Fixture must expose the coordinator revision / worker stop generation difference"
    );
    let probe = ok(
        &engine,
        6,
        "registers_probe",
        json!({"context":first["context"]}),
    );
    assert_eq!(
        reads(&transcript).len(),
        10,
        "Scope All must not broadcast reads"
    );
    assert!(
        probe["probe"]["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["owner"] == "core:core0")
    );
    ok(&engine, 7, "select_core", json!({"index":1}));
    let second = ok(&engine, 8, "registers_list", json!({}));
    assert!(second["probe"].is_null());
    assert_ne!(second["context"]["session"], first["context"]["session"]);
    assert!(
        request(
            &engine,
            9,
            "registers_probe",
            json!({"context":first["context"]})
        )
        .is_err()
    );
    assert_eq!(reads(&transcript).len(), 10);
    let probe = ok(
        &engine,
        10,
        "registers_probe",
        json!({"context":second["context"]}),
    );
    assert!(
        probe["probe"]["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["owner"] == "core:core1")
    );
    assert_eq!(reads(&transcript).len(), 20);
    ok(&engine, 11, "quit", json!({}));
}
