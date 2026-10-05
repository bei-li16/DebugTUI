#![cfg(windows)]
//! Real engine/MI pipe integration against a strict local fixture; no board.
use debugtui::{
    config::Project,
    session::{self, Event, Request},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

#[test]
fn memory_dumps_use_complete_scoped_mi_responses_and_reject_running_results() {
    for running in [false, true] {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let output = root
            .join("artifacts")
            .join(format!("memory dump {} {running}", std::process::id()));
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
            .insert("DEBUGTUI_TEST_REGISTERS".into(), json!([]).to_string());
        project.gdb.env.insert(
            "DEBUGTUI_TEST_TRANSCRIPT".into(),
            transcript.to_string_lossy().into_owned(),
        );
        project.gdb.env.insert(
            "DEBUGTUI_TEST_MEMORY_BLOCKS".into(),
            r#"[{begin="0x100000008",contents="007f"},{begin="0x10000000a",contents="80ff"}]"#
                .into(),
        );
        if running {
            project
                .gdb
                .env
                .insert("DEBUGTUI_TEST_MEMORY_RUN_ON_READ".into(), "1".into());
        }
        project.registers.catalogue = root.join("profiles/registers/cortex-r52.toml");
        project.target.endpoint = "localhost:3333".into();
        project.session.on_exit = "disconnect".into();
        let engine = session::spawn(project);
        response(&engine, 1, "connect", json!({}));
        let listed = response(&engine, 2, "registers_list", json!({}));
        engine
            .send(Request::new(
                3,
                "memory_dump",
                json!({"address":"0x100000008","count":4,"context":listed["context"]}),
            ))
            .unwrap();
        loop {
            if let Event::Response {
                id: 3,
                ok,
                result,
                error,
            } = engine.events.recv_timeout(Duration::from_secs(10)).unwrap()
            {
                if running {
                    assert!(!ok);
                    assert!(error.unwrap().contains("running state changed"));
                } else {
                    assert!(ok, "{error:?}");
                    assert_eq!(result["address"], "0x100000008");
                    assert_eq!(result["bytes"], json!([0, 127, 128, 255]));
                    assert_eq!(result["context"], listed["context"]);
                    assert_eq!(result["endpoint"], "localhost:3333");
                }
                break;
            }
        }
        let state = response(&engine, 4, "status", json!({}));
        assert_eq!(
            state["memory"],
            json!([]),
            "raw ranges must not publish an unqualified snapshot"
        );
        let commands = fs::read_to_string(&transcript).unwrap();
        assert_eq!(
            commands
                .lines()
                .filter(|line| line.starts_with("-data-read-memory-bytes "))
                .collect::<Vec<_>>(),
            ["-data-read-memory-bytes \"0x100000008\" 4"]
        );
        assert!(!commands.contains("-exec-interrupt"));
        response(&engine, 5, "quit", json!({}));
    }
}

fn response(engine: &session::EngineHandle, id: u64, method: &str, params: Value) -> Value {
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
            assert!(ok, "{method}: {error:?}");
            return result;
        }
    }
}

#[test]
fn all_core_control_scope_keeps_memory_reads_on_the_selected_core_and_rejects_old_contexts() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = root
        .join("artifacts")
        .join(format!("memory multicore {}", std::process::id()));
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
        .insert("DEBUGTUI_TEST_REGISTERS".into(), json!([]).to_string());
    project.gdb.env.insert(
        "DEBUGTUI_TEST_TRANSCRIPT".into(),
        transcript.to_string_lossy().into_owned(),
    );
    project.gdb.env.insert(
        "DEBUGTUI_TEST_MEMORY_BLOCKS".into(),
        r#"[{begin="0x100000008",contents="007f80ff"}]"#.into(),
    );
    project.registers.catalogue = root.join("profiles/registers/cortex-r52.toml");
    project.cores = (0..2)
        .map(|index| debugtui::config::Core {
            name: format!("core{index}"),
            endpoint: format!("localhost:{}", 3333 + index),
            ..Default::default()
        })
        .collect();
    let engine = debugtui::coordinator::spawn(project);
    response(&engine, 1, "connect", json!({}));
    response(&engine, 20, "select_core", json!({"index":0}));
    response(&engine, 2, "control_scope", json!({"scope":"all"}));
    let first = response(&engine, 3, "registers_list", json!({}));
    assert_eq!(first["context"]["core"], "core0");
    let read = response(
        &engine,
        4,
        "memory_dump",
        json!({"address":"0x100000008","count":4,"context":first["context"]}),
    );
    assert_eq!(read["target"], "core0");
    assert_eq!(read["endpoint"], "localhost:3333");
    response(&engine, 5, "select_core", json!({"index":1}));
    engine
        .send(Request::new(
            6,
            "memory_dump",
            json!({"address":"0x100000008","count":4,"context":first["context"]}),
        ))
        .unwrap();
    loop {
        if let Event::Response {
            id: 6, ok, error, ..
        } = engine.events.recv_timeout(Duration::from_secs(10)).unwrap()
        {
            assert!(!ok);
            assert!(error.unwrap().contains("expired core"));
            break;
        }
    }
    let second = response(&engine, 7, "registers_list", json!({}));
    assert_eq!(second["context"]["core"], "core1");
    assert_ne!(first["context"]["session"], second["context"]["session"]);
    let read = response(
        &engine,
        8,
        "memory_dump",
        json!({"address":"0x100000008","count":4,"context":second["context"]}),
    );
    assert_eq!(read["target"], "core1");
    assert_eq!(read["endpoint"], "localhost:3334");
    let commands = fs::read_to_string(&transcript).unwrap();
    assert_eq!(
        commands
            .lines()
            .filter(|line| line.starts_with("-data-read-memory-bytes "))
            .count(),
        2
    );
    assert!(!commands.contains("-exec-interrupt"));
    response(&engine, 9, "quit", json!({}));
}

