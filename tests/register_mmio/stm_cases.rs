// Independent STM v1.1/STM-500 words; never use production descriptors/generator.
const STM_WORDS: &[(u64, u32)] = &[
    (0xff0, 13),
    (0xff4, 0x90),
    (0xff8, 5),
    (0xffc, 0xb1),
    (0xfbc, 0x47710a63),
    (0xfcc, 0x63),
    (0xfe0, 0x63),
    (0xfe4, 0xb9),
    (0xfe8, 0x0b),
    (0xfec, 0),
    (0xfd0, 4),
    (0xfc8, 65536),
    (0xea0, 0x8240),
    (0xea4, 0x42),
    (0xea8, 127),
    (0xdfc, 0x11),
    (0xdf8, 0x20200001),
    (0xcfc, 2),
    (0xe80, 0x12340021),
    (0xe00, 0x55aa55aa),
    (0xe60, 3),
    (0xe64, 4),
    (0xd60, 1),
    (0xd00, 0xabcdef01),
];
fn stm_fixture(name: &str, multi: bool) -> (Project, PathBuf, Value) {
    let (mut p, out) = fixture(name);
    p.registers.facts.clear();
    p.registers.topology.chip = "board".into();
    p.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTERS".into(),
        json!(["cpsr", "midr"]).to_string(),
    );
    // A deliberately unadapted CPU must not determine a separate STM identity.
    p.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTER_VALUES".into(),
        json!({"cpsr":"0x1a","midr":"0xdeadbeef"}).to_string(),
    );
    if multi {
        p.cores = (0..4)
            .map(|i| Core {
                name: format!("cpu.{i}"),
                endpoint: format!("localhost:{}", 6730 + i),
                ..Default::default()
            })
            .collect();
    } else {
        p.target.endpoint = "localhost:6730".into();
    }
    for name in ["stm", "stm_hwe", "stm_dma"] {
        p.registers.component_owners.insert(
            name.into(),
            [("chip:board".into(), route(0x51000000, "", true))].into(),
        );
    }
    let mut memory = json!({});
    for i in 0..if multi { 4 } else { 1 } {
        for &(off, value) in STM_WORDS {
            let value = if off == 0xe80 {
                value + i * 0x100
            } else {
                value
            };
            memory[format!("localhost:{}", 6730 + i)][format!("0x{:x}", 0x51000000 + off)] = json!(
                value
                    .to_le_bytes()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            );
        }
    }
    fs::write(out.join("memory.json"), memory.to_string()).unwrap();
    p.gdb.env.insert(
        "DEBUGTUI_TEST_OWNER_MEMORY_FILE".into(),
        out.join("memory.json").to_string_lossy().into_owned(),
    );
    (p, out, memory)
}
fn stm_probe(e: &EngineHandle, id: u64) -> Value {
    let list = call(e, id, "registers_list", json!({}));
    call(
        e,
        id + 1,
        "registers_probe",
        json!({"context":list["context"]}),
    )
}

