#![cfg(windows)]
//! End-to-end writes through the actual worker/MI pipes, without a board.
#[path = "support/artifacts.rs"]
mod test_artifacts;

use debugtui::{
    config::{ControlScope, Core, Project},
    coordinator,
    session::{self, Event, Request},
};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, time::Duration};
#[path = "write_access/bitfield_cases.rs"]
mod bitfield_cases;
#[path = "write_access/float_cases.rs"]
mod float_cases;
#[path = "write_access/reference_cases.rs"]
mod reference_cases;
#[path = "write_access/wide_cases.rs"]
mod wide_cases;

fn fixture(label: &str, flags: &[(&str, &str)]) -> (Project, PathBuf) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let directory = test_artifacts::root().join(format!("write {label} {}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let transcript = directory.join("commands.txt");
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
        json!(["", "r0", "", "pc"]).to_string(),
    );
    project.gdb.env.insert(
        "DEBUGTUI_TEST_TRANSCRIPT".into(),
        transcript.to_string_lossy().into_owned(),
    );
    for (key, value) in flags {
        project.gdb.env.insert((*key).into(), (*value).into());
    }
    project.registers.cpu = "cortex-r52".into();
    project.target.endpoint = "localhost:3333".into();
    project.session.on_exit = "disconnect".into();
    project.session.timeout_ms = 1000;
    (project, transcript)
}
fn request(
    engine: &session::EngineHandle,
    id: u64,
    method: &str,
    params: Value,
) -> (bool, Value, Option<String>) {
    engine.send(Request::new(id, method, params)).unwrap();
    loop {
        if let Event::Response {
            id: found,
            ok,
            result,
            error,
        } = engine.events.recv_timeout(Duration::from_secs(15)).unwrap()
            && found == id
        {
            return (ok, result, error);
        }
    }
}
fn ok(engine: &session::EngineHandle, id: u64, method: &str, params: Value) -> Value {
    let (ok, result, error) = request(engine, id, method, params);
    assert!(ok, "{method}: {error:?}");
    result
}
fn preview(engine: &session::EngineHandle, id: u64, value: &str) -> Value {
    let catalogue = ok(engine, id, "registers_list", json!({}));
    ok(
        engine,
        id + 1,
        "write_preview",
        json!({"target":{"kind":"register","id":"r0"},"context":catalogue["context"],"selection":{"kind":"register"},"input":{"kind":"unsigned","text":value}}),
    )
}
fn writes(transcript: &PathBuf) -> Vec<String> {
    fs::read_to_string(transcript)
        .unwrap()
        .lines()
        .filter(|line| line.starts_with("-data-write-register-values "))
        .map(str::to_owned)
        .collect()
}

fn ram_fixture(label: &str, flags: &[(&str, &str)]) -> (Project, PathBuf) {
    let (mut project, transcript) = fixture(label, flags);
    project
        .gdb
        .env
        .insert("DEBUGTUI_TEST_RAM".into(), "1".into());
    project
        .gdb
        .env
        .insert("DEBUGTUI_TEST_RAM_BASE".into(), "0x100000000".into());
    project.writes=toml::from_str("[[regions]]\nid='ram'\nkind='ram'\nstart='0x100000001'\nend='0x100001001'\nwidths=[8,32,64,128]\nbyte_writable=true\nlittle_endian=true\n[[regions]]\nid='flash'\nkind='flash'\nstart='0x8000000'\nend='0x8010000'\nwidths=[8,32]\n[[regions]]\nid='mmio'\nkind='mmio'\nstart='0x40000000'\nend='0x40000100'\nwidths=[8,32]\n").unwrap();
    (project, transcript)
}
fn ram_preview(
    engine: &session::EngineHandle,
    id: u64,
    address: &str,
    bits: u16,
    kind: &str,
    text: &str,
) -> (bool, Value, Option<String>) {
    let context = ok(engine, id, "registers_list", json!({}))["context"].clone();
    request(
        engine,
        id + 1,
        "write_preview",
        json!({"target":{"kind":"memory","address":address,"bits":bits},"selection":{"kind":"register"},"context":context,"input":{"kind":kind,"text":text}}),
    )
}
fn memory_writes(transcript: &PathBuf) -> Vec<String> {
    fs::read_to_string(transcript)
        .unwrap()
        .lines()
        .filter(|l| l.starts_with("-data-write-memory-bytes"))
        .map(str::to_owned)
        .collect()
}

