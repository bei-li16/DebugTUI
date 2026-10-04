#![cfg(windows)]
//! Real engine/MI pipe integration against a strict local fixture; no board.
use debugtui::{
    config::Project,
    session::{self, Event, Request},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

fn response(engine: &session::EngineHandle, id: u64, method: &str, params: Value) -> Value {
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
fn catalogue_refresh_is_on_demand_and_snapshots_retain_precise_stale_values() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = root
        .join("artifacts")
        .join(format!("register access {}", std::process::id()));
    fs::create_dir_all(&output).unwrap();
    let transcript = output.join("commands.txt");
    fs::write(&transcript, "").unwrap();
    let mut project = Project::default();
    project.gdb.executable = "node".into();
    project.gdb.args = vec![
        root.join("tests/mock-gdb.cjs")
            .to_string_lossy()
            .into_owned(),
    ];
    project.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTERS".into(),
        json!(["r0", "", "pc", "cntpct"]).to_string(),
    );
    project.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTER_VALUES".into(),
        json!({"r0":"0x12345678", "cntpct":"0xfedcba9876543210"}).to_string(),
    );
    project.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTER_ERRORS".into(),
        json!(["pc"]).to_string(),
    );
    project.gdb.env.insert(
        "DEBUGTUI_TEST_TRANSCRIPT".into(),
        transcript.to_string_lossy().into_owned(),
    );
    project.target.endpoint = "localhost:1234".into();
    project.registers.catalogue = root.join("profiles/registers/cortex-r52.toml");
    let engine = session::spawn(project);
    response(&engine, 1, "connect", json!({}));
    let commands = fs::read_to_string(&transcript).unwrap();
    assert!(
        !commands.contains("-data-list-register-values"),
        "connect/stop caused an unsolicited register read: {commands}"
    );
    assert!(!commands.contains("-data-list-register-names"));
    let read = response(
        &engine,
        2,
        "registers_read",
        json!({"ids":["r0", "pc", "cntpct"]}),
    );
    assert_eq!(read["samples"][0]["value"]["hex"], "0x12345678");
    assert_eq!(read["samples"][1]["state"], "unavailable");
    assert_eq!(read["samples"][2]["value"]["hex"], "0xfedcba9876543210");
    let commands = fs::read_to_string(&transcript).unwrap();
    let reads: Vec<_> = commands
        .lines()
        .filter(|command| command.starts_with("-data-list-register-values"))
        .collect();
    assert_eq!(
        reads,
        [
            "-data-list-register-values r 0",
            "-data-list-register-values r 2",
            "-data-list-register-values r 3"
        ]
    );
    let snapshot = response(&engine, 3, "status", json!({}));
    assert_eq!(snapshot["registers"][0]["name"], "r0");
    assert_eq!(snapshot["registers"][0]["value"], "0x12345678");
    assert_eq!(
        snapshot["register_samples"][2]["value"]["hex"],
        "0xfedcba9876543210"
    );
    response(&engine, 4, "step", json!({}));
    response(&engine, 5, "wait_stopped", json!({}));
    let stopped = response(&engine, 6, "status", json!({}));
    assert_eq!(stopped["register_samples"][0]["state"], "stale");
    assert_eq!(stopped["register_samples"][0]["value"]["hex"], "0x12345678");
    assert!(stopped["registers"][0]["error"].as_bool().unwrap());
    let final_commands = fs::read_to_string(&transcript).unwrap();
    assert_eq!(
        final_commands
            .lines()
            .filter(|command| command.starts_with("-data-list-register-values"))
            .count(),
        3
    );
    response(&engine, 7, "quit", json!({}));
}