#[test]
fn catalogue_refresh_is_on_demand_and_snapshots_retain_precise_stale_values() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = root
        .join("artifacts")
        .join(format!("register access {}", std::process::id()));
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
    project.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTERS".into(),
        json!(["r0", "", "pc", "cntpct"]).to_string(),
    );
    project.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTER_VALUES".into(),
        json!({"r0":"0x12345678", "cntpct":"0xfedcba9876543210"}).to_string(),
    );
    project.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTER_ERRORS".into(),
        json!(["pc"]).to_string(),
    );
    project.gdb.env.insert(
        "DEBUGTUI_TEST_TRANSCRIPT".into(),
        transcript.to_string_lossy().into_owned(),
    );
    project.target.endpoint = "localhost:1234".into();
    project.registers.catalogue = root.join("profiles/registers/cortex-r52.toml");
    let engine = session::spawn(project);
    response(&engine, 1, "connect", json!({}));
    let commands = fs::read_to_string(&transcript).unwrap();
    assert!(
        !commands.contains("-data-list-register-values"),
        "connect/stop caused an unsolicited register read: {commands}"
    );
    assert!(!commands.contains("-data-list-register-names"));
    let catalogue = response(&engine, 10, "registers_list", json!({}));
    assert_eq!(catalogue["catalogue"]["cpu"], "cortex-r52");
    let r0 = catalogue["catalogue"]["registers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "r0")
        .unwrap();
    assert_eq!(r0["bits"], 32);
    assert!(!r0["description"].as_str().unwrap().is_empty());
    let read = response(
        &engine,
        2,
        "registers_read",
        json!({"ids":["r0", "pc", "cntpct"]}),
    );
    assert_eq!(read["samples"][0]["value"]["hex"], "0x12345678");
    assert_eq!(read["samples"][1]["state"], "unavailable");
    assert_eq!(read["samples"][2]["value"]["hex"], "0xfedcba9876543210");
    assert_eq!(read["context"], catalogue["context"]);
    for value in read["samples"].as_array().unwrap() {
        assert_eq!(value["context"], read["context"]);
        assert_eq!(value["owner"], "core:default");
        assert!(value["timestamp_ms"].is_u64());
        assert!(value["implementation"].is_string());
        assert!(value["reason"].is_string());
    }
    assert_eq!(read["samples"][0]["source"], "gdb:r0");
    assert_eq!(read["samples"][0]["value"]["bits"], 32);
    assert_eq!(read["samples"][2]["value"]["bits"], 64);
    assert_eq!(read["samples"][1]["source"], "gdb:pc");
    assert_eq!(read["samples"][1]["reason"], "unknown");
    assert!(
        read["samples"][1]["detail"]
            .as_str()
            .unwrap()
            .contains("Register is inaccessible")
    );
    assert!(read["samples"][1]["value"].is_null());
    let commands = fs::read_to_string(&transcript).unwrap();
    let reads: Vec<_> = commands
        .lines()
        .filter(|command| command.starts_with("-data-list-register-values"))
        .collect();
    assert_eq!(
        reads,
        [
            "-data-list-register-values r 0",
            "-data-list-register-values r 2",
            "-data-list-register-values r 3"
        ]
    );
    let snapshot = response(&engine, 3, "status", json!({}));
    assert_eq!(snapshot["registers"][0]["name"], "r0");
    assert_eq!(snapshot["registers"][0]["value"], "0x12345678");
    assert_eq!(snapshot["registers"][0]["error"], false);
    assert_eq!(snapshot["registers"][0]["changed"], false);
    assert_eq!(snapshot["register_samples"], read["samples"]);
    assert_eq!(
        snapshot["register_samples"][2]["value"]["hex"],
        "0xfedcba9876543210"
    );
    response(&engine, 4, "step", json!({}));
    response(&engine, 5, "wait_stopped", json!({}));
    let stopped = response(&engine, 6, "status", json!({}));
    assert_eq!(stopped["register_samples"][0]["state"], "stale");
    assert_eq!(stopped["register_samples"][0]["value"]["hex"], "0x12345678");
    assert_eq!(
        stopped["register_samples"][0]["timestamp_ms"],
        read["samples"][0]["timestamp_ms"]
    );
    assert_eq!(
        stopped["register_samples"][1]["detail"],
        read["samples"][1]["detail"]
    );
    assert!(stopped["registers"][0]["error"].as_bool().unwrap());
    let final_commands = fs::read_to_string(&transcript).unwrap();
    assert_eq!(
        final_commands
            .lines()
            .filter(|command| command.starts_with("-data-list-register-values"))
            .count(),
        3
    );
    response(&engine, 7, "quit", json!({}));
}

