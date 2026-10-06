//! Whole-module checks use real Coordinator/worker/MI and independently supplied bytes.
use super::*;

fn put(bytes: &mut Value, address: u32, value: u32) {
    bytes[format!("0x{address:x}")] = json!(
        value
            .to_le_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
}
fn save(memory: &Path, bytes: &Value) {
    fs::write(
        memory,
        serde_json::to_vec(&json!({"localhost:4900":bytes})).unwrap(),
    )
    .unwrap();
}
fn start(project: Project) -> EngineHandle {
    let engine = coordinator::spawn(project);
    call(&engine, 1, "connect", json!({}));
    let list = call(&engine, 2, "registers_list", json!({}));
    call(
        &engine,
        3,
        "registers_probe",
        json!({"context":list["context"]}),
    );
    engine
}
fn data_commands(mi: &Path) -> Vec<String> {
    fs::read_to_string(mi)
        .unwrap()
        .lines()
        .filter(|s| s.starts_with("-data-list-register-values r "))
        .map(str::to_owned)
        .collect()
}
fn read_only(mi: &Path) {
    let text = fs::read_to_string(mi).unwrap();
    for command in ["-data-write-", "-exec-", "-var-assign", "monitor ", "mcr "] {
        assert!(
            !text.contains(command),
            "Unexpected mutating command: {command}"
        );
    }
}
fn samples(result: &Value) -> &[Value] {
    result["samples"].as_array().unwrap()
}

#[test]
fn debug_modules_manual_dhcsr_and_live_trcena_evidence_control_dwt_without_writes() {
    for (cpu, cpuid) in [
        ("cortex-m3", 0x412fc231),
        ("cortex-m4", 0x410fc241),
        ("cortex-m7", 0x411fc271),
    ] {
        let (project, out, mi, memory) = fixture(cpu);
        let mut bytes = values(cpuid, 0, 8, false);
        put(&mut bytes, 0xe000edf0, 0x03010003);
        save(&memory, &bytes);
        let engine = start(project);
        let baseline = reads(&mi).len();
        let auto = call(
            &engine,
            4,
            "registers_read",
            json!({"ids":["dcb.dhcsr","dwt.ctrl"]}),
        );
        assert_eq!(auto["samples"][0]["state"], "not_read");
        assert!(
            auto["samples"][0]["detail"]
                .as_str()
                .unwrap()
                .contains("manual")
        );
        assert_eq!(auto["samples"][1]["reason"], "feature_disabled");
        assert!(
            auto["samples"][1]["detail"]
                .as_str()
                .unwrap()
                .contains("NeedEnable")
        );
        assert_eq!(reads(&mi).len(), baseline);
        let manual = call(
            &engine,
            5,
            "registers_read",
            json!({"ids":["dcb.dhcsr"],"manual":true}),
        );
        assert_eq!(manual["samples"][0]["state"], "valid");
        assert_eq!(manual["samples"][0]["value"]["hex"], "0x03010003");
        assert_eq!(reads(&mi).len(), baseline + 1);
        assert_eq!(
            reads(&mi).last().unwrap(),
            "-data-read-memory-bytes 0xe000edf0 4"
        );
        // The fixture changes existing target state externally; DebugTUI never enables it.
        put(&mut bytes, 0xe000edfc, 1 << 24);
        put(&mut bytes, 0xe0001000, 0xa0000000);
        save(&memory, &bytes);
        call(&engine, 6, "registers_read", json!({"ids":["dcb.demcr"]}));
        let allowed = call(&engine, 7, "registers_read", json!({"ids":["dwt.ctrl"]}));
        assert_eq!(allowed["samples"][0]["state"], "valid");
        assert_eq!(allowed["samples"][0]["value"]["hex"], "0xa0000000");
        let matrix = call(&engine, 8, "registers_matrix", json!({}));
        assert_eq!(matrix["effective_facts"]["dwt.comparators"], 10);
        // A newer disabled or failed enable observation revokes old capacity.
        for failure in [false, true] {
            if failure {
                bytes.as_object_mut().unwrap().remove("0xe000edfc");
            } else {
                put(&mut bytes, 0xe000edfc, 0);
            }
            save(&memory, &bytes);
            call(&engine, 9, "registers_read", json!({"ids":["dcb.demcr"]}));
            let count = reads(&mi).len();
            let denied = call(
                &engine,
                10,
                "registers_read",
                json!({"ids":["dwt.ctrl"],"manual":true}),
            );
            assert_eq!(denied["samples"][0]["state"], "unavailable");
            assert!(denied["samples"][0]["value"].is_null());
            assert_eq!(
                denied["samples"][0]["reason"],
                if failure {
                    "unknown"
                } else {
                    "feature_disabled"
                }
            );
            let matrix = call(&engine, 11, "registers_matrix", json!({}));
            assert!(matrix["effective_facts"]["dwt.comparators"].is_null());
            assert_eq!(reads(&mi).len(), count);
        }
        fs::write(out.join("debug-module-evidence.json"),serde_json::to_vec_pretty(&json!({"cpu":cpu,"automatic":auto,"manual":manual,"enabled":allowed,"board_tests_executed":false})).unwrap()).unwrap();
        call(&engine, 12, "quit", json!({}));
        read_only(&mi);
    }
}

#[test]
fn fpb_revisions_split_maximum_and_zero_counts_are_observed_not_guessed() {
    for (raw, revision, code, literal) in [
        (0, Some(0), 0, 0),
        (0x10007ff1, Some(1), 127, 15),
        (0x20000000, None, 0, 0),
    ] {
        let (project, _, mi, memory) = fixture("cortex-m4");
        let mut bytes = values(0x410fc241, 0, 8, false);
        put(&mut bytes, 0xe0002000, raw);
        save(&memory, &bytes);
        let engine = start(project);
        let count = reads(&mi).len();
        let matrix = call(&engine, 4, "registers_matrix", json!({}));
        if let Some(revision) = revision {
            assert_eq!(matrix["effective_facts"]["fpb.revision"], revision);
            assert_eq!(matrix["effective_facts"]["fpb.code_comparators"], code);
            assert_eq!(
                matrix["effective_facts"]["fpb.literal_comparators"],
                literal
            );
        } else {
            assert!(matrix["effective_facts"]["fpb.revision"].is_null());
            assert!(matrix["effective_facts"]["fpb.code_comparators"].is_null());
        }
        assert_eq!(reads(&mi).len(), count);
        let raw_sample = call(&engine, 5, "registers_read", json!({"ids":["fpb.ctrl"]}));
        assert_eq!(raw_sample["samples"][0]["state"], "valid");
        assert_eq!(
            u32::from_str_radix(
                raw_sample["samples"][0]["value"]["hex"]
                    .as_str()
                    .unwrap()
                    .trim_start_matches("0x"),
                16
            )
            .unwrap(),
            raw
        );
        bytes.as_object_mut().unwrap().remove("0xe0002000");
        save(&memory, &bytes);
        call(&engine, 6, "registers_read", json!({"ids":["fpb.ctrl"]}));
        let matrix = call(&engine, 7, "registers_matrix", json!({}));
        assert!(matrix["effective_facts"]["fpb.code_comparators"].is_null());
        call(&engine, 8, "quit", json!({}));
        read_only(&mi);
    }
}

fn floating(project: &mut Project) {
    project.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTERS".into(),
        json!(["r0", "", "d0", "d15", "fpscr"]).to_string(),
    );
    project.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTER_VALUES".into(),
        json!({"d0":"0x7fc01234bf800000","d15":"0xff80000000000001","fpscr":"0x02400000"})
            .to_string(),
    );
}