#[test]
fn stm_four_workers_scope_all_require_proof_and_preserve_chip_route_and_bank_selectors() {
    let (p, out, _) = stm_fixture("STM four workers", true);
    let e = coordinator::spawn(p);
    call(&e, 1, "connect", json!({}));
    call(&e, 2, "control_scope", json!({"scope":"all"}));
    for i in 0..4 {
        call(&e, 3, "select_core", json!({"index":i}));
        let before = memory_reads(&out).len();
        let no = call(
            &e,
            4,
            "registers_read",
            json!({"ids":["stm_tcsr"],"manual":true}),
        );
        assert_eq!(no["samples"][0]["reason"], "reader_unsupported");
        assert_eq!(memory_reads(&out).len(), before);
        let proof = stm_probe(&e, 5);
        assert_eq!(proof["facts"]["stm.present"], 1);
        assert_eq!(proof["facts"]["stm_hwe.events"], 64);
        assert_eq!(memory_reads(&out).len(), before + 36);
        let ids = [
            "stm_tcsr",
            "stm_sper",
            "stm_spscr",
            "stm_spmscr",
            "stm_hebsr",
            "stm_heer",
            "stm_lar",
            "stm_tsstimr",
            "stm_dmastart",
            "stm_dmastop",
        ];
        let data = call(&e, 7, "registers_read", json!({"ids":ids}));
        let s = &data["samples"][0];
        assert_eq!(s["state"], "valid");
        assert_eq!(
            s["value"]["hex"],
            format!("0x{:08x}", 0x12340021 + i * 0x100)
        );
        assert_eq!(s["owner"], "chip:board");
        assert!(s["owner_generation"].is_u64());
        assert_eq!(s["context"]["core"], format!("cpu.{i}"));
        assert_eq!(
            s["provenance"]["access"]["route"]["endpoint"],
            format!("localhost:{}", 6730 + i)
        );
        for s in data["samples"].as_array().unwrap().iter().skip(6) {
            assert_eq!(s["reason"], "write_only");
            assert!(s["provenance"]["access"].is_null());
        }
        assert_eq!(memory_reads(&out).len(), before + 42);
        let matrix = call(&e, 8, "registers_matrix", json!({}));
        assert_eq!(
            matrix["planned_classes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["id"] == "stm")
                .unwrap()["hardware_support"],
            "unverified"
        );
    }
    let commands = fs::read_to_string(out.join("commands.txt")).unwrap();
    assert!(
        !commands.contains("-data-write")
            && !commands.contains("-exec-continue")
            && !commands.contains("0x51000000 4")
    );
    call(&e, 9, "quit", json!({}));
}
#[test]
fn stm_probe_rejects_bad_identity_missing_owner_unaligned_or_different_optional_routes_before_data_io()
 {
    for scenario in ["arch", "permission", "mapping", "unaligned", "optional"] {
        let (mut p, out, mut memory) = stm_fixture(scenario, false);
        let expected = match scenario {
            "arch" => {
                memory["localhost:6730"]["0x51000fbc"] = json!("630a7047");
                5
            }
            "permission" => {
                memory["localhost:6730"]
                    .as_object_mut()
                    .unwrap()
                    .remove("0x51000ff4");
                2
            }
            "mapping" => {
                p.registers
                    .components
                    .insert("stm".into(), route(0x51000000, "", true));
                p.registers.component_owners.clear();
                0
            }
            "unaligned" => {
                p.registers
                    .component_owners
                    .get_mut("stm")
                    .unwrap()
                    .get_mut("chip:board")
                    .unwrap()
                    .base += 4;
                0
            }
            "optional" => {
                p.registers
                    .component_owners
                    .get_mut("stm_hwe")
                    .unwrap()
                    .get_mut("chip:board")
                    .unwrap()
                    .base += 0x1000;
                p.registers
                    .component_owners
                    .get_mut("stm_dma")
                    .unwrap()
                    .get_mut("chip:board")
                    .unwrap()
                    .little_endian = false;
                30
            }
            _ => unreachable!(),
        };
        fs::write(out.join("memory.json"), memory.to_string()).unwrap();
        let e = coordinator::spawn(p);
        call(&e, 1, "connect", json!({}));
        let proof = stm_probe(&e, 2);
        assert_eq!(
            proof["probe"]["facts"]["stm.present"].is_null(),
            scenario != "optional"
        );
        assert!(proof["probe"]["facts"]["stm_hwe.present"].is_null());
        assert!(proof["probe"]["facts"]["stm_dma.present"].is_null());
        assert_eq!(memory_reads(&out).len(), expected, "{scenario}");
        let data = call(
            &e,
            4,
            "registers_read",
            json!({"ids":[if scenario=="optional"{"stm_heer"}else{"stm_tcsr"}],"manual":true}),
        );
        assert_eq!(data["samples"][0]["reason"], "reader_unsupported");
        assert_eq!(memory_reads(&out).len(), expected);
        call(&e, 5, "quit", json!({}));
    }
}
#[test]
fn stm_fresh_ap_proof_uses_actual_big_endian_target_and_never_falls_back_to_gdb() {
    let (mut p, out, _) = stm_fixture("STM AP", false);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap().to_string();
    for owners in p.registers.component_owners.values_mut() {
        for c in owners.values_mut() {
            c.channel = "stm_ap".into();
            c.little_endian = false;
        }
    }
    p.memory_access_source = "stm-test".into();
    p.memory_access = vec![MemoryAccess {
        id: "stm_ap".into(),
        target: "soc.stm.ap".into(),
        tcl_endpoint: endpoint.clone(),
        ..Default::default()
    }];
    let packets = Arc::new(Mutex::new(vec![]));
    let saved = packets.clone();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        for _ in 0..38 {
            let mut bytes = vec![];
            loop {
                let mut b = [0];
                stream.read_exact(&mut b).unwrap();
                if b[0] == 0x1a {
                    break;
                }
                bytes.push(b[0]);
            }
            let text = String::from_utf8(bytes).unwrap();
            assert!(text.contains("soc.stm.ap"));
            let (_, raw) = STM_WORDS
                .iter()
                .find(|(off, _)| {
                    text.contains(&format!("read_memory 0x{:x} 32 1", 0x51000000 + off))
                })
                .unwrap();
            saved.lock().unwrap().push(text);
            stream
                .write_all(format!("0:0x{raw:x}\x1a").as_bytes())
                .unwrap();
        }
    });
    let e = coordinator::spawn(p);
    call(&e, 1, "connect", json!({}));
    let proof = stm_probe(&e, 2);
    assert_eq!(proof["facts"]["stm.part"], 0x963);
    let data = call(
        &e,
        4,
        "registers_read",
        json!({"ids":["stm_tcsr","stm_sper"]}),
    );
    assert_eq!(data["samples"][0]["value"]["hex"], "0x12340021");
    let a = &data["samples"][0]["provenance"]["access"]["route"];
    assert_eq!(a["kind"], "tcl_memory");
    assert_eq!(a["endpoint"], endpoint);
    assert_eq!(a["target"], "soc.stm.ap");
    assert_eq!(a["byte_order"], "big");
    assert_eq!(a["bus_width"], 32);
    assert_eq!(a["count"], 1);
    assert_eq!(a["configuration_source"], "stm-test");
    assert!(memory_reads(&out).is_empty());
    server.join().unwrap();
    assert_eq!(packets.lock().unwrap().len(), 38);
    call(&e, 5, "quit", json!({}));
}
#[test]
fn stm_peer_activity_discards_shared_proof_during_and_after_reads() {
    for during in [false, true] {
        let (mut p, out, _) = stm_fixture("STM shared", true);
        let notice = out.join("notify.json");
        fs::write(&notice, "{}").unwrap();
        p.gdb.env.insert(
            "DEBUGTUI_TEST_NOTIFY_FILE".into(),
            notice.to_string_lossy().into_owned(),
        );
        if during {
            p.gdb
                .env
                .insert("DEBUGTUI_TEST_OWNER_MEMORY_DELAY_MS".into(), "30".into());
        }
        let e = coordinator::spawn(p);
        call(&e, 1, "connect", json!({}));
        call(&e, 2, "select_core", json!({"index":0}));
        let watcher = if during {
            let commands = out.join("commands.txt");
            let notice = notice.clone();
            Some(thread::spawn(move || {
                let end = Instant::now() + Duration::from_secs(15);
                loop {
                    if fs::read_to_string(&commands)
                        .unwrap_or_default()
                        .contains("-data-read-memory-bytes")
                    {
                        fs::write(notice, json!({"localhost:6731":"running"}).to_string()).unwrap();
                        break;
                    }
                    assert!(Instant::now() < end);
                    thread::sleep(Duration::from_millis(10));
                }
            }))
        } else {
            None
        };
        let proof = stm_probe(&e, 3);
        if let Some(w) = watcher {
            w.join().unwrap();
            assert!(proof["probe"]["facts"]["stm.present"].is_null());
        } else {
            assert_eq!(proof["facts"]["stm.present"], 1);
            let s = call(&e, 5, "registers_read", json!({"ids":["stm_tcsr"]}));
            assert_eq!(s["samples"][0]["state"], "valid");
            fs::write(&notice, json!({"localhost:6731":"running"}).to_string()).unwrap();
        }
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            let s = call(&e, 90, "status", json!({}));
            if s["cores"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["name"] == "cpu.1" && c["state"] == "RUNNING")
            {
                break;
            }
            assert!(Instant::now() < end);
            thread::sleep(Duration::from_millis(20));
        }
        let matrix = call(&e, 6, "registers_matrix", json!({}));
        assert!(matrix["probe"]["facts"]["stm.present"].is_null());
        let before = memory_reads(&out).len();
        let s = call(
            &e,
            7,
            "registers_read",
            json!({"ids":["stm_tcsr"],"manual":true}),
        );
        assert_eq!(s["samples"][0]["reason"], "reader_unsupported");
        assert_eq!(memory_reads(&out).len(), before);
        fs::write(notice, "{}").unwrap();
        call(&e, 8, "quit", json!({}));
    }
}

