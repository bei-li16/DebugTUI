use super::*;

fn float_fixture(
    label: &str,
    bits: u16,
    endian: &str,
    failure: &str,
    apply_only: bool,
) -> (Project, PathBuf) {
    variable_fixture(
        label,
        &[
            ("DEBUGTUI_TEST_FLOAT_VARIABLE", "1"),
            (
                "DEBUGTUI_TEST_VARIABLE_ADDRESS",
                if bits == 32 {
                    "0x100000004"
                } else {
                    "0x100000008"
                },
            ),
            (
                "DEBUGTUI_TEST_FLOAT_WIDTH",
                if bits == 32 { "32" } else { "64" },
            ),
            ("DEBUGTUI_TEST_FLOAT_ENDIAN", endian),
            ("DEBUGTUI_TEST_FLOAT_FAILURE", failure),
            (
                "DEBUGTUI_TEST_FLOAT_APPLY_ONLY",
                if apply_only { "1" } else { "" },
            ),
        ],
    )
}
fn float_preview(
    engine: &session::EngineHandle,
    id: u64,
    bits: u16,
) -> (bool, Value, Option<String>) {
    let context = ok(engine, id, "registers_list", json!({}))["context"].clone();
    let text = if bits == 32 {
        "12 00 c0 7f"
    } else {
        "12 00 00 00 00 00 f8 7f"
    };
    request(
        engine,
        id + 1,
        "write_preview",
        json!({"target":{"kind":"variable","pane":"watch","expression":"counter"},"selection":{"kind":"register"},"context":context,"input":{"kind":"bytes","text":text,"little_endian":true}}),
    )
}
fn expected(bits: u16) -> &'static str {
    if bits == 32 {
        "0x7fc00012"
    } else {
        "0x7ff8000000000012"
    }
}
fn assert_literal_cleanup(transcript: &PathBuf) {
    let log = fs::read_to_string(transcript).unwrap();
    assert_eq!(
        log.lines()
            .filter(|l| l.contains("python import gdb;"))
            .count(),
        log.lines()
            .filter(|l| l.contains("python gdb.set_convenience_variable(") && l.contains(", None)"))
            .count()
    );
    assert!(!log.contains("-data-write-memory"));
}

