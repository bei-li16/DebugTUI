#![cfg(windows)]
//! Real worker/TCP pipe, with transaction control flow executed by Tcl 8.6.
//! Uses Python's standard tkinter Tcl interpreter; never connects a board.
use debugtui::{
    config::{Core, Project},
    session::{self, Event, Request},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
static NEXT: AtomicU64 = AtomicU64::new(1);
#[path = "selector_access/banked_cases.rs"]
mod banked_cases;
#[path = "selector_access/cancel_cases.rs"]
mod cancel_cases;
#[path = "selector_access/gic_cases.rs"]
mod gic_cases;
#[path = "selector_access/m_cache_cases.rs"]
mod m_cache_cases;
#[path = "selector_access/m_multicore_cases.rs"]
mod m_multicore_cases;
#[path = "selector_access/m_profile_mpu_cases.rs"]
mod m_profile_mpu_cases;
#[path = "selector_access/matrix_cases.rs"]
mod matrix_cases;
#[path = "selector_access/mpu_cases.rs"]
mod mpu_cases;
#[path = "selector_access/mrrc_cases.rs"]
mod mrrc_cases;
#[path = "selector_access/pmu_cases.rs"]
mod pmu_cases;
#[path = "selector_access/running_cases.rs"]
mod running_cases;
#[path = "selector_access/timer_adapter_cases.rs"]
mod timer_adapter_cases;
#[path = "selector_access/timer_cases.rs"]
mod timer_cases;
#[path = "selector_access/vfp_cases.rs"]
mod vfp_cases;
#[path = "selector_access/vfp_write_cases.rs"]
mod vfp_write_cases;
struct Fixture {
    project: Project,
    transcript: PathBuf,
    state: Arc<Mutex<Value>>,
    cancel_on_transaction: Arc<Mutex<Option<Request>>>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take()
            && let Err(error) = worker.join()
            && !std::thread::panicking()
        {
            std::panic::resume_unwind(error);
        }
    }
}
fn fixture(fault: &'static str) -> Fixture {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::var_os("DEBUGTUI_TEST_ARTIFACT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("artifacts"))
        .join(format!(
            "selector-{}-{sequence}-{nonce:x}",
            std::process::id()
        ));
    fs::create_dir_all(directory.parent().unwrap()).unwrap();
    // Windows reuses process IDs; an old fixture must never supply this run's raw inputs.
    fs::create_dir(&directory).unwrap();
    let transcript = directory.join("commands.txt");
    fs::write(&transcript, "").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap().to_string();
    listener.set_nonblocking(true).unwrap();
    let state = Arc::new(Mutex::new(json!({})));
    let captured = state.clone();
    let cancel_on_transaction = Arc::new(Mutex::new(None::<Request>));
    let cancelling = cancel_on_transaction.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    // Start one interpreter before clients connect. Importing tkinter per MRC
    // can exceed the real 500 ms TCP deadline under parallel test load.
    let mut command =
        Command::new(std::env::var("DEBUGTUI_TEST_PYTHON").unwrap_or_else(|_| "python".into()));
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000);
    let mut process = command
        .arg(root.join("scripts/test-support/selector-tcl.py"))
        .arg("--jsonl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(fs::File::create(directory.join("tcl-stderr.txt")).unwrap())
        .spawn()
        .unwrap();
    let mut tcl_input = process.stdin.take().unwrap();
    let mut tcl_output = BufReader::new(process.stdout.take().unwrap());
    let mut ready = String::new();
    tcl_output.read_line(&mut ready).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&ready).unwrap()["ready"],
        true
    );
    let worker = std::thread::spawn(move || {
        // Several workers and the bank transaction may keep separate connections
        // open. Poll frames, then execute them serially in the one Tcl interpreter.
        let mut connections: Vec<(std::net::TcpStream, Vec<u8>)> = vec![];
        while !stopping.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(true).unwrap();
                    connections.push((stream, vec![]));
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => panic!("TCL fixture accept: {e}"),
            }
            let mut complete = None;
            for index in (0..connections.len()).rev() {
                let mut byte = [0];
                let mut closed = false;
                let (stream, packet) = &mut connections[index];
                loop {
                    match stream.read(&mut byte) {
                        Ok(0) => {
                            closed = true;
                            break;
                        }
                        Ok(_) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(error)
                            if matches!(
                                error.kind(),
                                std::io::ErrorKind::ConnectionReset
                                    | std::io::ErrorKind::ConnectionAborted
                            ) =>
                        {
                            closed = true;
                            break;
                        }
                        Err(error) => panic!("TCL fixture read: {error}"),
                    }
                    if byte[0] == 0x1a {
                        complete = Some((index, std::mem::take(packet)));
                        break;
                    }
                    packet.push(byte[0]);
                }
                if closed {
                    connections.remove(index);
                }
                if complete.is_some() {
                    break;
                }
            }
            let Some((index, packet)) = complete else {
                std::thread::park_timeout(Duration::from_millis(2));
                continue;
            };
            let stream = &mut connections[index].0;
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
            let current = captured.lock().unwrap().clone();
            let active_fault = current
                .get("fault")
                .and_then(Value::as_str)
                .unwrap_or(fault);
            let input = json!({"script":script,"state":current,"fault":active_fault});
            // Cancellation arrives after the worker submitted its atomic Tcl
            // request. Execute the entire real Tcl flow, including restoration.
            {
                let mut pending = cancelling.lock().unwrap();
                if pending
                    .as_ref()
                    .is_some_and(|request| match request.method.as_str() {
                        "registers_select" => script.contains("arm mcr"),
                        "registers_mpu" | "registers_cache" => {
                            script.contains("arm mrc") || script.contains("write_memory")
                        }
                        "registers_read" => {
                            script.contains("aarch64 vfp")
                                || script.contains("aarch64 banked")
                                || script.contains("aarch64 timer")
                                || script.contains("aarch64 pmu")
                                || script.contains("aarch64 gic")
                                || (current["allow_running_memory"] == true
                                    && script.contains("read_memory"))
                        }
                        _ => false,
                    })
                {
                    pending.take().unwrap().cancel_read();
                }
            }
            writeln!(tcl_input, "{input}").unwrap();
            tcl_input.flush().unwrap();
            let mut response = String::new();
            tcl_output.read_line(&mut response).unwrap();
            let result: Value = serde_json::from_str(&response).unwrap();
            *captured.lock().unwrap() = result["state"].clone();
            if matches!(active_fault, "m_disconnect_reply" | "c_disconnect_reply")
                && script.contains("write_memory")
            {
                connections.remove(index);
                continue;
            }
            let code = if result["ok"] == true { 0 } else { 1 };
            let response = format!(
                "__DEBUGTUI_RPC__{code}:{}",
                if matches!(active_fault, "m_incomplete_reply" | "c_incomplete_reply")
                    && script.contains("write_memory")
                {
                    "410fc231 00000800"
                } else {
                    result["value"].as_str().unwrap()
                }
            );
            fs::write(
                directory.join("last-tcl.json"),
                serde_json::to_vec_pretty(&json!({"input":input,"output":result})).unwrap(),
            )
            .unwrap();
            stream.set_nonblocking(false).unwrap();
            stream
                .set_write_timeout(Some(Duration::from_millis(500)))
                .unwrap();
            if stream
                .write_all(response.as_bytes())
                .and_then(|_| stream.write_all(&[0x1a]))
                .is_err()
            {
                connections.remove(index);
            } else {
                stream.set_nonblocking(true).unwrap();
            }
        }
        drop(tcl_input);
        assert!(process.wait().unwrap().success());
    });
    let mut project = Project::default();
    project.gdb.executable = "node".into();
    project.gdb.args = vec![
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/mock-gdb.cjs")
            .to_string_lossy()
            .into_owned(),
    ];
    project.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTERS".into(),
        json!([
            "cpsr", "midr", "id_pfr1", "id_dfr0", "mpuir", "hmpuir", "cpacr", "pmcr", "icc_ctlr",
            "ich_vtr"
        ])
        .to_string(),
    );
    project.gdb.env.insert("DEBUGTUI_TEST_REGISTER_VALUES".into(),json!({"cpsr":"0x1a","midr":"0x411fd134","id_pfr1":"0x10111001","id_dfr0":"0x03010066","mpuir":"0x1800","hmpuir":"0x14","cpacr":"0xf00000","pmcr":"0x41132000","icc_ctlr":"0x400","ich_vtr":"0x90180003"}).to_string());
    project.gdb.env.insert(
        "DEBUGTUI_TEST_TRANSCRIPT".into(),
        transcript.to_string_lossy().into_owned(),
    );
    project.registers.cpu = "cortex-r52".into();
    project.registers.cp15_command = "arm mrc".into();
    project.registers.selector_command = "arm mcr".into();
    project.registers.tcl_endpoint = endpoint;
    project
        .registers
        .targets
        .insert("default".into(), "cpu0".into());
    project.target.endpoint = format!("localhost:{}", 24000 + sequence);
    project.session.on_exit = "disconnect".into();
    Fixture {
        project,
        transcript,
        state,
        cancel_on_transaction,
        stop,
        worker: Some(worker),
    }
}
fn request(
    engine: &session::EngineHandle,
    id: u64,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    engine.send(Request::new(id, method, params)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
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
fn probe(engine: &session::EngineHandle, id: u64) -> Value {
    let listed = ok(engine, id, "registers_list", json!({}));
    ok(
        engine,
        id + 1,
        "registers_probe",
        json!({"context":listed["context"]}),
    )["context"]
        .clone()
}
fn selector_writes(state: &Value) -> Vec<&Value> {
    state["trace"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry[1] == "mcr" && entry[4] != "7")
        .collect()
}

#[test]
fn real_tcl_selector_transactions_read_pairs_and_restore_each_selector_and_target() {
    for (kind, index, selector, old) in [
        ("mpu_el1", 23, "prselr", 1),
        ("mpu_el2", 19, "hprselr", 2),
        ("pmu", 3, "pmselr", 31),
    ] {
        let mut fixture = fixture("");
        if kind == "pmu" {
            fixture.project.registers.pmu_command = "aarch64 pmu".into();
        }
        let engine = session::spawn(fixture.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let context = probe(&engine, 2);
        let result = ok(
            &engine,
            4,
            "registers_select",
            json!({"context":context,"kind":kind,"index":index}),
        );
        assert_eq!(result["target"], "cpu0");
        assert_eq!(result["evidence"]["selected"], index);
        assert_eq!(
            result["evidence"]["original"],
            result["evidence"]["restored"]
        );
        assert_eq!(result["samples"].as_array().unwrap().len(), 2);
        for sample in result["samples"].as_array().unwrap() {
            let provenance = &sample["provenance"];
            assert_eq!(provenance["acquisition"], "selector_bank");
            let access = &provenance["access"];
            assert_eq!(access["route"]["target"], "cpu0");
            assert_eq!(
                access["route"]["endpoint"],
                fixture.project.registers.tcl_endpoint
            );
            assert_eq!(access["phase"], "responded");
            assert_eq!(access["context"], context);
            assert!(
                access["route"]["operation"]
                    .as_str()
                    .unwrap()
                    .contains(&format!("index {index}"))
            );
        }
        assert!(
            result["samples"]
                .as_array()
                .unwrap()
                .iter()
                .all(|s| s["state"] == "valid"
                    && s["owner"] == "core:default"
                    && s["context"] == context)
        );
        if kind == "mpu_el1" {
            assert_eq!(result["region"]["base"], "0x20170000");
            assert_eq!(result["region"]["limit_inclusive"], "0x2017ffff");
        }
        if kind == "pmu" {
            assert_eq!(result["samples"][1]["value"]["hex"], "0xf123ab03");
        }
        let state = fixture.state.lock().unwrap().clone();
        assert_eq!(state["current"], "outside");
        assert_eq!(state["targets"]["cpu0"][selector], old);
        assert_eq!(selector_writes(&state).len(), 2);
        assert!(!state["trace"].to_string().contains("resume"));
        assert_eq!(
            ok(&engine, 5, "registers_list", json!({}))["context"],
            context
        );
        ok(&engine, 6, "quit", json!({}));
    }
}

#[test]
fn selector_preconditions_reject_before_mcr_and_restored_errors_remain_recoverable() {
    for fault in [
        "no_sync",
        "count_changed",
        "invalid_original",
        "data_error",
        "select_error",
        "select_barrier",
    ] {
        let fixture = fixture(fault);
        let engine = session::spawn(fixture.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let context = probe(&engine, 2);
        let error = request(
            &engine,
            4,
            "registers_select",
            json!({"context":context,"kind":"mpu_el1","index":23}),
        )
        .unwrap_err();
        assert!(error.contains("Selector read"), "{fault}: {error}");
        let state = fixture.state.lock().unwrap().clone();
        assert_eq!(state["current"], "outside");
        assert_eq!(state["targets"]["cpu0"]["prselr"], 1);
        assert_eq!(
            selector_writes(&state).len(),
            if matches!(fault, "no_sync" | "count_changed" | "invalid_original") {
                0
            } else {
                2
            }
        );
        let status = ok(&engine, 5, "status", json!({}));
        assert_eq!(status["state"], "STOPPED");
        assert_eq!(status["register_probe"].is_null(), fault == "count_changed");
        for id in ["prbar23", "prlar23"] {
            let sample = status["register_samples"]
                .as_array()
                .unwrap()
                .iter()
                .find(|sample| sample["id"] == id)
                .unwrap();
            assert_eq!(sample["state"], "unavailable");
            assert_ne!(sample["reason"], "hardware_not_implemented");
        }
        assert!(
            ok(
                &engine,
                6,
                "registers_read",
                json!({"ids":["cpsr"],"context":context})
            )["samples"][0]["state"]
                == "valid"
        );
        ok(&engine, 7, "quit", json!({}));
    }
}

#[test]
fn selector_restoration_failures_quarantine_without_retry_and_invalidate_capability_cache() {
    for fault in [
        "restore_error",
        "restore_mismatch",
        "restore_barrier",
        "target_restore",
        "target_mismatch",
    ] {
        let fixture = fixture(fault);
        let engine = session::spawn(fixture.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let context = probe(&engine, 2);
        let error = request(
            &engine,
            4,
            "registers_select",
            json!({"context":context,"kind":"mpu_el1","index":23}),
        )
        .unwrap_err();
        assert!(error.contains("outcome unknown"), "{fault}: {error}");
        let status = ok(&engine, 5, "status", json!({}));
        assert_eq!(status["state"], "FAULT");
        assert!(status["register_probe"].is_null());
        let before = fixture.state.lock().unwrap().clone();
        let commands = fs::read_to_string(&fixture.transcript).unwrap();
        assert!(
            request(
                &engine,
                6,
                "registers_select",
                json!({"context":context,"kind":"mpu_el1","index":23})
            )
            .is_err()
        );
        assert_eq!(*fixture.state.lock().unwrap(), before);
        assert_eq!(fs::read_to_string(&fixture.transcript).unwrap(), commands);
        let _ = request(&engine, 7, "quit", json!({}));
    }
}

#[test]
fn a_failed_selector_refresh_retains_only_unavailable_last_known_values() {
    let fixture = fixture("");
    let engine = session::spawn(fixture.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let context = probe(&engine, 2);
    let params = json!({"context":context,"kind":"mpu_el1","index":23});
    let first = ok(&engine, 4, "registers_select", params.clone());
    fixture.state.lock().unwrap()["fault"] = json!("data_error");
    assert!(
        request(&engine, 5, "registers_select", params)
            .unwrap_err()
            .contains("data read failed")
    );
    let status = ok(&engine, 6, "status", json!({}));
    for old in first["samples"].as_array().unwrap() {
        let sample = status["register_samples"]
            .as_array()
            .unwrap()
            .iter()
            .find(|sample| sample["id"] == old["id"])
            .unwrap();
        assert_eq!(sample["value"], old["value"]);
        assert_eq!(sample["state"], "unavailable");
        assert!(
            sample["detail"]
                .as_str()
                .unwrap()
                .contains("last sample at")
        );
    }
    assert_eq!(status["state"], "STOPPED");
    assert_eq!(
        fixture.state.lock().unwrap()["targets"]["cpu0"]["prselr"],
        1
    );
    ok(&engine, 7, "quit", json!({}));
}

#[test]
fn deferred_selector_driver_runs_against_the_actual_binary_and_real_tcl_fixture() {
    let fixture = fixture("");
    let directory = fixture.transcript.parent().unwrap();
    let project_path = directory.join("selector-driver.toml");
    let mut project = fixture.project.clone();
    project.registers.pmu_command = "aarch64 pmu".into();
    project.version = 1;
    project.cores = (0..2)
        .map(|i| Core {
            name: format!("core{i}"),
            endpoint: format!("localhost:{}", 25000 + i),
            ..Default::default()
        })
        .collect();
    project.registers.targets.remove("default");
    for i in 0..2 {
        project
            .registers
            .targets
            .insert(format!("core{i}"), format!("cpu{i}"));
    }
    fs::write(&project_path, toml::to_string(&project).unwrap()).unwrap();
    let original = fs::read(&project_path).unwrap();
    let case_path = directory.join("selector-driver.json");
    let mut spec: Value = serde_json::from_str(include_str!(
        "fixtures/register-selectors-board.example.json"
    ))
    .unwrap();
    spec["frame_function"] = json!("main");
    fs::write(&case_path, serde_json::to_vec_pretty(&spec).unwrap()).unwrap();
    let output = Command::new("node")
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("scripts/test-register-selectors-hardware.cjs"),
        )
        .args(["--run", "--software-fixture", "--core", "core0", "--binary"])
        .arg(env!("CARGO_BIN_EXE_debugtui"))
        .arg("--project")
        .arg(&project_path)
        .arg("--case")
        .arg(&case_path)
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
        stdout.contains("\"passed\":5,\"failed\":0,\"skipped\":0"),
        "{stdout}"
    );
    assert_eq!(fs::read(project_path).unwrap(), original);
    let state = fixture.state.lock().unwrap();
    assert_eq!(state["current"], "outside");
    assert_eq!(state["targets"]["cpu0"]["prselr"], 1);
    assert_eq!(state["targets"]["cpu0"]["hprselr"], 2);
    assert_eq!(state["targets"]["cpu0"]["pmselr"], 31);
    assert_eq!(state["targets"]["cpu1"]["prselr"], 1);
    assert_eq!(state["targets"]["cpu1"]["hprselr"], 2);
    assert_eq!(state["targets"]["cpu1"]["pmselr"], 31);
    assert_eq!(selector_writes(&state).len(), 6);
    assert!(
        selector_writes(&state)
            .iter()
            .all(|entry| entry[0] == "cpu0")
    );
}

#[test]
fn multicore_scope_all_selector_read_modifies_only_selected_physical_core_then_restores_it() {
    let mut fixture = fixture("");
    fixture.project.cores = (0..2)
        .map(|i| Core {
            name: format!("core{i}"),
            endpoint: format!("localhost:{}", 24500 + i),
            ..Default::default()
        })
        .collect();
    fixture.project.registers.targets.remove("default");
    fixture
        .project
        .registers
        .targets
        .insert("core0".into(), "cpu0".into());
    fixture
        .project
        .registers
        .targets
        .insert("core1".into(), "cpu1".into());
    let engine = debugtui::coordinator::spawn(fixture.project.clone());
    ok(&engine, 1, "connect", json!({}));
    ok(&engine, 2, "control_scope", json!({"scope":"all"}));
    ok(&engine, 3, "select_core", json!({"index":1}));
    let context = probe(&engine, 4);
    let result = ok(
        &engine,
        6,
        "registers_select",
        json!({"context":context,"kind":"mpu_el2","index":19}),
    );
    assert_eq!(result["target"], "cpu1");
    assert!(
        result["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["owner"] == "core:core1")
    );
    let state = fixture.state.lock().unwrap().clone();
    assert_eq!(state["targets"]["cpu0"]["hprselr"], 2);
    assert_eq!(state["targets"]["cpu1"]["hprselr"], 2);
    assert!(
        selector_writes(&state)
            .iter()
            .all(|write| write[0] == "cpu1")
    );
    assert_eq!(selector_writes(&state).len(), 2);
    ok(&engine, 7, "select_core", json!({"index":0}));
    assert!(
        request(
            &engine,
            8,
            "registers_select",
            json!({"context":context,"kind":"mpu_el2","index":19})
        )
        .is_err()
    );
    assert_eq!(*fixture.state.lock().unwrap(), state);
    ok(&engine, 9, "quit", json!({}));
}
