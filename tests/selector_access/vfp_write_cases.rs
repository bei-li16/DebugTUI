use super::*;

fn configured(fault: &'static str) -> Fixture {
    let mut f = fixture(fault);
    f.project.registers.catalogue =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("profiles/registers/cortex-r52.toml");
    f.project.registers.vfp_command = "aarch64 vfp".into();
    f.project.registers.vfp_write_command = "aarch64 vfp_write".into();
    f
}
fn preview(
    engine: &session::EngineHandle,
    id: u64,
    name: &str,
    kind: &str,
    text: &str,
) -> Result<Value, String> {
    let context = ok(engine, id, "registers_list", json!({}))["context"].clone();
    request(
        engine,
        id + 1,
        "write_preview",
        json!({"context":context,"target":{"kind":"register","id":name},
        "selection":{"kind":"register"},"input":{"kind":kind,"text":text}}),
    )
}
fn writes(f: &Fixture) -> Vec<Value> {
    f.state.lock().unwrap()["trace"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row[1] == "vfp_write")
        .cloned()
        .collect()
}

#[test]
fn vfp_permission_or_actual_frame_changes_stop_before_any_data_write() {
    for initially_off in [true, false] {
        let mut f = configured("");
        f.project.gdb.env.insert(
            if initially_off {
                "DEBUGTUI_TEST_REGISTER_READONLY"
            } else {
                "DEBUGTUI_TEST_REGISTER_PERMISSION_AFTER"
            }
            .into(),
            if initially_off { "1" } else { "2" }.into(),
        );
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let draft = preview(&engine, 2, "s0", "unsigned", "0");
        if initially_off {
            assert!(draft.unwrap_err().contains("may-write-registers"));
        } else {
            let draft = draft.unwrap();
            let result = ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}));
            assert_eq!(result["outcome"], "not_sent");
            assert!(
                result["error"]
                    .as_str()
                    .unwrap()
                    .contains("may-write-registers")
            );
        }
        assert!(writes(&f).is_empty());
        ok(&engine, 5, "quit", json!({}));
    }
    let mut f = configured("");
    let path = f.transcript.parent().unwrap().join("physical-context.json");
    fs::write(&path, r#"{"thread":"1","frame":0}"#).unwrap();
    f.project.gdb.env.insert(
        "DEBUGTUI_TEST_CONTEXT_FILE".into(),
        path.to_string_lossy().into_owned(),
    );
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let draft = preview(&engine, 2, "q15", "unsigned", "0").unwrap();
    fs::write(&path, r#"{"thread":"1","frame":1}"#).unwrap();
    let result = ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}));
    assert_eq!(result["outcome"], "not_sent");
    assert!(
        result["error"]
            .as_str()
            .unwrap()
            .contains("physical frame 0")
    );
    assert!(writes(&f).is_empty());
    ok(&engine, 5, "quit", json!({}));
}

