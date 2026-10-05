use super::*;

#[test]
fn deferred_register_matrix_driver_runs_actual_binary_and_rejects_bad_independent_values() {
    let mut f = fixture("");
    f.project.registers.vfp_command = "aarch64 vfp".into();
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
    let dir = f.transcript.parent().unwrap();
    let project = dir.join("matrix-driver.toml");
    let case = dir.join("matrix-driver.json");
    fs::write(&project, toml::to_string(&f.project).unwrap()).unwrap();
    let original = fs::read(&project).unwrap();
    for negative in [false, true] {
        let spec = json!({"software_example":true,"evidence_source":"independent frozen fixture bit patterns","control_scope":"all","entries":[
            {"id":"s0","bits":32,"support":"observed_value","expected_hex":if negative{"0x3f800001"}else{"0x3f800000"}},
            {"id":"d0","bits":64,"support":"observed_value","expected_hex":"0x800000013f800000"},
            {"id":"q0","bits":128,"support":"observed_value","expected_hex":"0x7ff8000112345678800000013f800000"}
        ]});
        fs::write(&case, serde_json::to_vec_pretty(&spec).unwrap()).unwrap();
        let output = Command::new("node")
            .arg(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("scripts/test-register-matrix-hardware.cjs"),
            )
            .args([
                "--run",
                "--software-fixture",
                "--core",
                "core1",
                "--binary",
                env!("CARGO_BIN_EXE_debugtui"),
                "--project",
            ])
            .arg(&project)
            .arg("--case")
            .arg(&case)
            .output()
            .unwrap();
        let text = String::from_utf8(output.stdout).unwrap();
        assert_eq!(
            output.status.success(),
            !negative,
            "{text}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains(if negative {
                "\"passed\":1,\"failed\":1,\"skipped\":0"
            } else {
                "\"passed\":4,\"failed\":0,\"skipped\":0"
            }),
            "{text}"
        );
        assert_eq!(fs::read(&project).unwrap(), original);
    }
}

fn row<'a>(matrix: &'a Value, id: &str) -> &'a Value {
    matrix["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .unwrap()
}
#[test]
fn register_matrix_exports_native_vfp_receipts_and_quarantined_failures_without_target_io() {
    let mut f = fixture("");
    f.project.registers.vfp_command = "aarch64 vfp".into();
    f.project.registers.timer_command = "aarch64 timer".into();
    let engine = session::spawn(f.project.clone());
    ok(&engine, 1, "connect", json!({}));
    let baseline = ok(
        &engine,
        2,
        "registers_read",
        json!({"ids":["q0","d0","s0"],"manual":true}),
    );
    assert!(
        baseline["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["state"] == "valid")
    );
    let commands = fs::read_to_string(&f.transcript).unwrap();
    let before = f.state.lock().unwrap().clone();
    let matrix = ok(&engine, 3, "registers_matrix", json!({}));
    for (id, bits) in [("q0", 128), ("d0", 64), ("s0", 32)] {
        let item = row(&matrix, id);
        assert_eq!(item["bits"], bits);
        assert_eq!(item["support"], "observed_value");
        let observation = &matrix["observations"][item["observation"].as_u64().unwrap() as usize];
        assert_eq!(
            observation["provenance"]["access"]["vfp"]["dscr"]["hex"],
            "0x01000200"
        );
        assert_eq!(
            observation["provenance"]["access"],
            baseline["samples"][0]["provenance"]["access"]
        );
    }
    assert_eq!(
        row(&matrix, "cntpct")["plan"]["protocol"],
        debugtui::registers::timer::PROTOCOL
    );
    assert_eq!(fs::read_to_string(&f.transcript).unwrap(), commands);
    assert_eq!(*f.state.lock().unwrap(), before);
    f.state.lock().unwrap()["fault"] = json!("vfp_fault");
    let failed = ok(
        &engine,
        4,
        "registers_read",
        json!({"ids":["q0"],"manual":true}),
    );
    assert_ne!(failed["samples"][0]["state"], "valid");
    let commands = fs::read_to_string(&f.transcript).unwrap();
    let before = f.state.lock().unwrap().clone();
    let matrix = ok(&engine, 5, "registers_matrix", json!({}));
    assert_ne!(row(&matrix, "q0")["support"], "observed_value");
    assert!(!matrix["environment"]["access_fault"].is_null());
    assert_eq!(row(&matrix, "q0")["plan"]["available"], false);
    let observation =
        &matrix["observations"][row(&matrix, "q0")["observation"].as_u64().unwrap() as usize];
    assert_eq!(observation["value"], baseline["samples"][0]["value"]);
    assert_eq!(
        observation["last_value_provenance"]["provenance"],
        baseline["samples"][0]["provenance"]
    );
    assert_eq!(fs::read_to_string(&f.transcript).unwrap(), commands);
    assert_eq!(*f.state.lock().unwrap(), before);
    ok(&engine, 6, "quit", json!({}));
}