#[test]
fn ram_drafts_write_exact_64bit_range_keep_sentinels_and_accept_nonzero_frame() {
    let (project, transcript) = ram_fixture("ram bounds", &[]);
    let engine = session::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    let (success, draft, error) =
        ram_preview(&engine, 2, "0x100000001", 32, "bytes", "12 34 56 78");
    assert!(success, "{error:?}");
    assert_eq!(draft["plan"]["bytes"], json!([0x12, 0x34, 0x56, 0x78]));
    assert!(memory_writes(&transcript).is_empty());
    ok(&engine, 4, "write_cancel", json!({"draft":draft["draft"]}));
    assert_eq!(
        ok(&engine, 5, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
        "not_sent"
    );
    ok(&engine, 6, "frame", json!({"level":1}));
    let (success, draft, error) = ram_preview(
        &engine,
        7,
        "0x100000001",
        32768,
        "bytes",
        &"fe".repeat(4096),
    );
    assert!(success, "{error:?}");
    let result = ok(&engine, 9, "write_apply", json!({"draft":draft["draft"]}));
    assert_eq!(result["outcome"], "verified");
    assert_eq!(result["observed_bytes"].as_array().unwrap().len(), 4096);
    assert_eq!(memory_writes(&transcript).len(), 1);
    assert_eq!(
        ok(&engine, 10, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
        "not_sent"
    );
    for address in ["0x100000000", "0x100001001"] {
        let context = ok(&engine, 11, "registers_list", json!({}))["context"].clone();
        let read = ok(
            &engine,
            12,
            "memory_dump",
            json!({"address":address,"count":1,"context":context}),
        );
        assert_eq!(read["bytes"], json!([0xaa]));
    }
    ok(&engine, 13, "quit", json!({}));
}
#[test]
fn ram_preview_rejects_implicit_flash_mmio_expressions_bad_width_routes_and_permissions() {
    let (project, transcript) = ram_fixture("ram refused", &[]);
    let engine = session::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    for (address, bits, kind, text) in [
        ("0x100000000", 8, "bytes", "ff"),
        ("0x100001001", 8, "bytes", "ff"),
        ("0xffffffffffffffff", 8, "bytes", "ff"),
        ("0x100000001", 32, "unsigned", "1"),
        ("counter++", 8, "bytes", "ff"),
        ("0x8000000", 8, "bytes", "ff"),
        ("0x40000000", 8, "bytes", "ff"),
        ("0x100000001", 8, "bytes", "ff;reset"),
        ("0x100000001", 24, "unsigned", "1"),
    ] {
        assert!(
            !ram_preview(&engine, 2, address, bits, kind, text).0,
            "unexpected accepted {address} {bits}"
        );
    }
    assert!(memory_writes(&transcript).is_empty());
    ok(&engine, 4, "quit", json!({}));
    for flag in [
        "DEBUGTUI_TEST_MEMORY_READONLY",
        "DEBUGTUI_TEST_NO_MEMORY_WRITER",
        "DEBUGTUI_TEST_MEMORY_PERMISSION_CHANGE",
    ] {
        let (project, transcript) = ram_fixture(flag, &[(flag, "1")]);
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        let (accepted, draft, _) =
            ram_preview(&engine, 2, "0x100000004", 32, "unsigned", "0x12345678");
        if flag.ends_with("PERMISSION_CHANGE") {
            assert!(accepted);
            let result = ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}));
            assert_eq!(result["outcome"], "not_sent");
        } else {
            assert!(!accepted);
        }
        assert!(memory_writes(&transcript).is_empty());
        ok(&engine, 5, "quit", json!({}));
    }
}
#[test]
fn ram_outcomes_distinguish_mismatch_unverifiable_and_unknown_without_retry() {
    for (flag, value, expected) in [
        ("DEBUGTUI_TEST_MEMORY_WRITE_MISMATCH", "1", "mismatch"),
        ("DEBUGTUI_TEST_MEMORY_VERIFY_ERROR", "1", "accepted"),
        ("DEBUGTUI_TEST_MEMORY_WRITE_RUN", "1", "accepted"),
        ("DEBUGTUI_TEST_MEMORY_WRITE_ERROR", "error", "unknown"),
        ("DEBUGTUI_TEST_MEMORY_WRITE_ERROR", "closed", "unknown"),
        ("DEBUGTUI_TEST_MEMORY_WRITE_ERROR", "timeout", "unknown"),
    ] {
        let (project, transcript) = ram_fixture(&format!("{flag}{value}"), &[(flag, value)]);
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        let (success, draft, error) =
            ram_preview(&engine, 2, "0x100000004", 32, "unsigned", "0x12345678");
        assert!(success, "{error:?}");
        let result = ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}));
        assert_eq!(result["outcome"], expected, "{result}");
        assert_eq!(
            memory_writes(&transcript),
            ["-data-write-memory-bytes 0x100000004 78563412"]
        );
        assert_eq!(
            ok(&engine, 5, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
            "not_sent"
        );
        let _ = request(&engine, 6, "quit", json!({}));
    }
}

