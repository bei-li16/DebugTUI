use super::*;

#[test]
fn all_timer_routes_preserve_unknown_presence_exact_encodings_errors_and_selected_core() {
    // Values and encodings are independent of the catalogue generator.
    let entries = [
        ("cntfrq", "15 0 14 0 0", "0x05f5e100", 32),
        ("cntkctl", "15 0 14 1 0", "0x000003ad", 32),
        ("cntp_tval", "15 0 14 2 0", "0xfffffff0", 32),
        ("cntp_ctl", "15 0 14 2 1", "0x00000007", 32),
        ("cntv_tval", "15 0 14 3 0", "0x80000001", 32),
        ("cntv_ctl", "15 0 14 3 1", "0x00000004", 32),
        ("cnthctl", "15 4 14 1 0", "0x000000ab", 32),
        ("cnthp_tval", "15 4 14 2 0", "0x7ffffffe", 32),
        ("cnthp_ctl", "15 4 14 2 1", "0x00000001", 32),
        ("cntpct", "15 0 14", "0x89abcdef01234567", 64),
        ("cntvct", "15 1 14", "0x89abcdee01234567", 64),
        ("cntp_cval", "15 2 14", "0x0123456789abcdef", 64),
        ("cntv_cval", "15 3 14", "0xfedcba9876543210", 64),
        ("cntvoff", "15 4 14", "0x0000000100000000", 64),
        ("cnthp_cval", "15 6 14", "0x8000000000000001", 64),
    ];
    let ids: Vec<_> = entries.iter().map(|e| e.0).collect();
    for declared in [None, Some(0)] {
        let mut f = fixture("");
        f.project.registers.cp15_command = "aarch64 mrc".into();
        f.project.registers.selector_command = "aarch64 mcr".into();
        f.project.registers.cp15_64_command = "aarch64 mrrc".into();
        f.project.cores = (0..2)
            .map(|i| Core {
                name: format!("core{i}"),
                endpoint: format!("localhost:{}", 27600 + i),
                ..Default::default()
            })
            .collect();
        f.project.registers.targets = [
            ("core0".into(), "cpu0".into()),
            ("core1".into(), "cpu1".into()),
        ]
        .into();
        if let Some(value) = declared {
            f.project
                .registers
                .facts
                .insert("timer.present".into(), value);
        }
        let mut cpu = json!({"timer32":{},"timer64":{}});
        for (_, encoding, value, bits) in entries {
            cpu[if bits == 32 { "timer32" } else { "timer64" }][encoding] = json!(value);
        }
        *f.state.lock().unwrap() = json!({"targets":{"cpu1":cpu},"trace":[]});
        let engine = debugtui::coordinator::spawn(f.project.clone());
        ok(&engine, 1, "connect", json!({}));
        ok(&engine, 2, "control_scope", json!({"scope":"all"}));
        ok(&engine, 3, "select_core", json!({"index":1}));
        let context = ok(&engine, 4, "registers_list", json!({}))["context"].clone();
        let skipped = ok(
            &engine,
            5,
            "registers_read",
            json!({"context":context,"ids":ids,"manual":declared.is_some()}),
        );
        for sample in skipped["samples"].as_array().unwrap() {
            assert_eq!(
                sample["state"],
                if declared.is_some() {
                    "unsupported"
                } else {
                    "not_read"
                }
            );
            assert_eq!(
                sample["implementation"],
                if declared.is_some() { "no" } else { "unknown" }
            );
            assert!(sample["provenance"]["access"].is_null());
            assert!(sample["value"].is_null());
        }
        assert!(
            f.state.lock().unwrap()["trace"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let context = probe(&engine, 6);
        let listed = ok(&engine, 8, "registers_list", json!({}));
        assert_eq!(listed["facts"]["timer.present"], 1);
        f.state.lock().unwrap()["trace"] = json!([]);
        let read = ok(
            &engine,
            9,
            "registers_read",
            json!({"context":context,"ids":ids}),
        );
        for (id, encoding, value, bits) in entries {
            let sample = read["samples"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["id"] == id)
                .unwrap();
            assert_eq!(sample["state"], "valid", "{id}: {sample}");
            assert_eq!(sample["implementation"], "yes");
            assert_eq!(sample["value"], json!({"bits":bits,"hex":value}));
            assert_eq!(sample["owner"], "core:core1");
            assert_eq!(sample["provenance"]["access"]["route"]["target"], "cpu1");
            assert!(
                sample["provenance"]["access"]["command"]
                    .as_str()
                    .unwrap()
                    .contains(&format!(
                        "{} {encoding}",
                        if bits == 32 {
                            "aarch64 mrc"
                        } else {
                            "aarch64 mrrc"
                        }
                    ))
            );
            assert_eq!(
                sample["eligibility"]["conditions"][0]["source"],
                "observation"
            );
            assert_eq!(
                sample["eligibility"]["conditions"][0]["configured_value"],
                json!(declared)
            );
            assert_eq!(sample["eligibility"]["probe"]["context"], context);
        }
        let state = f.state.lock().unwrap().clone();
        let trace = state["trace"].as_array().unwrap();
        for (_, encoding, _, bits) in entries {
            let op = if bits == 32 { "mrc" } else { "mrrc" };
            let matched: Vec<_> = trace
                .iter()
                .filter(|t| {
                    t[1] == op
                        && t.as_array().unwrap()[2..]
                            .iter()
                            .map(|v| v.as_str().unwrap())
                            .collect::<Vec<_>>()
                            .join(" ")
                            == encoding
                })
                .collect();
            assert_eq!(matched.len(), 1, "{encoding}");
            assert_eq!(matched[0][0], "cpu1");
        }
        assert!(selector_writes(&state).is_empty());
        assert_eq!(state["current"], "outside");
        f.state.lock().unwrap()["fault"] = json!("timer_read_error");
        let failed = ok(
            &engine,
            10,
            "registers_read",
            json!({"context":context,"ids":["cnthp_tval","cntpct"],"manual":true}),
        );
        assert_eq!(failed["samples"][0]["implementation"], "yes");
        assert_eq!(failed["samples"][0]["reason"], "unknown");
        assert_eq!(failed["samples"][0]["state"], "unavailable");
        assert!(failed["samples"][0]["value"].is_null());
        assert_eq!(failed["samples"][1]["state"], "valid");
        let status = ok(&engine, 11, "status", json!({}));
        assert_eq!(status["state"], "STOPPED");
        let retained = status["register_samples"]
            .as_array()
            .unwrap()
            .iter()
            .find(|sample| sample["id"] == "cnthp_tval")
            .unwrap();
        assert_eq!(retained["state"], "unavailable");
        assert_eq!(retained["implementation"], "yes");
        assert_eq!(retained["value"]["hex"], "0x7ffffffe");
        assert_eq!(retained["last_value_eligibility"]["kind"], "known");
        ok(&engine, 12, "step", json!({}));
        ok(&engine, 13, "wait_stopped", json!({}));
        let state = f.state.lock().unwrap().clone();
        let new_context = ok(&engine, 14, "registers_list", json!({}))["context"].clone();
        let expired = ok(
            &engine,
            15,
            "registers_read",
            json!({"context":new_context,"ids":ids}),
        );
        for sample in expired["samples"].as_array().unwrap() {
            assert_eq!(
                sample["state"],
                if declared.is_some() {
                    "unsupported"
                } else {
                    "not_read"
                }
            );
        }
        assert_eq!(*f.state.lock().unwrap(), state);
        assert!(
            request(
                &engine,
                16,
                "registers_read",
                json!({"context":context,"ids":ids,"manual":true})
            )
            .is_err()
        );
        assert_eq!(*f.state.lock().unwrap(), state);
        ok(&engine, 17, "quit", json!({}));
    }
}
