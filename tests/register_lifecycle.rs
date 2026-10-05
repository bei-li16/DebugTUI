#![cfg(windows)]
//! Real session/coordinator and MI pipes; physical hardware is never contacted.
use debugtui::{
    config::{Core, Project},
    coordinator,
    session::{self, EngineHandle, Event, Request},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

fn fixture(name: &str) -> (Project, PathBuf) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = root
        .join("artifacts")
        .join(format!("register lifecycle {name} {}", std::process::id()));
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("commands.txt"), "").unwrap();
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
        .insert("DEBUGTUI_TEST_REGISTERS".into(), json!(["r0"]).to_string());
    project.gdb.env.insert(
        "DEBUGTUI_TEST_TRANSCRIPT".into(),
        out.join("commands.txt").to_string_lossy().into_owned(),
    );
    project.target.endpoint = "localhost:1234".into();
    project.session.on_exit = "disconnect".into();
    project.registers.catalogue = root.join("profiles/registers/cortex-r52.toml");
    (project, out)
}
fn call(engine: &EngineHandle, id: u64, method: &str, params: Value, expected_ok: bool) -> Value {
    engine.send(Request::new(id, method, params)).unwrap();
    wait_response(engine, id, method, expected_ok)
}
fn wait_response(engine: &EngineHandle, id: u64, method: &str, expected_ok: bool) -> Value {
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
        {
            if id != found {
                continue;
            }
            assert_eq!(ok, expected_ok, "{method}: {error:?}; {result}");
            return if ok { result } else { json!({"error":error}) };
        }
    }
}
fn sample<'a>(snapshot: &'a Value, id: &str) -> &'a Value {
    snapshot["register_samples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == id)
        .unwrap()
}
fn reads(out: &std::path::Path) -> usize {
    fs::read_to_string(out.join("commands.txt"))
        .unwrap()
        .lines()
        .filter(|c| c.starts_with("-data-list-register-values r "))
        .count()
}

