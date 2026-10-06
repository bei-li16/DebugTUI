//! Actual Coordinator/MI/TCP dispatch; fixture responses do not emulate ARM execution.
use debugtui::{
    config::{Core, MemoryAccess, Project},
    coordinator,
    registers::{Component, CoreConfig},
    session::{EngineHandle, Event, Request},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn call(engine: &EngineHandle, id: u64, method: &str, params: Value) -> Value {
    engine.send(Request::new(id, method, params)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Event::Response {
            id: found,
            ok,
            result,
            error,
        } = engine
            .events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap()
            && found == id
        {
            assert!(ok, "{method}: {error:?}");
            return result;
        }
    }
}
fn description(cpu: &str) -> String {
    format!(
        r#"
version=1
cpu='{cpu}'
architecture='armv7m'
[[groups]]
id='system'
name='System'
[[registers]]
id='mpu_type'
name='MPU TYPE'
group='system'
bits=32
access='ro'
reader={{kind='core_private',address=0xe000ed90}}
fields=[{{name='DREGION',segments=[{{offset=8,width=8}}]}}]
[[registers]]
id='demcr'
name='DEMCR'
group='system'
bits=32
access='rw'
reader={{kind='core_private',address=0xe000edfc}}
fields=[{{name='TRCENA',segments=[{{offset=24,width=1}}]}}]
[[registers]]
id='mpu_ctrl'
name='MPU CTRL'
group='system'
bits=32
access='rw'
reader={{kind='core_private',address=0xe000ed94}}
present_if={{reg='mpu_type',field='DREGION',op='ge',value=1}}
[[registers]]
id='dwt_ctrl'
name='DWT CTRL'
group='system'
bits=32
access='rw'
reader={{kind='core_private',address=0xe0001000}}
access_rule={{need_enable={{reg='demcr',field='TRCENA',op='eq',value=1}}}}
[[registers]]
id='alias'
name='DWT low half'
group='system'
bits=16
access='ro'
reader={{kind='alias',source='dwt_ctrl',offset=0}}
[[registers]]
id='dhcsr'
name='DHCSR'
group='system'
bits=32
access='rw'
reader={{kind='core_private',address=0xe000edf0}}
read_side_effect=true
[[registers]]
id='unknown'
name='Unresolved owner'
group='system'
bits=32
access='ro'
scope='unknown'
reader={{kind='gdb',name='r0'}}
[[registers]]
id='privileged'
name='Unproven current Debug EL'
group='system'
bits=32
access='ro'
reader={{kind='backend',name='fixture_privileged'}}
access_rule={{min_el=2}}
"#
    )
}

#[test]
fn actual_multicore_ppb_field_gates_aliases_and_unknown_owner_are_isolated() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let artifact_root = std::env::var_os("DEBUGTUI_TEST_ARTIFACT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("artifacts"));
    let out = artifact_root.join(format!(
        "register-policy-{}-{:x}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&out).unwrap();
    let transcript = out.join("mi.txt");
    fs::write(&transcript, "").unwrap();
    let mut project = Project::default();
    project.gdb.executable = "node".into();
    project.gdb.args = vec![
        root.join("tests/mock-gdb.cjs")
            .to_string_lossy()
            .into_owned(),
    ];
    project
        .gdb
        .env
        .insert("DEBUGTUI_TEST_REGISTERS".into(), "[]".into());
    project.gdb.env.insert(
        "DEBUGTUI_TEST_TRANSCRIPT".into(),
        transcript.to_string_lossy().into_owned(),
    );
    project.session.on_exit = "disconnect".into();
    let mut servers = Vec::new();
    let mut expected_routes = Vec::new();
    for (index, cpu, target, reads) in [
        (
            0,
            "cortex-m4",
            "soc.m4",
            vec![
                (0xe000ed90u64, 0u32),
                (0xe000edfc, 0),
                (0xe000edf0, 0x01010001),
            ],
        ),
        (
            2,
            "cortex-m7",
            "soc.m7",
            vec![
                (0xe000ed90u64, 0x800u32),
                (0xe000edfc, 0x01000000),
                (0xe000ed94, 0x5),
                (0xe0001000, 0x41234567),
                (0xe000edf0, 0x02020001),
            ],
        ),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap().to_string();
        let target_name = target.to_string();
        servers.push(thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(15)))
                .unwrap();
            let mut packets = Vec::new();
            for (address, value) in reads {
                let mut packet = Vec::new();
                let mut byte = [0];
                loop {
                    stream.read_exact(&mut byte).unwrap();
                    if byte[0] == 0x1a {
                        break;
                    }
                    packet.push(byte[0]);
                }
                let command = String::from_utf8(packet).unwrap();
                assert!(
                    command.contains(&format!(
                        r#"\"{target_name}\" read_memory 0x{address:x} 32 1"#
                    )),
                    "{command}"
                );
                for forbidden in [
                    "targets ",
                    "halt",
                    "resume",
                    "write_memory",
                    "mcr",
                    "fixture_privileged",
                ] {
                    assert!(!command.contains(forbidden), "{command}");
                }
                packets.push(command);
                stream
                    .write_all(format!("__DEBUGTUI_RPC__0:0x{value:08x}\x1a").as_bytes())
                    .unwrap();
            }
            // Connection must close on quit; any extra read proves a gate/alias bug.
            let mut byte = [0];
            assert_eq!(
                stream.read(&mut byte).unwrap(),
                0,
                "unexpected extra PPB request"
            );
            packets
        }));
        let name = format!("core.{index}");
        let channel = format!("ppb{index}");
        let catalogue = out.join(format!("{cpu}.toml"));
        fs::write(&catalogue, description(cpu)).unwrap();
        project.memory_access.push(MemoryAccess {
            id: channel.clone(),
            label: cpu.into(),
            tcl_endpoint: endpoint.clone(),
            target: target.into(),
            cores: vec![name.clone()],
            while_running: false,
        });
        let binding = Component {
            base: 0,
            channel: channel.clone(),
            little_endian: true,
        };
        project.cores.push(Core {
            name: name.clone(),
            endpoint: format!("localhost:{}", 4700 + index),
            registers: Some(CoreConfig {
                cpu: Some(cpu.into()),
                catalogue: Some(catalogue),
                targets: Some(BTreeMap::from([(name.clone(), target.into())])),
                // Saved mode and declared EL must not authorize the privileged reader.
                facts: Some(BTreeMap::from([
                    ("cpu.mode".into(), 0x1a),
                    ("cpu.debug_el".into(), 2),
                ])),
                component_owners: Some(BTreeMap::from([(
                    "ppb".into(),
                    BTreeMap::from([(format!("core:{name}"), binding)]),
                )])),
                ..Default::default()
            }),
            ..Default::default()
        });
        expected_routes.push((name, endpoint, target.to_string(), channel));
    }
    project.validate().unwrap();
    let engine = coordinator::spawn(project);
    call(&engine, 1, "connect", json!({}));
    let mut reports = Vec::new();
    let mut request_id = 10;
    for (selection, (core, endpoint, target, channel)) in expected_routes.iter().enumerate() {
        call(
            &engine,
            request_id,
            "select_core",
            json!({"index":selection}),
        );
        request_id += 1;
        let denied = call(
            &engine,
            request_id,
            "registers_read",
            json!({"ids":["mpu_ctrl","dwt_ctrl","alias","unknown","privileged"],"manual":true}),
        );
        request_id += 1;
        for sample in denied["samples"].as_array().unwrap() {
            assert!(sample["value"].is_null());
            assert!(sample["provenance"]["access"].is_null(), "{sample}");
        }
        assert!(denied["samples"][3]["owner"].is_null());
        assert!(
            denied["samples"][4]["detail"]
                .as_str()
                .unwrap()
                .contains("current Debug EL is unproven")
        );
        let identity = call(
            &engine,
            request_id,
            "registers_read",
            json!({"ids":["mpu_type","demcr"],"manual":true}),
        );
        request_id += 1;
        for sample in identity["samples"].as_array().unwrap() {
            assert_eq!(sample["state"], "valid");
            assert_eq!(sample["owner"], format!("core:{core}"));
            let route = &sample["provenance"]["access"]["route"];
            assert_eq!(route["endpoint"], *endpoint);
            assert_eq!(route["target"], *target);
            assert_eq!(route["channel"], *channel);
        }
        let data = call(
            &engine,
            request_id,
            "registers_read",
            json!({"ids":["mpu_ctrl","dwt_ctrl","alias"],"manual":true}),
        );
        request_id += 1;
        if selection == 0 {
            assert_eq!(data["samples"][0]["reason"], "hardware_not_implemented");
            for sample in &data["samples"].as_array().unwrap()[1..] {
                assert_eq!(sample["reason"], "feature_disabled");
                assert!(sample["provenance"]["access"].is_null());
            }
        } else {
            for sample in data["samples"].as_array().unwrap() {
                assert_eq!(sample["state"], "valid", "{sample}");
            }
            assert_eq!(data["samples"][1]["value"]["hex"], "0x41234567");
            assert_eq!(data["samples"][2]["value"]["hex"], "0x4567");
            assert_eq!(
                data["samples"][2]["provenance"]["aliases"][0]["source"],
                "dwt_ctrl"
            );
        }
        let no_poll = call(
            &engine,
            request_id,
            "registers_read",
            json!({"ids":["dhcsr"],"manual":false}),
        );
        request_id += 1;
        assert_eq!(no_poll["samples"][0]["state"], "not_read");
        assert!(no_poll["samples"][0]["provenance"]["access"].is_null());
        let manual = call(
            &engine,
            request_id,
            "registers_read",
            json!({"ids":["dhcsr"],"manual":true}),
        );
        request_id += 1;
        assert_eq!(manual["samples"][0]["state"], "valid");
        reports.push(json!({"denied":denied,"identity":identity,"data":data,"no_poll":no_poll,"manual":manual}));
    }
    call(&engine, request_id, "quit", json!({}));
    let packets: Vec<_> = servers.into_iter().map(|s| s.join().unwrap()).collect();
    let mi = fs::read_to_string(transcript).unwrap();
    assert!(
        !mi.contains("-data-list-register-values"),
        "unknown owner leaked GDB read"
    );
    fs::write(
        out.join("evidence.json"),
        serde_json::to_vec_pretty(
            &json!({"board_tests_executed":false,"reports":reports,"packets":packets}),
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn actual_core_private_gdb_memory_reuses_the_known_worker_connection() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let artifacts = std::env::var_os("DEBUGTUI_TEST_ARTIFACT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("artifacts"));
    let out = artifacts.join(format!(
        "register-policy-gdb-{}-{:x}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&out).unwrap();
    let transcript = out.join("mi.txt");
    fs::write(&transcript, "").unwrap();
    let catalogue = out.join("registers.toml");
    fs::write(&catalogue, description("cortex-m4")).unwrap();
    let mut p = Project::default();
    p.target.endpoint = "localhost:4777".into();
    p.registers.catalogue = catalogue;
    p.registers.component_owners.insert(
        "ppb".into(),
        BTreeMap::from([(
            "core:default".into(),
            Component {
                base: 0,
                channel: String::new(),
                little_endian: true,
            },
        )]),
    );
    p.gdb.executable = "node".into();
    p.gdb.args = vec![
        root.join("tests/mock-gdb.cjs")
            .to_string_lossy()
            .into_owned(),
    ];
    p.gdb
        .env
        .insert("DEBUGTUI_TEST_REGISTERS".into(), "[]".into());
    p.gdb.env.insert(
        "DEBUGTUI_TEST_MEMORY_BLOCKS".into(),
        r#"[{begin="0xe000ed90",contents="00080000"}]"#.into(),
    );
    p.gdb.env.insert(
        "DEBUGTUI_TEST_TRANSCRIPT".into(),
        transcript.to_string_lossy().into_owned(),
    );
    p.session.on_exit = "disconnect".into();
    p.validate().unwrap();
    let engine = coordinator::spawn(p);
    call(&engine, 1, "connect", json!({}));
    let read = call(
        &engine,
        2,
        "registers_read",
        json!({"ids":["mpu_type"],"manual":true}),
    );
    let sample = &read["samples"][0];
    assert_eq!(sample["state"], "valid", "{sample}");
    assert_eq!(sample["value"]["hex"], "0x00000800");
    let route = &sample["provenance"]["access"]["route"];
    assert_eq!(route["kind"], "gdb_memory");
    assert_eq!(route["endpoint"], route["configured_endpoint"]);
    assert_eq!(route["address"], "0xe000ed90");
    let matrix = call(&engine, 3, "registers_matrix", json!({}));
    let row = matrix["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "mpu_type")
        .unwrap();
    assert_eq!(row["plan"]["available"], true);
    call(&engine, 4, "quit", json!({}));
    let mi = fs::read_to_string(transcript).unwrap();
    assert_eq!(mi.matches("-data-read-memory-bytes").count(), 1);
    assert!(!mi.contains("-data-list-register-values"));
    fs::write(
        out.join("evidence.json"),
        serde_json::to_vec_pretty(
            &json!({"board_tests_executed":false,"read":read,"matrix":matrix}),
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn shipped_m_catalogues_read_faults_and_require_manual_single_side_effect_reads() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let artifacts = std::env::var_os("DEBUGTUI_TEST_ARTIFACT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("artifacts"));
    let out = artifacts.join(format!(
        "register-m-catalogues-{}-{:x}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&out).unwrap();
    let memory = out.join("memory.json");
    fs::write(
        &memory,
        r#"{"localhost:4888":{"0xe000ed28":"82820003","0xe000e010":"05000100","0xe000edf0":"03000303"}}"#,
    )
    .unwrap();
    let mut reports = Vec::new();
    for cpu in ["cortex-m3", "cortex-m4", "cortex-m7"] {
        let transcript = out.join(format!("{cpu}.mi.txt"));
        fs::write(&transcript, "").unwrap();
        let mut project = Project::default();
        project.target.endpoint = "localhost:4888".into();
        project.registers.cpu = cpu.into();
        project.registers.component_owners.insert(
            "ppb".into(),
            BTreeMap::from([(
                "core:default".into(),
                Component {
                    base: 0,
                    channel: String::new(),
                    little_endian: true,
                },
            )]),
        );
        project.gdb.executable = "node".into();
        project.gdb.args = vec![
            root.join("tests/mock-gdb.cjs")
                .to_string_lossy()
                .into_owned(),
        ];
        project
            .gdb
            .env
            .insert("DEBUGTUI_TEST_REGISTERS".into(), "[]".into());
        project.gdb.env.insert(
            "DEBUGTUI_TEST_OWNER_MEMORY_FILE".into(),
            memory.to_string_lossy().into_owned(),
        );
        project.gdb.env.insert(
            "DEBUGTUI_TEST_TRANSCRIPT".into(),
            transcript.to_string_lossy().into_owned(),
        );
        project.session.on_exit = "disconnect".into();
        project.validate().unwrap();
        let engine = coordinator::spawn(project);
        let list = call(&engine, 1, "registers_list", json!({}));
        assert_eq!(list["catalogue"]["cpu"], cpu);
        assert_eq!(list["source"], format!("builtin:{cpu}"));
        call(&engine, 2, "connect", json!({}));
        let automatic = call(
            &engine,
            3,
            "registers_read",
            json!({"ids":["systick.ctrl","dcb.dhcsr"],"manual":false}),
        );
        assert!(
            automatic["samples"]
                .as_array()
                .unwrap()
                .iter()
                .all(|s| s["state"] != "valid")
        );
        assert!(
            !fs::read_to_string(&transcript)
                .unwrap()
                .contains("-data-read-memory-bytes")
        );
        let faults = call(
            &engine,
            4,
            "registers_read",
            json!({"ids":["scb.cfsr"],"manual":true}),
        );
        assert_eq!(faults["samples"][0]["state"], "valid", "{faults}");
        assert_eq!(faults["samples"][0]["value"]["hex"], "0x03008282");
        let manual = call(
            &engine,
            5,
            "registers_read",
            json!({"ids":["systick.ctrl","dcb.dhcsr"],"manual":true}),
        );
        assert!(
            manual["samples"]
                .as_array()
                .unwrap()
                .iter()
                .all(|s| s["state"] == "valid"),
            "{manual}"
        );
        assert_eq!(manual["samples"][0]["value"]["hex"], "0x00010005");
        assert_eq!(manual["samples"][1]["value"]["hex"], "0x03030003");
        call(
            &engine,
            6,
            "registers_read",
            json!({"ids":["systick.ctrl","dcb.dhcsr"],"manual":false}),
        );
        call(&engine, 7, "quit", json!({}));
        let mi = fs::read_to_string(&transcript).unwrap();
        assert_eq!(
            mi.matches("-data-read-memory-bytes").count(),
            3,
            "{cpu} {mi}"
        );
        let reads: Vec<Vec<&str>> = mi
            .lines()
            .filter(|line| line.starts_with("-data-read-memory-bytes "))
            .map(|line| {
                line.split_whitespace()
                    .skip(1)
                    .map(|arg| arg.trim_matches('"'))
                    .collect()
            })
            .collect();
        assert_eq!(
            reads,
            vec![
                vec!["0xe000ed28", "4"],
                vec!["0xe000e010", "4"],
                vec!["0xe000edf0", "4"]
            ]
        );
        assert!(
            !mi.contains("-data-write-memory")
                && !mi.contains("-data-list-register-values")
                && !mi.contains("-exec-continue")
        );
        reports.push(json!({"cpu":cpu,"automatic":automatic,"faults":faults,"manual":manual}));
    }
    fs::write(
        out.join("evidence.json"),
        serde_json::to_vec_pretty(&json!({"board_tests_executed":false,"reports":reports}))
            .unwrap(),
    )
    .unwrap();
}

#[test]
fn actual_executable_loads_structured_policy_and_rejects_bad_schema_before_connect() {
    use std::process::{Command, Stdio};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let artifacts = std::env::var_os("DEBUGTUI_TEST_ARTIFACT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("artifacts"));
    let out = artifacts.join(format!(
        "register-policy-cli-{}-{:x}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(out.join("tools")).unwrap();
    fs::create_dir_all(out.join("config")).unwrap();
    fs::write(out.join("tools/debug-env.toml"), "").unwrap();
    let mut project = Project::default();
    project.tools.root = out.join("tools");
    project.registers.catalogue = out.join("registers.toml");
    fs::write(out.join("debug.toml"), toml::to_string(&project).unwrap()).unwrap();
    let valid = description("cortex-m4");
    for (case, text) in [
        ("valid", valid.clone()),
        (
            "missing-field",
            valid.replace("field='DREGION'", "field='MISSING'"),
        ),
        ("expression", valid.replace("op='ge'", "op='eval'")),
        (
            "wrong-scope",
            valid.replacen("bits=32", "scope='chip'\nbits=32", 1),
        ),
        (
            "outside-ppb",
            valid.replace("address=0xe000ed90", "address=0xe0100000"),
        ),
        (
            "non-memory-runtime",
            valid.replace(
                "access_rule={min_el=2}",
                "access_rule={min_el=2,need_halt=false}",
            ),
        ),
    ] {
        fs::write(&project.registers.catalogue, text).unwrap();
        let mut process = Command::new(env!("CARGO_BIN_EXE_debugtui"))
            .args([
                "--project",
                out.join("debug.toml").to_str().unwrap(),
                "--headless",
                "--stdio",
            ])
            .env("DEBUGTUI_CONFIG_DIR", out.join("config"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        process.stdin.take().unwrap().write_all(b"{\"id\":1,\"method\":\"registers_list\",\"params\":{}}\n{\"id\":2,\"method\":\"registers_matrix\",\"params\":{}}\n{\"id\":3,\"method\":\"quit\",\"params\":{}}\n").ok();
        let result = process.wait_with_output().unwrap();
        let stdout = String::from_utf8(result.stdout).unwrap();
        let stderr = String::from_utf8(result.stderr).unwrap();
        fs::write(out.join(format!("{case}.jsonl")), &stdout).unwrap();
        fs::write(out.join(format!("{case}.stderr")), &stderr).unwrap();
        assert!(
            !stdout.contains("\"channel\":\"mi>\""),
            "{case} started debugger I/O"
        );
        if case == "valid" {
            assert!(result.status.success(), "{stderr}");
            let events: Vec<Value> = stdout
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            let list = &events.iter().find(|e| e["id"] == 1).unwrap()["result"]["catalogue"];
            let ctrl = list["registers"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == "mpu_ctrl")
                .unwrap();
            assert_eq!(ctrl["present_if"]["reg"], "mpu_type");
            let matrix = &events.iter().find(|e| e["id"] == 2).unwrap()["result"];
            let unknown = matrix["rows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == "unknown")
                .unwrap();
            assert_eq!(unknown["support"], "unknown_owner");
        } else {
            assert!(!result.status.success(), "{case} unexpectedly loaded");
            assert!(
                stderr.contains("Register") || stderr.contains("register"),
                "{stderr}"
            );
        }
    }
}
