#![cfg(windows)]
//! Request cancellation across actual worker/MI pipes, without target control.
#[path = "support/artifacts.rs"]
mod test_artifacts;

use debugtui::{
    config::Project,
    session::{self, EngineHandle, Event, Request},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

fn fixture(name: &str, registers: Value, values: Value) -> (EngineHandle, PathBuf) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let directory =
        test_artifacts::root().join(format!("register-cancel-{name}-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let transcript = directory.join("commands.txt");
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
        .insert("DEBUGTUI_TEST_REGISTERS".into(), registers.to_string());
    project
        .gdb
        .env
        .insert("DEBUGTUI_TEST_REGISTER_VALUES".into(), values.to_string());
    project
        .gdb
        .env
        .insert("DEBUGTUI_TEST_REGISTER_DELAY_MS".into(), "200".into());
    project.gdb.env.insert(
        "DEBUGTUI_TEST_TRANSCRIPT".into(),
        transcript.to_string_lossy().into_owned(),
    );
    project.registers.cpu = "cortex-r52".into();
    project.target.endpoint = format!("localhost:{}", 30000 + name.len());
    project.session.on_exit = "disconnect".into();
    let engine = session::spawn(project);
    send(&engine, 1, "connect", json!({})).unwrap();
    (engine, transcript)
}
fn wait(engine: &EngineHandle, id: u64) -> Result<Value, String> {
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
            return if ok { Ok(result) } else { Err(error.unwrap()) };
        }
    }
}
fn send(engine: &EngineHandle, id: u64, method: &str, params: Value) -> Result<Value, String> {
    engine.send(Request::new(id, method, params)).unwrap();
    wait(engine, id)
}
fn reads(path: &PathBuf) -> usize {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| line.starts_with("-data-list-register-values r "))
        .count()
}
fn cancel_after_first_read(engine: &EngineHandle, path: &PathBuf, request: Request) -> String {
    let previous = reads(path);
    engine.send(request.clone()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while reads(path) == previous {
        assert!(Instant::now() < deadline, "No register transaction began");
        std::thread::sleep(Duration::from_millis(5));
    }
    request.cancel_read();
    let error = wait(engine, request.id).unwrap_err();
    assert_eq!(reads(path), previous + 1, "No subsequent register was read");
    assert!(error.contains("cancelled"), "{error}");
    error
}
#[test]
fn register_read_cancel_discards_partial_and_final_values_preserves_old_samples_and_session() {
    for ids in [json!(["r1", "r2"]), json!(["r1"])] {
        let (engine, transcript) = fixture(
            if ids.as_array().unwrap().len() == 1 {
                "final"
            } else {
                "partial"
            },
            json!(["r0", "r1", "r2"]),
            json!({"r0":"0x80000001"}),
        );
        send(&engine, 2, "registers_read", json!({"ids":["r0"]})).unwrap();
        let before = send(&engine, 3, "status", json!({})).unwrap();
        cancel_after_first_read(
            &engine,
            &transcript,
            Request::new(4, "registers_read", json!({"ids":ids})),
        );
        let after = send(&engine, 5, "status", json!({})).unwrap();
        assert_eq!(after["register_samples"], before["register_samples"]);
        assert_eq!(after["state"], "STOPPED");
        assert_eq!(after["generation"], before["generation"]);
        assert!(
            !engine
                .cancellation
                .load(std::sync::atomic::Ordering::Relaxed)
        );
        send(&engine, 6, "registers_read", json!({"ids":["r2"]})).unwrap();
        let commands = fs::read_to_string(&transcript).unwrap();
        assert!(!commands.contains("-exec-interrupt"));
        assert!(!commands.contains("-exec-continue"));
        send(&engine, 7, "quit", json!({})).unwrap();
    }
}
#[test]
fn queued_register_cancel_sends_no_mi_and_local_token_is_not_accepted_over_json() {
    let (engine, transcript) = fixture("queued", json!(["r0"]), json!({}));
    let request = Request::new(2, "registers_read", json!({"ids":["r0"]}));
    request.cancel_read();
    engine.send(request.clone()).unwrap();
    assert!(wait(&engine, 2).unwrap_err().contains("cancelled"));
    assert_eq!(reads(&transcript), 0);
    let serialized = serde_json::to_value(&request).unwrap();
    assert!(serialized.get("read_cancel").is_none());
    let mut input = serialized;
    input["id"] = json!(3);
    input["read_cancel"] = json!(true);
    engine.send(serde_json::from_value(input).unwrap()).unwrap();
    assert_eq!(wait(&engine, 3).unwrap()["samples"][0]["state"], "valid");
    assert_eq!(reads(&transcript), 1);
    // The cancellation method has no effect on session/control request dispatch.
    let status = Request::new(4, "status", json!({}));
    status.cancel_read();
    engine.send(status).unwrap();
    assert_eq!(wait(&engine, 4).unwrap()["state"], "STOPPED");
    send(&engine, 5, "quit", json!({})).unwrap();
}
#[test]
fn capability_probe_cancel_retains_previous_current_evidence_and_stops_optional_reads() {
    let (engine, transcript) = fixture(
        "probe",
        json!([
            "cpsr", "midr", "id_pfr1", "id_dfr0", "mpuir", "hmpuir", "cpacr", "pmcr", "icc_ctlr",
            "ich_vtr"
        ]),
        json!({"cpsr":"0x1a","midr":"0x411fd134","id_pfr1":"0x10111001","id_dfr0":"0x03010066","mpuir":"0x1800","hmpuir":"0x14","cpacr":"0xf00000","pmcr":"0x41132000","icc_ctlr":"0x400","ich_vtr":"0x90180003"}),
    );
    let context = send(&engine, 2, "registers_list", json!({})).unwrap()["context"].clone();
    send(&engine, 3, "registers_probe", json!({"context":context})).unwrap();
    let before = send(&engine, 4, "status", json!({})).unwrap();
    cancel_after_first_read(
        &engine,
        &transcript,
        Request::new(5, "registers_probe", json!({"context":context})),
    );
    let after = send(&engine, 6, "status", json!({})).unwrap();
    assert_eq!(after["register_probe"], before["register_probe"]);
    assert_eq!(after["register_samples"], before["register_samples"]);
    send(&engine, 7, "quit", json!({})).unwrap();
}
