use super::*;

fn render(a: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| draw(f, a)).unwrap();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                .collect::<String>()
                + "\n"
        })
        .collect()
}

fn key(a: &mut App, code: KeyCode) {
    a.key(KeyEvent::new(code, KeyModifiers::NONE), None);
}

fn click(a: &mut App, area: Rect) {
    a.mouse(
        MouseEvent {
            kind: MouseEventKind::Down(event::MouseButton::Left),
            column: area.x,
            row: area.y,
            modifiers: KeyModifiers::NONE,
        },
        None,
    );
}

#[test]
fn files_filter_uses_matched_indices_for_keyboard_mouse_and_scrolling() {
    let mut a = App::new(Project::default(), false);
    a.snapshot.files = [
        "main.c",
        "drivers/espi_std.c",
        "drivers/Spi.c",
        "hal/espi_hal.c",
        "drivers/Spi_Irq.c",
    ]
    .map(str::to_owned)
    .into();
    a.select_pane(7);
    render(&mut a, 120, 36);
    let area = a.file_search.area;
    click(&mut a, area);
    for ch in "spi".chars() {
        key(&mut a, KeyCode::Char(ch));
    }
    let text = render(&mut a, 120, 36);
    for file in ["Spi.c", "Spi_Irq.c", "espi_hal.c", "espi_std.c"] {
        assert!(text.contains(file));
    }
    assert!(!text.contains("main.c"));
    assert_eq!(a.view_len(7), 4);
    key(&mut a, KeyCode::Down);
    key(&mut a, KeyCode::Enter);
    assert_eq!(a.source_file, "drivers/Spi_Irq.c");
    assert_eq!(a.pane, 0);
    a.select_pane(7);
    render(&mut a, 120, 36);
    let rows = a.view_rects[7];
    click(
        &mut a,
        Rect {
            y: rows.y + 2,
            ..rows
        },
    );
    assert_eq!(a.source_file, "drivers/espi_std.c");
    a.select_pane(7);
    a.key(
        KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL),
        None,
    );
    a.key(
        KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
        None,
    );
    assert!(a.file_search.query.is_empty());
    assert_eq!(a.selection, 0);
    a.search_paste("unmatched\nvalue");
    assert!(render(&mut a, 120, 36).contains("No matching files"));
    key(&mut a, KeyCode::Enter);
    assert_eq!(a.pane, 7);
    assert_eq!(a.source_file, "drivers/espi_std.c");
}

#[test]
fn filtering_does_not_dispatch_letter_shortcuts_and_leaving_restores_input() {
    let (engine, requests) = session::test_channel();
    let mut a = App::new(Project::default(), false);
    a.select_pane(7);
    a.key(
        KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL),
        Some(&engine),
    );
    for ch in "qfr?".chars() {
        a.key(
            KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            Some(&engine),
        );
    }
    assert_eq!(a.file_search.query, "qfr?");
    assert!(!a.quitting && !a.help);
    assert!(requests.try_recv().is_err());
    key(&mut a, KeyCode::Tab);
    assert!(!a.file_search.editing);
    assert_ne!(a.pane, 7);
}

#[test]
fn long_ci_paths_keep_the_matching_filename_visible() {
    let mut a = App::new(Project::default(), false);
    a.snapshot.files = vec![format!(
        "/ci/{}/drivers/Spi_Irq.c",
        "long-workspace/".repeat(14)
    )];
    a.select_pane(7);
    a.file_search.query = "spi".into();
    assert!(render(&mut a, 80, 24).contains("Spi_Irq.c"));
    key(&mut a, KeyCode::Enter);
    assert_eq!(a.source_file, a.snapshot.files[0]);
}

#[test]
fn other_dialogs_own_paste_when_file_search_was_focused() {
    let mut a = App::new(Project::default(), false);
    a.select_pane(7);
    a.file_search.editing = true;
    a.open_help(false);
    assert!(!a.search_paste("unrelated command"));
    assert!(a.file_search.query.is_empty());
    a.palette = false;
    a.open_source_list();
    assert!(!a.search_paste("open-file-filter"));
    assert!(a.file_search.query.is_empty());
}

#[test]
fn symbols_debounce_bound_requests_and_reject_stale_query_and_core_replies() {
    let (engine, requests) = session::test_channel();
    let mut a = App::new(Project::default(), false);
    a.snapshot.state = "STOPPED".into();
    a.open_symbol_search();
    a.search_paste("spi");
    assert!(!a.ensure_symbol_search(Some(&engine)));
    a.symbol_search.changed -= Duration::from_secs(1);
    assert!(a.ensure_symbol_search(Some(&engine)));
    let old = requests.try_recv().unwrap();
    assert_eq!(old.method, "symbols");
    assert_eq!(old.params["query"], "spi");
    a.search_paste("_irq");
    a.symbol_search.changed -= Duration::from_secs(1);
    assert!(!a.ensure_symbol_search(Some(&engine)));
    assert!(a.symbol_response(old.id, &json!({"symbols":[{"name":"old","kind":"function","file":"wrong.c","line":1,"description":""}]}), None));
    assert!(a.symbol_search.items.is_empty());
    assert!(a.ensure_symbol_search(Some(&engine)));
    let current = requests.try_recv().unwrap();
    let mut snapshot = a.snapshot.clone();
    snapshot.core = Some(session::CoreStatus {
        index: 1,
        name: "core1".into(),
        endpoint: "localhost:3334".into(),
        state: "STOPPED".into(),
    });
    a.update(Event::Snapshot {
        snapshot: Box::new(snapshot),
    });
    a.symbol_response(current.id, &json!({"symbols":[{"name":"old","kind":"function","file":"wrong.c","line":1,"description":""}]}), None);
    assert!(a.symbol_search.items.is_empty());
    a.snapshot.state = "RUNNING".into();
    a.ensure_symbol_search(Some(&engine));
    assert!(a.symbol_search.hint.contains("Pause/connect"));
    assert!(requests.try_recv().is_err()); // No implicit pause, expression evaluation or target run.
}

