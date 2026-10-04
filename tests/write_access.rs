#![cfg(windows)]
//! End-to-end writes through the actual worker/MI pipes, without a board.
use debugtui::{
    config::{ControlScope, Core, Project},
    coordinator,
    session::{self, Event, Request},
};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, time::Duration};

fn fixture(label: &str, flags: &[(&str, &str)]) -> (Project, PathBuf) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let directory = root
        .join("artifacts")
        .join(format!("write {label} {}", std::process::id()));
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