#[test]
fn actual_frame_or_thread_changes_discard_the_batch_without_overwriting_last_valid_values() {
    for change in ["frame-before", "frame-during", "thread-during"] {
        let (mut project, out) = fixture(change);
        let context_file = out.join("context.json");
        fs::write(&context_file, json!({"frame":0,"thread":"1"}).to_string()).unwrap();
        project.gdb.env.insert(
            "DEBUGTUI_TEST_CONTEXT_FILE".into(),
            context_file.to_string_lossy().into_owned(),
        );
        project
            .gdb
            .env
            .insert("DEBUGTUI_TEST_REGISTER_DELAY_MS".into(), "200".into());
        let engine = session::spawn(project);
        call(&engine, 1, "connect", json!({}), true);
        let before = call(&engine, 2, "registers_read", json!({"ids":["r0"]}), true);
        let original = &before["samples"][0];
        assert_eq!(reads(&out), 1);
        let response = if change == "frame-before" {
            fs::write(&context_file, json!({"frame":1,"thread":"1"}).to_string()).unwrap();
            call(
                &engine,
                3,
                "registers_read",
                json!({"ids":["r0","pc"]}),
                false,
            )
        } else {
            engine
                .send(Request::new(
                    3,
                    "registers_read",
                    json!({"ids":["r0","pc"]}),
                ))
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            while reads(&out) != 2 {
                assert!(
                    Instant::now() < deadline,
                    "second register read did not start"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            let next = if change == "frame-during" {
                json!({"frame":1,"thread":"1"})
            } else {
                json!({"frame":0,"thread":"2"})
            };
            fs::write(&context_file, next.to_string()).unwrap();
            wait_response(&engine, 3, "registers_read", false)
        };
        assert!(
            response["error"].as_str().unwrap().contains("frame"),
            "{change}: {response}"
        );
        let snapshot = call(&engine, 4, "status", json!({}), true);
        let expired = sample(&snapshot, "r0");
        assert_eq!(expired["state"], "stale");
        assert_eq!(expired["value"], original["value"]);
        assert_eq!(expired["timestamp_ms"], original["timestamp_ms"]);
        assert!(
            snapshot["generation"].as_u64().unwrap()
                > before["context"]["generation"].as_u64().unwrap()
        );
        assert_eq!(snapshot["register_samples"].as_array().unwrap().len(), 1);
        assert_eq!(reads(&out), if change == "frame-before" { 1 } else { 2 });
        call(&engine, 5, "quit", json!({}), true);
    }
}

#[test]
fn frame_changes_expire_gdb_mrrc_fallback_and_nested_aliases_but_keep_physical_samples() {
    let (mut project, out) = fixture("frames");
    let catalogue = out.join("catalogue.toml");
    fs::write(
        &catalogue,
        r#"
version=1
cpu="cortex-r52"
architecture="armv8-r-aarch32"
[[groups]]
id="core"
name="Core"
[[registers]]
id="q0"
name="Q0"
group="core"
bits=128
access="ro"
reader={kind="gdb",name="q0"}
[[registers]]
id="d1"
name="D1"
group="core"
bits=64
access="ro"
reader={kind="alias",source="q0",offset=64}
[[registers]]
id="s3"
name="S3"
group="core"
bits=32
access="ro"
reader={kind="alias",source="d1",offset=32}
[[registers]]
id="cntpct"
name="CNTPCT"
group="core"
bits=64
access="ro"
reader={kind="cp15_64",cp=15,op1=0,crm=14}
[[registers]]
id="physical"
name="Physical MMIO"
group="core"
bits=32
access="ro"
reader={kind="mmio",component="probe",offset=0}
[[registers]]
id="physical_alias"
name="Physical alias"
group="core"
bits=16
access="ro"
reader={kind="alias",source="physical",offset=8}
"#,
    )
    .unwrap();
    project.registers.catalogue = catalogue;
    project.registers.components.insert(
        "probe".into(),
        debugtui::registers::Component {
            base: 0x20000000,
            channel: String::new(),
            little_endian: true,
        },
    );
    project.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTERS".into(),
        json!(["q0", "cntpct"]).to_string(),
    );
    project.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTER_FRAME_VALUES".into(),
        json!({
            "0":{"q0":"0xfedcba98765432100123456789abcdef", "cntpct":"0xfedcba9876543210"},
            "1":{"q0":"0x8123456789abcdef1023456789abcdef", "cntpct":"0x8123456789abcdef"}
        })
        .to_string(),
    );
    project.gdb.env.insert(
        "DEBUGTUI_TEST_MEMORY_BLOCKS".into(),
        "[{begin=\"0x20000000\",contents=\"78563412\"}]".into(),
    );
    let engine = session::spawn(project);
    call(&engine, 1, "connect", json!({}), true);
    let read = call(
        &engine,
        2,
        "registers_read",
        json!({"ids":["s3","q0","cntpct","physical_alias","physical"]}),
        true,
    );
    assert_eq!(read["samples"][0]["value"]["hex"], "0xfedcba98");
    assert_eq!(
        read["samples"][0]["source"],
        "alias:d1@32 <- alias:q0@64 <- gdb:q0"
    );
    assert_eq!(read["samples"][2]["source"], "gdb:cntpct");
    assert_eq!(
        read["samples"][2]["provenance"]["catalogue_reader"]["kind"],
        "cp15_64"
    );
    assert_eq!(
        read["samples"][2]["provenance"]["access"]["route"]["kind"],
        "gdb_register"
    );
    assert_eq!(
        read["samples"][2]["provenance"]["access"]["route"]["name"],
        "cntpct"
    );
    assert_eq!(
        read["samples"][2]["provenance"]["access"]["phase"],
        "responded"
    );
    assert_eq!(read["samples"][2]["view"], "selected_frame");
    assert_eq!(read["samples"][3]["view"], "physical_core");
    assert_eq!(read["samples"][3]["value"]["hex"], "0x3456");
    assert_eq!(reads(&out), 2);
    call(&engine, 3, "frame", json!({"level":1}), true);
    let snapshot = call(&engine, 4, "status", json!({}), true);
    for id in ["s3", "q0", "cntpct"] {
        assert_eq!(sample(&snapshot, id)["state"], "stale");
    }
    for id in ["physical", "physical_alias"] {
        assert_eq!(sample(&snapshot, id)["state"], "valid");
    }
    assert_eq!(
        sample(&snapshot, "cntpct")["value"]["hex"],
        "0xfedcba9876543210"
    );
    assert!(snapshot["registers"][0]["error"].as_bool().unwrap());
    call(
        &engine,
        5,
        "registers_read",
        json!({"ids":["s3"],"context":read["context"]}),
        false,
    );
    assert_eq!(reads(&out), 2);
    let read = call(&engine, 6, "registers_read", json!({"ids":["s3"]}), true);
    assert_eq!(read["samples"][0]["value"]["hex"], "0x81234567");
    assert_eq!(read["samples"][0]["context"]["frame"], 1);
    call(&engine, 7, "frame", json!({"level":0}), true);
    let snapshot = call(&engine, 8, "status", json!({}), true);
    assert_eq!(sample(&snapshot, "s3")["state"], "stale");
    let read = call(&engine, 9, "registers_read", json!({"ids":["s3"]}), true);
    assert_eq!(read["samples"][0]["value"]["hex"], "0xfedcba98");
    call(&engine, 10, "quit", json!({}), true);
}