#[test]
fn shared_ram_owner_requires_trusted_peer_states_and_all_scope_never_broadcasts() {
    let (mut project, transcript) = ram_fixture("shared ram", &[]);
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
    project.registers.topology.chip = "fixture".into();
    project.writes.regions[0].scope = debugtui::registers::Scope::Chip;
    // JSON clients cannot forge the coordinator-only field, including via serialization.
    let forged:Request=serde_json::from_value(json!({"id":1,"method":"write_preview","params":{},"write_peers":[{"index":0,"name":"core0","endpoint":"localhost:3333","state":"STOPPED"}]})).unwrap();
    assert!(
        serde_json::to_value(forged)
            .unwrap()
            .get("write_peers")
            .is_none()
    );
    let direct = session::spawn(project.clone());
    ok(&direct, 1, "connect", json!({}));
    let context = ok(&direct, 2, "registers_list", json!({}))["context"].clone();
    let denied = request(
        &direct,
        3,
        "write_preview",
        json!({"context":context,"target":{"kind":"memory","address":"0x100000004","bits":32},"selection":{"kind":"register"},"input":{"kind":"unsigned","text":"42"},"write_peers":[{"name":"core0","state":"STOPPED"},{"name":"core1","state":"STOPPED"}]}),
    );
    assert!(!denied.0);
    assert!(memory_writes(&transcript).is_empty());
    ok(&direct, 4, "quit", json!({}));
    drop(direct);
    project.multicore.scope = ControlScope::All;
    project
        .gdb
        .env
        .insert("DEBUGTUI_TEST_PAUSE".into(), "running".into());
    let engine = coordinator::spawn(project);
    ok(&engine, 5, "connect", json!({}));
    let (success, draft, error) = ram_preview(&engine, 6, "0x100000004", 32, "unsigned", "42");
    assert!(success, "{error:?}");
    assert_eq!(draft["owner"], "chip:fixture");
    assert_eq!(
        ok(&engine, 8, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
        "verified"
    );
    assert_eq!(memory_writes(&transcript).len(), 1);
    let (success, draft, error) = ram_preview(&engine, 9, "0x100000004", 32, "unsigned", "7");
    assert!(success, "{error:?}");
    ok(&engine, 11, "select_core", json!({"index":1}));
    ok(&engine, 12, "continue", json!({"scope":"core"}));
    ok(&engine, 13, "select_core", json!({"index":0}));
    assert_eq!(
        ok(&engine, 14, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
        "not_sent"
    );
    assert!(!ram_preview(&engine, 15, "0x100000004", 32, "unsigned", "7").0);
    assert_eq!(memory_writes(&transcript).len(), 1);
    let _ = request(&engine, 17, "quit", json!({}));
}

fn variable_fixture(label: &str, flags: &[(&str, &str)]) -> (Project, PathBuf) {
    let (mut project, transcript) = ram_fixture(label, flags);
    project
        .gdb
        .env
        .insert("DEBUGTUI_TEST_VARIABLES".into(), "1".into());
    project.watch = vec!["counter".into()];
    (project, transcript)
}
fn variable_preview(
    engine: &session::EngineHandle,
    id: u64,
    text: &str,
) -> (bool, Value, Option<String>) {
    let context = ok(engine, id, "registers_list", json!({}))["context"].clone();
    request(
        engine,
        id + 1,
        "write_preview",
        json!({"target":{"kind":"variable","pane":"watch","expression":"counter"},"selection":{"kind":"register"},"context":context,"input":{"kind":"unsigned","text":text}}),
    )
}
fn variable_assignments(transcript: &PathBuf) -> Vec<String> {
    fs::read_to_string(transcript)
        .unwrap()
        .lines()
        .filter(|l| l.starts_with("-var-assign "))
        .map(str::to_owned)
        .collect()
}
fn deferred_typed_variable_driver(mut project: Project, transcript: &PathBuf, input: Value) {
    project.version = 2;
    let directory = transcript.parent().unwrap();
    let config = directory.join("typed-project.toml");
    fs::write(&config, toml::to_string_pretty(&project).unwrap()).unwrap();
    let case = directory.join("typed-case.json");
    fs::write(&case,json!({"frame":0,"frame_function":"main","target":{"kind":"variable","pane":"watch","expression":"counter"},"probe":"counter","path_expression":"counter","input":input,"little_endian":true,"owner":"core:default","scope":"core","sentinels":["before","after"]}).to_string()).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = std::process::Command::new("node")
        .arg(root.join("scripts/test-variable-write-hardware.cjs"))
        .args([
            "--run",
            "--software-fixture",
            "--binary",
            env!("CARGO_BIN_EXE_debugtui"),
            "--project",
        ])
        .arg(config)
        .args(["--core", "default", "--fixture-function", "main", "--case"])
        .arg(case)
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
    let report_directory = stdout
        .lines()
        .find_map(|line| {
            line.strip_prefix("RESULT ")
                .and_then(|s| s.split_once("} ").map(|(_, d)| PathBuf::from(d)))
        })
        .unwrap();
    let report: Value =
        serde_json::from_str(&fs::read_to_string(report_directory.join("report.json")).unwrap())
            .unwrap();
    assert_eq!(report["board_tests_executed"], false);
    assert_eq!(
        variable_assignments(transcript).len(),
        2,
        "one assignment and one verified explicit restoration"
    );
    assert!(memory_writes(transcript).is_empty());
}
#[test]
fn variable_drafts_use_typed_assignment_cancel_replay_and_nonzero_frame() {
    let (project, transcript) = variable_fixture("variable typed", &[]);
    let engine = session::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    let (success, draft, error) = variable_preview(&engine, 2, "4294967295");
    assert!(success, "{error:?}");
    assert_eq!(draft["metadata"]["scalar"]["bits"], 32);
    ok(&engine, 4, "write_cancel", json!({"draft":draft["draft"]}));
    assert_eq!(
        ok(&engine, 5, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
        "not_sent"
    );
    assert!(variable_assignments(&transcript).is_empty());
    ok(&engine, 6, "frame", json!({"level":1}));
    let (success, draft, error) = variable_preview(&engine, 7, "4294967295");
    assert!(success, "{error:?}");
    let result = ok(&engine, 9, "write_apply", json!({"draft":draft["draft"]}));
    assert_eq!(result["outcome"], "verified", "{result}");
    assert_eq!(result["observed"]["hex"], "0xffffffff");
    assert_eq!(variable_assignments(&transcript).len(), 1);
    assert!(variable_assignments(&transcript)[0].contains("(__typeof__(counter))(0xffffffff)"));
    assert!(memory_writes(&transcript).is_empty());
    assert_eq!(
        ok(&engine, 10, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
        "not_sent"
    );
    ok(&engine, 11, "quit", json!({}));
}
#[test]
fn variable_preview_rejects_const_volatile_optimized_noneditable_mmio_and_unproven_storage() {
    for (flag, value) in [
        ("DEBUGTUI_TEST_VARIABLE_TYPE", "const unsigned int"),
        ("DEBUGTUI_TEST_VARIABLE_TYPE", "volatile unsigned int"),
        ("DEBUGTUI_TEST_VARIABLE_OPTIMIZED", "1"),
        ("DEBUGTUI_TEST_VARIABLE_NOT_EDITABLE", "1"),
        ("DEBUGTUI_TEST_VARIABLE_ADDRESS", "0x40000000"),
        ("DEBUGTUI_TEST_VARIABLE_ADDRESS", "0x8000000"),
        (
            "DEBUGTUI_TEST_VARIABLE_NO_ADDRESS",
            "Cannot take address of a bitfield",
        ),
        ("DEBUGTUI_TEST_NO_VARIABLE_WRITER", "1"),
        ("DEBUGTUI_TEST_MEMORY_READONLY", "1"),
    ] {
        let (project, transcript) =
            variable_fixture(&format!("variable reject {flag} {value}"), &[(flag, value)]);
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        let result = variable_preview(&engine, 2, "7");
        assert!(!result.0, "{flag} {value}: {result:?}");
        assert!(variable_assignments(&transcript).is_empty());
        let commands = fs::read_to_string(&transcript).unwrap();
        if flag == "DEBUGTUI_TEST_VARIABLE_ADDRESS"
            || flag == "DEBUGTUI_TEST_VARIABLE_TYPE"
            || flag == "DEBUGTUI_TEST_VARIABLE_NO_ADDRESS"
            || flag == "DEBUGTUI_TEST_MEMORY_READONLY"
        {
            let inspection = commands
                .split("-gdb-set may-call-functions off")
                .last()
                .unwrap();
            assert!(
                !inspection.contains("-var-create "),
                "Rejected storage must not fetch a root variable: {inspection}"
            );
        }
        if commands.contains("-gdb-set may-call-functions off") {
            assert!(commands.contains("-gdb-set may-call-functions on"));
        }
        ok(&engine, 4, "quit", json!({}));
    }
}
#[test]
fn variable_apply_rechecks_type_address_permissions_frame_and_explicit_watch_route() {
    for flag in [
        "DEBUGTUI_TEST_VARIABLE_TYPE_CHANGE",
        "DEBUGTUI_TEST_VARIABLE_ADDRESS_CHANGE",
        "DEBUGTUI_TEST_VARIABLE_PERMISSION_CHANGE",
        "frame-change",
    ] {
        let (project, transcript) =
            variable_fixture(&format!("variable change {flag}"), &[(flag, "1")]);
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        let (success, draft, error) = variable_preview(&engine, 2, "7");
        assert!(success, "{flag}: {error:?}");
        if flag == "frame-change" {
            ok(&engine, 4, "frame", json!({"level":1}));
        }
        let result = ok(&engine, 5, "write_apply", json!({"draft":draft["draft"]}));
        assert_eq!(result["outcome"], "not_sent", "{flag}: {result}");
        assert!(variable_assignments(&transcript).is_empty());
        ok(&engine, 6, "quit", json!({}));
    }
    let (mut project, transcript) = variable_fixture("variable AP route", &[]);
    project.ui.refresh.insert(
        "single|watch:counter".into(),
        debugtui::config::RefreshPolicy {
            channel: "ap".into(),
            ..Default::default()
        },
    );
    let engine = session::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    let result = variable_preview(&engine, 2, "7");
    assert!(!result.0 && result.2.unwrap().contains("explicit bus channel"));
    assert!(variable_assignments(&transcript).is_empty());
    ok(&engine, 4, "quit", json!({}));
}
#[test]
fn variable_post_send_outcomes_are_precise_and_never_retry_or_use_memory_fallback() {
    for (flag, value, expected) in [
        ("DEBUGTUI_TEST_VARIABLE_MISMATCH", "1", "mismatch"),
        ("DEBUGTUI_TEST_VARIABLE_VERIFY_ERROR", "1", "accepted"),
        ("DEBUGTUI_TEST_VARIABLE_WRITE_RUN", "1", "accepted"),
        ("DEBUGTUI_TEST_VARIABLE_CLEANUP_ERROR", "1", "verified"),
        ("DEBUGTUI_TEST_VARIABLE_WRITE_ERROR", "error", "unknown"),
        ("DEBUGTUI_TEST_VARIABLE_WRITE_ERROR", "closed", "unknown"),
        ("DEBUGTUI_TEST_VARIABLE_WRITE_ERROR", "timeout", "unknown"),
    ] {
        let (project, transcript) = variable_fixture(
            &format!("variable outcome {flag} {value}"),
            &[(flag, value)],
        );
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        let (success, draft, error) = variable_preview(&engine, 2, "7");
        assert!(success, "{error:?}");
        let result = ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}));
        assert_eq!(result["outcome"], expected, "{flag}: {result}");
        assert_eq!(variable_assignments(&transcript).len(), 1);
        assert!(memory_writes(&transcript).is_empty());
        assert_eq!(
            ok(&engine, 5, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
            "not_sent"
        );
        let _ = request(&engine, 6, "quit", json!({}));
    }
}
#[test]
fn late_stop_notification_cannot_clear_unknown_variable_write_fault() {
    let (project, transcript) = variable_fixture(
        "variable unknown late stop",
        &[("DEBUGTUI_TEST_VARIABLE_WRITE_ERROR", "error-stop")],
    );
    let engine = session::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    let (success, draft, error) = variable_preview(&engine, 2, "7");
    assert!(success, "{error:?}");
    assert_eq!(
        ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
        "unknown"
    );
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(ok(&engine, 5, "status", json!({}))["state"], "FAULT");
    let before = fs::read_to_string(&transcript).unwrap();
    assert!(!variable_preview(&engine, 6, "8").0);
    assert_eq!(fs::read_to_string(&transcript).unwrap(), before);
    assert_eq!(variable_assignments(&transcript).len(), 1);
    let _ = request(&engine, 8, "quit", json!({}));
}
#[test]
fn variable_function_policy_preserves_off_and_restoration_fault_blocks_further_inspection() {
    let (project, transcript) = variable_fixture(
        "variable calls already off",
        &[("DEBUGTUI_TEST_VARIABLE_CALLS_OFF", "1")],
    );
    let engine = session::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    let (success, draft, error) = variable_preview(&engine, 2, "7");
    assert!(success, "{error:?}");
    assert_eq!(
        ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
        "verified"
    );
    assert!(
        !fs::read_to_string(&transcript)
            .unwrap()
            .contains("-gdb-set may-call-functions")
    );
    ok(&engine, 5, "quit", json!({}));
    let (project, transcript) = variable_fixture(
        "variable function restore failure",
        &[("DEBUGTUI_TEST_VARIABLE_RESTORE_ERROR", "1")],
    );
    let engine = session::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    let result = variable_preview(&engine, 2, "7");
    assert!(!result.0 && result.2.unwrap().contains("restoration failed"));
    assert_eq!(ok(&engine, 4, "status", json!({}))["state"], "FAULT");
    let before = fs::read_to_string(&transcript).unwrap();
    assert!(!variable_preview(&engine, 5, "8").0);
    assert_eq!(fs::read_to_string(&transcript).unwrap(), before);
    assert!(variable_assignments(&transcript).is_empty());
    let _ = request(&engine, 7, "quit", json!({}));
}
#[test]
fn deferred_variable_board_case_uses_actual_binary_and_independent_ram_then_restores() {
    let (mut project, transcript) = variable_fixture("deferred variable driver", &[]);
    project.version = 2;
    let config = transcript.parent().unwrap().join("variable-project.toml");
    fs::write(&config, toml::to_string_pretty(&project).unwrap()).unwrap();
    let case_file = transcript.parent().unwrap().join("variable-case.json");
    fs::write(&case_file, json!({"frame":0,"frame_function":"main","target":{"kind":"variable","pane":"watch","expression":"counter"},"probe":"counter","input":{"kind":"unsigned","text":"0x12345678"},"little_endian":true,"owner":"core:default","scope":"core","sentinels":["before","after"]}).to_string()).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = std::process::Command::new("node")
        .arg(root.join("scripts/test-variable-write-hardware.cjs"))
        .args([
            "--run",
            "--software-fixture",
            "--binary",
            env!("CARGO_BIN_EXE_debugtui"),
            "--project",
        ])
        .arg(config)
        .arg("--case")
        .arg(case_file)
        .args(["--core", "default", "--fixture-function", "main"])
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
    let directory = stdout
        .lines()
        .find_map(|l| {
            l.strip_prefix("RESULT ")
                .and_then(|s| s.split_once("} ").map(|(_, p)| p))
        })
        .unwrap();
    let report: Value =
        serde_json::from_slice(&fs::read(PathBuf::from(directory).join("report.json")).unwrap())
            .unwrap();
    assert_eq!(report["board_tests_executed"], false);
    assert_eq!(
        variable_assignments(&transcript).len(),
        2,
        "one typed test write and one explicit verified restoration"
    );
    assert!(memory_writes(&transcript).is_empty());
}
#[test]
fn local_collapse_while_running_sends_no_target_inspection() {
    let (project, transcript) =
        variable_fixture("locals collapse", &[("DEBUGTUI_TEST_PAUSE", "running")]);
    let engine = session::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    ok(&engine, 2, "continue", json!({}));
    let before = fs::read_to_string(&transcript).unwrap();
    assert_eq!(
        ok(
            &engine,
            3,
            "local_expand",
            json!({"expression":"counter","expanded":false})
        )["expanded"],
        false
    );
    assert_eq!(fs::read_to_string(&transcript).unwrap(), before);
    let _ = request(&engine, 4, "quit", json!({}));
}

fn tcl_script(packet: &str) -> String {
    let quoted = packet
        .strip_prefix("set __dt_code [catch \"")
        .expect("generated RPC envelope");
    let mut script = String::new();
    let mut chars = quoted.chars();
    while let Some(c) = chars.next() {
        if c == '"' {
            break;
        }
        if c == '\\' {
            script.push(chars.next().unwrap());
        } else {
            script.push(c);
        }
    }
    script
}
fn bus_fixture(
    label: &str,
    big: bool,
    failure: &'static str,
) -> (
    Project,
    std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    std::thread::JoinHandle<()>,
) {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{Arc, Mutex},
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap().to_string();
    listener.set_nonblocking(true).unwrap();
    let commands = Arc::new(Mutex::new(Vec::new()));
    let trace = commands.clone();
    let worker = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(pair) => break pair,
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::park_timeout(Duration::from_millis(10))
                }
                Err(_) => return,
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut storage = 0xaa112201u32;
        let mut ram = vec![0xaau8; 4098];
        let mut written = false;
        let mut endian_queries = 0;
        loop {
            let mut packet = Vec::new();
            let mut byte = [0];
            loop {
                match stream.read(&mut byte) {
                    Ok(1) if byte[0] == 0x1a => break,
                    Ok(1) => packet.push(byte[0]),
                    _ => return,
                }
            }
            let command = tcl_script(std::str::from_utf8(&packet).unwrap());
            trace.lock().unwrap().push(command.clone());
            let mut code = 0;
            let value = if command == "\"cpu0\" curstate" {
                if failure == "running" {
                    "running".into()
                } else {
                    "halted".into()
                }
            } else if command == "\"ap\" cget -endian" {
                endian_queries += 1;
                if failure == "endian-change" && endian_queries > 1 {
                    "unknown".into()
                } else if big {
                    "big".into()
                } else {
                    "little".into()
                }
            } else if command.starts_with("\"ap\" read_memory ") {
                if command.starts_with("\"ap\" read_memory 0x1000") {
                    let words: Vec<_> = command.split_whitespace().collect();
                    assert_eq!(words[3], "8");
                    let base = u64::from_str_radix(words[2].trim_start_matches("0x"), 16).unwrap();
                    let start = (base - 0x100000000) as usize;
                    let count: usize = words[4].parse().unwrap();
                    ram[start..start + count]
                        .iter()
                        .map(u8::to_string)
                        .collect::<Vec<_>>()
                        .join(" ")
                } else {
                    assert!(command == "\"ap\" read_memory 0x40000000 32 1");
                    if failure == "verify-error" && written {
                        code = 1;
                        "readback unavailable".into()
                    } else {
                        format!("0x{:x}", if big { storage.swap_bytes() } else { storage })
                    }
                }
            } else if command.starts_with("\"ap\" write_memory ") {
                assert!(!command.contains(';'));
                let words: Vec<_> = command.split_whitespace().collect();
                if words[3] == "8" {
                    let base = u64::from_str_radix(words[2].trim_start_matches("0x"), 16).unwrap();
                    let start = (base - 0x100000000) as usize;
                    let values = words[4..]
                        .iter()
                        .map(|w| w.trim_matches(['{', '}']).parse::<u8>().unwrap())
                        .collect::<Vec<_>>();
                    ram[start..start + values.len()].copy_from_slice(&values);
                    String::new()
                } else {
                    assert_eq!(words[3], "32");
                    assert_eq!(words.len(), 5);
                    let raw = words[4].trim_matches(['{', '}']);
                    let native = u32::from_str_radix(raw.trim_start_matches("0x"), 16).unwrap();
                    let logical = if big { native.swap_bytes() } else { native };
                    if words[2] == "0x40000000" {
                        storage = (logical & 0xff0000ff)
                            | (storage & 0x00ff0000)
                            | ((storage & 0xff00) & !(logical & 0xff00));
                    } else {
                        assert!(matches!(words[2], "0x40000004" | "0x40000008"));
                    }
                    written = true;
                    if failure == "write-error" {
                        code = 1;
                        "error after device write".into()
                    } else {
                        String::new()
                    }
                }
            } else {
                panic!("Unexpected TCL command {command}")
            };
            stream
                .write_all(format!("__DEBUGTUI_RPC__{code}:{value}\x1a").as_bytes())
                .unwrap();
        }
    });
    let (mut project, _) = fixture(label, &[]);
    project.program.svd =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/write-mmio.svd");
    project.memory_access.push(debugtui::config::MemoryAccess {
        id: "ap".into(),
        target: "ap".into(),
        tcl_endpoint: endpoint,
        ..Default::default()
    });
    project.writes=toml::from_str("[[regions]]\nid='peripherals'\nkind='mmio'\nstart='0x40000000'\nend='0x40000100'\nchannel='ap'\nwidths=[8,16,32,64]\nlittle_endian=true\nhalted_targets=['cpu0']\n[[svd_overrides]]\nregister='PORT.MIXED'\nreserved='preserve'\nread_only_write='ignored'\n").unwrap();
    (project, commands, worker)
}
fn bus_preview(
    engine: &session::EngineHandle,
    id: u64,
    register: &str,
    selection: Value,
    text: &str,
) -> (bool, Value, Option<String>) {
    let context = ok(engine, id, "registers_list", json!({}))["context"].clone();
    request(
        engine,
        id + 1,
        "write_preview",
        json!({"target":{"kind":"peripheral","peripheral":"PORT","register":register,"channel":"ap"},"selection":selection,"context":context,"input":{"kind":"unsigned","text":text}}),
    )
}
#[test]
fn mmio_field_rmw_uses_one_native_word_preserves_reserved_and_neutralizes_w1c() {
    for big in [false, true] {
        let (project, commands, worker) = bus_fixture(&format!("mmio endian {big}"), big, "");
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        let (success, draft, error) = bus_preview(
            &engine,
            2,
            "MIXED",
            json!({"kind":"field","name":"RW"}),
            "5",
        );
        assert!(success, "{error:?}");
        assert!(
            !commands
                .lock()
                .unwrap()
                .iter()
                .any(|s| s.contains("read_memory") || s.contains("write_memory"))
        );
        let result = ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}));
        assert_eq!(result["outcome"], "verified", "{result}");
        assert_eq!(result["command_value"]["hex"], "0xaa000005");
        assert_eq!(result["observed"]["hex"], "0xaa112205");
        assert_eq!(
            commands
                .lock()
                .unwrap()
                .iter()
                .filter(|s| s.contains("write_memory"))
                .cloned()
                .collect::<Vec<_>>(),
            [format!(
                "\"ap\" write_memory 0x40000000 32 {{{}}}",
                if big { "0x050000aa" } else { "0xaa000005" }
            )]
        );
        assert_eq!(
            ok(&engine, 5, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
            "not_sent"
        );
        let (success, draft, error) =
            bus_preview(&engine, 6, "WO", json!({"kind":"register"}), "42");
        assert!(success, "{error:?}");
        let result = ok(&engine, 8, "write_apply", json!({"draft":draft["draft"]}));
        assert_eq!(result["outcome"], "accepted");
        assert!(
            !commands
                .lock()
                .unwrap()
                .iter()
                .any(|s| s.contains("read_memory 0x40000004"))
        );
        let (success, draft, error) =
            bus_preview(&engine, 9, "READ_CLEAR", json!({"kind":"register"}), "42");
        assert!(success, "{error:?}");
        assert_eq!(
            ok(&engine, 11, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
            "accepted"
        );
        assert!(
            !commands
                .lock()
                .unwrap()
                .iter()
                .any(|s| s.contains("read_memory 0x40000008"))
        );
        ok(&engine, 12, "quit", json!({}));
        drop(engine);
        worker.join().unwrap();
    }
}