#[test]
fn deferred_stm_driver_runs_actual_binary_and_rejects_unready_identity_or_baseline_mismatch() {
    use std::process::Command;
    let driver =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/test-register-stm-hardware.cjs");
    let default = Command::new("node").arg(&driver).output().unwrap();
    assert!(default.status.success());
    assert!(
        String::from_utf8_lossy(&default.stdout)
            .contains("\"passed\":0,\"failed\":0,\"skipped\":5")
    );
    for scenario in ["positive", "not_ready", "identity", "baseline"] {
        let (mut p, out, mut memory) = stm_fixture(scenario, false);
        let mut spec: Value =
            serde_json::from_str(include_str!("../fixtures/register-stm-board.example.json"))
                .unwrap();
        spec["frame_function"] = json!("main");
        let mut expressions = json!({"(unsigned long long)debugtui_stm_reference[0].ready":if scenario=="not_ready"{"0"}else{"1"},"(unsigned long long)debugtui_stm_reference[0].core_tag":"0"});
        for word in spec["registers"].as_array().unwrap() {
            expressions[format!(
                "(unsigned long long){}",
                word["reference"].as_str().unwrap()
            )] = word["expected"].clone();
        }
        if scenario == "baseline" {
            expressions["(unsigned long long)debugtui_stm_reference[0].raw[0]"] =
                json!("0x12340020");
        }
        if scenario == "identity" {
            memory["localhost:6730"]["0x51000fbc"] = json!("630a7047");
            fs::write(out.join("memory.json"), memory.to_string()).unwrap();
        }
        p.gdb.env.insert(
            "DEBUGTUI_TEST_EXPRESSION_VALUES".into(),
            expressions.to_string(),
        );
        let project = out.join("project.toml");
        fs::write(&project, toml::to_string(&p).unwrap()).unwrap();
        let original = fs::read(&project).unwrap();
        let case = out.join("case.json");
        fs::write(&case, spec.to_string()).unwrap();
        let output = Command::new("node")
            .arg(&driver)
            .args([
                "--run",
                "--software-fixture",
                "--core",
                "default",
                "--binary",
            ])
            .arg(env!("CARGO_BIN_EXE_debugtui"))
            .arg("--project")
            .arg(&project)
            .arg("--case")
            .arg(&case)
            .output()
            .unwrap();
        fs::write(out.join("driver-stdout.log"), &output.stdout).unwrap();
        fs::write(out.join("driver-stderr.log"), &output.stderr).unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(
            output.status.success(),
            scenario == "positive",
            "{scenario}: {stdout} / {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let dir = stdout
            .lines()
            .find(|l| l.starts_with("RESULT "))
            .unwrap()
            .split_whitespace()
            .last()
            .unwrap();
        let report: Value =
            serde_json::from_slice(&fs::read(PathBuf::from(dir).join("report.json")).unwrap())
                .unwrap();
        assert_eq!(report["board_tests_executed"], false);
        assert_eq!(
            report["counts"]["passed"],
            match scenario {
                "positive" => 5,
                "not_ready" => 1,
                "identity" => 2,
                "baseline" => 3,
                _ => unreachable!(),
            }
        );
        assert_eq!(
            memory_reads(&out).len(),
            match scenario {
                "positive" => 48,
                "not_ready" => 0,
                "identity" => 5,
                "baseline" => 42,
                _ => unreachable!(),
            }
        );
        assert_eq!(fs::read(project).unwrap(), original);
        let commands = fs::read_to_string(out.join("commands.txt")).unwrap();
        assert!(
            !commands.contains("-data-write")
                && !commands.contains("-exec-continue")
                && !commands.contains("debugtui_stm_capture")
        );
    }
}

#[test]
fn stm_changed_post_identity_stops_later_addresses_and_failed_refresh_keeps_old_origin() {
    let (mut p, out, mut memory) = stm_fixture("STM changed post identity", false);
    p.gdb
        .env
        .insert("DEBUGTUI_TEST_OWNER_MEMORY_DELAY_MS".into(), "30".into());
    let e = coordinator::spawn(p);
    call(&e, 1, "connect", json!({}));
    let commands = out.join("commands.txt");
    let file = out.join("memory.json");
    let watcher = thread::spawn(move || {
        let end = Instant::now() + Duration::from_secs(15);
        loop {
            let text = fs::read_to_string(&commands).unwrap_or_default();
            if text
                .lines()
                .filter(|s| *s == "-data-read-memory-bytes 0x51000cfc 4")
                .count()
                == 2
            {
                memory["localhost:6730"]["0x51000ff0"] = json!("00000000");
                fs::write(file, memory.to_string()).unwrap();
                break;
            }
            assert!(Instant::now() < end);
            thread::sleep(Duration::from_millis(5));
        }
    });
    let proof = stm_probe(&e, 2);
    watcher.join().unwrap();
    assert!(proof["probe"]["facts"]["stm.present"].is_null());
    assert!(
        proof["probe"]["notes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s.as_str().unwrap().contains("changed during verification"))
    );
    assert_eq!(memory_reads(&out).len(), 22);
    call(&e, 4, "quit", json!({}));
    let (p, out, mut memory) = stm_fixture("STM failed refresh", false);
    let e = coordinator::spawn(p);
    call(&e, 1, "connect", json!({}));
    stm_probe(&e, 2);
    let old = call(&e, 4, "registers_read", json!({"ids":["stm_tcsr"]}));
    assert_eq!(old["samples"][0]["state"], "valid");
    memory["localhost:6730"]
        .as_object_mut()
        .unwrap()
        .remove("0x51000e80");
    fs::write(out.join("memory.json"), memory.to_string()).unwrap();
    let failed = call(&e, 5, "registers_read", json!({"ids":["stm_tcsr"]}));
    assert_eq!(failed["samples"][0]["state"], "error");
    memory["localhost:6730"]
        .as_object_mut()
        .unwrap()
        .remove("0x51000ff0");
    fs::write(out.join("memory.json"), memory.to_string()).unwrap();
    stm_probe(&e, 6);
    let before = memory_reads(&out).len();
    let refused = call(
        &e,
        8,
        "registers_read",
        json!({"ids":["stm_tcsr"],"manual":true}),
    );
    assert_eq!(refused["samples"][0]["reason"], "reader_unsupported");
    assert_eq!(memory_reads(&out).len(), before);
    let status = call(&e, 9, "status", json!({}));
    let kept = status["register_samples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "stm_tcsr")
        .unwrap();
    assert_eq!(kept["value"], old["samples"][0]["value"]);
    assert_eq!(
        kept["last_value_provenance"]["provenance"],
        old["samples"][0]["provenance"]
    );
    call(&e, 10, "quit", json!({}));
}