#[test]
fn fpu_configuration_and_gdb_d_s_storage_read_with_disabled_or_privileged_cpacr() {
    for (cpu, cpuid, cpacr) in [
        ("cortex-m4", 0x410fc241, 0),
        ("cortex-m4", 0x410fc241, 0x00500000),
        ("cortex-m7", 0x411fc271, 0x00f00000),
    ] {
        let (mut project, out, mi, memory) = fixture(cpu);
        floating(&mut project);
        let mut bytes = values(cpuid, 0, 8, false);
        for (address, value) in [
            (0xe000ed88, cpacr),
            (0xe000ef34, 0xc0000000),
            (0xe000ef38, 0x20001000),
            (0xe000ef3c, 0x02400000),
        ] {
            put(&mut bytes, address, value);
        }
        save(&memory, &bytes);
        let engine = start(project);
        let result = call(
            &engine,
            4,
            "registers_read",
            json!({"ids":["scb.cpacr","fpu.fpccr","fpu.fpcar","fpu.fpdscr","d0","s0","s1","d15","s30","s31","fpscr"],"manual":true}),
        );
        for sample in samples(&result) {
            assert_eq!(sample["state"], "valid", "{}", sample["id"]);
            assert_eq!(sample["owner"], "core:default");
        }
        let expected = [
            format!("0x{cpacr:08x}"),
            "0xc0000000".into(),
            "0x20001000".into(),
            "0x02400000".into(),
            "0x7fc01234bf800000".into(),
            "0xbf800000".into(),
            "0x7fc01234".into(),
            "0xff80000000000001".into(),
            "0x00000001".into(),
            "0xff800000".into(),
            "0x02400000".into(),
        ];
        for (sample, raw) in samples(&result).iter().zip(expected) {
            assert_eq!(sample["value"]["hex"], raw);
        }
        assert_eq!(
            data_commands(&mi),
            [
                "-data-list-register-values r 2",
                "-data-list-register-values r 3",
                "-data-list-register-values r 4"
            ]
        );
        assert_eq!(result["samples"][4]["view"], "selected_frame");
        assert_eq!(result["samples"][0]["view"], "physical_core");
        assert_eq!(
            result["samples"][5]["provenance"]["access"],
            result["samples"][4]["provenance"]["access"]
        );
        // Invalid new MVFR0 must remove old implementation proof even with user-declared facts.
        put(&mut bytes, 0xe000ef40, 0);
        save(&memory, &bytes);
        call(&engine, 5, "registers_read", json!({"ids":["fpu.mvfr0"]}));
        let before = (reads(&mi).len(), data_commands(&mi).len());
        let revoked = call(
            &engine,
            6,
            "registers_read",
            json!({"ids":["d0","s0","fpu.fpccr","fpscr"],"manual":true}),
        );
        for sample in samples(&revoked) {
            assert_eq!(sample["state"], "unavailable");
            assert_eq!(sample["implementation"], "unknown");
            assert!(sample["value"].is_null());
        }
        assert_eq!((reads(&mi).len(), data_commands(&mi).len()), before);
        fs::write(out.join("fpu-module-evidence.json"),serde_json::to_vec_pretty(&json!({"cpu":cpu,"cpacr":cpacr,"values":result,"revoked":revoked,"board_tests_executed":false})).unwrap()).unwrap();
        call(&engine, 7, "quit", json!({}));
        read_only(&mi);
    }
}