#[test]
fn restart_errors_console_symbol_replacement_and_elf_reconnect_invalidate_old_samples() {
    for fails in [false, true] {
        let (mut project, out) = fixture(&format!("reset-{fails}"));
        project.actions.restart = vec!["monitor fixture_reset".into()];
        project.gdb.env.insert(
            "DEBUGTUI_TEST_SYMBOL_REGISTERS".into(),
            json!(["", "r0"]).to_string(),
        );
        if fails {
            project
                .gdb
                .env
                .insert("DEBUGTUI_TEST_RESET_ERROR".into(), "1".into());
        }
        let engine = session::spawn(project);
        call(&engine, 1, "connect", json!({}), true);
        let before = call(&engine, 2, "registers_read", json!({"ids":["r0"]}), true);
        call(&engine, 3, "restart", json!({}), !fails);
        let snapshot = call(&engine, 4, "status", json!({}), true);
        assert_eq!(sample(&snapshot, "r0")["state"], "stale");
        assert_eq!(
            sample(&snapshot, "r0")["value"],
            before["samples"][0]["value"]
        );
        assert!(
            snapshot["generation"].as_u64().unwrap()
                > before["context"]["generation"].as_u64().unwrap()
        );
        assert_eq!(reads(&out), 1);
        let read = call(&engine, 5, "registers_read", json!({"ids":["r0"]}), true);
        assert_eq!(read["samples"][0]["value"]["hex"], "0x00000000");
        call(
            &engine,
            6,
            "console",
            json!({"command":"file replacement.elf"}),
            true,
        );
        let snapshot = call(&engine, 7, "status", json!({}), true);
        assert_eq!(sample(&snapshot, "r0")["state"], "stale");
        call(&engine, 8, "registers_read", json!({"ids":["r0"]}), true);
        let transcript = fs::read_to_string(out.join("commands.txt")).unwrap();
        assert_eq!(
            transcript
                .lines()
                .filter(|c| c.contains("monitor fixture_reset"))
                .count(),
            1
        );
        assert!(
            transcript
                .lines()
                .any(|c| c == "-data-list-register-values r 1")
        );
        call(
            &engine,
            9,
            "set_elf",
            json!({"path":"cannot-change-connected.elf"}),
            false,
        );
        call(&engine, 10, "disconnect", json!({}), true);
        let elf = out.join("replacement.elf");
        fs::write(&elf, b"software MI fixture ELF identity").unwrap();
        call(&engine, 11, "set_elf", json!({"path":elf}), true);
        call(&engine, 12, "connect", json!({}), true);
        let snapshot = call(&engine, 13, "status", json!({}), true);
        assert!(snapshot["register_samples"].as_array().unwrap().is_empty());
        assert_ne!(snapshot["register_session"], before["context"]["session"]);
        call(
            &engine,
            14,
            "registers_read",
            json!({"ids":["r0"],"context":before["context"]}),
            false,
        );
        assert_eq!(reads(&out), 3);
        let transcript = fs::read_to_string(out.join("commands.txt")).unwrap();
        assert!(transcript.contains("-file-exec-and-symbols "));
        assert!(transcript.contains("replacement.elf"));
        call(&engine, 15, "quit", json!({}), true);
    }
}