#[test]
fn exact_float_host_buffers_follow_target_endian_and_cancel_does_not_assign() {
    for bits in [32, 64] {
        for endian in ["little", "big"] {
            let (project, transcript) =
                float_fixture(&format!("float {bits} {endian}"), bits, endian, "", false);
            let engine = session::spawn(project);
            ok(&engine, 1, "connect", json!({}));
            let (accepted, draft, error) = float_preview(&engine, 2, bits);
            assert!(accepted, "{error:?}");
            assert_eq!(draft["literal"]["kind"], "exact_float_bits");
            assert_eq!(draft["plan"]["value"]["hex"], expected(bits));
            ok(&engine, 4, "write_cancel", json!({"draft":draft["draft"]}));
            assert_eq!(
                ok(&engine, 5, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
                "not_sent"
            );
            assert!(variable_assignments(&transcript).is_empty());
            let (accepted, draft, error) = float_preview(&engine, 6, bits);
            assert!(accepted, "{error:?}");
            let result = ok(&engine, 8, "write_apply", json!({"draft":draft["draft"]}));
            assert_eq!(result["outcome"], "verified", "{result}");
            assert_eq!(result["observed"]["hex"], expected(bits));
            let context = ok(&engine, 9, "registers_list", json!({}))["context"].clone();
            let memory = ok(
                &engine,
                10,
                "memory_dump",
                json!({"address":if bits==32 {"0x100000004"} else {"0x100000008"},"count":bits/8,"context":context}),
            );
            let mut bytes = if bits == 32 {
                vec![0x12, 0, 0xc0, 0x7f]
            } else {
                vec![0x12, 0, 0, 0, 0, 0, 0xf8, 0x7f]
            };
            if endian == "big" {
                bytes.reverse();
            }
            assert_eq!(memory["bytes"], json!(bytes));
            assert_eq!(variable_assignments(&transcript).len(), 1);
            assert_literal_cleanup(&transcript);
            ok(&engine, 11, "quit", json!({}));
        }
    }
}
#[test]
fn unsupported_or_unproven_float_literal_preview_sends_no_assignment() {
    for failure in [
        "python",
        "type-size",
        "collision",
        "endian",
        "wrong-bits",
        "literal-read",
        "literal-delete",
        "wrong-bits+cleanup",
    ] {
        let (project, transcript) = float_fixture(
            &format!("float reject {failure}"),
            32,
            "little",
            failure,
            false,
        );
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        let (accepted, _, error) = float_preview(&engine, 2, 32);
        assert!(!accepted, "{failure}");
        assert!(error.is_some());
        if failure == "wrong-bits+cleanup" {
            let text = error.unwrap();
            assert!(
                text.contains("mismatch") && text.contains("cleanup"),
                "{text}"
            );
        }
        assert!(variable_assignments(&transcript).is_empty());
        // A rejected host constant leaves the session available for ordinary reads.
        assert_eq!(
            ok(&engine, 4, "evaluate", json!({"expression":"before"}))["value"],
            "170"
        );
        let log = fs::read_to_string(&transcript).unwrap();
        assert!(log.contains("-gdb-set may-call-functions on"));
        if failure == "collision" {
            assert!(!log.contains("python gdb.set_convenience_variable("));
        }
        assert!(!log.contains("-data-write-memory"));
        ok(&engine, 5, "quit", json!({}));
    }
}
#[test]
fn float_literal_is_rechecked_at_apply_and_running_during_probe_prevents_assignment() {
    for failure in [
        "python",
        "type-size",
        "endian",
        "wrong-bits",
        "literal-read",
        "literal-delete",
        "running-after-probe",
    ] {
        let (project, transcript) = float_fixture(
            &format!("float apply reject {failure}"),
            32,
            "little",
            failure,
            true,
        );
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        let (accepted, draft, error) = float_preview(&engine, 2, 32);
        assert!(accepted, "{failure}: {error:?}");
        let result = ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}));
        assert_eq!(result["outcome"], "not_sent", "{failure}: {result}");
        assert_eq!(result["code"], "precondition_failed");
        assert!(variable_assignments(&transcript).is_empty());
        assert_eq!(
            ok(&engine, 5, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
            "not_sent"
        );
        let _ = request(&engine, 6, "quit", json!({}));
    }
    let (project, transcript) = float_fixture(
        "float arithmetic changed",
        32,
        "little",
        "change-expression",
        true,
    );
    let engine = session::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    let context = ok(&engine, 2, "registers_list", json!({}))["context"].clone();
    let draft = ok(
        &engine,
        3,
        "write_preview",
        json!({"target":{"kind":"variable","pane":"watch","expression":"counter"},"selection":{"kind":"register"},"context":context,"input":{"kind":"float","text":"Infinity"}}),
    );
    assert_eq!(draft["literal"]["kind"], "expression");
    let result = ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}));
    assert_eq!(result["outcome"], "not_sent");
    assert!(variable_assignments(&transcript).is_empty());
    ok(&engine, 5, "quit", json!({}));
}
#[test]
fn float_post_send_failures_report_actual_outcome_and_never_replay() {
    for (failure, outcome) in [
        ("cleanup", "verified"),
        ("readback", "accepted"),
        ("unknown", "unknown"),
        ("unknown+cleanup", "unknown"),
    ] {
        let (project, transcript) =
            float_fixture(&format!("float sent {failure}"), 64, "big", failure, true);
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        let (accepted, draft, error) = float_preview(&engine, 2, 64);
        assert!(accepted, "{error:?}");
        let result = ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}));
        assert_eq!(result["outcome"], outcome, "{failure}: {result}");
        assert!(result["error"].is_string());
        if failure == "unknown+cleanup" {
            let text = result["error"].as_str().unwrap();
            assert!(
                text.contains("after float assignment") && text.contains("cleanup"),
                "{text}"
            );
        }
        assert_eq!(variable_assignments(&transcript).len(), 1);
        assert_literal_cleanup(&transcript);
        assert_eq!(
            ok(&engine, 5, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
            "not_sent"
        );
        let _ = request(&engine, 6, "quit", json!({}));
    }
}
#[test]
fn special_float_scope_all_uses_only_current_worker_and_preserves_peer_bits() {
    let (mut project, transcript) = float_fixture("float scope all", 32, "little", "", false);
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
    let (accepted, draft, error) = float_preview(&engine, 3, 32);
    assert!(accepted, "{error:?}");
    assert_eq!(draft["owner"], "core:core0");
    let result = ok(&engine, 5, "write_apply", json!({"draft":draft["draft"]}));
    assert_eq!(result["outcome"], "verified");
    assert_eq!(variable_assignments(&transcript).len(), 1);
    assert_literal_cleanup(&transcript);
    let before = fs::read_to_string(&transcript)
        .unwrap()
        .lines()
        .filter(|l| l.contains("python import gdb;"))
        .count();
    ok(&engine, 6, "select_core", json!({"index":1}));
    let context = ok(&engine, 7, "registers_list", json!({}))["context"].clone();
    let read = ok(
        &engine,
        8,
        "memory_dump",
        json!({"address":"0x100000004","count":4,"context":context}),
    );
    assert_eq!(read["bytes"], json!([0, 0, 0x80, 0x3f]));
    assert_eq!(
        before,
        fs::read_to_string(&transcript)
            .unwrap()
            .lines()
            .filter(|l| l.contains("python import gdb;"))
            .count()
    );
    ok(&engine, 9, "quit", json!({}));
}
#[test]
fn deferred_special_float_case_runs_actual_binary_and_restores_after_independent_reads() {
    for (bits, endian) in [(32, "little"), (64, "big")] {
        let (mut project, transcript) =
            float_fixture(&format!("float driver {bits}"), bits, endian, "", false);
        project.version = 2;
        let directory = transcript.parent().unwrap();
        let config = directory.join("float-project.toml");
        fs::write(&config, toml::to_string_pretty(&project).unwrap()).unwrap();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut specification: Value = serde_json::from_str(
            &fs::read_to_string(root.join(format!(
                "tests/fixtures/variable-float{bits}-board.example.json"
            )))
            .unwrap(),
        )
        .unwrap();
        specification["frame_function"] = json!("main");
        specification["target"] = json!({"kind":"variable","pane":"watch","expression":"counter"});
        specification["probe"] = json!("counter");
        specification["path_expression"] = json!("counter");
        specification["little_endian"] = json!(endian == "little");
        specification["owner"] = json!("core:default");
        specification["sentinels"] = json!(["before", "after"]);
        let case = directory.join("float-case.json");
        fs::write(&case, serde_json::to_string_pretty(&specification).unwrap()).unwrap();
        let output = std::process::Command::new("node")
            .arg(root.join("scripts/test-variable-write-hardware.cjs"))
            .args([
                "--run",
                "--software-fixture",
                "--special-floats",
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
            stdout.contains("\"passed\":6,\"failed\":0,\"skipped\":0"),
            "{stdout}"
        );
        let report_directory = stdout
            .lines()
            .find_map(|line| {
                line.strip_prefix("RESULT ")
                    .and_then(|s| s.split_once("} ").map(|(_, d)| PathBuf::from(d)))
            })
            .unwrap();
        let report: Value = serde_json::from_str(
            &fs::read_to_string(report_directory.join("report.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(report["board_tests_executed"], false);
        assert_eq!(
            variable_assignments(&transcript).len(),
            8,
            "one initial assignment, six samples, one explicit restoration"
        );
        assert_literal_cleanup(&transcript);
    }
}
