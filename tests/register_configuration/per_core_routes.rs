//! Real coordinator/MI/Tcl dispatch; the strict local servers do not model ARM execution.
use super::test_artifacts;
use debugtui::{
    config::{Core, Project},
    coordinator,
    registers::CoreConfig,
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
    let deadline = Instant::now() + Duration::from_secs(15);
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

#[test]
fn per_core_register_routes_reach_distinct_actual_tcp_targets_and_receipts() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = test_artifacts::root().join(format!(
        "per-core-routes-{}-{:x}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&out).unwrap();
    let transcript = out.join("commands.txt");
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
    let mut routes = Vec::new();
    for (index, model, target, raw) in [
        (0, "cortex-m4", "soc.m4", "0x11223344"),
        (2, "cortex-m7", "soc.m7", "0xaabbccdd"),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap().to_string();
        let target_name = target.to_owned();
        servers.push(thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut packet = Vec::new();
            loop {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                if byte[0] == 0x1a {
                    break;
                }
                packet.push(byte[0]);
            }
            let text = String::from_utf8(packet).unwrap();
            assert!(
                text.contains(&format!(r#"targets \"{target_name}\""#)),
                "{text}"
            );
            assert!(
                text.contains(r#"get_reg -force \[list \"backend_probe\"\]"#),
                "{text}"
            );
            stream
                .write_all(format!("__DEBUGTUI_RPC__0:{raw}\x1a").as_bytes())
                .unwrap();
            text
        }));
        let file = out.join(format!("core{index}.toml"));
        fs::write(&file, format!("version=1\ncpu='{model}'\narchitecture='armv7e-m'\n[[groups]]\nid='core'\nname='Core'\n[[registers]]\nid='backend_probe'\nname='Backend probe'\ngroup='core'\nbits=32\naccess='ro'\nreader={{kind='backend',name='backend_probe'}}\n")).unwrap();
        let name = format!("core.{index}");
        routes.push((endpoint.clone(), target.to_owned()));
        project.cores.push(Core {
            name: name.clone(),
            endpoint: format!("localhost:{}", 4800 + index),
            registers: Some(CoreConfig {
                cpu: Some(model.into()),
                catalogue: Some(file),
                tcl_endpoint: Some(endpoint),
                targets: Some(BTreeMap::from([(name, target.into())])),
                ..Default::default()
            }),
            ..Default::default()
        });
    }
    project.validate().unwrap();
    let engine = coordinator::spawn(project);
    call(&engine, 1, "connect", json!({}));
    let mut evidence = Vec::new();
    for (index, expected, name) in [(0, "0x11223344", "core.0"), (1, "0xaabbccdd", "core.2")] {
        call(
            &engine,
            2 + index * 3,
            "select_core",
            json!({"index":index}),
        );
        let listed = call(&engine, 3 + index * 3, "registers_list", json!({}));
        let read = call(
            &engine,
            4 + index * 3,
            "registers_read",
            json!({"ids":["backend_probe"],"manual":true}),
        );
        let sample = &read["samples"][0];
        assert_eq!(sample["value"]["hex"], expected);
        assert_eq!(sample["context"]["core"], name);
        assert_eq!(sample["owner"], format!("core:{name}"));
        let route = &sample["provenance"]["access"]["route"];
        assert_eq!(route["endpoint"], routes[index as usize].0);
        assert_eq!(route["target"], routes[index as usize].1);
        evidence.push(json!({"list":listed,"sample":sample}));
    }
    call(&engine, 20, "quit", json!({}));
    let packets: Vec<_> = servers
        .into_iter()
        .map(|server| server.join().unwrap())
        .collect();
    fs::write(
        out.join("evidence.json"),
        serde_json::to_vec_pretty(
            &json!({"board_tests_executed":false,"samples":evidence,"packets":packets}),
        )
        .unwrap(),
    )
    .unwrap();
}