#[test]
fn tcl_ram_bulk_write_verifies_4096_bytes_and_keeps_address_order_and_sentinels() {
    let (mut project, commands, worker) = bus_fixture("tcl ram", true, "");
    project.writes=toml::from_str("[[regions]]\nid='bus-ram'\nkind='ram'\nstart='0x100000001'\nend='0x100001001'\nchannel='ap'\nwidths=[8]\nbyte_writable=true\nhalted_targets=['cpu0']\n").unwrap();
    let engine = session::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    let context = ok(&engine, 2, "registers_list", json!({}))["context"].clone();
    let draft = ok(
        &engine,
        3,
        "write_preview",
        json!({"target":{"kind":"memory","address":"0x100000001","bits":32768,"channel":"ap"},"selection":{"kind":"register"},"input":{"kind":"bytes","text":"fe".repeat(4096)},"context":context}),
    );
    let result = ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}));
    assert_eq!(result["outcome"], "verified", "{result}");
    assert_eq!(result["observed_bytes"].as_array().unwrap().len(), 4096);
    for address in ["0x100000000", "0x100001001"] {
        let context = ok(&engine, 5, "registers_list", json!({}))["context"].clone();
        assert_eq!(
            ok(
                &engine,
                6,
                "memory_dump",
                json!({"address":address,"count":1,"channel":"ap","context":context})
            )["bytes"],
            json!([0xaa])
        );
    }
    let writes = commands
        .lock()
        .unwrap()
        .iter()
        .filter(|s| s.contains("write_memory"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(writes.len(), 1);
    assert!(writes[0].starts_with("\"ap\" write_memory 0x100000001 8 {254 254"));
    ok(&engine, 7, "quit", json!({}));
    drop(engine);
    worker.join().unwrap();
}
#[test]
fn mmio_rejects_unknown_reserved_ro_effect_wide_writer_and_physical_running_cpu() {
    let (mut project, commands, worker) = bus_fixture("mmio refused", false, "");
    project.writes.svd_overrides.clear();
    let engine = session::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    for (register, selection) in [
        ("MIXED", json!({"kind":"field","name":"RW"})),
        ("MIXED", json!({"kind":"field","name":"RO"})),
        ("WIDE", json!({"kind":"register"})),
    ] {
        let (success, _, error) = bus_preview(&engine, 2, register, selection, "5");
        assert!(!success, "{register}: {error:?}");
    }
    assert!(commands.lock().unwrap().is_empty());
    ok(&engine, 4, "quit", json!({}));
    drop(engine);
    worker.join().unwrap();
    // The preceding pure metadata rejections never connect to this listener.
    let (project, commands, worker) = bus_fixture("mmio running", false, "running");
    let engine = session::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    assert!(
        !bus_preview(
            &engine,
            2,
            "MIXED",
            json!({"kind":"field","name":"RW"}),
            "5"
        )
        .0
    );
    assert_eq!(*commands.lock().unwrap(), ["\"cpu0\" curstate"]);
    ok(&engine, 4, "quit", json!({}));
    drop(engine);
    worker.join().unwrap();
}
#[test]
fn mmio_apply_rechecks_endian_and_distinguishes_readback_failure_from_post_write_error() {
    for (failure, expected) in [
        ("endian-change", "not_sent"),
        ("verify-error", "accepted"),
        ("write-error", "unknown"),
    ] {
        let (project, commands, worker) = bus_fixture(failure, false, failure);
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        let (success, draft, error) = bus_preview(
            &engine,
            2,
            "MIXED",
            json!({"kind":"field","name":"RW"}),
            "5",
        );
        assert!(success, "{error:?}");
        let result = ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}));
        assert_eq!(result["outcome"], expected, "{result}");
        assert_eq!(
            commands
                .lock()
                .unwrap()
                .iter()
                .filter(|s| s.contains("write_memory"))
                .count(),
            usize::from(failure != "endian-change")
        );
        assert_eq!(
            ok(&engine, 5, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
            "not_sent"
        );
        let _ = request(&engine, 6, "quit", json!({}));
        drop(engine);
        worker.join().unwrap();
    }
}

