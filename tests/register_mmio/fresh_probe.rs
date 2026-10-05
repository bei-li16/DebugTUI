// Included inside the real-worker MMIO fixtures; independent manual TRM table.
const PROOF_WORDS: &[(u64, u16, u64)] = &[
    (0x40000d00, 32, 0x411fd134),
    (0x40000ff0, 32, 0x0d),
    (0x40000ff4, 32, 0x90),
    (0x40000ff8, 32, 5),
    (0x40000ffc, 32, 0xb1),
    (0x40000fa8, 32, 0x80000102),
    (0x40000fac, 32, 0),
    (0x40000d28, 32, 0),
    (0x40000d2c, 32, 0x10707100),
    (0x30000008, 32, 0x0101443b),
    (0x3000fff0, 32, 0x0d),
    (0x3000fff4, 32, 0xf0),
    (0x3000fff8, 32, 5),
    (0x3000fffc, 32, 0xb1),
    (0x30000004, 32, 0x02480001),
    (0x70000004, 32, 0x0101443b),
    (0x7000fff0, 32, 0x0d),
    (0x7000fff4, 32, 0xf0),
    (0x7000fff8, 32, 5),
    (0x7000fffc, 32, 0xb1),
    (0x70000008, 64, 0x0000000200000210),
];
fn proof_fixture(name: &str) -> (Project, PathBuf, Value) {
    let (mut p, out) = fixture(name);
    p.registers.mmio_probe = true;
    p.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTERS".into(),
        json!(["cpsr", "midr"]).to_string(),
    );
    p.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTER_VALUES".into(),
        json!({"cpsr":"0x1a","midr":"0x411fd134"}).to_string(),
    );
    p.cores = ["logical.77", "logical.9"]
        .into_iter()
        .enumerate()
        .map(|(i, name)| Core {
            name: name.into(),
            endpoint: format!("localhost:{}", 6430 + i),
            ..Default::default()
        })
        .collect();
    p.registers.topology.clusters = [
        ("logical.77".into(), "A".into()),
        ("logical.9".into(), "A".into()),
    ]
    .into();
    for (component, owner, base) in [
        ("debug_external", "core:logical.77", 0x40000000),
        ("gicd", "cluster:A", 0x30000000),
        ("gicr", "core:logical.77", 0x70000000),
    ] {
        p.registers.component_owners.insert(
            component.into(),
            [(owner.into(), route(base, "", true))].into(),
        );
        p.registers
            .components
            .insert(component.into(), route(0xdead0000, "", true));
    }
    p.registers.facts.insert("gicd.interrupts".into(), 992);
    let mut memory = serde_json::Map::new();
    for &(address, bits, raw) in PROOF_WORDS {
        let bytes = raw.to_le_bytes()[..usize::from(bits / 8)]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        memory.insert(format!("0x{address:x}"), json!(bytes));
    }
    memory.insert("0x30000084".into(), json!("01000000"));
    let memories = json!({"localhost:6430":memory,"localhost:6431":{}});
    fs::write(out.join("memory.json"), memories.to_string()).unwrap();
    p.gdb.env.insert(
        "DEBUGTUI_TEST_OWNER_MEMORY_FILE".into(),
        out.join("memory.json").to_string_lossy().into_owned(),
    );
    (p, out, memories)
}
fn probe(e: &EngineHandle, id: u64) -> Value {
    let list = call(e, id, "registers_list", json!({}));
    call(
        e,
        id + 1,
        "registers_probe",
        json!({"context":list["context"]}),
    )
}
#[test]
fn fresh_probe_gates_data_and_keeps_exact_shared_evidence_after_an_ordinary_read() {
    let (p, out, _) = proof_fixture("fresh GDB");
    let e = coordinator::spawn(p);
    call(&e, 1, "connect", json!({}));
    call(&e, 90, "select_core", json!({"index":0}));
    call(&e, 2, "control_scope", json!({"scope":"all"}));
    let r = call(
        &e,
        3,
        "registers_read",
        json!({"ids":["ed_midr"],"manual":true}),
    );
    assert_eq!(r["samples"][0]["reason"], "reader_unsupported");
    assert!(memory_reads(&out).is_empty());
    let r = probe(&e, 4);
    assert_eq!(r["facts"]["gicd.interrupts"], 64);
    assert_eq!(r["facts"]["gicr.target_id"], 2);
    assert_eq!(r["facts"]["debug_external.breakpoints"], 8);
    assert_eq!(memory_reads(&out).len(), 42);
    for &(address, bits, _) in PROOF_WORDS {
        assert_eq!(
            memory_reads(&out)
                .iter()
                .filter(|s| **s == format!("-data-read-memory-bytes 0x{address:x} {}", bits / 8))
                .count(),
            2
        );
    }
    let samples = r["probe"]["samples"].as_array().unwrap();
    assert_eq!(
        samples
            .iter()
            .filter(|s| s["source"] == "mmio:gicd"
                && s["owner"] == "cluster:A"
                && !s["owner_generation"].is_null())
            .count(),
        12
    );
    let r = call(
        &e,
        6,
        "registers_read",
        json!({"ids":["gicd_igroupr1","gicd_igroupr2","edprsr","editr"],"manual":false}),
    );
    assert_eq!(r["samples"][0]["state"], "valid");
    assert_eq!(r["samples"][1]["implementation"], "no");
    assert!(r["samples"][2]["provenance"]["access"].is_null());
    assert!(r["samples"][3]["provenance"]["access"].is_null());
    let basis = &r["samples"][0]["eligibility"]["probe"]["observations"];
    assert!(
        basis
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == "mmio_probe.gicd_typer.after" && s["owner"] == "cluster:A")
    );
    call(&e, 7, "registers_read", json!({"ids":["gicd_typer"]}));
    let list = call(&e, 8, "registers_list", json!({}));
    assert_eq!(list["probe"]["facts"]["gicd.interrupts"]["value"], 64);
    call(
        &e,
        9,
        "registers_read",
        json!({"ids":["gicd_igroupr2"],"manual":true}),
    );
    assert_eq!(memory_reads(&out).len(), 44);
    call(&e, 10, "select_core", json!({"index":1}));
    let other = probe(&e, 11);
    assert!(other["probe"]["facts"]["gicd.present"].is_null());
    assert_eq!(memory_reads(&out).len(), 44);
    call(&e, 13, "quit", json!({}));
    let text = fs::read_to_string(out.join("commands.txt")).unwrap();
    assert!(
        !text.contains("dead0000")
            && !text.contains("-exec-continue")
            && !text.contains("-data-write")
    );
}
#[test]
fn fresh_probe_rejects_early_identity_mismatch_and_failed_refresh_without_data_io() {
    for (address, bytes, expected_reads) in [
        ("0x40000d00", "64d11f41", 1),
        ("0x40000ff4", "f0000000", 3),
        ("0x30000008", "3b040001", 31),
    ] {
        let (p, out, mut memory) = proof_fixture("identity reject");
        memory["localhost:6430"][address] = json!(bytes);
        fs::write(out.join("memory.json"), memory.to_string()).unwrap();
        let e = coordinator::spawn(p);
        call(&e, 1, "connect", json!({}));
        call(&e, 90, "select_core", json!({"index":0}));
        let r = probe(&e, 2);
        assert!(r["probe"]["facts"]["gicd.present"].is_null());
        assert_eq!(memory_reads(&out).len(), expected_reads);
        let r = call(
            &e,
            4,
            "registers_read",
            json!({"ids":["gicd_typer"],"manual":true}),
        );
        assert_eq!(r["samples"][0]["reason"], "reader_unsupported");
        assert_eq!(memory_reads(&out).len(), expected_reads);
        call(&e, 5, "quit", json!({}));
    }
    let (p, out, mut memory) = proof_fixture("failed refresh");
    let e = coordinator::spawn(p);
    call(&e, 1, "connect", json!({}));
    call(&e, 90, "select_core", json!({"index":0}));
    probe(&e, 2);
    let old = call(&e, 4, "registers_read", json!({"ids":["ed_midr"]}));
    assert_eq!(old["samples"][0]["state"], "valid");
    memory["localhost:6430"]
        .as_object_mut()
        .unwrap()
        .remove("0x40000d00");
    fs::write(out.join("memory.json"), memory.to_string()).unwrap();
    probe(&e, 5);
    let r = call(
        &e,
        7,
        "registers_read",
        json!({"ids":["ed_midr"],"manual":true}),
    );
    assert_eq!(r["samples"][0]["reason"], "reader_unsupported");
    assert!(r["samples"][0]["value"].is_null());
    let status = call(&e, 70, "status", json!({}));
    let retained = status["register_samples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "ed_midr")
        .unwrap();
    assert_eq!(retained["value"], old["samples"][0]["value"]);
    assert_eq!(
        retained["last_value_provenance"]["provenance"],
        old["samples"][0]["provenance"]
    );
    assert_eq!(memory_reads(&out).len(), 44);
    call(&e, 8, "quit", json!({}));
}
#[test]
fn fresh_probe_uses_actual_ap_target_and_two_word_big_endian_route_without_gdb_fallback() {
    let (mut p, out, _) = proof_fixture("fresh AP");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap().to_string();
    for bindings in p.registers.component_owners.values_mut() {
        for binding in bindings.values_mut() {
            binding.channel = "mmio_probe_ap".into();
            binding.little_endian = false;
        }
    }
    p.memory_access_source = "fresh-probe-test".into();
    p.memory_access = vec![MemoryAccess {
        id: "mmio_probe_ap".into(),
        target: "ap.actual".into(),
        tcl_endpoint: endpoint.clone(),
        cores: vec!["logical.77".into()],
        ..Default::default()
    }];
    let packets = Arc::new(Mutex::new(vec![]));
    let saved = packets.clone();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        for _ in 0..43 {
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
            assert!(text.contains("ap.actual"));
            let &(address, bits, raw) = PROOF_WORDS
                .iter()
                .find(|(address, _, _)| text.contains(&format!("read_memory 0x{address:x} ")))
                .unwrap();
            assert!(text.contains(&format!("read_memory 0x{address:x} 32 {}", bits / 32)));
            saved.lock().unwrap().push(text);
            let payload = if bits == 64 {
                format!("0:0x{:x} 0x{:x}", raw >> 32, raw & 0xffffffff)
            } else {
                format!("0:0x{raw:x}")
            };
            stream
                .write_all(format!("{payload}\x1a").as_bytes())
                .unwrap();
        }
    });
    let e = coordinator::spawn(p);
    call(&e, 1, "connect", json!({}));
    call(&e, 90, "select_core", json!({"index":0}));
    let r = probe(&e, 2);
    assert_eq!(r["facts"]["gicd.interrupts"], 64);
    assert_eq!(r["facts"]["gicr.target_id"], 2);
    let s = r["probe"]["samples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "mmio_probe.gicr_typer")
        .unwrap();
    let a = &s["provenance"]["access"]["route"];
    assert_eq!(s["value"]["hex"], "0x0000000200000210");
    assert_eq!(a["kind"], "tcl_memory");
    assert_eq!(a["target"], "ap.actual");
    assert_eq!(a["endpoint"], endpoint);
    assert_eq!(a["configuration_source"], "fresh-probe-test");
    assert_eq!(a["byte_order"], "big");
    assert_eq!(a["count"], 2);
    assert_eq!(a["atomic"], false);
    let data = call(&e, 4, "registers_read", json!({"ids":["gicr_typer"]}));
    assert_eq!(data["samples"][0]["state"], "valid");
    assert!(memory_reads(&out).is_empty());
    server.join().unwrap();
    assert_eq!(packets.lock().unwrap().len(), 43);
    call(&e, 5, "quit", json!({}));
}
#[test]
fn deferred_fresh_probe_driver_compares_independent_ram_and_rejects_unready_or_changed_baseline() {
    use std::process::Command;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let driver = root.join("scripts/test-register-mmio-probe-hardware.cjs");
    let default = Command::new("node").arg(&driver).output().unwrap();
    assert!(default.status.success());
    assert!(
        String::from_utf8_lossy(&default.stdout)
            .contains("\"passed\":0,\"failed\":0,\"skipped\":5")
    );
    for scenario in ["positive", "not_ready", "baseline_changed"] {
        let (mut p, out, _) = proof_fixture(scenario);
        let mut spec: Value = serde_json::from_str(include_str!(
            "../fixtures/register-mmio-probe-board.example.json"
        ))
        .unwrap();
        spec["frame_function"] = json!("main");
        spec["components"]["debug_external"]["base"] = json!("0x40000000");
        spec["components"]["gicr"]["base"] = json!("0x70000000");
        for (component, owner) in [
            ("debug_external", "core:logical.77"),
            ("gicr", "core:logical.77"),
            ("gicd", "cluster:A"),
        ] {
            spec["components"][component]["owner"] = json!(owner);
            spec["components"][component]["route"]["endpoint"] = json!("localhost:6430");
        }
        let mut expressions = json!({});
        for w in spec["words"].as_array().unwrap() {
            expressions[format!("(unsigned long long){}", w["reference"].as_str().unwrap())] =
                w["expected"].clone();
        }
        expressions["(unsigned long long)debugtui_mmio_probe_reference[0].ready"] =
            json!(if scenario == "not_ready" { "0" } else { "1" });
        if scenario == "baseline_changed" {
            expressions["(unsigned long long)debugtui_mmio_probe_reference[0].raw[20]"] =
                json!("0x0000000300000210");
        }
        p.gdb.env.insert(
            "DEBUGTUI_TEST_EXPRESSION_VALUES".into(),
            expressions.to_string(),
        );
        let project = out.join("project.toml");
        fs::write(&project, toml::to_string(&p).unwrap()).unwrap();
        let unchanged = fs::read(&project).unwrap();
        let case = out.join("case.json");
        fs::write(&case, spec.to_string()).unwrap();
        let output = Command::new("node")
            .arg(&driver)
            .args([
                "--run",
                "--software-fixture",
                "--core",
                "logical.77",
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
            if scenario == "positive" {
                5
            } else if scenario == "not_ready" {
                1
            } else {
                2
            }
        );
        assert_eq!(
            memory_reads(&out).len(),
            if scenario == "not_ready" { 0 } else { 42 }
        );
        assert_eq!(fs::read(&project).unwrap(), unchanged);
        let commands = fs::read_to_string(out.join("commands.txt")).unwrap();
        assert!(
            !commands.contains("-exec-continue")
                && !commands.contains("-data-write")
                && !commands.contains("debugtui_mmio_probe_capture")
        );
    }
}
#[test]
fn fresh_probe_discards_shared_capacities_when_a_peer_runs_during_or_after_proof() {
    for during in [false, true] {
        let (mut p, out, _) = proof_fixture("shared epoch");
        let notice = out.join("notice.json");
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
        call(&e, 90, "select_core", json!({"index":0}));
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
                        fs::write(notice, json!({"localhost:6431":"running"}).to_string()).unwrap();
                        break;
                    }
                    assert!(Instant::now() < end);
                    thread::sleep(Duration::from_millis(10));
                }
            }))
        } else {
            None
        };
        let r = probe(&e, 2);
        if let Some(watcher) = watcher {
            watcher.join().unwrap();
            assert!(r["probe"]["facts"]["gicd.present"].is_null());
        } else {
            assert_eq!(r["probe"]["facts"]["gicd.present"]["value"], 1);
            fs::write(&notice, json!({"localhost:6431":"running"}).to_string()).unwrap();
        }
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            let status = call(&e, 100, "status", json!({}));
            if status["cores"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["name"] == "logical.9" && c["state"] == "RUNNING")
            {
                break;
            }
            assert!(Instant::now() < end);
            thread::sleep(Duration::from_millis(20));
        }
        let list = call(&e, 4, "registers_list", json!({}));
        assert!(list["probe"]["facts"]["gicd.present"].is_null());
        assert_eq!(list["probe"]["facts"]["gicr.present"]["value"], 1);
        let r = call(
            &e,
            5,
            "registers_read",
            json!({"ids":["gicd_typer","ed_midr"],"manual":true}),
        );
        assert_eq!(r["samples"][0]["reason"], "reader_unsupported");
        assert_eq!(r["samples"][1]["state"], "valid");
        assert_eq!(memory_reads(&out).len(), 43);
        // All shared/private proof assertions are complete. Remove the fixture's
        // persistent RUNNING injection so ordinary quit can interrupt the peer
        // without the mock immediately forcing it back to RUNNING before frame reads.
        fs::write(&notice, "{}").unwrap();
        call(&e, 6, "quit", json!({}));
    }
}
