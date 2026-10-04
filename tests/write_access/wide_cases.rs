use super::*;

fn wide_fixture(label: &str, failure: &str, apply_only: bool) -> (Project, PathBuf) {
    variable_fixture(
        label,
        &[
            ("DEBUGTUI_TEST_WIDE_VARIABLE", "1"),
            ("DEBUGTUI_TEST_VARIABLE_TYPE", "__int128 unsigned"),
            ("DEBUGTUI_TEST_VARIABLE_ADDRESS", "0x100000010"),
            ("DEBUGTUI_TEST_WIDE_FAILURE", failure),
            (
                "DEBUGTUI_TEST_WIDE_APPLY_ONLY",
                if apply_only { "1" } else { "" },
            ),
        ],
    )
}
const VALUE: &str = "0xfedcba98765432100123456789abcdef";
#[test]
fn wide_typed_write_proves_literal_preserves_all_128_bits_and_independent_memory() {
    let (project, transcript) = wide_fixture("wide exact", "", false);
    let engine = session::spawn(project);
    ok(&engine, 1, "connect", json!({}));
    let (accepted, draft, error) = variable_preview(&engine, 2, VALUE);
    assert!(accepted, "{error:?}");
    assert_eq!(draft["metadata"]["scalar"]["bits"], 128);
    assert_eq!(draft["plan"]["value"]["hex"], VALUE);
    let result = ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}));
    assert_eq!(result["outcome"], "verified");
    assert_eq!(result["observed"]["hex"], VALUE);
    let context = ok(&engine, 5, "registers_list", json!({}))["context"].clone();
    let read = ok(
        &engine,
        6,
        "memory_dump",
        json!({"address":"0x100000010","count":16,"context":context}),
    );
    assert_eq!(
        read["bytes"],
        json!([
            0xef, 0xcd, 0xab, 0x89, 0x67, 0x45, 0x23, 0x01, 0x10, 0x32, 0x54, 0x76, 0x98, 0xba,
            0xdc, 0xfe
        ])
    );
    let writes = variable_assignments(&transcript);
    assert_eq!(writes.len(), 1);
    assert!(!writes[0].contains("unsigned __int128"));
    assert!(memory_writes(&transcript).is_empty());
    assert_eq!(
        ok(&engine, 7, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
        "not_sent"
    );
    ok(&engine, 8, "quit", json!({}));
}
#[test]
fn wide_literal_truncation_or_capability_change_prevents_assignment() {
    for failure in ["truncate", "unsupported", "cleanup", "running"] {
        for apply_only in [false, true] {
            let (project, transcript) = wide_fixture(
                &format!("wide refused {failure} {apply_only}"),
                failure,
                apply_only,
            );
            let engine = session::spawn(project);
            ok(&engine, 1, "connect", json!({}));
            let (accepted, draft, error) = variable_preview(&engine, 2, VALUE);
            if apply_only {
                assert!(accepted, "{error:?}");
                let result = ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}));
                assert_eq!(result["outcome"], "not_sent", "{result}");
                assert_eq!(
                    ok(&engine, 5, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
                    "not_sent"
                );
            } else {
                assert!(!accepted, "{failure}");
            }
            assert!(variable_assignments(&transcript).is_empty());
            assert!(memory_writes(&transcript).is_empty());
            let _ = request(&engine, 6, "quit", json!({}));
        }
    }
}
#[test]
fn wide_post_send_errors_keep_accepted_or_unknown_and_never_retry() {
    for (failure, outcome) in [("readback", "accepted"), ("unknown", "unknown")] {
        let (project, transcript) = wide_fixture(&format!("wide sent {failure}"), failure, true);
        let engine = session::spawn(project);
        ok(&engine, 1, "connect", json!({}));
        let (accepted, draft, error) = variable_preview(&engine, 2, VALUE);
        assert!(accepted, "{error:?}");
        let result = ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}));
        assert_eq!(result["outcome"], outcome, "{result}");
        assert_eq!(variable_assignments(&transcript).len(), 1);
        assert_eq!(
            ok(&engine, 5, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
            "not_sent"
        );
        assert!(memory_writes(&transcript).is_empty());
        let _ = request(&engine, 6, "quit", json!({}));
    }
}
#[test]
fn deferred_wide_case_uses_actual_binary_and_restores_all_16_bytes() {
    let (project, transcript) = wide_fixture("wide deferred driver", "", false);
    deferred_typed_variable_driver(
        project,
        &transcript,
        json!({"kind":"unsigned","text":VALUE}),
    );
}