#[test]
fn register_drafts_preview_cancel_apply_exact_sparse_index_and_do_not_replay() {
    let (project, transcript) = fixture("draft lifecycle", &[]);
    let engine = session::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    let draft = preview(&engine, 2, "4294967295");
    assert_eq!(draft["plan"]["value"]["hex"], "0xffffffff");
    assert_eq!(draft["outcome"], "not_sent");
    assert!(writes(&transcript).is_empty());
    let cancelled = ok(&engine, 4, "write_cancel", json!({"draft":draft["draft"]}));
    assert_eq!(cancelled["cancelled"], true);
    assert_eq!(
        ok(&engine, 5, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
        "not_sent"
    );
    assert!(writes(&transcript).is_empty());
    let next = preview(&engine, 6, "4294967295");
    let result = ok(&engine, 8, "write_apply", json!({"draft":next["draft"]}));
    assert_eq!(result["outcome"], "verified");
    assert_eq!(result["observed"]["hex"], "0xffffffff");
    assert_eq!(result["owner"], "default");
    assert_eq!(result["atomic"], false);
    assert!(
        result["context_after"]["generation"].as_u64().unwrap()
            > next["context"]["generation"].as_u64().unwrap()
    );
    assert_eq!(
        writes(&transcript),
        ["-data-write-register-values x 1 0xffffffff"]
    );
    assert_eq!(
        ok(&engine, 9, "write_apply", json!({"draft":next["draft"]}))["outcome"],
        "not_sent"
    );
    let sent_cancel = ok(&engine, 10, "write_cancel", json!({"draft":next["draft"]}));
    assert_eq!(sent_cancel["cancelled"], false);
    assert!(
        sent_cancel["outcome"].is_null(),
        "cancelling a sent draft must not claim it was not sent"
    );
    ok(&engine, 11, "quit", json!({}));
}
#[test]
fn permissions_input_frame_thread_and_reconnect_expire_before_any_write_request() {
    for (label, flags) in [
        ("rejections", vec![]),
        ("no writer", vec![("DEBUGTUI_TEST_NO_REGISTER_WRITER", "1")]),
        (
            "changed thread",
            vec![("DEBUGTUI_TEST_WRITE_THREAD_CHANGE", "1")],
        ),
        ("readonly", vec![("DEBUGTUI_TEST_REGISTER_READONLY", "1")]),
        (
            "permission changed",
            vec![("DEBUGTUI_TEST_WRITE_PERMISSION_CHANGE", "1")],
        ),
    ] {
        let (project, transcript) = fixture(label, &flags);
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        let listed = ok(&engine, 2, "registers_list", json!({}));
        let params = |id: &str, text: &str| json!({"target":{"kind":"register","id":id},"context":listed["context"],"selection":{"kind":"register"},"input":{"kind":"unsigned","text":text}});
        for (id, text) in [
            ("midr", "1"),
            ("cpsr", "1"),
            ("r0", "4294967296"),
            ("r0", "1\n-exec-continue"),
            ("r0", "func()"),
        ] {
            assert!(!request(&engine, 3, "write_preview", params(id, text)).0);
        }
        let (success, draft, error) = request(&engine, 4, "write_preview", params("r0", "1"));
        if label == "readonly" {
            assert!(!success);
            assert!(error.unwrap().contains("disabled"));
        } else if label == "no writer" {
            assert!(!success);
            assert!(error.unwrap().contains("does not implement"));
        } else if matches!(label, "changed thread" | "permission changed") {
            assert!(success);
            assert_eq!(
                ok(&engine, 5, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
                "not_sent"
            );
        } else {
            assert!(success);
            ok(&engine, 6, "frame", json!({"level":1}));
            assert_eq!(
                ok(&engine, 7, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
                "not_sent"
            );
            ok(&engine, 8, "frame", json!({"level":0}));
            let draft = preview(&engine, 9, "1");
            ok(&engine, 11, "reconnect", json!({}));
            assert_eq!(
                ok(&engine, 12, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
                "not_sent"
            );
        }
        assert!(writes(&transcript).is_empty());
        ok(&engine, 13, "quit", json!({}));
    }
}
#[test]
fn accepted_mismatch_verification_error_and_post_send_failures_have_distinct_results_without_retries()
 {
    for (label, key, flag, expected) in [
        ("mismatch", "DEBUGTUI_TEST_WRITE_MISMATCH", "1", "mismatch"),
        (
            "verification unavailable",
            "DEBUGTUI_TEST_WRITE_VERIFY_ERROR",
            "1",
            "accepted",
        ),
        ("running", "DEBUGTUI_TEST_WRITE_RUN", "1", "accepted"),
        (
            "error after write",
            "DEBUGTUI_TEST_WRITE_ERROR",
            "error",
            "unknown",
        ),
        (
            "lost reply",
            "DEBUGTUI_TEST_WRITE_ERROR",
            "closed",
            "unknown",
        ),
        (
            "timed out",
            "DEBUGTUI_TEST_WRITE_ERROR",
            "timeout",
            "unknown",
        ),
    ] {
        let (project, transcript) = fixture(label, &[(key, flag)]);
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        let draft = preview(&engine, 2, "0x87654321");
        let result = ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}));
        assert_eq!(result["outcome"], expected, "{label}: {result}");
        assert_eq!(writes(&transcript).len(), 1);
        assert_eq!(
            ok(&engine, 5, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
            "not_sent"
        );
        assert_eq!(writes(&transcript).len(), 1);
        if expected == "unknown" {
            assert_eq!(ok(&engine, 6, "status", json!({}))["state"], "FAULT");
        }
        let _ = request(&engine, 7, "quit", json!({}));
    }
}
#[test]
fn scope_all_writes_only_the_selected_core_and_switching_away_and_back_discards_drafts() {
    let (mut project, transcript) = fixture("two cores", &[]);
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
    project.multicore.scope = ControlScope::All;
    let engine = coordinator::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    ok(&engine, 2, "select_core", json!({"index":0}));
    let draft = preview(&engine, 3, "0x12345678");
    assert_eq!(draft["owner"], "core0");
    ok(&engine, 5, "select_core", json!({"index":1}));
    ok(&engine, 6, "select_core", json!({"index":0}));
    assert_eq!(
        ok(&engine, 7, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
        "not_sent"
    );
    assert!(writes(&transcript).is_empty());
    let draft = preview(&engine, 8, "0x12345678");
    let result = ok(&engine, 10, "write_apply", json!({"draft":draft["draft"]}));
    assert_eq!(result["outcome"], "verified");
    assert_eq!(result["owner"], "core0");
    assert_eq!(
        writes(&transcript),
        ["-data-write-register-values x 1 0x12345678"]
    );
    ok(&engine, 11, "quit", json!({}));
}

#[test]
fn deferred_board_write_case_runs_against_software_fixture_without_claiming_board_evidence() {
    let (mut project, transcript) = fixture("deferred case driver", &[]);
    project.version = 2;
    project.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTERS".into(),
        json!(["r0", "r1", "r2", "r3"]).to_string(),
    );
    let config = transcript.parent().unwrap().join("project.toml");
    fs::write(&config, toml::to_string_pretty(&project).unwrap()).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = std::process::Command::new("node")
        .arg(root.join("scripts/test-register-write-hardware.cjs"))
        .args([
            "--run",
            "--software-fixture",
            "--binary",
            env!("CARGO_BIN_EXE_debugtui"),
            "--project",
        ])
        .arg(config)
        .args(["--core", "default", "--fixture-function", "main"])
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
    let directory = stdout
        .lines()
        .find_map(|line| {
            line.strip_prefix("RESULT ").and_then(|s| {
                s.split_once("} ")
                    .map(|(_, directory)| PathBuf::from(directory))
            })
        })
        .unwrap();
    let report: Value =
        serde_json::from_str(&fs::read_to_string(directory.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["board_tests_executed"], false);
    assert_eq!(report["layer"], "local MI software fixture; no board");
    let count = writes(&transcript).len();
    assert_eq!(
        count, 2,
        "one verified write and one explicit restoration; cancel/replay send none"
    );
}

#[test]
fn deferred_ram_board_case_runs_actual_binary_on_software_fixture_and_restores_range() {
    let (mut project, transcript) = ram_fixture("deferred ram driver", &[]);
    project.version = 2;
    let config = transcript.parent().unwrap().join("ram-project.toml");
    fs::write(&config, toml::to_string_pretty(&project).unwrap()).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = std::process::Command::new("node")
        .arg(root.join("scripts/test-memory-write-hardware.cjs"))
        .args([
            "--run",
            "--software-fixture",
            "--binary",
            env!("CARGO_BIN_EXE_debugtui"),
            "--project",
        ])
        .arg(config)
        .args([
            "--core",
            "default",
            "--fixture-function",
            "main",
            "--address",
            "0x100000004",
            "--bytes",
            "12345678",
        ])
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
    let directory = stdout
        .lines()
        .find_map(|l| {
            l.strip_prefix("RESULT ")
                .and_then(|s| s.split_once("} ").map(|(_, path)| path))
        })
        .unwrap();
    let report: Value =
        serde_json::from_slice(&fs::read(PathBuf::from(directory).join("report.json")).unwrap())
            .unwrap();
    assert_eq!(report["board_tests_executed"], false);
    assert_eq!(
        memory_writes(&transcript),
        [
            "-data-write-memory-bytes 0x100000004 12345678",
            "-data-write-memory-bytes 0x100000004 aaaaaaaa"
        ]
    );
}