#[test]
fn unproven_or_invalid_fpu_identity_never_sends_configuration_or_regfile_reads() {
    for raw in [None, Some(0), Some(0x10110022), Some(0x10110121)] {
        let (mut project, _, mi, memory) = fixture("cortex-m4");
        floating(&mut project);
        project.registers.facts.insert("vfp.present".into(), 1);
        project.registers.facts.insert("vfp.d_registers".into(), 32);
        let mut bytes = values(0x410fc241, 0, 8, false);
        if let Some(raw) = raw {
            put(&mut bytes, 0xe000ef40, raw);
        } else {
            bytes.as_object_mut().unwrap().remove("0xe000ef40");
        }
        save(&memory, &bytes);
        let engine = start(project);
        let before = (reads(&mi).len(), data_commands(&mi).len());
        let result = call(
            &engine,
            4,
            "registers_read",
            json!({"ids":["fpu.fpccr","fpu.fpcar","fpu.fpdscr","d0","s0","fpscr"],"manual":true}),
        );
        for sample in samples(&result) {
            assert_eq!(sample["implementation"], "unknown");
            assert_eq!(sample["state"], "unavailable");
            assert!(sample["value"].is_null());
        }
        assert_eq!((reads(&mi).len(), data_commands(&mi).len()), before);
        call(&engine, 5, "quit", json!({}));
        read_only(&mi);
    }
}

