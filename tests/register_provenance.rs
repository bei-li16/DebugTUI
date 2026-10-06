#![cfg(windows)]
//! Actual MI pipes and a strict independent TCP bus fixture; no hardware.
#[path = "support/artifacts.rs"]
mod test_artifacts;

use debugtui::{
    config::{MemoryAccess, Project},
    registers::Component,
    session::{self, Event, Request},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn call(engine: &session::EngineHandle, id: u64, method: &str, params: Value) -> Value {
    engine.send(Request::new(id, method, params)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
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
fn mmio_channel_metadata_matches_exact_bus_requests_and_preserves_old_origin_on_failed_refresh() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = test_artifacts::root().join(format!(
        "register provenance {} {:x}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&out).unwrap();
    let catalogue = out.join("catalogue.toml");
    fs::write(
        &catalogue,
        r#"
version=1
cpu="cortex-r52"
architecture="armv8-r-aarch32"
[[groups]]
id="bus"
name="Bus"
[[registers]]
id="bus32"
name="Bus32"
group="bus"
bits=32
access="ro"
reader={kind="mmio",component="little",offset=0}
[[registers]]
id="bus64"
name="Bus64"
group="bus"
bits=64
access="ro"
reader={kind="mmio",component="little",offset=8}
[[registers]]
id="low32"
name="Low32"
group="bus"
bits=32
access="ro"
reader={kind="alias",source="bus64",offset=0}
[[registers]]
id="big64"
name="Big64"
group="bus"
bits=64
access="ro"
reader={kind="mmio",component="big",offset=8}
"#,
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap().to_string();
    let packets = Arc::new(Mutex::new(Vec::new()));
    let observed = packets.clone();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        for reply in [
            "0:0x11223344",
            "0:0x01234567 0x89abcdef",
            "0:0x01234567 0x89abcdef",
            "1:bus access denied",
        ] {
            let mut packet = Vec::new();
            loop {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                if byte[0] == 0x1a {
                    break;
                }
                packet.push(byte[0]);
            }
            observed
                .lock()
                .unwrap()
                .push(String::from_utf8(packet).unwrap());
            stream
                .write_all(format!("__DEBUGTUI_RPC__{reply}\x1a").as_bytes())
                .unwrap();
        }
    });
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
        .insert("DEBUGTUI_TEST_REGISTERS".into(), json!(["r0"]).to_string());
    project.target.endpoint = "localhost:4450".into();
    project.gdb.env.insert(
        "DEBUGTUI_TEST_TRANSCRIPT".into(),
        out.join("commands.txt").to_string_lossy().into_owned(),
    );
    project.session.on_exit = "disconnect".into();
    project.registers.catalogue = catalogue;
    project.memory_access_source = "project".into();
    project.memory_access = vec![MemoryAccess {
        id: "ap0".into(),
        target: "ap.actual".into(),
        tcl_endpoint: endpoint.clone(),
        ..Default::default()
    }];
    for (name, little_endian) in [("little", true), ("big", false)] {
        project.registers.components.insert(
            name.into(),
            Component {
                base: 0x20000000,
                channel: "ap0".into(),
                little_endian,
            },
        );
    }
    let engine = session::spawn(project);
    call(&engine, 1, "connect", json!({}));
    let read = call(
        &engine,
        2,
        "registers_read",
        json!({"ids":["bus32","low32","bus64","big64"]}),
    );
    let samples = read["samples"].as_array().unwrap();
    for (i, bits, address, order, hex) in [
        (0, 32, "0x20000000", "little", "0x11223344"),
        (1, 64, "0x20000008", "little", "0x01234567"),
        (2, 64, "0x20000008", "little", "0x89abcdef01234567"),
        (3, 64, "0x20000008", "big", "0x0123456789abcdef"),
    ] {
        assert_eq!(samples[i]["state"], "valid", "{}", samples[i]);
        assert_eq!(samples[i]["value"]["hex"], hex);
        let access = &samples[i]["provenance"]["access"];
        assert_eq!(access["phase"], "responded");
        assert_eq!(access["context"], read["context"]);
        assert_eq!(
            access["route"],
            json!({"kind":"tcl_memory","endpoint":endpoint,"target":"ap.actual","channel":"ap0","configuration_source":"project","address":address,"bits":bits,"bus_width":32,"count":bits/32,"byte_order":order,"atomic":false})
        );
        assert_eq!(
            access["command"],
            format!("\"ap.actual\" read_memory {address} 32 {}", bits / 32)
        );
    }
    assert_eq!(
        samples[1]["provenance"]["access"],
        samples[2]["provenance"]["access"]
    );
    assert_eq!(
        samples[1]["provenance"]["aliases"],
        json!([{"source":"bus64","offset":0,"bits":32}])
    );
    assert_eq!(
        packets.lock().unwrap().len(),
        3,
        "alias must not trigger another bus read"
    );
    let failed = call(&engine, 3, "registers_read", json!({"ids":["bus32"]}));
    assert_eq!(failed["samples"][0]["state"], "error");
    assert!(failed["samples"][0]["value"].is_null());
    let snapshot = call(&engine, 4, "status", json!({}));
    let retained = snapshot["register_samples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "bus32")
        .unwrap();
    assert_eq!(retained["value"]["hex"], "0x11223344");
    assert_eq!(
        retained["last_value_provenance"],
        json!({"status":"known","provenance":samples[0]["provenance"]})
    );
    assert_eq!(retained["provenance"], failed["samples"][0]["provenance"]);
    assert_eq!(
        retained["provenance"]["access"]["phase"], "responded",
        "receipt of an error is not validity of a value"
    );
    server.join().unwrap();
    let wire = packets.lock().unwrap();
    assert_eq!(wire.len(), 4);
    for (packet, expected) in wire.iter().zip([
        "\\\"ap.actual\\\" read_memory 0x20000000 32 1",
        "\\\"ap.actual\\\" read_memory 0x20000008 32 2",
        "\\\"ap.actual\\\" read_memory 0x20000008 32 2",
        "\\\"ap.actual\\\" read_memory 0x20000000 32 1",
    ]) {
        assert!(packet.contains(expected), "{packet}");
    }
    call(&engine, 5, "quit", json!({}));
}
