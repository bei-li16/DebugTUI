use super::*;

fn configured(fault: &'static str) -> Fixture {
    let mut f = fixture(fault);
    f.project.registers.vfp_command = "aarch64 vfp".into();
    let mut names: Vec<Value> =
        serde_json::from_str(&f.project.gdb.env["DEBUGTUI_TEST_REGISTERS"]).unwrap();
    names.push(json!("r0"));
    let mut values: Value =
        serde_json::from_str(&f.project.gdb.env["DEBUGTUI_TEST_REGISTER_VALUES"]).unwrap();
    values["r0"] = json!("0x11223344");
    f.project.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTERS".into(),
        serde_json::to_string(&names).unwrap(),
    );
    f.project
        .gdb
        .env
        .insert("DEBUGTUI_TEST_REGISTER_VALUES".into(), values.to_string());
    f
}
fn vfp_trace(state: &Value) -> Vec<String> {
    state["trace"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row[1] == "vfp")
        .map(|row| row[2].as_str().unwrap().to_string())
        .collect()
}
#[test]
fn every_vfp_storage_view_uses_shared_physical_pairs_with_exact_alias_bits() {
    let f = configured("");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let mut ids: Vec<String> = (0..32).map(|n| format!("d{n}")).collect();
    ids.extend((0..32).map(|n| format!("s{n}")));
    ids.extend((0..16).map(|n| format!("q{n}")));
    ids.extend(["fpsid", "fpscr", "mvfr0", "mvfr1", "mvfr2", "fpexc"].map(str::to_string));
    let read = ok(
        &engine,
        2,
        "registers_read",
        json!({"ids":ids,"manual":true}),
    );
    let samples = read["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 86);
    for sample in samples {
        assert_eq!(sample["state"], "valid", "{sample}");
        assert_eq!(sample["owner"], "core:default");
        let access = &sample["provenance"]["access"];
        assert_eq!(access["route"]["target"], "cpu0");
        assert_eq!(
            access["route"]["endpoint"],
            f.project.registers.tcl_endpoint
        );
        assert_eq!(access["phase"], "responded");
        assert!(
            access["route"]["operation"]
                .as_str()
                .unwrap()
                .starts_with("VFP read ")
        );
        if sample["id"].as_str().unwrap().starts_with(['s', 'd', 'q']) {
            let id = sample["id"].as_str().unwrap();
            let proof: debugtui::registers::vfp::PairEvidence =
                serde_json::from_value(access["vfp_pair"].clone()).unwrap();
            let value: debugtui::registers::RawValue =
                serde_json::from_value(sample["value"].clone()).unwrap();
            assert_eq!(proof.view(id).unwrap(), value, "{id}");
            assert_eq!(proof.features().unwrap().d_registers, 32);
        } else {
            assert!(access["vfp_pair"].is_null());
        }
    }
    let access =
        |id: &str| &samples.iter().find(|s| s["id"] == id).unwrap()["provenance"]["access"];
    for id in ["d1", "s0", "s1", "q0"] {
        assert_eq!(
            access(id),
            access("d0"),
            "cached physical pair must preserve its original request"
        );
    }
    assert_eq!(access("d0")["route"]["operation"], "VFP read d0");
    let hex = |name: &str| {
        samples.iter().find(|s| s["id"] == name).unwrap()["value"]["hex"]
            .as_str()
            .unwrap()
    };
    assert_eq!(hex("d0"), "0x800000003f800000");
    assert_eq!(hex("d1"), "0x7ff8000012345678");
    assert_eq!(hex("s0"), "0x3f800000");
    assert_eq!(hex("s1"), "0x80000000");
    assert_eq!(hex("q0"), "0x7ff8000012345678800000003f800000");
    assert_eq!(hex("q15"), "0x7ff8000012345687800000003f80000f");
    assert_eq!(hex("fpscr"), "0xa000009f");
    let state = f.state.lock().unwrap();
    let trace = vfp_trace(&state);
    assert_eq!(trace.len(), 22); // 16 physical pairs + 6 controls, no overlapping reread.
    assert!(
        trace
            .iter()
            .filter(|n| n.starts_with('d'))
            .all(|n| n[1..].parse::<u8>().unwrap() % 2 == 0)
    );
    assert!(!trace.iter().any(|n| n.starts_with('q')));
    assert_eq!(state["current"], "outside");
    assert!(selector_writes(&state).is_empty());
    drop(state);
    assert!(
        !fs::read_to_string(&f.transcript)
            .unwrap()
            .contains("get_reg")
    );
    ok(&engine, 3, "quit", json!({}));
}

#[test]
fn all_d16_storage_samples_keep_pair_capacity_without_publishing_high_d_or_q() {
    let f = configured("vfp_d16");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let ids: Vec<String> = (0..32)
        .map(|n| format!("s{n}"))
        .chain((0..32).map(|n| format!("d{n}")))
        .chain((0..16).map(|n| format!("q{n}")))
        .collect();
    let read = ok(
        &engine,
        2,
        "registers_read",
        json!({"ids":ids,"manual":true}),
    );
    for sample in read["samples"].as_array().unwrap() {
        let id = sample["id"].as_str().unwrap();
        let index = id[1..].parse::<u8>().unwrap();
        if id.starts_with('q') || (id.starts_with('d') && index >= 16) {
            assert_eq!(sample["reason"], "hardware_not_implemented", "{sample}");
            assert_eq!(sample["implementation"], "no");
            assert!(sample["value"].is_null());
        } else {
            assert_eq!(sample["state"], "valid", "{sample}");
            let proof: debugtui::registers::vfp::PairEvidence =
                serde_json::from_value(sample["provenance"]["access"]["vfp_pair"].clone()).unwrap();
            assert_eq!(proof.features().unwrap().d_registers, 16);
            assert!(!proof.features().unwrap().neon);
            assert_eq!(proof.view(id).unwrap().hex, sample["value"]["hex"]);
        }
    }
    let trace = vfp_trace(&f.state.lock().unwrap());
    // Eight shared data pairs plus 16 high-D and 8 uncached high-Q refusals.
    assert_eq!(trace.len(), 32);
    for index in (0..16).step_by(2) {
        assert_eq!(
            trace
                .iter()
                .filter(|id| **id == format!("d{index}"))
                .count(),
            1
        );
    }
    assert!(
        trace
            .iter()
            .filter(|id| id.starts_with('q'))
            .all(|id| id[1..].parse::<u8>().unwrap() >= 8)
    );
    assert!(selector_writes(&f.state.lock().unwrap()).is_empty());
    ok(&engine, 3, "quit", json!({}));
}

#[test]
fn failed_vfp_refresh_retains_original_pair_capacity_and_raw_origin() {
    let f = configured("");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let first = ok(
        &engine,
        2,
        "registers_read",
        json!({"ids":["q0","s0","d1"],"manual":true}),
    );
    let origins: Vec<_> = first["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|sample| (sample["value"].clone(), sample["provenance"].clone()))
        .collect();
    f.state.lock().unwrap()["fault"] = json!("vfp_short");
    let failed = ok(
        &engine,
        3,
        "registers_read",
        json!({"ids":["q0","s0","d1"],"manual":true}),
    );
    let status = ok(&engine, 4, "status", json!({}));
    for (sample, (value, origin)) in failed["samples"].as_array().unwrap().iter().zip(origins) {
        assert_eq!(sample["reason"], "reader_unsupported", "{sample}");
        assert!(sample["value"].is_null());
        assert!(sample["provenance"]["access"]["vfp_pair"].is_null());
        let retained = status["register_samples"]
            .as_array()
            .unwrap()
            .iter()
            .find(|old| old["id"] == sample["id"])
            .unwrap();
        assert_eq!(retained["value"], value);
        assert_eq!(retained["last_value_provenance"]["provenance"], origin);
    }
    assert_eq!(status["state"], "STOPPED");
    ok(&engine, 5, "quit", json!({}));
}

#[test]
fn independent_fp_patterns_prove_all_raw_and_float_views_on_the_selected_owner() {
    use debugtui::registers::{
        RawValue,
        display::{Format, Lane},
        vfp::PairEvidence,
    };
    let baseline: Value =
        serde_json::from_str(include_str!("../fixtures/register-storage-patterns.json")).unwrap();
    let mut f = configured("");
    f.project.cores = (0..2)
        .map(|i| Core {
            name: format!("core{i}"),
            endpoint: format!("localhost:{}", 27900 + i),
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
    ok(&engine, 2, "select_core", json!({"index":0}));
    ok(
        &engine,
        3,
        "registers_read",
        json!({"ids":["fpexc"],"manual":true}),
    );
    let peer_before = f.state.lock().unwrap()["targets"]["cpu0"].clone();
    let trace_start = f.state.lock().unwrap()["trace"].as_array().unwrap().len();
    for index in 0..16 {
        let expected = RawValue::parse(
            baseline["views"][format!("q{index}")].as_str().unwrap(),
            128,
        )
        .unwrap();
        f.state.lock().unwrap()["targets"]["cpu1"]["vfp_pairs"][index.to_string()] =
            json!(expected.integer().unwrap().to_string());
    }
    ok(&engine, 4, "control_scope", json!({"scope":"all"}));
    ok(&engine, 5, "select_core", json!({"index":1}));
    let ids: Vec<_> = (0..16)
        .map(|n| format!("q{n}"))
        .chain((0..32).map(|n| format!("d{n}")))
        .chain((0..32).map(|n| format!("s{n}")))
        .collect();
    let read = ok(
        &engine,
        6,
        "registers_read",
        json!({"ids":ids,"manual":true}),
    );
    assert_eq!(read["samples"].as_array().unwrap().len(), 80);
    for sample in read["samples"].as_array().unwrap() {
        let id = sample["id"].as_str().unwrap();
        assert_eq!(sample["state"], "valid", "{sample}");
        assert_eq!(sample["owner"], "core:core1");
        assert_eq!(sample["value"]["hex"], baseline["views"][id]);
        let value: RawValue = serde_json::from_value(sample["value"].clone()).unwrap();
        let access = &sample["provenance"]["access"];
        assert_eq!(access["context"], sample["context"]);
        assert_eq!(access["route"]["target"], "cpu1");
        let proof: PairEvidence = serde_json::from_value(access["vfp_pair"].clone()).unwrap();
        assert_eq!(proof.view(id).unwrap(), value);
        if let Some(expected) = baseline["scalar_float"].get(id) {
            assert_eq!(
                Format::Float { bits: value.bits }.render(&value).unwrap(),
                expected.as_str().unwrap()
            );
        }
        for (section, bits) in [("float32_vectors", 32), ("float64_vectors", 64)] {
            if let Some(expected) = baseline[section].get(id) {
                assert_eq!(
                    Format::Vector {
                        lane_bits: bits,
                        interpretation: Lane::Float
                    }
                    .render(&value)
                    .unwrap(),
                    expected.as_str().unwrap()
                );
            }
        }
    }
    let state = f.state.lock().unwrap().clone();
    assert_eq!(state["targets"]["cpu0"], peer_before);
    let new_trace: Vec<_> = state["trace"]
        .as_array()
        .unwrap()
        .iter()
        .skip(trace_start)
        .filter(|row| row[1] == "vfp")
        .collect();
    assert_eq!(new_trace.len(), 16);
    assert!(new_trace.iter().all(|row| row[0] == "cpu1"));
    assert!(selector_writes(&state).is_empty());
    ok(&engine, 7, "quit", json!({}));
}
#[test]
fn disabled_vfp_retains_presence_evidence_and_never_enables_or_breaks_core_control() {
    let f = configured("vfp_disabled");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let listed = ok(&engine, 2, "registers_list", json!({}));
    let probe = ok(
        &engine,
        3,
        "registers_probe",
        json!({"context":listed["context"]}),
    );
    assert_eq!(probe["facts"]["vfp.present"], 1);
    assert_eq!(probe["facts"]["vfp.d_registers"], 32);
    assert_eq!(probe["facts"]["vfp.neon"], 1);
    assert_eq!(probe["facts"]["vfp.enabled"], 0);
    assert!(
        probe["probe"]["samples"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["id"] == "fpexc" || s["id"] == "mvfr0")
            .all(|s| s["state"] == "valid")
    );
    let read = ok(
        &engine,
        4,
        "registers_read",
        json!({"ids":["d0","fpscr","r0","fpexc"],"manual":true}),
    );
    assert_eq!(read["samples"][0]["reason"], "feature_disabled");
    assert_eq!(read["samples"][1]["reason"], "feature_disabled");
    assert_eq!(read["samples"][2]["value"]["hex"], "0x11223344");
    assert_eq!(read["samples"][3]["value"]["hex"], "0x00000700");
    assert_eq!(ok(&engine, 5, "status", json!({}))["state"], "STOPPED");
    ok(&engine, 6, "step", json!({}));
    assert!(selector_writes(&f.state.lock().unwrap()).is_empty());
    ok(&engine, 7, "quit", json!({}));
}
#[test]
fn d16_capacity_refuses_high_d_and_q_without_confusing_disabled_with_absent() {
    let f = configured("vfp_d16");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let read = ok(
        &engine,
        2,
        "registers_read",
        json!({"ids":["d0","q0","d16","d15"],"manual":true}),
    );
    for index in [1, 2] {
        assert_eq!(read["samples"][index]["reason"], "hardware_not_implemented");
        assert_eq!(read["samples"][index]["implementation"], "no");
    }
    assert_eq!(read["samples"][0]["state"], "valid");
    assert_eq!(read["samples"][3]["state"], "valid");
    assert!(!vfp_trace(&f.state.lock().unwrap()).contains(&"q0".into()));
    let listed = ok(&engine, 3, "registers_list", json!({}));
    let probe = ok(
        &engine,
        4,
        "registers_probe",
        json!({"context":listed["context"]}),
    );
    assert_eq!(probe["facts"]["vfp.d_registers"], 16);
    assert_eq!(probe["facts"]["vfp.double_precision"], 0);
    assert_eq!(probe["facts"]["vfp.neon"], 0);
    assert_eq!(probe["facts"]["vfp.enabled"], 1);
    ok(&engine, 5, "quit", json!({}));
}
#[test]
fn unsupported_protocol_identity_width_and_permissions_never_fall_back_to_gdb() {
    for (fault, reason) in [
        ("vfp_protocol", "reader_unsupported"),
        ("vfp_identity", "reader_unsupported"),
        ("vfp_short", "reader_unsupported"),
        ("vfp_trapped", "access_restricted"),
    ] {
        let f = configured(fault);
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let read = ok(
            &engine,
            2,
            "registers_read",
            json!({"ids":["d0","r0"],"manual":true}),
        );
        assert_eq!(read["samples"][0]["reason"], reason, "{read}");
        assert_eq!(read["samples"][1]["state"], "valid");
        assert_eq!(ok(&engine, 3, "status", json!({}))["state"], "STOPPED");
        if fault == "vfp_protocol" {
            assert!(vfp_trace(&f.state.lock().unwrap()).is_empty());
        }
        ok(&engine, 4, "step", json!({}));
        ok(&engine, 5, "quit", json!({}));
    }
    let f = fixture("");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let read = ok(
        &engine,
        2,
        "registers_read",
        json!({"ids":["d0"],"manual":true}),
    );
    assert_eq!(read["samples"][0]["reason"], "reader_unsupported");
    ok(&engine, 3, "quit", json!({}));
}
#[test]
fn vfp_uncertainty_quarantines_shared_service_without_retry_or_partial_aliases() {
    let f = configured("vfp_fault");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let read = ok(
        &engine,
        2,
        "registers_read",
        json!({"ids":["d0","s0","q0"],"manual":true}),
    );
    assert!(
        read["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["value"].is_null())
    );
    assert_eq!(ok(&engine, 3, "status", json!({}))["state"], "FAULT");
    assert_eq!(vfp_trace(&f.state.lock().unwrap()).len(), 1);
    assert!(request(&engine, 4, "continue", json!({})).is_err());
    ok(&engine, 5, "quit", json!({}));
}
#[test]
fn vfp_scope_all_has_one_physical_owner_and_rejects_another_core_context() {
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
    ok(&engine, 3, "select_core", json!({"index":1}));
    let listed = ok(&engine, 4, "registers_list", json!({}));
    let read = ok(
        &engine,
        5,
        "registers_read",
        json!({"context":listed["context"],"ids":["q0","d0","d1","s0"],"manual":true}),
    );
    assert!(
        read["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["owner"] == "core:core1")
    );
    assert_eq!(
        read["samples"][0]["value"]["hex"],
        "0x7ff8000112345678800000013f800000"
    );
    assert_eq!(vfp_trace(&f.state.lock().unwrap()).len(), 1);
    ok(&engine, 6, "select_core", json!({"index":0}));
    assert!(
        request(
            &engine,
            7,
            "registers_read",
            json!({"context":listed["context"],"ids":["d0"],"manual":true})
        )
        .is_err()
    );
    assert!(
        f.state.lock().unwrap()["trace"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s[1] == "vfp")
            .all(|s| s[0] == "cpu1")
    );
    ok(&engine, 8, "quit", json!({}));
}

#[test]
fn unadapted_vfp_features_keep_raw_evidence_without_authorizing_data_or_gdb_fallback() {
    let f = configured("vfp_unknown");
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let listed = ok(&engine, 2, "registers_list", json!({}));
    let probe = ok(
        &engine,
        3,
        "registers_probe",
        json!({"context":listed["context"]}),
    );
    assert!(probe["facts"]["vfp.d_registers"].is_null());
    assert!(probe["facts"]["vfp.neon"].is_null());
    assert_eq!(
        probe["probe"]["samples"]
            .as_array()
            .unwrap()
            .iter()
            .find(|sample| sample["id"] == "mvfr1")
            .unwrap()["value"]["hex"],
        "0x12113111"
    );
    let state = f.state.lock().unwrap().clone();
    let automatic = ok(&engine, 4, "registers_read", json!({"ids":["d0","q0"]}));
    assert!(
        automatic["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["value"].is_null())
    );
    assert_eq!(*f.state.lock().unwrap(), state);
    let manual = ok(
        &engine,
        5,
        "registers_read",
        json!({"ids":["d0"],"manual":true}),
    );
    assert_eq!(manual["samples"][0]["reason"], "reader_unsupported");
    assert_eq!(ok(&engine, 6, "status", json!({}))["state"], "STOPPED");
    ok(&engine, 7, "quit", json!({}));
}

#[test]
fn vfp_context_change_discards_raw_pair_even_when_backend_refuses_access() {
    for fault in ["vfp_context_change", "vfp_refusal_context_change"] {
        let mut f = configured(fault);
        let path = f.transcript.parent().unwrap().join("physical-context.json");
        fs::write(&path, r#"{"thread":"1","frame":0}"#).unwrap();
        f.project.gdb.env.insert(
            "DEBUGTUI_TEST_CONTEXT_FILE".into(),
            path.to_string_lossy().into_owned(),
        );
        f.state.lock().unwrap()["context_file"] = json!(path);
        let engine = session::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        let result = ok(
            &engine,
            2,
            "registers_read",
            json!({"ids":["d0","s0","q0"],"manual":true}),
        );
        assert_eq!(result["samples"][0]["reason"], "access_restricted");
        assert!(
            result["samples"][0]["detail"]
                .as_str()
                .unwrap()
                .contains("discarded")
        );
        assert!(
            result["samples"]
                .as_array()
                .unwrap()
                .iter()
                .all(|s| s["value"].is_null())
        );
        assert_eq!(vfp_trace(&f.state.lock().unwrap()).len(), 1);
        assert_eq!(f.state.lock().unwrap()["current"], "outside");
        assert!(ok(&engine, 3, "registers_list", json!({}))["probe"].is_null());
        ok(&engine, 4, "quit", json!({}));
    }
}

#[test]
fn nonphysical_frame_refuses_vfp_before_any_protocol_or_instruction() {
    let mut f = configured("");
    f.project
        .gdb
        .env
        .insert("DEBUGTUI_TEST_CAPABILITY_FRAME_CHANGE".into(), "1".into());
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let error = request(&engine, 2, "registers_read", json!({"ids":["r0"]})).unwrap_err();
    assert!(error.contains("thread/frame changed"), "{error}");
    let snapshot = ok(&engine, 20, "status", json!({}));
    assert!(
        snapshot["register_samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|sample| sample["id"] != "r0" || sample["state"] != "valid")
    );
    let result = ok(
        &engine,
        3,
        "registers_read",
        json!({"ids":["d0"],"manual":true}),
    );
    assert_eq!(result["samples"][0]["reason"], "access_restricted");
    assert!(
        f.state.lock().unwrap()["trace"]
            .as_array()
            .is_none_or(|rows| rows
                .iter()
                .all(|row| row[1] != "vfp" && row[1] != "debugtui_vfp_protocol"))
    );
    ok(&engine, 4, "quit", json!({}));
}

#[test]
fn deferred_vfp_driver_runs_independent_full_d16_disabled_and_trapped_cases() {
    for fault in ["", "vfp_d16", "vfp_disabled", "vfp_trapped"] {
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
        let mut spec: Value =
            serde_json::from_str(include_str!("../fixtures/register-vfp-board.example.json"))
                .unwrap();
        spec["frame_function"] = json!("main");
        let single = fault == "vfp_d16";
        let enabled = fault != "vfp_disabled";
        let trapped = fault == "vfp_trapped";
        let (m0, m1) = if single {
            (0x10110021u32, 0x11000011u32)
        } else {
            (0x10110222, 0x12111111)
        };
        spec["expected_mvfr0"] = json!(format!("0x{m0:08x}"));
        spec["expected_mvfr1"] = json!(format!("0x{m1:08x}"));
        spec["expected_enabled"] = json!(enabled);
        spec["expected_trapped"] = json!(trapped);
        let mut names: Vec<Value> =
            serde_json::from_str(&f.project.gdb.env["DEBUGTUI_TEST_REGISTERS"]).unwrap();
        let mut values: Value =
            serde_json::from_str(&f.project.gdb.env["DEBUGTUI_TEST_REGISTER_VALUES"]).unwrap();
        for name in spec["stable_registers"].as_array().unwrap() {
            if !names.contains(name) {
                names.push(name.clone());
                values[name.as_str().unwrap()] = json!("0x11223344");
            }
        }
        f.project.gdb.env.insert(
            "DEBUGTUI_TEST_REGISTERS".into(),
            serde_json::to_string(&names).unwrap(),
        );
        f.project
            .gdb
            .env
            .insert("DEBUGTUI_TEST_REGISTER_VALUES".into(), values.to_string());
        let mut references = serde_json::Map::new();
        references.insert(
            "*(unsigned int *)&debugtui_vfp_reference_ready".into(),
            json!("1"),
        );
        let control_words = [
            if trapped { 1 << 10 } else { 0 },
            0x41034025,
            m0,
            m1,
            if single { 0x40 } else { 0x43 },
            if enabled { 0x40000700 } else { 0x700 },
            0xa000009f,
        ];
        for (index, word) in control_words.into_iter().enumerate() {
            references.insert(
                format!("((unsigned int *)&debugtui_vfp_reference)[{index}]"),
                json!(format!("0x{word:08x}")),
            );
        }
        for index in 0..32u32 {
            let pair = u64::from(index / 2);
            let word = if index % 2 == 0 {
                0x800000003f800000u64 + pair
            } else {
                0x7ff8000012345678u64 + pair
            };
            for high in 0..2 {
                references.insert(
                    format!(
                        "((unsigned int *)&debugtui_vfp_reference)[{}]",
                        8 + index * 2 + high
                    ),
                    json!(format!("0x{:08x}", (word >> (high * 32)) as u32)),
                );
            }
        }
        f.project.gdb.env.insert(
            "DEBUGTUI_TEST_EXPRESSION_VALUES".into(),
            Value::Object(references).to_string(),
        );
        let directory = f.transcript.parent().unwrap();
        let project = directory.join("vfp-driver.toml");
        let case = directory.join("vfp-driver.json");
        fs::write(&project, toml::to_string(&f.project).unwrap()).unwrap();
        fs::write(&case, serde_json::to_vec_pretty(&spec).unwrap()).unwrap();
        let original = fs::read(&project).unwrap();
        let output = Command::new("node")
            .arg(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("scripts/test-register-vfp-hardware.cjs"),
            )
            .args(["--run", "--software-fixture", "--core", "core0", "--binary"])
            .arg(env!("CARGO_BIN_EXE_debugtui"))
            .arg("--project")
            .arg(&project)
            .arg("--case")
            .arg(&case)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{fault}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .contains("\"passed\":5,\"failed\":0,\"skipped\":0")
        );
        assert_eq!(fs::read(&project).unwrap(), original);
        let state = f.state.lock().unwrap();
        assert_eq!(state["current"], "outside");
        assert!(selector_writes(&state).is_empty());
        assert!(
            state["trace"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| row[1] == "vfp")
                .all(|row| row[0] == "cpu0")
        );
    }
}
