//! Production session/Coordinator/MI tests. The fixture supplies bytes, not ARM execution.
#[path = "m_capability_access/m_modules_cases.rs"]
mod m_modules_cases;
use debugtui::{
    config::{Core, Project},
    coordinator,
    registers::{Component, CoreConfig},
    session::{EngineHandle, Event, Request},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn request(engine: &EngineHandle, id: u64, method: &str, params: Value) -> Result<Value, String> {
    engine.send(Request::new(id, method, params)).unwrap();
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        if let Event::Response {
            id: found,
            ok,
            result,
            error,
        } = engine
            .events
            .recv_timeout(end.saturating_duration_since(Instant::now()))
            .unwrap()
            && found == id
        {
            return if ok {
                Ok(result)
            } else {
                Err(error.unwrap_or_default())
            };
        }
    }
}
fn call(engine: &EngineHandle, id: u64, method: &str, params: Value) -> Value {
    request(engine, id, method, params).unwrap()
}
fn fixture(cpu: &str) -> (Project, PathBuf, PathBuf, PathBuf) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = std::env::var_os("DEBUGTUI_TEST_ARTIFACT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("artifacts"))
        .join(format!(
            "m-probe-{}-{unique}-{:x}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir_all(&out).unwrap();
    let transcript = out.join("mi.txt");
    fs::write(&transcript, "").unwrap();
    let memory = out.join("memory.json");
    let mut project = Project::default();
    project.target.endpoint = "localhost:4900".into();
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
    project.gdb.env = BTreeMap::from([
        ("DEBUGTUI_TEST_REGISTERS".into(), "[]".into()),
        (
            "DEBUGTUI_TEST_OWNER_MEMORY_FILE".into(),
            memory.to_string_lossy().into_owned(),
        ),
        (
            "DEBUGTUI_TEST_TRANSCRIPT".into(),
            transcript.to_string_lossy().into_owned(),
        ),
    ]);
    project.session.on_exit = "disconnect".into();
    (project, out, transcript, memory)
}
fn values(cpuid: u32, ictr: u32, mpu: u32, trace: bool) -> Value {
    // Expected architecture addresses are independent of catalogue generation.
    let mut result = serde_json::Map::new();
    for (address, value) in [
        (0xe000ed00u32, cpuid),
        (0xe000e004, ictr),
        (0xe000ed90, mpu << 8),
        (0xe000edfc, u32::from(trace) << 24),
        (0xe0002000, 0x100052a1),
        (0xe0001000, 0x40000000),
        (0xe000ef40, 0x10110021),
        (0xe000ef44, 0x11000011),
        (0xe000ef48, 0),
        (0xe000e100, 0x12345678),
    ] {
        result.insert(
            format!("0x{address:x}"),
            json!(
                value
                    .to_le_bytes()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            ),
        );
    }
    Value::Object(result)
}
fn reads(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|s| s.starts_with("-data-read-memory-bytes"))
        .map(|s| s.replace('"', ""))
        .collect()
}
fn no_mutations(path: &Path) {
    let mi = fs::read_to_string(path).unwrap();
    for forbidden in [
        "-data-write-memory",
        "-exec-continue",
        "-data-list-register-values",
        "0xe000edf0",
        "0xe000e010",
        "mcr ",
        "monitor ",
    ] {
        assert!(!mi.contains(forbidden), "unexpected {forbidden}: {mi}");
    }
}

#[test]
fn shipped_m_models_probe_actual_ids_and_bound_nvic_reads_without_writes() {
    for (cpu, cpuid, ictr, regions, trace) in [
        ("cortex-m3", 0x412fc231, 0, 0, false),
        ("cortex-m4", 0x410fc241, 1, 8, true),
        ("cortex-m7", 0x411fc271, 7, 16, true),
    ] {
        let (mut p, out, mi, memory) = fixture(cpu);
        p.registers.facts = BTreeMap::from([
            ("mpu.regions".into(), 255),
            ("nvic.priority_bits".into(), 5),
        ]);
        fs::write(
            &memory,
            serde_json::to_vec(&json!({"localhost:4900":values(cpuid,ictr,regions,trace)}))
                .unwrap(),
        )
        .unwrap();
        p.validate().unwrap();
        let engine = coordinator::spawn(p);
        call(&engine, 1, "connect", json!({}));
        let denied = call(
            &engine,
            2,
            "registers_read",
            json!({"ids":["nvic.iser0"],"manual":true}),
        );
        assert!(denied["samples"][0]["value"].is_null());
        assert!(reads(&mi).is_empty());
        let list = call(&engine, 3, "registers_list", json!({}));
        let result = call(
            &engine,
            4,
            "registers_probe",
            json!({"context":list["context"]}),
        );
        assert_eq!(result["facts"]["mpu.regions"], regions, "{result}");
        assert_eq!(result["facts"]["nvic.banks"], ictr + 1);
        assert_eq!(result["facts"]["nvic.lines_upper_bound"], (ictr + 1) * 32);
        assert_eq!(result["probe"]["nvic"]["priority_bits"]["value"], 5);
        assert_eq!(
            result["probe"]["nvic"]["priority_bits"]["source"],
            "configuration: registers.facts.nvic.priority_bits"
        );
        assert!(result["probe"]["nvic"]["interrupts"].is_null());
        if trace {
            assert_eq!(result["facts"]["dwt.comparators"], 4);
            assert_eq!(result["facts"]["vfp.d_registers"], 16);
        } else {
            assert!(result["facts"]["dwt.comparators"].is_null());
            assert!(result["facts"]["vfp.present"].is_null());
        }
        assert_eq!(reads(&mi).len(), if trace { 9 } else { 5 });
        for sample in result["probe"]["samples"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["state"] == "valid")
        {
            assert_eq!(sample["owner"], "core:default");
            assert_eq!(sample["context"], list["context"]);
            assert_eq!(sample["provenance"]["access"]["phase"], "responded");
            assert_eq!(
                sample["provenance"]["access"]["route"]["endpoint"],
                "localhost:4900"
            );
        }
        let data = call(
            &engine,
            5,
            "registers_read",
            json!({"ids":["nvic.iser0"],"manual":true}),
        );
        assert_eq!(data["samples"][0]["value"]["hex"], "0x12345678");
        if ictr < 7 {
            let count = reads(&mi).len();
            let outside = call(
                &engine,
                6,
                "registers_read",
                json!({"ids":[format!("nvic.iser{}",ictr+1)],"manual":true}),
            );
            assert_eq!(outside["samples"][0]["implementation"], "no");
            assert!(outside["samples"][0]["value"].is_null());
            assert_eq!(reads(&mi).len(), count);
        }
        let matrix = call(&engine, 7, "registers_matrix", json!({}));
        assert_eq!(matrix["effective_facts"]["mpu.regions"], regions);
        call(&engine, 8, "quit", json!({}));
        no_mutations(&mi);
        fs::write(out.join("evidence.json"),serde_json::to_vec_pretty(&json!({"board_tests_executed":false,"cpu":cpu,"probe":result,"matrix":matrix,"reads":reads(&mi)})).unwrap()).unwrap();
    }
}

#[test]
fn heterogeneous_scope_all_probe_is_owned_and_keeps_svd_priority_source() {
    let (mut p, out, mi, memory) = fixture("cortex-m4");
    fs::write(&memory,serde_json::to_vec(&json!({"localhost:4910":values(0x410fc241,0,8,false),"localhost:4912":values(0x411fc271,2,16,true)})).unwrap()).unwrap();
    let svd = out.join("device.svd");
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/peripherals.svd"),
    )
    .unwrap()
    .replacen(
        "<registers>",
        "<interrupt><name>TEST_IRQ</name><value>31</value></interrupt><registers>",
        1,
    );
    fs::write(&svd, source).unwrap();
    p.program.svd = svd;
    p.cores = [
        ("core0", "cortex-m4", "localhost:4910"),
        ("core2", "cortex-m7", "localhost:4912"),
    ]
    .into_iter()
    .map(|(name, cpu, endpoint)| Core {
        name: name.into(),
        endpoint: endpoint.into(),
        registers: Some(CoreConfig {
            cpu: Some(cpu.into()),
            facts: Some(BTreeMap::from([("nvic.priority_bits".into(), 5)])),
            component_owners: Some(BTreeMap::from([(
                "ppb".into(),
                BTreeMap::from([(
                    format!("core:{name}"),
                    Component {
                        base: 0,
                        channel: String::new(),
                        little_endian: true,
                    },
                )]),
            )])),
            ..Default::default()
        }),
        ..Default::default()
    })
    .collect();
    p.validate().unwrap();
    let engine = coordinator::spawn(p);
    call(&engine, 1, "connect", json!({}));
    call(&engine, 2, "control_scope", json!({"scope":"all"}));
    let mut reports = vec![];
    for (selection, name, endpoint, regions, priority, expected_reads) in [
        (0, "core0", "localhost:4910", 8, 4, 8),
        (1, "core2", "localhost:4912", 16, 5, 9),
    ] {
        call(
            &engine,
            10 + selection * 10,
            "select_core",
            json!({"index":selection}),
        );
        let listed = call(&engine, 11 + selection * 10, "registers_list", json!({}));
        assert!(
            listed["facts"]["mpu.regions"].is_null(),
            "peer capability leaked"
        );
        let before = reads(&mi).len();
        let result = call(
            &engine,
            12 + selection * 10,
            "registers_probe",
            json!({"context":listed["context"]}),
        );
        assert_eq!(
            reads(&mi).len() - before,
            expected_reads,
            "Scope All broadcast or missing probe"
        );
        assert_eq!(result["facts"]["mpu.regions"], regions);
        assert_eq!(result["probe"]["nvic"]["priority_bits"]["value"], priority);
        assert_eq!(result["probe"]["context"]["core"], name);
        for sample in result["probe"]["samples"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["state"] == "valid")
        {
            assert_eq!(sample["owner"], format!("core:{name}"));
            assert_eq!(
                sample["provenance"]["access"]["route"]["endpoint"],
                endpoint
            );
        }
        let source = result["probe"]["nvic"]["priority_bits"]["source"]
            .as_str()
            .unwrap();
        if selection == 0 {
            assert!(source.contains("device.svd"));
            assert!(result["probe"]["nvic"]["interrupts"].is_array());
        } else {
            assert!(source.starts_with("configuration:"));
            assert!(result["probe"]["nvic"]["interrupts"].is_null());
        }
        let matrix = call(&engine, 13 + selection * 10, "registers_matrix", json!({}));
        assert_eq!(
            matrix["probe"]["identity"]["model"],
            if selection == 0 {
                "Cortex-M4"
            } else {
                "Cortex-M7"
            }
        );
        assert_eq!(matrix["effective_facts"]["mpu.regions"], regions);
        reports.push(result);
    }
    call(&engine, 40, "select_core", json!({"index":0}));
    let again = call(&engine, 41, "registers_list", json!({}));
    assert_eq!(again["facts"]["mpu.regions"], 8);
    call(&engine, 42, "quit", json!({}));
    no_mutations(&mi);
    fs::write(
        out.join("evidence.json"),
        serde_json::to_vec_pretty(
            &json!({"board_tests_executed":false,"reports":reports,"reads":reads(&mi)}),
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn unknown_m_id_sends_only_cpuid_and_new_identity_replaces_previous_capacities() {
    for cpuid in [Some(0), Some(0x411fc271), None] {
        let (p, out, mi, memory) = fixture("cortex-m4");
        let mut bytes = values(cpuid.unwrap_or(0), 0, 8, true);
        if cpuid.is_none() {
            bytes.as_object_mut().unwrap().remove("0xe000ed00");
        }
        fs::write(
            &memory,
            serde_json::to_vec(&json!({"localhost:4900":bytes})).unwrap(),
        )
        .unwrap();
        let engine = coordinator::spawn(p);
        call(&engine, 1, "connect", json!({}));
        let listed = call(&engine, 2, "registers_list", json!({}));
        let result = call(
            &engine,
            3,
            "registers_probe",
            json!({"context":listed["context"]}),
        );
        assert!(result["facts"]["mpu.regions"].is_null());
        assert_eq!(reads(&mi), ["-data-read-memory-bytes 0xe000ed00 4"]);
        assert!(result["probe"]["nvic"]["priority_bits"].is_null());
        call(&engine, 4, "quit", json!({}));
        no_mutations(&mi);
        fs::write(
            out.join("evidence.json"),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .unwrap();
    }
    let (p, _out, mi, memory) = fixture("cortex-m4");
    fs::write(
        &memory,
        serde_json::to_vec(&json!({"localhost:4900":values(0x410fc241,0,8,true)})).unwrap(),
    )
    .unwrap();
    let engine = coordinator::spawn(p);
    call(&engine, 1, "connect", json!({}));
    let listed = call(&engine, 2, "registers_list", json!({}));
    call(
        &engine,
        3,
        "registers_probe",
        json!({"context":listed["context"]}),
    );
    fs::write(
        &memory,
        serde_json::to_vec(&json!({"localhost:4900":values(0,0,8,true)})).unwrap(),
    )
    .unwrap();
    call(
        &engine,
        4,
        "registers_read",
        json!({"ids":["scb.cpuid"],"manual":true}),
    );
    let current = call(&engine, 5, "registers_list", json!({}));
    assert!(current["facts"]["mpu.regions"].is_null());
    let before = reads(&mi).len();
    let denied = call(
        &engine,
        6,
        "registers_read",
        json!({"ids":["nvic.iser0"],"manual":true}),
    );
    assert!(denied["samples"][0]["value"].is_null());
    assert_eq!(reads(&mi).len(), before);
    call(&engine, 7, "quit", json!({}));
    no_mutations(&mi);
}

#[test]
fn invalid_or_failed_m_capacity_is_retained_but_cannot_authorize_nvic_instances() {
    for missing_ictr in [false, true] {
        let (p, out, mi, memory) = fixture("cortex-m4");
        let mut bytes = values(0x410fc241, 15, 255, false);
        if missing_ictr {
            bytes.as_object_mut().unwrap().remove("0xe000e004");
        }
        fs::write(
            &memory,
            serde_json::to_vec(&json!({"localhost:4900":bytes})).unwrap(),
        )
        .unwrap();
        let engine = coordinator::spawn(p);
        call(&engine, 1, "connect", json!({}));
        let listed = call(&engine, 2, "registers_list", json!({}));
        let result = call(
            &engine,
            3,
            "registers_probe",
            json!({"context":listed["context"]}),
        );
        assert!(result["facts"]["nvic.banks"].is_null());
        assert!(result["facts"]["mpu.regions"].is_null());
        let sample = result["probe"]["samples"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "scs.ictr")
            .unwrap();
        assert_eq!(
            sample["state"],
            if missing_ictr { "error" } else { "valid" }
        );
        if !missing_ictr {
            assert_eq!(sample["value"]["hex"], "0x0000000f");
        }
        let before = reads(&mi).len();
        let denied = call(
            &engine,
            4,
            "registers_read",
            json!({"ids":["nvic.iser0","mpu.ctrl"],"manual":true}),
        );
        assert!(
            denied["samples"]
                .as_array()
                .unwrap()
                .iter()
                .all(|s| s["value"].is_null())
        );
        assert_eq!(reads(&mi).len(), before);
        call(&engine, 5, "quit", json!({}));
        no_mutations(&mi);
        fs::write(
            out.join("evidence.json"),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn run_notification_during_m_probe_discards_local_evidence_and_stops_next_io() {
    let (mut p, out, mi, memory) = fixture("cortex-m4");
    fs::write(
        &memory,
        serde_json::to_vec(&json!({"localhost:4900":values(0x410fc241,0,8,true)})).unwrap(),
    )
    .unwrap();
    let notify = out.join("notify.json");
    fs::write(&notify, "{}").unwrap();
    p.gdb.env.insert(
        "DEBUGTUI_TEST_NOTIFY_FILE".into(),
        notify.to_string_lossy().into_owned(),
    );
    p.gdb
        .env
        .insert("DEBUGTUI_TEST_OWNER_MEMORY_DELAY_MS".into(), "250".into());
    let engine = coordinator::spawn(p);
    call(&engine, 1, "connect", json!({}));
    let listed = call(&engine, 2, "registers_list", json!({}));
    engine
        .send(Request::new(
            3,
            "registers_probe",
            json!({"context":listed["context"]}),
        ))
        .unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    while reads(&mi).is_empty() {
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(5));
    }
    fs::write(&notify, r#"{"localhost:4900":"running"}"#).unwrap();
    loop {
        if let Event::Response {
            id: 3, ok, error, ..
        } = engine.events.recv_timeout(Duration::from_secs(5)).unwrap()
        {
            assert!(!ok);
            assert!(error.unwrap().contains("Context changed"));
            break;
        }
    }
    assert_eq!(reads(&mi).len(), 1);
    let state = call(&engine, 4, "status", json!({}));
    assert_eq!(state["state"], "RUNNING");
    assert!(state["register_probe"].is_null());
    call(&engine, 5, "quit", json!({}));
    no_mutations(&mi);
}

#[test]
fn m_probe_uses_real_tcl_core_private_route_and_completed_memory_provenance() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    let (mut p, out, mi, memory) = fixture("cortex-m3");
    fs::write(&memory, "{}").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = listener.local_addr().unwrap().to_string();
    let server_endpoint = endpoint.clone();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(e) => panic!("{e}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut packets = vec![];
        for (address, value) in [
            (0xe000ed00u32, 0x412fc231u32),
            (0xe000e004, 0),
            (0xe000ed90, 0x800),
            (0xe000edfc, 1 << 24),
            (0xe0002000, 0x100052a1),
            (0xe0001000, 0x40000000),
            (0xe000e100, 0x12345678),
        ] {
            let mut packet = vec![];
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
                command.contains(&format!(r#"\"test.m3\" read_memory 0x{address:x} 32 1"#)),
                "{command}"
            );
            for forbidden in [
                "targets ",
                "write_memory",
                "mcr ",
                "halt",
                "resume",
                "0xe000edf0",
                "0xe000e010",
            ] {
                assert!(!command.contains(forbidden), "{command}");
            }
            packets.push(command);
            stream
                .write_all(format!("__DEBUGTUI_RPC__0:0x{value:08x}\x1a").as_bytes())
                .unwrap();
        }
        let mut byte = [0];
        assert_eq!(
            stream.read(&mut byte).unwrap(),
            0,
            "unexpected extra probe I/O"
        );
        (server_endpoint, packets)
    });
    p.registers
        .targets
        .insert("default".into(), "test.m3".into());
    p.registers
        .component_owners
        .get_mut("ppb")
        .unwrap()
        .get_mut("core:default")
        .unwrap()
        .channel = "ppb_m3".into();
    p.memory_access = vec![debugtui::config::MemoryAccess {
        id: "ppb_m3".into(),
        label: "M3 private PPB".into(),
        tcl_endpoint: endpoint.clone(),
        target: "test.m3".into(),
        cores: vec!["default".into()],
        while_running: true,
    }];
    p.validate().unwrap();
    let engine = coordinator::spawn(p);
    call(&engine, 1, "connect", json!({}));
    let listed = call(&engine, 2, "registers_list", json!({}));
    let result = call(
        &engine,
        3,
        "registers_probe",
        json!({"context":listed["context"]}),
    );
    assert_eq!(result["facts"]["mpu.regions"], 8);
    assert_eq!(result["facts"]["dwt.comparators"], 4);
    for sample in result["probe"]["samples"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["state"] == "valid")
    {
        let access = &sample["provenance"]["access"];
        assert_eq!(access["phase"], "responded");
        assert_eq!(access["context"], listed["context"]);
        assert_eq!(access["route"]["kind"], "tcl_memory");
        assert_eq!(access["route"]["endpoint"], endpoint);
        assert_eq!(access["route"]["target"], "test.m3");
        assert_eq!(access["route"]["bus_width"], 32);
        assert_eq!(access["route"]["count"], 1);
    }
    let data = call(
        &engine,
        4,
        "registers_read",
        json!({"ids":["nvic.iser0"],"manual":true}),
    );
    assert_eq!(data["samples"][0]["value"]["hex"], "0x12345678");
    call(&engine, 5, "quit", json!({}));
    assert!(reads(&mi).is_empty());
    no_mutations(&mi);
    let packets = server.join().unwrap();
    assert_eq!(packets.1.len(), 7);
    fs::write(
        out.join("evidence.json"),
        serde_json::to_vec_pretty(
            &json!({"board_tests_executed":false,"probe":result,"packets":packets}),
        )
        .unwrap(),
    )
    .unwrap();
}
