use super::*;
fn bitfield_fixture(label: &str, flags: &[(&str, &str)], little: bool) -> (Project, PathBuf) {
    let (mut project, transcript) = variable_fixture(
        label,
        &[
            ("DEBUGTUI_TEST_BITFIELD_VARIABLE", "1"),
            ("DEBUGTUI_TEST_VARIABLE_ADDRESS", "0x100000000"),
        ],
    );
    for (key, value) in flags {
        project.gdb.env.insert((*key).into(), (*value).into());
    }
    if !little {
        project
            .gdb
            .env
            .insert("DEBUGTUI_TEST_BITFIELD_BIG".into(), "1".into());
    }
    project.writes.regions[0].little_endian = Some(little);
    project.writes.regions[0].start = "0x100000000".into();
    (project, transcript)
}
fn field_preview(
    engine: &session::EngineHandle,
    id: u64,
    value: &str,
) -> (bool, Value, Option<String>) {
    let context = ok(engine, id, "registers_list", json!({}))["context"].clone();
    request(
        engine,
        id + 1,
        "write_preview",
        json!({"target":{"kind":"variable","pane":"watch","expression":"counter","path":[2]},"selection":{"kind":"register"},"context":context,"input":{"kind":"signed","text":value}}),
    )
}
#[test]
fn bitfield_actual_width_signed_extension_and_fresh_neighbours_are_preserved_in_both_orders() {
    for little in [true, false] {
        let (project, transcript) = bitfield_fixture(
            &format!("field fresh {little}"),
            &[("DEBUGTUI_TEST_BITFIELD_FAILURE", "fresh-neighbour")],
            little,
        );
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        let (accepted, draft, error) = field_preview(&engine, 2, "-32");
        assert!(accepted, "{error:?}");
        assert_eq!(draft["metadata"]["scalar"]["bits"], 6);
        assert_eq!(draft["metadata"]["bitfield"]["layout"]["bit_offset"], 37);
        assert_eq!(draft["plan"]["selected_mask"]["hex"], "0x3f");
        assert_eq!(draft["plan"]["needs_fresh_read"], true);
        assert!(variable_assignments(&transcript).is_empty());
        let result = ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}));
        assert_eq!(result["outcome"], "verified", "{result}");
        assert_eq!(result["observed"]["hex"], "0x20");
        assert_eq!(result["neighbours_preserved"], true);
        // Neighbour was 0x55 at Preview and 7 at Apply. Expected bytes must use 7.
        let bytes: Vec<u8> = serde_json::from_value(result["parent_after_bytes"].clone()).unwrap();
        let word = if little {
            u32::from_le_bytes(bytes[4..8].try_into().unwrap())
        } else {
            u32::from_be_bytes(bytes[4..8].try_into().unwrap())
        };
        assert_eq!(
            if little {
                (word >> 11) & 127
            } else {
                (word >> 14) & 127
            },
            7
        );
        assert_eq!(variable_assignments(&transcript).len(), 1);
        assert!(memory_writes(&transcript).is_empty());
        assert_eq!(
            ok(&engine, 5, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
            "not_sent"
        );
        ok(&engine, 6, "quit", json!({}));
    }
}
#[test]
fn bitfields_reject_range_layout_endian_storage_and_inconsistent_extension_before_writing() {
    for mode in [
        "no-layout",
        "layout-change",
        "endian",
        "extension",
        "read",
        "running",
    ] {
        let (project, transcript) = bitfield_fixture(
            &format!("field refused {mode}"),
            &[("DEBUGTUI_TEST_BITFIELD_FAILURE", mode)],
            true,
        );
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        assert!(!field_preview(&engine, 2, "-32").0, "{mode}");
        assert!(variable_assignments(&transcript).is_empty());
        assert!(memory_writes(&transcript).is_empty());
        let _ = request(&engine, 4, "quit", json!({}));
    }
    for value in ["32", "-33"] {
        let (project, transcript) = bitfield_fixture(&format!("field overflow {value}"), &[], true);
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        assert!(!field_preview(&engine, 2, value).0);
        assert!(variable_assignments(&transcript).is_empty());
        ok(&engine, 4, "quit", json!({}));
    }
    for kind in ["missing-endian", "shared-owner", "insufficient-range"] {
        let (mut project, transcript) =
            bitfield_fixture(&format!("field policy {kind}"), &[], true);
        if kind == "missing-endian" {
            project.writes.regions[0].little_endian = None;
        }
        if kind == "shared-owner" {
            project.writes.regions[0].scope = debugtui::registers::Scope::Chip;
        }
        if kind == "insufficient-range" {
            project.writes.regions[0].end = "0x100000008".into();
        }
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        assert!(!field_preview(&engine, 2, "-32").0, "{kind}");
        assert!(variable_assignments(&transcript).is_empty());
        ok(&engine, 4, "quit", json!({}));
    }
}
#[test]
fn bitfield_apply_rechecks_layout_readability_and_cancel_without_sending() {
    for mode in ["layout-change", "parent-change", "read", "running"] {
        let (project, transcript) = bitfield_fixture(
            &format!("field changed {mode}"),
            &[
                ("DEBUGTUI_TEST_BITFIELD_FAILURE", mode),
                ("DEBUGTUI_TEST_BITFIELD_APPLY_ONLY", "1"),
            ],
            true,
        );
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        let (accepted, draft, error) = field_preview(&engine, 2, "-32");
        assert!(accepted, "{error:?}");
        assert_eq!(
            ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
            "not_sent"
        );
        assert!(variable_assignments(&transcript).is_empty());
        let _ = request(&engine, 5, "quit", json!({}));
    }
    for flag in [
        "DEBUGTUI_TEST_VARIABLE_PERMISSION_CHANGE",
        "DEBUGTUI_TEST_VARIABLE_ADDRESS_CHANGE",
    ] {
        let (project, transcript) =
            bitfield_fixture(&format!("field changed {flag}"), &[(flag, "1")], true);
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        let (accepted, draft, error) = field_preview(&engine, 2, "-32");
        assert!(accepted, "{error:?}");
        assert_eq!(
            ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
            "not_sent",
            "{flag}"
        );
        assert!(variable_assignments(&transcript).is_empty());
        ok(&engine, 5, "quit", json!({}));
    }
    let (project, transcript) = bitfield_fixture("field cancel", &[], true);
    let engine = session::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    let (accepted, draft, error) = field_preview(&engine, 2, "-32");
    assert!(accepted, "{error:?}");
    ok(&engine, 4, "write_cancel", json!({"draft":draft["draft"]}));
    assert_eq!(
        ok(&engine, 5, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
        "not_sent"
    );
    assert!(variable_assignments(&transcript).is_empty());
    ok(&engine, 6, "quit", json!({}));
}
#[test]
fn bitfield_parent_mismatch_readback_unknown_and_cleanup_results_never_retry() {
    for (mode, outcome) in [
        ("neighbour", "mismatch"),
        ("field-mismatch", "mismatch"),
        ("verify", "accepted"),
        ("unknown", "unknown"),
        ("cleanup", "verified"),
    ] {
        let (project, transcript) = bitfield_fixture(
            &format!("field outcome {mode}"),
            &[("DEBUGTUI_TEST_BITFIELD_FAILURE", mode)],
            true,
        );
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        let (accepted, draft, error) = field_preview(&engine, 2, "-32");
        assert!(accepted, "{error:?}");
        let result = ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}));
        assert_eq!(result["outcome"], outcome, "{result}");
        if mode == "neighbour" {
            assert_eq!(result["neighbours_preserved"], false);
            assert_eq!(result["observed"]["hex"], "0x20");
        }
        if mode == "field-mismatch" {
            assert_eq!(result["neighbours_preserved"], true);
            assert_eq!(result["parent_matches_expected"], false);
            assert_eq!(result["observed"]["hex"], "0x21");
        }
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
fn bitfield_scope_all_writes_one_selected_parent_and_peer_remains_unchanged() {
    let (mut project, transcript) = bitfield_fixture("field scope all", &[], true);
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
    let (accepted, draft, error) = field_preview(&engine, 3, "-32");
    assert!(accepted, "{error:?}");
    assert_eq!(draft["owner"], "core:core0");
    assert_eq!(
        ok(&engine, 5, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
        "verified"
    );
    assert_eq!(variable_assignments(&transcript).len(), 1);
    ok(&engine, 6, "select_core", json!({"index":1}));
    assert_eq!(
        ok(
            &engine,
            7,
            "evaluate",
            json!({"expression":"counter.middle"})
        )["value"],
        "-2"
    );
    ok(&engine, 8, "quit", json!({}));
}
#[test]
fn deferred_bitfield_case_runs_actual_binary_with_independent_parent_bytes_and_verified_restore() {
    let (mut project, transcript) = bitfield_fixture("field deferred driver", &[], true);
    project.version = 2;
    let directory = transcript.parent().unwrap();
    let config = directory.join("project.toml");
    let case = directory.join("bitfield.json");
    fs::write(&config, toml::to_string_pretty(&project).unwrap()).unwrap();
    fs::write(&case,json!({"frame":0,"frame_function":"main","target":{"kind":"variable","pane":"watch","expression":"counter","path":[2]},"parent":"counter","field":"counter.middle","expected_layout":{"bit_offset":37,"bits":6,"declared_bytes":4,"parent_bytes":12},"input":{"kind":"signed","text":"-32"},"little_endian":true,"owner":"core:default","scope":"core","sentinels":["counter.before","counter.after"]}).to_string()).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = std::process::Command::new("node")
        .arg(root.join("scripts/test-variable-bitfield-hardware.cjs"))
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
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("\"passed\":5,\"failed\":0,\"skipped\":0"),
        "{stdout}"
    );
    let directory = stdout
        .lines()
        .find_map(|line| {
            line.strip_prefix("RESULT ")
                .and_then(|s| s.split_once("} ").map(|(_, p)| PathBuf::from(p)))
        })
        .unwrap();
    let report: Value =
        serde_json::from_str(&fs::read_to_string(directory.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["board_tests_executed"], false);
    assert_eq!(variable_assignments(&transcript).len(), 2);
    assert!(memory_writes(&transcript).is_empty());
}