#[test]
fn fpu_backend_missing_names_and_read_failure_preserve_precise_old_evidence() {
    for missing in [true, false] {
        let (mut project, out, mi, memory) = fixture("cortex-m4");
        floating(&mut project);
        if missing {
            project.gdb.env.insert(
                "DEBUGTUI_TEST_REGISTERS".into(),
                json!(["r0", "s0", "s1"]).to_string(),
            );
        }
        let overrides = out.join("register-values.json");
        project.gdb.env.insert(
            "DEBUGTUI_TEST_REGISTER_VALUES_FILE".into(),
            overrides.to_string_lossy().into_owned(),
        );
        save(&memory, &values(0x410fc241, 0, 8, false));
        let engine = start(project);
        let first = call(
            &engine,
            4,
            "registers_read",
            json!({"ids":["d0","s0","fpscr"],"manual":true}),
        );
        if missing {
            for sample in samples(&first) {
                assert_eq!(sample["state"], "unsupported");
                assert_eq!(sample["reason"], "reader_unsupported");
                assert!(sample["value"].is_null());
            }
            assert!(data_commands(&mi).is_empty());
        } else {
            assert_eq!(first["samples"][0]["state"], "valid");
            fs::write(overrides, json!({"d0":"not-a-register-value"}).to_string()).unwrap();
            let failed = call(
                &engine,
                5,
                "registers_read",
                json!({"ids":["d0","s0"],"manual":true}),
            );
            for sample in samples(&failed) {
                assert_ne!(sample["state"], "valid");
                assert!(sample["value"].is_null());
            }
            let status = call(&engine, 6, "status", json!({}));
            let stored = status["register_samples"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["id"] == "d0")
                .unwrap();
            assert_ne!(stored["state"], "valid");
            assert_eq!(stored["value"], first["samples"][0]["value"]);
            assert_eq!(
                stored["last_value_provenance"]["provenance"],
                first["samples"][0]["provenance"]
            );
        }
        call(&engine, 7, "quit", json!({}));
        read_only(&mi);
    }
}

#[test]
fn m_modules_deferred_driver_runs_real_exe_and_rejects_wrong_independent_fpu_values() {
    use std::process::Command;
    let (mut project, out, mi, memory) = fixture("cortex-m4");
    floating(&mut project);
    let mut bytes = values(0x410fc241, 0, 8, false);
    for (address, value) in [
        (0xe000edf0, 0x03030003),
        (0xe000ed88, 0),
        (0xe000ef34, 0xc0000000),
        (0xe000ef38, 0x20001000),
        (0xe000ef3c, 0x02400000),
    ] {
        put(&mut bytes, address, value);
    }
    save(&memory, &bytes);
    let project_file = out.join("module-driver.toml");
    fs::write(&project_file, toml::to_string(&project).unwrap()).unwrap();
    let original = fs::read(&project_file).unwrap();
    let case_file = out.join("module-driver.json");
    let mut spec: Value = serde_json::from_str(include_str!(
        "../fixtures/m-profile-modules-board.example.json"
    ))
    .unwrap();
    fs::write(&case_file, serde_json::to_vec(&spec).unwrap()).unwrap();
    let run = || {
        Command::new("node")
            .arg(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("scripts/test-m-profile-modules-hardware.cjs"),
            )
            .args([
                "--run",
                "--software-fixture",
                "--core",
                "default",
                "--binary",
            ])
            .arg(env!("CARGO_BIN_EXE_debugtui"))
            .arg("--project")
            .arg(&project_file)
            .arg("--case")
            .arg(&case_file)
            .env("DEBUGTUI_TEST_ARTIFACT_ROOT", &out)
            .output()
            .unwrap()
    };
    let result = run();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        String::from_utf8_lossy(&result.stdout).contains("\"passed\":6,\"failed\":0,\"skipped\":0")
    );
    spec["fpu"]["storage"]["d0"] = json!("0x0000000000000000");
    fs::write(&case_file, serde_json::to_vec(&spec).unwrap()).unwrap();
    let wrong = run();
    assert!(!wrong.status.success());
    assert!(String::from_utf8_lossy(&wrong.stderr).contains("M-MOD-H04-FPU"));
    assert_eq!(fs::read(&project_file).unwrap(), original);
    read_only(&mi);
}
