use super::*;

fn reference_fixture(label: &str, flags: &[(&str, &str)]) -> (Project, PathBuf) {
    let (mut project, transcript) =
        variable_fixture(label, &[("DEBUGTUI_TEST_VARIABLE_TYPE", "unsigned int &")]);
    for (key, value) in flags {
        project.gdb.env.insert((*key).into(), (*value).into());
    }
    (project, transcript)
}
#[test]
fn reference_drafts_keep_qualifiers_referent_storage_and_one_typed_assignment() {
    let (project, transcript) = reference_fixture("reference typed", &[]);
    let engine = session::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    let (accepted, draft, error) = variable_preview(&engine, 2, "4294967295");
    assert!(accepted, "{error:?}");
    assert_eq!(draft["metadata"]["reference"], true);
    assert_eq!(draft["metadata"]["type_name"], "unsigned int &");
    assert_eq!(draft["metadata"]["address"], "0x100000004");
    ok(&engine, 4, "write_cancel", json!({"draft":draft["draft"]}));
    assert_eq!(
        ok(&engine, 5, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
        "not_sent"
    );
    assert!(variable_assignments(&transcript).is_empty());
    let (accepted, draft, error) = variable_preview(&engine, 6, "4294967295");
    assert!(accepted, "{error:?}");
    let result = ok(&engine, 8, "write_apply", json!({"draft":draft["draft"]}));
    assert_eq!(result["outcome"], "verified");
    assert_eq!(result["observed"]["hex"], "0xffffffff");
    let writes = variable_assignments(&transcript);
    assert_eq!(writes.len(), 1);
    assert!(writes[0].contains("(__typeof__(*(&(counter))))(0xffffffff)"));
    assert!(memory_writes(&transcript).is_empty());
    assert_eq!(
        ok(&engine, 9, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
        "not_sent"
    );
    ok(&engine, 10, "quit", json!({}));
}
#[test]
fn references_reject_non_writable_referents_and_recheck_binding_type_and_permission() {
    for (index, (flag, value)) in [
        ("DEBUGTUI_TEST_VARIABLE_TYPE", "const unsigned int &"),
        ("DEBUGTUI_TEST_VARIABLE_TYPE", "volatile unsigned int &"),
        ("DEBUGTUI_TEST_VARIABLE_TYPE", "int * const &"),
        ("DEBUGTUI_TEST_VARIABLE_ADDRESS", "0x8000004"),
        ("DEBUGTUI_TEST_VARIABLE_ADDRESS", "0x40000000"),
        ("DEBUGTUI_TEST_MEMORY_READONLY", "1"),
        ("DEBUGTUI_TEST_VARIABLE_OPTIMIZED", "1"),
        (
            "DEBUGTUI_TEST_VARIABLE_NO_ADDRESS",
            "Cannot access referent address",
        ),
        (
            "DEBUGTUI_TEST_VARIABLE_NO_ADDRESS",
            "Referenced value is in register",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let (project, transcript) = reference_fixture(
            &format!("reference refused {index} {flag}"),
            &[(flag, value)],
        );
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        assert!(!variable_preview(&engine, 2, "7").0, "{flag} {value}");
        assert!(variable_assignments(&transcript).is_empty());
        assert!(memory_writes(&transcript).is_empty());
        ok(&engine, 4, "quit", json!({}));
    }
    for flag in [
        "DEBUGTUI_TEST_VARIABLE_ADDRESS_CHANGE",
        "DEBUGTUI_TEST_VARIABLE_TYPE_CHANGE",
        "DEBUGTUI_TEST_VARIABLE_PERMISSION_CHANGE",
    ] {
        let (project, transcript) =
            reference_fixture(&format!("reference changed {flag}"), &[(flag, "1")]);
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        let (accepted, draft, error) = variable_preview(&engine, 2, "7");
        assert!(accepted, "{error:?}");
        let result = ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}));
        assert_eq!(result["outcome"], "not_sent", "{result}");
        assert!(variable_assignments(&transcript).is_empty());
        ok(&engine, 5, "quit", json!({}));
    }
}
#[test]
fn reference_scope_all_writes_only_current_referent_and_preserves_peer() {
    let (mut project, transcript) = reference_fixture("reference scope all", &[]);
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
    let (accepted, draft, error) = variable_preview(&engine, 3, "50");
    assert!(accepted, "{error:?}");
    assert_eq!(draft["owner"], "core:core0");
    assert_eq!(
        ok(&engine, 5, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
        "verified"
    );
    assert_eq!(variable_assignments(&transcript).len(), 1);
    ok(&engine, 6, "select_core", json!({"index":1}));
    assert_eq!(
        ok(&engine, 7, "evaluate", json!({"expression":"counter"}))["value"],
        "42"
    );
    ok(&engine, 8, "quit", json!({}));
}
#[test]
fn deferred_reference_case_uses_actual_binary_independent_ram_and_verified_restore() {
    let (project, transcript) = reference_fixture("reference deferred driver", &[]);
    deferred_typed_variable_driver(
        project,
        &transcript,
        json!({"kind":"unsigned","text":"0x89abcdef"}),
    );
}