#[test]
fn multicore_selected_caches_shared_reset_and_console_expire_every_affected_worker_once() {
    for reset_error in [false, true] {
        multicore_reset(reset_error);
    }
}
fn multicore_reset(reset_error: bool) {
    let (mut project, out) = fixture(&format!("multicore-{reset_error}"));
    project.cores = vec![
        Core {
            name: "core0".into(),
            endpoint: "localhost:3333".into(),
            ..Default::default()
        },
        Core {
            name: "core1".into(),
            endpoint: "localhost:3334".into(),
            ..Default::default()
        },
    ];
    project.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTER_ENDPOINT_VALUES".into(),
        json!({"localhost:3333":{"r0":"0x11111111"},"localhost:3334":{"r0":"0x22222222"}})
            .to_string(),
    );
    project.multicore.restart = vec!["monitor fixture_reset".into()];
    project.multicore.restart_core = "core0".into();
    if reset_error {
        project
            .gdb
            .env
            .insert("DEBUGTUI_TEST_RESET_ERROR".into(), "1".into());
    }
    let engine = coordinator::spawn(project);
    call(&engine, 1, "connect", json!({}), true);
    let mut contexts = vec![];
    for (core, hex) in [(0, "0x11111111"), (1, "0x22222222")] {
        call(
            &engine,
            10 + core,
            "select_core",
            json!({"index":core}),
            true,
        );
        let read = call(
            &engine,
            20 + core,
            "registers_read",
            json!({"ids":["r0"],"scope":"all"}),
            true,
        );
        contexts.push(read["context"].clone());
        assert_eq!(read["samples"][0]["owner"], format!("core:core{core}"));
        assert_eq!(read["samples"][0]["value"]["hex"], hex);
    }
    call(&engine, 30, "select_core", json!({"index":0}), true);
    let snapshot = call(&engine, 31, "status", json!({}), true);
    assert_eq!(sample(&snapshot, "r0")["state"], "valid");
    assert_eq!(sample(&snapshot, "r0")["value"]["hex"], "0x11111111");
    call(
        &engine,
        32,
        "registers_read",
        json!({"ids":["r0"],"context":contexts[1]}),
        false,
    );
    assert_eq!(reads(&out), 2);
    call(
        &engine,
        33,
        "restart",
        json!({"scope":"core"}),
        !reset_error,
    );
    for core in [0, 1] {
        call(
            &engine,
            40 + core,
            "select_core",
            json!({"index":core}),
            true,
        );
        let snapshot = call(&engine, 50 + core, "status", json!({}), true);
        assert_eq!(sample(&snapshot, "r0")["state"], "stale");
        assert_eq!(sample(&snapshot, "r0")["owner"], format!("core:core{core}"));
        assert_eq!(
            sample(&snapshot, "r0")["value"]["hex"],
            if core == 0 {
                "0x11111111"
            } else {
                "0x22222222"
            }
        );
        call(
            &engine,
            60 + core,
            "registers_read",
            json!({"ids":["r0"],"context":contexts[core as usize]}),
            false,
        );
    }
    assert_eq!(reads(&out), 2);
    assert_eq!(
        fs::read_to_string(out.join("commands.txt"))
            .unwrap()
            .lines()
            .filter(|c| c.contains("monitor fixture_reset"))
            .count(),
        1
    );
    call(&engine, 70, "reconnect", json!({}), true);
    for core in [0, 1] {
        call(
            &engine,
            80 + core,
            "select_core",
            json!({"index":core}),
            true,
        );
        let snapshot = call(&engine, 90 + core, "status", json!({}), true);
        assert!(snapshot["register_samples"].as_array().unwrap().is_empty());
        assert_ne!(
            snapshot["register_session"],
            contexts[core as usize]["session"]
        );
        let fresh = call(
            &engine,
            95 + core,
            "registers_read",
            json!({"ids":["r0"]}),
            true,
        );
        assert_eq!(fresh["samples"][0]["state"], "valid");
        assert_eq!(
            fresh["samples"][0]["context"]["session"],
            snapshot["register_session"]
        );
        assert_eq!(fresh["samples"][0]["owner"], format!("core:core{core}"));
    }
    assert_eq!(reads(&out), 4);
    call(
        &engine,
        97,
        "console",
        json!({"command":"monitor fixture_reset","scope":"all"}),
        !reset_error,
    );
    for core in [0, 1] {
        call(
            &engine,
            110 + core,
            "select_core",
            json!({"index":core}),
            true,
        );
        let snapshot = call(&engine, 120 + core, "status", json!({}), true);
        assert_eq!(sample(&snapshot, "r0")["state"], "stale");
        assert_eq!(sample(&snapshot, "r0")["owner"], format!("core:core{core}"));
    }
    assert_eq!(reads(&out), 4);
    assert_eq!(
        fs::read_to_string(out.join("commands.txt"))
            .unwrap()
            .lines()
            .filter(|c| c.contains("monitor fixture_reset"))
            .count(),
        2
    );
    call(&engine, 100, "quit", json!({}), true);
}