#[test]
fn gdb_reader_returns_128_bits_and_derives_aliases_from_one_parent_sample() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = root
        .join("artifacts")
        .join(format!("register raw aliases {}", std::process::id()));
    fs::create_dir_all(&output).unwrap();
    let transcript = output.join("commands.txt");
    fs::write(&transcript, "").unwrap();
    let catalogue_path = output.join("customer-registers.toml");
    fs::write(
        &catalogue_path,
        r#"
version = 1
cpu = "cortex-r52"
architecture = "armv8-r-aarch32"
[[groups]]
id = "simd"
name = "SIMD"
[[registers]]
id = "q0"
name = "Q0"
group = "simd"
bits = 128
access = "ro"
reader = { kind = "gdb", name = "q0" }
description = "Exact raw vector supplied by the declared GDB target description."
[[registers]]
id = "d1"
name = "D1"
group = "simd"
bits = 64
access = "ro"
reader = { kind = "alias", source = "q0", offset = 64 }
[[registers]]
id = "s3"
name = "S3"
group = "simd"
bits = 32
access = "ro"
reader = { kind = "alias", source = "d1", offset = 32 }
"#,
    )
    .unwrap();
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
        .insert("DEBUGTUI_TEST_REGISTERS".into(), json!(["q0"]).to_string());
    project.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTER_VALUES".into(),
        json!({"q0":"0xfedcba98765432100123456789abcdef"}).to_string(),
    );
    project.gdb.env.insert(
        "DEBUGTUI_TEST_TRANSCRIPT".into(),
        transcript.to_string_lossy().into_owned(),
    );
    project.target.endpoint = "localhost:1234".into();
    project.registers.catalogue = catalogue_path;
    let engine = session::spawn(project);
    response(&engine, 1, "connect", json!({}));
    let read = response(
        &engine,
        2,
        "registers_read",
        json!({"ids":["s3","d1","q0"]}),
    );
    for (i, id, bits, hex) in [
        (0, "s3", 32, "0xfedcba98"),
        (1, "d1", 64, "0xfedcba9876543210"),
        (2, "q0", 128, "0xfedcba98765432100123456789abcdef"),
    ] {
        let sample = &read["samples"][i];
        assert_eq!(sample["id"], id);
        assert_eq!(sample["state"], "valid");
        assert_eq!(sample["value"]["bits"], bits);
        assert_eq!(sample["value"]["hex"], hex);
        assert_eq!(sample["owner"], "core:default");
        assert_eq!(sample["context"], read["context"]);
    }
    let commands = fs::read_to_string(&transcript).unwrap();
    assert_eq!(
        commands
            .lines()
            .filter(|c| c.starts_with("-data-list-register-values"))
            .collect::<Vec<_>>(),
        ["-data-list-register-values r 0"]
    );
    let snapshot = response(&engine, 3, "status", json!({}));
    assert_eq!(snapshot["register_samples"], read["samples"]);
    assert_eq!(
        snapshot["registers"],
        json!([{
            "name":"q0", "value":"0xfedcba98765432100123456789abcdef", "changed":false, "error":false
        }])
    );
    response(&engine, 4, "quit", json!({}));
}

#[test]
fn running_notification_during_a_read_discards_the_result_and_stops_the_batch() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = root
        .join("artifacts")
        .join(format!("register run race {}", std::process::id()));
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
    project.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTERS".into(),
        json!(["r0", "r1"]).to_string(),
    );
    project
        .gdb
        .env
        .insert("DEBUGTUI_TEST_REGISTER_RUN_ON_READ".into(), "1".into());
    project.gdb.env.insert(
        "DEBUGTUI_TEST_TRANSCRIPT".into(),
        transcript.to_string_lossy().into_owned(),
    );
    project.target.endpoint = "localhost:1234".into();
    project.session.on_exit = "disconnect".into();
    project.registers.catalogue = root.join("profiles/registers/cortex-r52.toml");
    let engine = session::spawn(project);
    response(&engine, 1, "connect", json!({}));
    let result = response(&engine, 2, "registers_read", json!({"ids":["r0", "r1"]}));
    assert_eq!(result["samples"].as_array().unwrap().len(), 1);
    assert_eq!(result["samples"][0]["state"], "stale");
    assert!(result["samples"][0]["value"].is_null());
    let commands = fs::read_to_string(&transcript).unwrap();
    assert_eq!(
        commands
            .lines()
            .filter(|command| command.starts_with("-data-list-register-values"))
            .collect::<Vec<_>>(),
        ["-data-list-register-values r 0"]
    );
    assert!(!commands.contains("-exec-interrupt"));
    response(&engine, 3, "quit", json!({}));
}
