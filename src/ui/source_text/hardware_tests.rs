//! Opt-in UI-to-GDB test: uses existing firmware, never builds/downloads.
//! DEBUGTUI_SOURCE_PROJECT names a THA6206 project; logs/previews go to
//! DEBUGTUI_RENDER_DIR. Project preferences and firmware are not changed.
//! DEBUGTUI_SOURCE_INITIALIZE additionally invokes the configured Reset / Run.
use super::*;

fn response(a: &mut App, engine: &EngineHandle, id: u64, events: &mut Vec<Value>) -> Value {
    let until = Instant::now() + Duration::from_secs(60);
    loop {
        let event = engine
            .events
            .recv_timeout(until.saturating_duration_since(Instant::now()))
            .expect("debugger response deadline");
        events.push(serde_json::to_value(&event).unwrap());
        let result = match &event {
            Event::Response {
                id: got,
                ok,
                result,
                error,
            } if *got == id => {
                assert!(*ok, "request {id}: {error:?}");
                Some(result.clone())
            }
            _ => None,
        };
        a.update(event);
        if let Some(result) = result {
            return result;
        }
    }
}
fn command(
    a: &mut App,
    engine: &EngineHandle,
    method: &str,
    params: Value,
    events: &mut Vec<Value>,
) -> Value {
    let id = a.next_id;
    a.submit(Some(engine), method, params);
    response(a, engine, id, events)
}
fn refresh(a: &mut App, engine: &EngineHandle, events: &mut Vec<Value>) {
    let value = command(a, engine, "status", json!({}), events);
    a.update(Event::Snapshot {
        snapshot: Box::new(serde_json::from_value(value).unwrap()),
    });
}

#[test]
#[ignore = "requires a connected THA6206 board and DEBUGTUI_SOURCE_PROJECT; pauses selected cores"]
fn tha6206_source_selection_to_real_watch() {
    let path = PathBuf::from(std::env::var("DEBUGTUI_SOURCE_PROJECT").unwrap());
    let root = PathBuf::from(std::env::var("DEBUGTUI_RENDER_DIR").unwrap());
    fs::create_dir_all(&root).unwrap();
    let mut project = Project::load(&path).unwrap();
    project.path = None; // No test Watch/breakpoint persistence into the user's project.
    project.session.log_dir = Some(root.join("logs"));
    project.watch.clear();
    project.breakpoints.clear();
    project.target.after_connect.clear();
    project.live_watch = None;
    for c in &mut project.cores {
        c.watch = Some(vec![]);
        c.breakpoints = Some(vec![]);
        c.after_connect.clear();
    }
    let source = project
        .program
        .source_root
        .join("DemoWorkspace/CodeProject/DemoMcal/Demo_Uart/Uart_Demo.c");
    assert!(source.is_file(), "{}", source.display());
    let original = fs::read(&source).unwrap();
    let original_elf = fs::read(&project.program.elf).unwrap();
    let count = project.cores.len().max(1);
    let engine = crate::coordinator::spawn(project.clone());
    let mut a = App::new(project, false);
    let mut events = vec![];
    let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        command(&mut a, &engine, "connect", json!({}), &mut events);
        if std::env::var_os("DEBUGTUI_SOURCE_INITIALIZE").is_some() {
            command(&mut a, &engine, "restart", json!({}), &mut events);
            command(&mut a, &engine, "run", json!({}), &mut events);
            command(
                &mut a,
                &engine,
                "wait_stopped",
                json!({"timeout_ms":8000}),
                &mut events,
            );
            refresh(&mut a, &engine, &mut events);
            assert_eq!(a.snapshot.frame.function, "_main");
            assert!(Path::new(&a.snapshot.frame.file).is_file());
        }
        for core in 0..count {
            if count > 1 {
                command(
                    &mut a,
                    &engine,
                    "select_core",
                    json!({"index":core}),
                    &mut events,
                );
            }
            refresh(&mut a, &engine, &mut events);
            if a.snapshot.state != "STOPPED" {
                command(&mut a, &engine, "pause", json!({}), &mut events);
                command(
                    &mut a,
                    &engine,
                    "wait_stopped",
                    json!({"timeout_ms":5000}),
                    &mut events,
                );
                refresh(&mut a, &engine, &mut events);
            }
            assert!(a.snapshot.watches.is_empty(), "Watch must be per-core");
            a.load_source(&source.to_string_lossy());
            a.select_pane(0);
            let row = a
                .source
                .iter()
                .position(|s| s.contains("uint32 uart_cnt"))
                .unwrap();
            let byte = a.source[row].find("uart_cnt").unwrap();
            a.source_line = row;
            a.source_top = row.saturating_sub(3);
            let mut t = Terminal::new(TestBackend::new(160, 45)).unwrap();
            t.draw(|f| draw(f, &mut a)).unwrap();
            let col = a.source_text.area.x + column(&a.source[row], byte + 2) as u16;
            let y = a.source_text.area.y + (row - a.source_top) as u16;
            for _ in 0..2 {
                for kind in [
                    MouseEventKind::Down(event::MouseButton::Left),
                    MouseEventKind::Up(event::MouseButton::Left),
                ] {
                    a.mouse(
                        MouseEvent {
                            kind,
                            column: col,
                            row: y,
                            modifiers: KeyModifiers::NONE,
                        },
                        Some(&engine),
                    );
                }
            }
            assert_eq!(a.source_text.text(&a.source).as_deref(), Some("uart_cnt"));
            a.key(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                Some(&engine),
            );
            crate::clipboard::COPIED.with(|v| assert_eq!(*v.borrow(), "uart_cnt"));
            super::super::visual_tests::capture(
                &root,
                &format!("core{core}-selection"),
                &mut a,
                160,
                45,
            );
            let id = a.next_id;
            a.key(
                KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
                Some(&engine),
            );
            response(&mut a, &engine, id, &mut events);
            refresh(&mut a, &engine, &mut events);
            let watch = a
                .snapshot
                .watches
                .iter()
                .find(|v| v.name == "uart_cnt")
                .expect("Watch appears");
            assert!(!watch.error, "{watch:?}");
            assert!(!watch.value.is_empty());
            assert_eq!(a.snapshot.watches.len(), 1);
            println!(
                "{} session{core} @ {}: uart_cnt = {}; PC={} {}:{}",
                path.display(),
                a.snapshot
                    .core
                    .as_ref()
                    .map(|c| c.endpoint.as_str())
                    .unwrap_or(&a.project.target.endpoint),
                watch.value,
                a.snapshot.frame.address,
                a.snapshot.frame.file,
                a.snapshot.frame.line
            );
            super::super::visual_tests::capture(
                &root,
                &format!("core{core}-watch"),
                &mut a,
                160,
                45,
            );
        }
        if count > 1 {
            command(
                &mut a,
                &engine,
                "select_core",
                json!({"index":0}),
                &mut events,
            );
            refresh(&mut a, &engine, &mut events);
            assert_eq!(a.snapshot.watches.len(), 1);
            assert_eq!(a.snapshot.watches[0].name, "uart_cnt");
        }
        assert_eq!(fs::read(&source).unwrap(), original);
        assert_eq!(fs::read(&a.project.program.elf).unwrap(), original_elf);
    }));
    command(&mut a, &engine, "quit", json!({}), &mut events);
    fs::write(
        root.join("ui-gdb-events.json"),
        serde_json::to_vec_pretty(&events).unwrap(),
    )
    .unwrap();
    if let Err(panic) = checked {
        std::panic::resume_unwind(panic);
    }
}