#[test]
fn symbol_navigation_maps_ci_paths_and_preserves_debugger_frame() {
    let root =
        std::env::temp_dir().join(format!("debugtui-symbol-navigation-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("Spi.c"),
        (1..=60)
            .map(|i| format!("// line {i}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let mut project = Project::default();
    project.source_map.push(crate::config::SourceMap {
        from: "/ci/src".into(),
        to: root.clone(),
    });
    let mut a = App::new(project, false);
    a.snapshot.frame.file = "stopped.c".into();
    a.snapshot.frame.line = 7;
    a.open_symbol_search();
    a.symbol_search.items = vec![Symbol {
        name: "Spi_Init".into(),
        kind: "function".into(),
        file: "/ci/src/Spi.c".into(),
        line: 32,
        description: "void Spi_Init(void);".into(),
    }];
    render(&mut a, 120, 36);
    let result = a.symbol_search.hits[0].0;
    click(&mut a, result);
    assert_eq!(a.source_file, "/ci/src/Spi.c");
    assert_eq!(a.source_line, 31);
    assert_eq!(a.source[31], "// line 32");
    assert_eq!(a.snapshot.frame.file, "stopped.c");
    assert_eq!(a.snapshot.frame.line, 7);
    assert!(!a.symbol_search.open);
    a.open_symbol_search();
    a.symbol_search.items[0].line = 0;
    key(&mut a, KeyCode::Enter);
    assert!(a.symbol_search.open);
    assert!(a.symbol_search.hint.contains("no source location"));
    fs::remove_file(root.join("Spi.c")).unwrap();
    fs::remove_dir(root).unwrap();
}

#[test]
fn closing_pending_search_can_retry_and_error_partial_results_are_visible() {
    let (engine, requests) = session::test_channel();
    let mut a = App::new(Project::default(), false);
    a.snapshot.state = "READY".into();
    a.open_symbol_search();
    a.search_paste("spi");
    a.symbol_search.changed -= Duration::from_secs(1);
    a.ensure_symbol_search(Some(&engine));
    let request = requests.try_recv().unwrap();
    key(&mut a, KeyCode::Esc);
    a.symbol_response(request.id, &json!({"symbols":[]}), None);
    a.open_symbol_search();
    a.symbol_search.changed -= Duration::from_secs(1);
    assert!(a.ensure_symbol_search(Some(&engine)));
    let request = requests.try_recv().unwrap();
    a.symbol_response(request.id, &Value::Null, Some("Unsupported MI command"));
    assert!(a.symbol_search.hint.contains("Unsupported MI command"));
    key(&mut a, KeyCode::F(4));
    a.symbol_search.changed -= Duration::from_secs(1);
    a.ensure_symbol_search(Some(&engine));
    let request = requests.try_recv().unwrap();
    a.symbol_response(
        request.id,
        &json!({"symbols":[],"warnings":["types unsupported"]}),
        None,
    );
    assert!(a.symbol_search.hint.contains("Partial results"));
}

#[test]
fn files_load_from_ready_elf_without_requesting_target_memory_or_registers() {
    let (engine, requests) = session::test_channel();
    let mut a = App::new(Project::default(), false);
    a.snapshot.state = "READY".into();
    a.select_pane(7);
    a.side_pane = 4;
    render(&mut a, 120, 36);
    assert!(a.ensure_visible_data(Some(&engine)));
    assert_eq!(requests.try_recv().unwrap().method, "files");
    assert!(requests.try_recv().is_err());
}

#[test]
fn search_layout_supports_narrow_terminals_and_exports_previews() {
    let mut a = App::new(Project::default(), true);
    a.snapshot.files = [
        "src/Spi.c",
        "src/Spi_Irq.c",
        "src/espi_hal.c",
        "src/espi_std.c",
        "src/main.c",
    ]
    .map(str::to_owned)
    .into();
    for (w, h) in [(45, 12), (80, 24), (120, 36)] {
        a.select_pane(7);
        a.file_search.query = "spi".into();
        let files = render(&mut a, w, h);
        assert!(files.contains("Find:"));
        assert!(a.file_search.area.width > 0);
        assert!(a.symbol_search.bar.width > 0);
        a.open_symbol_search();
        a.symbol_search.query = "process".into();
        a.symbol_search.invalidate();
        a.symbol_search.changed -= Duration::from_secs(1);
        a.ensure_symbol_search(None);
        let symbols = render(&mut a, w, h);
        assert!(symbols.contains("process_items"));
        assert!(
            a.symbol_search
                .hits
                .iter()
                .all(|(r, _)| r.right() <= w && r.bottom() <= h)
        );
        if let Ok(root) = std::env::var("DEBUGTUI_RENDER_DIR") {
            fs::create_dir_all(&root).unwrap();
            fs::write(
                Path::new(&root).join(format!("files-search-{w}x{h}.txt")),
                files,
            )
            .unwrap();
            fs::write(
                Path::new(&root).join(format!("symbols-search-{w}x{h}.txt")),
                symbols,
            )
            .unwrap();
        }
        a.symbol_search.open = false;
    }
}
