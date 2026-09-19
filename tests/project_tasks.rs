#![cfg(windows)]
use debugtui::{
    config::Project,
    session::{self, Event, Request},
};
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

fn response(engine: &session::EngineHandle, id: u64) -> (bool, Vec<String>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut logs = Vec::new();
    loop {
        let event = engine
            .events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("task response timed out");
        match event {
            Event::Log { channel, text, .. } => logs.push(format!("{channel}: {text}")),
            Event::Response {
                id: found,
                ok,
                error,
                ..
            } if found == id => {
                if let Some(error) = error {
                    logs.push(error);
                }
                return (ok, logs);
            }
            _ => {}
        }
    }
}

#[test]
fn shell_tasks_use_source_root_stream_both_pipes_and_report_failure_timeout_and_cancel() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("artifacts")
        .join(format!("task test 工程 {}", std::process::id()));
    fs::create_dir_all(root.join("scripts with spaces")).unwrap();
    fs::write(root.join("expected.flag"), "").unwrap();
    fs::write(root.join("scripts with spaces/run task.cmd"), "@echo off\r\nif not exist expected.flag exit /b 23\r\necho stdout-marker\r\necho stderr-marker 1>&2\r\necho %1>argument.txt\r\necho done>built.flag\r\n").unwrap();
    let mut p = Project::default();
    p.program.source_root = root.clone();
    p.tasks.build = r#""scripts with spaces\run task.cmd" "argument with spaces""#.into();
    p.tasks.download = "echo downloaded>downloaded.flag".into();
    let engine = debugtui::coordinator::spawn(p.clone());
    engine.send(Request::new(1, "build", json!({}))).unwrap();
    let (ok, logs) = response(&engine, 1);
    assert!(ok, "{logs:?}");
    assert!(logs.iter().any(|s| s == "build: stdout-marker"));
    assert!(logs.iter().any(|s| s == "build: stderr-marker"));
    assert!(root.join("built.flag").is_file());
    assert_eq!(
        fs::read_to_string(root.join("argument.txt"))
            .unwrap()
            .trim(),
        "\"argument with spaces\""
    );
    engine.send(Request::new(2, "download", json!({}))).unwrap();
    assert!(response(&engine, 2).0);
    assert!(root.join("downloaded.flag").is_file());
    engine.send(Request::new(3, "quit", json!({}))).unwrap();
    assert!(response(&engine, 3).0);

    p.tasks.build = "echo failure-marker 1>&2 & exit /b 7".into();
    let engine = session::spawn(p.clone());
    engine.send(Request::new(1, "build", json!({}))).unwrap();
    let (ok, logs) = response(&engine, 1);
    assert!(!ok && logs.iter().any(|s| s.contains("failure-marker")));
    assert!(logs.iter().any(|s| s.contains("exit code: 7")), "{logs:?}");
    engine.send(Request::new(2, "quit", json!({}))).unwrap();
    assert!(response(&engine, 2).0);

    p.tasks.build = "ping -n 6 127.0.0.1 >nul".into();
    p.tasks.timeout_ms = 150;
    let engine = session::spawn(p.clone());
    engine.send(Request::new(1, "build", json!({}))).unwrap();
    let (ok, logs) = response(&engine, 1);
    assert!(
        !ok && logs.iter().any(|s| s.contains("timed out")),
        "{logs:?}"
    );
    engine.send(Request::new(2, "quit", json!({}))).unwrap();
    assert!(response(&engine, 2).0);

    p.tasks.timeout_ms = 30_000;
    let engine = session::spawn(p);
    engine.send(Request::new(1, "build", json!({}))).unwrap();
    loop {
        if let Event::Snapshot { snapshot } =
            engine.events.recv_timeout(Duration::from_secs(5)).unwrap()
            && snapshot.state == "BUILDING"
        {
            break;
        }
    }
    engine.send(Request::new(2, "quit", json!({}))).unwrap();
    let (ok, logs) = response(&engine, 1);
    assert!(
        !ok && logs.iter().any(|s| s.contains("cancelled")),
        "{logs:?}"
    );
    assert!(response(&engine, 2).0);
}
