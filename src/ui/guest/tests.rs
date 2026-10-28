use super::*;
fn app() -> App {
    let mut project = Project::default();
    project.images.push(crate::images::Image {
        id: "vm0".into(),
        elf: "vm0.elf".into(),
        saved_context: Some(Default::default()),
        ..Default::default()
    });
    let mut app = App::new(project, false);
    app.snapshot.state = state::STOPPED.into();
    app
}
#[test]
fn guest_frame_navigation_is_read_only_and_late_results_cannot_cross_a_stop() {
    let (engine, rx) = session::test_channel();
    let mut app = app();
    app.open_guest(Some(&engine), "vm0");
    let request = rx.try_recv().unwrap();
    assert_eq!(request.method, "guest_context");
    assert_eq!(request.params["level"], 0);
    let result = json!({"stack":[{"level":0,"function":"notify","file":"","line":45,"address":"0x10020004"},{"level":1,"function":"guest_main","file":"","line":199,"address":"0x10020004"}],"selected_frame":{"level":0},"locals":[{"name":"r0","value":"2248146945","changed":false,"error":false}],"saved":{"registers":[0,0,0,0,0,0,0,0,0,0,0,0,0,4096,8192,12288,19]}});
    assert!(app.guest_response(request.id, &result, None));
    assert_eq!(app.snapshot.frame.level, 0);
    app.guest_key(
        KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
        Some(&engine),
    );
    let second = rx.try_recv().unwrap();
    assert_eq!(second.method, "guest_context");
    assert_eq!(second.params["level"], 1);
    let mut next = app.snapshot.clone();
    next.generation += 1;
    app.guest_snapshot(&next);
    assert!(!app.guest_view.open);
    assert!(app.guest_response(second.id, &result, None));
    assert!(!app.guest_view.open);
    assert!(rx.try_recv().is_err());
}
#[test]
fn guest_inspector_is_clipped_to_terminal_and_running_target_is_rejected() {
    let mut app = app();
    app.guest_view.open = true;
    app.guest_view.image = "vm0".into();
    app.guest_view.source = vec!["test();".into(); 100];
    for (width, height) in [(45, 12), (80, 24), (160, 45)] {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| app.guest_draw(f)).unwrap();
        assert!(
            app.guest_view
                .hits
                .iter()
                .all(|(r, _)| r.right() <= width && r.bottom() <= height)
        );
    }
    app.guest_view.open = false;
    app.snapshot.state = state::RUNNING.into();
    app.open_guest(None, "vm0");
    assert!(!app.guest_view.open);
}
