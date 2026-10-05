#![cfg(windows)]
//! Production catalogues and real workers against independent MI/TCP fixtures.
mod mmio_owners {
    use debugtui::{
        config::{Core, MemoryAccess, Project},
        coordinator,
        registers::Component,
        session::{EngineHandle, Event, Request},
    };
    use serde_json::{Value, json};
    use std::{
        fs,
        io::{Read, Write},
        net::TcpListener,
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
        thread,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    fn fixture(name: &str) -> (Project, PathBuf) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let out = root.join("artifacts").join(format!(
            "mmio owners {name} {} {:x}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&out).unwrap();
        let mut p = Project::default();
        p.gdb.executable = "node".into();
        p.gdb.args = vec![
            root.join("tests/mock-gdb.cjs")
                .to_string_lossy()
                .into_owned(),
        ];
        p.gdb
            .env
            .insert("DEBUGTUI_TEST_REGISTERS".into(), json!(["r0"]).to_string());
        p.gdb.env.insert(
            "DEBUGTUI_TEST_TRANSCRIPT".into(),
            out.join("commands.txt").to_string_lossy().into_owned(),
        );
        p.registers.cpu = "cortex-r52".into();
        p.registers.facts = [
            ("gicd.present".into(), 1),
            ("gicd.interrupts".into(), 64),
            ("gicr.present".into(), 1),
            ("debug_external.present".into(), 1),
        ]
        .into();
        p.session.on_exit = "disconnect".into();
        (p, out)
    }
    fn call(e: &EngineHandle, id: u64, method: &str, params: Value) -> Value {
        e.send(Request::new(id, method, params)).unwrap();
        let end = Instant::now() + Duration::from_secs(30);
        loop {
            if let Event::Response {
                id: found,
                ok,
                result,
                error,
            } = e
                .events
                .recv_timeout(end.saturating_duration_since(Instant::now()))
                .unwrap()
                && found == id
            {
                assert!(ok, "{method}: {error:?}");
                return result;
            }
        }
    }
    fn memory_reads(out: &Path) -> Vec<String> {
        fs::read_to_string(out.join("commands.txt"))
            .unwrap()
            .lines()
            .filter(|s| s.starts_with("-data-read-memory-bytes "))
            .map(str::to_owned)
            .collect()
    }
    fn route(base: u64, channel: &str, little_endian: bool) -> Component {
        Component {
            base,
            channel: channel.into(),
            little_endian,
        }
    }

    #[test]
    fn deferred_mmio_driver_compares_independent_ram_and_rejects_unready_identity_owner_or_value_mismatch()
     {
        use std::process::Command;
        // Deliberately separate from the production generator: exact addresses
        // from Tables 10-4/35/36/12-5, with wide high/low bytes in MI order.
        let addresses = [
            0x30000000u64,
            0x30000004,
            0x30000008,
            0x80000000,
            0x80000004,
            0x80000008,
            0x80000014,
            0x80010080,
            0x80010100,
            0x80010180,
            0x80010c00,
            0x80010c04,
            0x80010400,
            0x30006100,
            0x50000d00,
            0x50000fe0,
            0x50000fe4,
            0x50000fe8,
            0x50000fec,
            0x50000fd0,
            0x50000ff0,
            0x50000ff4,
            0x50000ff8,
            0x50000ffc,
        ];
        for scenario in [
            "positive",
            "not_ready",
            "reference_mismatch",
            "identity_mismatch",
            "owner_mismatch",
        ] {
            let (mut p, out) = fixture(scenario);
            p.target.endpoint = "localhost:3330".into();
            p.registers
                .topology
                .clusters
                .insert("default".into(), "A".into());
            for (component, owner, base) in [
                ("gicd", "cluster:A", 0x30000000),
                ("gicr", "core:default", 0x80000000),
                ("debug_external", "core:default", 0x50000000),
            ] {
                p.registers.component_owners.insert(
                    component.into(),
                    [(owner.into(), route(base, "", true))].into(),
                );
            }
            let mut spec: Value =
                serde_json::from_str(include_str!("fixtures/register-mmio-board.example.json"))
                    .unwrap();
            let mut expressions = json!({});
            let mut memory = json!({});
            for (entry, address) in spec["registers"].as_array().unwrap().iter().zip(addresses) {
                let value =
                    u64::from_str_radix(&entry["expected"].as_str().unwrap()[2..], 16).unwrap();
                let bytes = value.to_le_bytes();
                let count = entry["bits"].as_u64().unwrap() as usize / 8;
                let contents = bytes[..count]
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>();
                memory["localhost:3330"][format!("0x{address:x}")] = json!(contents);
                expressions[format!(
                    "(unsigned long long){}",
                    entry["reference"].as_str().unwrap()
                )] = entry["expected"].clone();
            }
            expressions["(unsigned long long)debugtui_mmio_reference[0].ready"] =
                json!(if scenario == "not_ready" { "0" } else { "1" });
            if scenario == "reference_mismatch" {
                expressions["(unsigned long long)debugtui_mmio_reference[0].raw[5]"] =
                    json!("0x0000000300000210");
            }
            if scenario == "identity_mismatch" {
                memory["localhost:3330"]["0x50000d00"] = json!("40d11f41");
            }
            if scenario == "owner_mismatch" {
                spec["components"]["gicr"]["owner"] = json!("core:other");
            }
            p.gdb.env.insert(
                "DEBUGTUI_TEST_EXPRESSION_VALUES".into(),
                expressions.to_string(),
            );
            let memory_file = out.join("memory.json");
            fs::write(&memory_file, serde_json::to_vec_pretty(&memory).unwrap()).unwrap();
            p.gdb.env.insert(
                "DEBUGTUI_TEST_OWNER_MEMORY_FILE".into(),
                memory_file.to_string_lossy().into_owned(),
            );
            let project = out.join("project.toml");
            fs::write(&project, toml::to_string(&p).unwrap()).unwrap();
            let original = fs::read(&project).unwrap();
            spec["frame_function"] = json!("main");
            let case = out.join("case.json");
            fs::write(&case, serde_json::to_vec_pretty(&spec).unwrap()).unwrap();
            let output = Command::new("node")
                .arg(
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("scripts/test-register-mmio-hardware.cjs"),
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
                .arg(&project)
                .arg("--case")
                .arg(&case)
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert_eq!(
                output.status.success(),
                scenario == "positive",
                "{scenario}: {stdout} / {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let report_dir = stdout
                .lines()
                .find(|l| l.starts_with("RESULT "))
                .unwrap()
                .split_whitespace()
                .last()
                .unwrap();
            let report: Value = serde_json::from_slice(
                &fs::read(PathBuf::from(report_dir).join("report.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(report["board_tests_executed"], false);
            assert_eq!(fs::read(&project).unwrap(), original);
            if scenario == "positive" {
                assert_eq!(report["counts"], json!({"passed":5,"failed":0,"skipped":0}));
                assert_eq!(memory_reads(&out).len(), 43);
            } else {
                assert_eq!(report["counts"]["failed"], 1);
                let failure = report["cases"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|e| e["status"] == "failed")
                    .unwrap()["error"]
                    .as_str()
                    .unwrap();
                match scenario {
                    "not_ready" => {
                        assert!(failure.contains("baseline not ready"));
                        assert!(memory_reads(&out).is_empty());
                    }
                    "owner_mismatch" => {
                        assert!(failure.contains("core:other"));
                        assert!(memory_reads(&out).is_empty());
                    }
                    "identity_mismatch" => assert_eq!(memory_reads(&out).len(), 1),
                    "reference_mismatch" => assert!(failure.contains("baseline differs")),
                    _ => unreachable!(),
                }
            }
            let transcript = fs::read_to_string(out.join("commands.txt")).unwrap();
            assert!(!transcript.contains("-exec-continue"));
            assert!(!transcript.contains("-data-write-memory"));
            assert!(!transcript.contains("-data-write-register"));
            assert!(transcript.contains("-gdb-exit"));
        }
    }

    #[test]
    fn four_workers_use_exact_core_and_cluster_mappings_and_missing_owners_never_fall_back() {
        let (mut p, out) = fixture("four workers");
        p.cores = ["core.0", "core.2", "core.5", "core.9"]
            .into_iter()
            .enumerate()
            .map(|(i, name)| Core {
                name: name.into(),
                endpoint: format!("localhost:{}", 6330 + i),
                ..Default::default()
            })
            .collect();
        p.registers.topology.clusters = [
            ("core.0".into(), "A".into()),
            ("core.2".into(), "A".into()),
            ("core.5".into(), "B".into()),
        ]
        .into();
        p.registers.component_owners.insert(
            "debug_external".into(),
            [
                ("core:core.0".into(), route(0x40000000, "", true)),
                ("core:core.2".into(), route(0x50000000, "", true)),
                ("core:core.5".into(), route(0x60000000, "", true)),
            ]
            .into(),
        );
        p.registers.component_owners.insert(
            "gicr".into(),
            [
                ("core:core.0".into(), route(0x70000000, "", true)),
                ("core:core.2".into(), route(0x80000000, "", true)),
                ("core:core.5".into(), route(0x90000000, "", true)),
            ]
            .into(),
        );
        p.registers.component_owners.insert(
            "gicd".into(),
            [
                ("cluster:A".into(), route(0x30000000, "", true)),
                ("cluster:B".into(), route(0x31000000, "", true)),
            ]
            .into(),
        );
        for name in ["gicd", "gicr", "debug_external"] {
            p.registers
                .components
                .insert(name.into(), route(0xdead0000, "", true));
        }
        let memories = json!({
            "localhost:6330":{"0x40000d00":"34d11f41","0x70000008":"0000000000000000","0x70010200":"01000000","0x30000004":"1e004802","0x30006100":"0100000000000000"},
            "localhost:6331":{"0x50000d00":"35d11f41","0x80000008":"0001000001000000","0x80010200":"02000000","0x30000004":"1e004802","0x30006100":"0100000000000000"},
            "localhost:6332":{"0x60000d00":"34d11f41","0x90000008":"1000000000000000","0x90010200":"04000000","0x31000004":"01004802","0x31006100":"0000000000000000"},
            "localhost:6333":{}});
        fs::write(out.join("memory.json"), memories.to_string()).unwrap();
        p.gdb.env.insert(
            "DEBUGTUI_TEST_OWNER_MEMORY_FILE".into(),
            out.join("memory.json").to_string_lossy().into_owned(),
        );
        let e = coordinator::spawn(p);
        call(&e, 1, "connect", json!({}));
        call(&e, 2, "control_scope", json!({"scope":"all"}));
        let mut contexts = vec![];
        for (core, bases, expected) in [
            (
                0,
                [
                    0x40000d00u64,
                    0x70000008,
                    0x70010200,
                    0x30000004,
                    0x30006100,
                ],
                [
                    "0x411fd134",
                    "0x0000000000000000",
                    "0x00000001",
                    "0x0248001e",
                    "0x0000000000000001",
                ],
            ),
            (
                1,
                [0x50000d00, 0x80000008, 0x80010200, 0x30000004, 0x30006100],
                [
                    "0x411fd135",
                    "0x0000000100000100",
                    "0x00000002",
                    "0x0248001e",
                    "0x0000000000000001",
                ],
            ),
            (
                2,
                [0x60000d00, 0x90000008, 0x90010200, 0x31000004, 0x31006100],
                [
                    "0x411fd134",
                    "0x0000000000000010",
                    "0x00000004",
                    "0x02480001",
                    "0x0000000000000000",
                ],
            ),
        ] {
            call(&e, 10 + core, "select_core", json!({"index":core}));
            let r = call(
                &e,
                20 + core,
                "registers_read",
                json!({"ids":["ed_midr","gicr_typer","gicr_ispendr0","gicd_typer","gicd_irouter32"]}),
            );
            contexts.push(r["context"].clone());
            for (i, s) in r["samples"].as_array().unwrap().iter().enumerate() {
                assert_eq!(s["state"], "valid", "{s}");
                assert_eq!(s["value"]["hex"], expected[i]);
                assert_eq!(
                    s["owner"],
                    if i < 3 {
                        format!("core:{}", ["core.0", "core.2", "core.5"][core as usize])
                    } else {
                        format!("cluster:{}", if core == 2 { "B" } else { "A" })
                    }
                );
                let a = &s["provenance"]["access"];
                assert_eq!(a["route"]["address"], format!("0x{:x}", bases[i]));
                assert_eq!(a["route"]["endpoint"], format!("localhost:{}", 6330 + core));
                assert_eq!(a["context"], r["context"]);
                assert_eq!(
                    s["provenance"]["catalogue_reader"]["require_owner_mapping"],
                    true
                );
                if i >= 3 {
                    assert!(s["owner_generation"].is_number());
                }
            }
        }
        assert_eq!(memory_reads(&out).len(), 15);
        call(&e, 30, "select_core", json!({"index":3}));
        let r = call(
            &e,
            31,
            "registers_read",
            json!({"ids":["ed_midr","gicr_typer","gicd_typer"],"manual":true}),
        );
        assert!(
            r["samples"]
                .as_array()
                .unwrap()
                .iter()
                .all(|s| s["value"].is_null() && s["provenance"]["access"].is_null())
        );
        assert_eq!(r["samples"][0]["reason"], "reader_unsupported");
        assert_eq!(r["samples"][1]["reason"], "reader_unsupported");
        assert!(r["samples"][2]["owner"].is_null());
        assert_eq!(memory_reads(&out).len(), 15);
        e.send(Request::new(
            32,
            "registers_read",
            json!({"context":contexts[1],"ids":["ed_midr"]}),
        ))
        .unwrap();
        loop {
            if let Event::Response {
                id: 32, ok, error, ..
            } = e.events.recv_timeout(Duration::from_secs(15)).unwrap()
            {
                assert!(!ok && error.unwrap().contains("expired"));
                break;
            }
        }
        assert_eq!(memory_reads(&out).len(), 15);
        let text = fs::read_to_string(out.join("commands.txt")).unwrap();
        assert!(
            !text.contains("dead0000")
                && !text.contains("-exec-continue")
                && !text.contains("-exec-interrupt")
        );
        call(&e, 33, "quit", json!({}));
    }

    #[test]
    fn ap_routes_keep_scoped_width_order_and_manual_effects_and_preserve_origin_on_error() {
        let (mut p, out) = fixture("AP");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap().to_string();
        let packets = Arc::new(Mutex::new(vec![]));
        let saved = packets.clone();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(30)))
                .unwrap();
            for (address, count, reply) in [
                ("0x80000008", 2, "0:0x00000110 0x00000001"),
                ("0x30006100", 2, "0:0x01234567 0x89abcdef"),
                ("0x50000088", 1, "0:0x01000200"),
                ("0x50000314", 1, "0:0x00000011"),
                ("0x50000088", 1, "0:0x1 0x2"),
            ] {
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
                assert!(
                    text.contains(&format!("read_memory {address} 32 {count}")),
                    "{text}"
                );
                assert!(text.contains("ap.actual"));
                saved.lock().unwrap().push(text);
                stream.write_all(format!("{reply}\x1a").as_bytes()).unwrap();
            }
        });
        p.cores = vec![Core {
            name: "core.2".into(),
            endpoint: "localhost:3330".into(),
            ..Default::default()
        }];
        p.registers
            .topology
            .clusters
            .insert("core.2".into(), "A".into());
        p.registers.component_owners.insert(
            "debug_external".into(),
            [
                ("core:core.2".into(), route(0x50000000, "ap", true)),
                ("core:core.0".into(), route(0x40000000, "wrong", true)),
            ]
            .into(),
        );
        p.registers.component_owners.insert(
            "gicr".into(),
            [("core:core.2".into(), route(0x80000000, "ap", true))].into(),
        );
        p.registers.component_owners.insert(
            "gicd".into(),
            [("cluster:A".into(), route(0x30000000, "ap", false))].into(),
        );
        p.memory_access = vec![MemoryAccess {
            id: "ap".into(),
            target: "ap.actual".into(),
            tcl_endpoint: endpoint.clone(),
            cores: vec!["core.2".into()],
            ..Default::default()
        }];
        p.memory_access_source = "project".into();
        let e = coordinator::spawn(p);
        call(&e, 1, "connect", json!({}));
        let r = call(
            &e,
            2,
            "registers_read",
            json!({"ids":["gicr_typer","gicd_irouter32","edscr"]}),
        );
        for (s, hex, address, bits, order) in r["samples"]
            .as_array()
            .unwrap()
            .iter()
            .zip([
                ("0x0000000100000110", "0x80000008", 64, "little"),
                ("0x0123456789abcdef", "0x30006100", 64, "big"),
                ("0x01000200", "0x50000088", 32, "little"),
            ])
            .map(|(s, (v, a, b, o))| (s, v, a, b, o))
        {
            assert_eq!(s["state"], "valid", "{s}");
            assert_eq!(s["value"]["hex"], hex);
            let a = &s["provenance"]["access"];
            assert_eq!(a["phase"], "responded");
            assert_eq!(
                a["route"],
                json!({"kind":"tcl_memory","endpoint":endpoint,"target":"ap.actual","channel":"ap","configuration_source":"project","address":address,"bits":bits,"bus_width":32,"count":bits/32,"byte_order":order,"atomic":false})
            );
            assert_eq!(a["context"], r["context"]);
            assert!(a["completed_ms"].as_u64().unwrap() >= a["timestamp_ms"].as_u64().unwrap());
        }
        let skipped = call(
            &e,
            3,
            "registers_read",
            json!({"ids":["edprsr","edpcsr_lo","dbgdtrtx_el0","editr","edlar","gicd_irouter64"]}),
        );
        assert!(
            skipped["samples"]
                .as_array()
                .unwrap()
                .iter()
                .all(|s| s["value"].is_null() && s["provenance"]["access"].is_null())
        );
        assert_eq!(skipped["samples"][3]["reason"], "write_only");
        assert_eq!(skipped["samples"][5]["reason"], "hardware_not_implemented");
        assert_eq!(packets.lock().unwrap().len(), 3);
        let manual = call(
            &e,
            4,
            "registers_read",
            json!({"ids":["edprsr","editr","gicd_irouter64"],"manual":true}),
        );
        assert_eq!(manual["samples"][0]["state"], "valid");
        assert!(manual["samples"][1]["value"].is_null() && manual["samples"][2]["value"].is_null());
        let failed = call(&e, 5, "registers_read", json!({"ids":["edscr"]}));
        assert_eq!(failed["samples"][0]["state"], "error");
        assert!(failed["samples"][0]["value"].is_null());
        let status = call(&e, 6, "status", json!({}));
        let old = status["register_samples"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "edscr")
            .unwrap();
        assert_eq!(old["value"]["hex"], "0x01000200");
        assert_eq!(
            old["last_value_provenance"],
            json!({"status":"known","provenance":r["samples"][2]["provenance"]})
        );
        assert_eq!(status["state"], "STOPPED");
        assert!(
            memory_reads(&out).is_empty(),
            "AP error must not fall back to GDB"
        );
        server.join().unwrap();
        assert_eq!(packets.lock().unwrap().len(), 5);
        call(&e, 7, "quit", json!({}));
    }
}