#[test]
fn vfp_scope_all_writes_only_the_selected_owner_and_rejects_a_peer_draft() {
    let mut f = configured("");
    f.project.cores = (0..2)
        .map(|i| Core {
            name: format!("core{i}"),
            endpoint: format!("localhost:{}", 27500 + i),
            ..Default::default()
        })
        .collect();
    f.project.registers.targets = [
        ("core0".into(), "cpu0".into()),
        ("core1".into(), "cpu1".into()),
    ]
    .into();
    let engine = debugtui::coordinator::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    ok(&engine, 2, "control_scope", json!({"scope":"all"}));
    ok(&engine, 20, "select_core", json!({"index":0}));
    let peer = ok(
        &engine,
        3,
        "registers_read",
        json!({"ids":["q0"],"manual":true}),
    )["samples"][0]["value"]
        .clone();
    ok(&engine, 4, "select_core", json!({"index":1}));
    let draft = preview(&engine, 5, "s0", "unsigned", "0x7fa12345").unwrap();
    assert_eq!(draft["owner"], "core1");
    assert_eq!(draft["target_name"], "cpu1");
    ok(&engine, 7, "select_core", json!({"index":0}));
    assert_eq!(
        ok(&engine, 8, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
        "not_sent"
    );
    assert!(writes(&f).is_empty());
    ok(&engine, 9, "select_core", json!({"index":1}));
    let draft = preview(&engine, 10, "s0", "unsigned", "0x7fa12345").unwrap();
    let result = ok(&engine, 12, "write_apply", json!({"draft":draft["draft"]}));
    assert_eq!(result["outcome"], "verified");
    assert_eq!(result["owner"], "core1");
    assert_eq!(writes(&f).len(), 1);
    assert_eq!(writes(&f)[0][0], "cpu1");
    ok(&engine, 13, "select_core", json!({"index":0}));
    assert_eq!(
        ok(
            &engine,
            14,
            "registers_read",
            json!({"ids":["q0"],"manual":true})
        )["samples"][0]["value"],
        peer
    );
    assert_eq!(f.state.lock().unwrap()["current"], "outside");
    ok(&engine, 15, "quit", json!({}));
}

#[test]
fn vfp_raw_bytes_use_explicit_order_and_applied_alias_drafts_expire() {
    let f = configured("");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let alias = preview(&engine, 2, "d31", "unsigned", "0").unwrap();
    let context = ok(&engine, 4, "registers_list", json!({}))["context"].clone();
    let draft = ok(
        &engine,
        5,
        "write_preview",
        json!({"context":context,"target":{"kind":"register","id":"q15"},"selection":{"kind":"register"},"input":{"kind":"bytes","text":"81 23 45 67 89 ab cd ef 7f f0 12 34 56 78 9a bc","little_endian":false}}),
    );
    assert_eq!(
        draft["plan"]["value"]["hex"],
        "0x8123456789abcdef7ff0123456789abc"
    );
    let result = ok(&engine, 6, "write_apply", json!({"draft":draft["draft"]}));
    assert_eq!(result["outcome"], "verified");
    assert_eq!(result["observed"], draft["plan"]["value"]);
    assert_eq!(
        ok(&engine, 7, "write_apply", json!({"draft":alias["draft"]}))["outcome"],
        "not_sent"
    );
    for text in [
        "0x100000000000000000000000000000000",
        "0x1; resume",
        "nan()",
        "-1",
    ] {
        assert!(
            preview(&engine, 8, "q15", "unsigned", text).is_err(),
            "{text}"
        );
    }
    assert_eq!(writes(&f).len(), 1);
    ok(&engine, 11, "quit", json!({}));
}

#[test]
fn deferred_vfp_host_driver_runs_actual_binary_and_stops_unknown_or_mismatch_without_restore() {
    for (name, fault) in [
        ("s31", ""),
        ("d31", ""),
        ("q15", ""),
        ("q15", "vfp_write_unknown"),
        ("q15", "vfp_write_mismatch"),
    ] {
        let mut f = configured(fault);
        f.project.cores = (0..2)
            .map(|i| Core {
                name: format!("core{i}"),
                endpoint: format!("localhost:{}", 26900 + i),
                ..Default::default()
            })
            .collect();
        f.project.registers.targets = [
            ("core0".into(), "cpu0".into()),
            ("core1".into(), "cpu1".into()),
        ]
        .into();
        let pair = if name == "s31" { 7u128 } else { 15 };
        let mut spec: Value = serde_json::from_str(include_str!(
            "../fixtures/register-vfp-host-write-board.example.json"
        ))
        .unwrap();
        spec["frame_function"] = json!("main");
        spec["expected_midr"] = json!("0x411fd134");
        spec["target"] = json!("cpu0");
        spec["peer_target"] = json!("cpu1");
        spec["peer_reference"] = json!("debugtui_vfp_peer_reference");
        spec["register"] = json!(name);
        spec["raw"] = json!(match name {
            "s31" => "0x7fa12345",
            "d31" => "0xfff0123456789abc",
            _ => "0x8123456789abcdef7ff0123456789abc",
        });
        let mut refs = serde_json::Map::new();
        for core in 0..2 {
            let offset = pair + if core == 1 { 0x100000000 } else { 0 };
            let value =
                ((0x7ff8000012345678u128 + offset) << 64) | (0x800000003f800000u128 + offset);
            spec[if core == 0 {
                "expected_before"
            } else {
                "expected_peer"
            }] = json!(format!("0x{value:032x}"));
            let reference = if core == 0 {
                "debugtui_vfp_reference"
            } else {
                "debugtui_vfp_peer_reference"
            };
            refs.insert(format!("*(unsigned int *)&{reference}_ready"), json!("1"));
            for word in 0..4 {
                refs.insert(
                    format!("((unsigned int *)&{reference})[{}]", 8 + pair * 4 + word),
                    json!(format!("0x{:08x}", (value >> (word * 32)) as u32)),
                );
            }
        }
        f.project.gdb.env.insert(
            "DEBUGTUI_TEST_EXPRESSION_VALUES".into(),
            Value::Object(refs).to_string(),
        );
        let mut names: Vec<Value> =
            serde_json::from_str(&f.project.gdb.env["DEBUGTUI_TEST_REGISTERS"]).unwrap();
        let mut values: Value =
            serde_json::from_str(&f.project.gdb.env["DEBUGTUI_TEST_REGISTER_VALUES"]).unwrap();
        for name in ["r0", "r1", "pc"] {
            names.push(json!(name));
            values[name] = json!("0x11223344");
        }
        f.project.gdb.env.insert(
            "DEBUGTUI_TEST_REGISTERS".into(),
            serde_json::to_string(&names).unwrap(),
        );
        f.project
            .gdb
            .env
            .insert("DEBUGTUI_TEST_REGISTER_VALUES".into(), values.to_string());
        let directory = f.transcript.parent().unwrap();
        let project = directory.join("vfp-host-writer.toml");
        let case = directory.join("vfp-host-writer.json");
        fs::write(&project, toml::to_string(&f.project).unwrap()).unwrap();
        fs::write(&case, serde_json::to_vec_pretty(&spec).unwrap()).unwrap();
        let original = fs::read(&project).unwrap();
        let output = Command::new("node")
            .arg(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("scripts/test-register-vfp-write-hardware.cjs"),
            )
            .args(["--run", "--software-fixture", "--core", "core0", "--binary"])
            .arg(env!("CARGO_BIN_EXE_debugtui"))
            .arg("--project")
            .arg(&project)
            .arg("--case")
            .arg(&case)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.success(),
            fault.is_empty(),
            "{name}/{fault}: {stdout}\n{stderr}"
        );
        assert!(
            stdout.contains(if fault.is_empty() {
                "\"passed\":5,\"failed\":0,\"skipped\":0"
            } else if fault == "vfp_write_unknown" {
                "\"passed\":2,\"failed\":2,\"skipped\":1"
            } else {
                "\"passed\":3,\"failed\":1,\"skipped\":1"
            }),
            "{stdout}\n{stderr}"
        );
        assert_eq!(fs::read(&project).unwrap(), original);
        assert_eq!(writes(&f).len(), if fault.is_empty() { 2 } else { 1 });
        assert!(writes(&f).iter().all(|row| row[0] == "cpu0"));
        assert_eq!(f.state.lock().unwrap()["current"], "outside");
    }
}
#[test]
fn vfp_drafts_preserve_raw_high_words_aliases_cancel_and_invalidate_old_values() {
    let f = configured("");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    for (index, (name, text)) in [
        ("s31", "0x7fa12345"),
        ("d31", "0xfff0123456789abc"),
        ("q15", "0x8123456789abcdef7ff0123456789abc"),
    ]
    .into_iter()
    .enumerate()
    {
        let base = 10 + index as u64 * 20;
        let draft = preview(&engine, base, name, "unsigned", text).unwrap();
        let count = writes(&f).len();
        let result = ok(
            &engine,
            base + 2,
            "write_apply",
            json!({"draft":draft["draft"]}),
        );
        assert_eq!(result["outcome"], "verified", "{result}");
        assert_eq!(result["channel"], "register_tcl");
        assert_eq!(result["target_name"], "cpu0");
        assert_eq!(result["neighbours_preserved"], true);
        assert_eq!(result["atomic"], false);
        assert_eq!(writes(&f).len(), count + 1);
        assert_eq!(
            ok(
                &engine,
                base + 3,
                "write_apply",
                json!({"draft":draft["draft"]})
            )["outcome"],
            "not_sent"
        );
        let read = ok(
            &engine,
            base + 4,
            "registers_read",
            json!({"ids":[name],"manual":true}),
        );
        assert_eq!(read["samples"][0]["value"], result["observed"]);
        assert!(
            result["context_after"]["generation"].as_u64().unwrap()
                > draft["context"]["generation"].as_u64().unwrap()
        );
    }
    let draft = preview(&engine, 80, "s0", "float", "-0.0").unwrap();
    let count = writes(&f).len();
    assert_eq!(draft["plan"]["value"]["hex"], "0x80000000");
    assert_eq!(
        ok(&engine, 82, "write_cancel", json!({"draft":draft["draft"]}))["cancelled"],
        true
    );
    assert_eq!(
        ok(&engine, 83, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
        "not_sent"
    );
    assert_eq!(writes(&f).len(), count);
    assert!(
        !fs::read_to_string(&f.transcript)
            .unwrap()
            .contains("-data-write-register-values")
    );
    ok(&engine, 90, "quit", json!({}));
}
#[test]
fn vfp_apply_uses_backend_fresh_sibling_bits_and_rechecks_protocol_and_capacity() {
    for fault in [
        "",
        "vfp_write_protocol",
        "vfp_disabled",
        "vfp_d16",
        "vfp_context_change",
        "vfp_write_refused",
    ] {
        let f = configured("");
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let draft = preview(
            &engine,
            2,
            "q15",
            "unsigned",
            "0x8123456789abcdef7ff0123456789abc",
        )
        .unwrap();
        f.state.lock().unwrap()["fault"] = json!(fault);
        let result = ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}));
        assert_eq!(
            result["outcome"],
            if fault.is_empty() {
                "verified"
            } else {
                "not_sent"
            },
            "{fault}: {result}"
        );
        assert_eq!(
            writes(&f).len(),
            usize::from(fault.is_empty() || fault == "vfp_write_refused")
        );
        ok(&engine, 5, "quit", json!({}));
    }
    let f = configured("");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let draft = preview(&engine, 2, "s0", "unsigned", "0x80000000").unwrap();
    f.state.lock().unwrap()["targets"]["cpu0"]["vfp_pairs"]["0"] =
        json!("123456789012345678901234567890123456789");
    let result = ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}));
    assert_eq!(result["outcome"], "verified");
    assert_ne!(
        result["physical_pair_before"],
        draft["physical_pair_before"]
    );
    assert_eq!(result["neighbours_preserved"], true);
    ok(&engine, 5, "quit", json!({}));
}
#[test]
fn vfp_unknown_or_forged_receipts_quarantine_without_retry_or_gdb_fallback() {
    for fault in [
        "vfp_write_unknown",
        "vfp_write_forged_expected",
        "vfp_write_forged_enable",
        "target_restore",
    ] {
        let f = configured("");
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let draft = preview(&engine, 2, "s31", "unsigned", "0x7fa12345").unwrap();
        f.state.lock().unwrap()["fault"] = json!(fault);
        // A target restoration failure during the fresh read prevents the data write.
        let result = ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}));
        assert_eq!(
            result["outcome"],
            if fault == "target_restore" {
                "not_sent"
            } else {
                "unknown"
            },
            "{fault}: {result}"
        );
        assert_eq!(writes(&f).len(), usize::from(fault != "target_restore"));
        assert_eq!(ok(&engine, 5, "status", json!({}))["state"], "FAULT");
        assert!(preview(&engine, 6, "s0", "unsigned", "0").is_err());
        assert_eq!(writes(&f).len(), usize::from(fault != "target_restore"));
        assert!(
            !fs::read_to_string(&f.transcript)
                .unwrap()
                .contains("-data-write-register-values")
        );
        ok(&engine, 9, "quit", json!({}));
    }
}
#[test]
fn vfp_mismatch_and_changed_context_do_not_claim_current_verified_values() {
    for (fault, outcome, neighbours) in [
        ("vfp_write_mismatch", "mismatch", true),
        ("vfp_write_neighbour_mismatch", "mismatch", false),
        ("vfp_write_context_change", "accepted", true),
        ("vfp_write_mismatch_context_change", "mismatch", true),
    ] {
        let mut f = configured("");
        let path = f.transcript.parent().unwrap().join("physical-context.json");
        fs::write(&path, r#"{"thread":"1","frame":0}"#).unwrap();
        f.project.gdb.env.insert(
            "DEBUGTUI_TEST_CONTEXT_FILE".into(),
            path.to_string_lossy().into_owned(),
        );
        f.state.lock().unwrap()["context_file"] = json!(path);
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let draft = preview(&engine, 2, "s31", "unsigned", "0x7fa12345").unwrap();
        f.state.lock().unwrap()["fault"] = json!(fault);
        let result = ok(&engine, 4, "write_apply", json!({"draft":draft["draft"]}));
        assert_eq!(result["outcome"], outcome, "{result}");
        assert_eq!(result["neighbours_preserved"], neighbours);
        assert_eq!(writes(&f).len(), 1);
        assert_eq!(
            ok(&engine, 5, "write_apply", json!({"draft":draft["draft"]}))["outcome"],
            "not_sent"
        );
        ok(&engine, 6, "quit", json!({}));
    }
}
#[test]
fn vfp_preview_needs_explicit_writer_physical_frame_permission_and_exact_raw_width() {
    for fault in [
        "vfp_write_protocol",
        "vfp_disabled",
        "vfp_trapped",
        "vfp_identity",
        "vfp_unknown",
    ] {
        let f = configured(fault);
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        assert!(preview(&engine, 2, "s0", "unsigned", "0").is_err());
        assert!(writes(&f).is_empty());
        ok(&engine, 5, "quit", json!({}));
    }
    let mut f = configured("");
    f.project.registers.vfp_write_command.clear();
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    assert!(
        preview(&engine, 2, "s0", "unsigned", "0")
            .unwrap_err()
            .contains("vfp_write_command")
    );
    assert!(writes(&f).is_empty());
    ok(&engine, 5, "quit", json!({}));
    let f = configured("");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    assert!(preview(&engine, 2, "s0", "unsigned", "0x100000000").is_err());
    assert!(preview(&engine, 4, "fpscr", "unsigned", "0").is_err());
    ok(&engine, 6, "frame", json!({"level":1}));
    assert!(preview(&engine, 7, "q0", "unsigned", "0").is_err());
    assert!(writes(&f).is_empty());
    ok(&engine, 10, "quit", json!({}));
}
