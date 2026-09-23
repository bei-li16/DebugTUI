use super::*;
use event::MouseButton::{Left, Right};

fn render(a: &mut App) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(120, 36)).unwrap();
    t.draw(|f| draw(f, a)).unwrap();
    t
}
fn app(lines: &[&str]) -> App {
    let mut a = App::new(Project::default(), false);
    a.source_file = "test.c".into();
    a.source = lines.iter().map(|s| s.to_string()).collect();
    a.source_comments = highlight::comment_starts(&a.source);
    a.snapshot.state = "STOPPED".into();
    a.fx.mode = crate::config::Motion::Off;
    a.select_pane(0);
    render(&mut a);
    a
}
fn mouse(a: &mut App, kind: MouseEventKind, col: u16, row: u16, e: Option<&EngineHandle>) {
    a.mouse(
        MouseEvent {
            kind,
            column: a.source_text.area.x + col,
            row: a.source_text.area.y + row,
            modifiers: KeyModifiers::NONE,
        },
        e,
    );
}
fn key(a: &mut App, code: KeyCode, modifiers: KeyModifiers, e: Option<&EngineHandle>) {
    a.key(KeyEvent::new(code, modifiers), e);
}
fn select(a: &mut App, row: usize, start: usize, end: usize) {
    a.source_text.anchor = Some(Position { row, byte: start });
    a.source_text.caret = Position { row, byte: end };
    a.source_line = row;
}

#[test]
fn dragging_copies_only_raw_source_and_keeps_breakpoint_gutter() {
    let (engine, requests) = session::test_channel();
    let mut a = app(&["\tuart_cnt++; // 中文", "\tplatform.cpu_num;"]);
    mouse(&mut a, MouseEventKind::Down(Left), 4, 0, Some(&engine));
    mouse(&mut a, MouseEventKind::Drag(Left), 12, 0, Some(&engine));
    mouse(&mut a, MouseEventKind::Up(Left), 12, 0, Some(&engine));
    assert_eq!(a.source_text.text(&a.source).as_deref(), Some("uart_cnt"));
    let t = render(&mut a);
    let r = a.source_text.area;
    assert_eq!(t.backend().buffer()[(r.x + 4, r.y)].bg, theme::ACCENT);
    assert_ne!(t.backend().buffer()[(r.x + 12, r.y)].bg, theme::ACCENT);
    assert_ne!(t.backend().buffer()[(r.x - 3, r.y)].bg, theme::ACCENT);
    key(
        &mut a,
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
        Some(&engine),
    );
    crate::clipboard::COPIED.with(|v| assert_eq!(*v.borrow(), "uart_cnt"));
    assert!(requests.try_recv().is_err());
    a.mouse(
        MouseEvent {
            kind: MouseEventKind::Down(Left),
            column: a.source_rect.x + 3,
            row: r.y,
            modifiers: KeyModifiers::NONE,
        },
        Some(&engine),
    );
    assert_eq!(requests.try_recv().unwrap().method, "break");
    assert!(a.source_text.range().is_none());
}

#[test]
fn double_click_unicode_word_reverse_multiline_and_shift_navigation() {
    let mut a = app(&["\t变量_1++;", "\treturn uart_cnt;"]);
    for _ in 0..2 {
        mouse(&mut a, MouseEventKind::Down(Left), 6, 0, None);
        mouse(&mut a, MouseEventKind::Up(Left), 6, 0, None);
    }
    assert_eq!(a.source_text.text(&a.source).as_deref(), Some("变量_1"));
    key(&mut a, KeyCode::Home, KeyModifiers::NONE, None);
    key(&mut a, KeyCode::Right, KeyModifiers::SHIFT, None);
    key(&mut a, KeyCode::Right, KeyModifiers::SHIFT, None);
    assert_eq!(a.source_text.text(&a.source).as_deref(), Some("\t变"));
    key(&mut a, KeyCode::Char('a'), KeyModifiers::CONTROL, None);
    assert_eq!(a.source_text.text(&a.source).unwrap(), a.source.join("\n"));
    a.source_text.anchor = Some(Position { row: 1, byte: 7 });
    a.source_text.caret = Position { row: 0, byte: 1 };
    assert_eq!(
        a.source_text.text(&a.source).as_deref(),
        Some("变量_1++;\n\treturn")
    );
    key(&mut a, KeyCode::Esc, KeyModifiers::NONE, None);
    assert!(a.source_text.range().is_none());
}

#[test]
fn copy_requires_source_focus_and_selection_and_f6_always_pauses() {
    let (engine, requests) = session::test_channel();
    let mut a = app(&["uart_cnt"]);
    key(
        &mut a,
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
        Some(&engine),
    );
    assert_eq!(requests.try_recv().unwrap().method, "pause");
    select(&mut a, 0, 0, 8);
    key(
        &mut a,
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
        Some(&engine),
    );
    assert!(requests.try_recv().is_err());
    key(&mut a, KeyCode::F(6), KeyModifiers::NONE, Some(&engine));
    assert_eq!(requests.try_recv().unwrap().method, "pause");
    a.select_pane(3);
    key(
        &mut a,
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
        Some(&engine),
    );
    assert_eq!(requests.try_recv().unwrap().method, "pause");
}

#[test]
fn watch_button_reuses_normal_request_and_preserves_failed_expression() {
    let (engine, requests) = session::test_channel();
    let mut a = app(&["\tplatform.cpu_num;", "uart_cnt"]);
    select(&mut a, 0, 1, 17);
    render(&mut a);
    let hit = a.source_text.buttons.iter().find(|(_, w)| *w).unwrap().0;
    a.mouse(
        MouseEvent {
            kind: MouseEventKind::Down(Left),
            column: hit.x,
            row: hit.y,
            modifiers: KeyModifiers::NONE,
        },
        Some(&engine),
    );
    let request = requests.try_recv().unwrap();
    assert_eq!(request.method, "watch");
    assert_eq!(request.params["expression"], "platform.cpu_num");
    assert_eq!(a.pane, 1);
    a.update(Event::Response {
        id: request.id,
        ok: false,
        result: Value::Null,
        error: Some("No symbol platform".into()),
    });
    assert_eq!(a.watch_input, "platform.cpu_num");
    a.select_pane(0);
    a.source_text.anchor = Some(Position::default());
    a.source_text.caret = Position { row: 1, byte: 8 };
    key(&mut a, KeyCode::Enter, KeyModifiers::CONTROL, Some(&engine));
    assert!(requests.try_recv().is_err());
    assert!(a.notice.contains("single-line"));
}

#[test]
fn right_click_selects_word_and_menu_supports_keyboard_and_dismissal() {
    let (engine, requests) = session::test_channel();
    let mut a = app(&["uart_cnt++;"]);
    mouse(&mut a, MouseEventKind::Down(Right), 3, 0, Some(&engine));
    assert_eq!(a.source_text.text(&a.source).as_deref(), Some("uart_cnt"));
    render(&mut a);
    assert_eq!(a.source_text.menu_hits.len(), 2);
    key(&mut a, KeyCode::Esc, KeyModifiers::NONE, Some(&engine));
    assert!(a.source_text.menu.is_none());
    assert!(requests.try_recv().is_err());
    mouse(&mut a, MouseEventKind::Down(Right), 3, 0, Some(&engine));
    key(&mut a, KeyCode::Enter, KeyModifiers::CONTROL, Some(&engine));
    assert_eq!(
        requests.try_recv().unwrap().params["expression"],
        "uart_cnt"
    );
}

#[test]
fn core_switch_and_file_close_clear_selection_but_watch_refresh_does_not() {
    let mut a = app(&["uart_cnt"]);
    select(&mut a, 0, 0, 8);
    a.update(Event::Snapshot {
        snapshot: Box::new(a.snapshot.clone()),
    });
    assert!(a.source_text.range().is_some());
    let mut next = a.snapshot.clone();
    next.core = Some(session::CoreStatus {
        index: 1,
        name: "core.1".into(),
        endpoint: "localhost:3334".into(),
        state: "STOPPED".into(),
    });
    a.update(Event::Snapshot {
        snapshot: Box::new(next),
    });
    assert!(a.source_text.range().is_none());
    a.source = vec!["uart_cnt".into()];
    select(&mut a, 0, 0, 8);
    a.hide_source();
    assert!(a.source_text.range().is_none());
}

#[test]
fn long_lines_scroll_text_without_moving_gutter_or_breakpoint_location() {
    let (engine, requests) = session::test_channel();
    let text = format!("{}uart_cnt", " ".repeat(160));
    let mut a = app(&[&text]);
    key(&mut a, KeyCode::End, KeyModifiers::NONE, Some(&engine));
    for _ in 0..8 {
        key(&mut a, KeyCode::Left, KeyModifiers::SHIFT, Some(&engine));
    }
    assert!(a.source_text.left > 0);
    let t = render(&mut a);
    let r = a.source_text.area;
    let displayed = (r.x..r.right())
        .map(|x| t.backend().buffer()[(x, r.y)].symbol())
        .collect::<String>();
    assert!(displayed.contains("uart_cnt"));
    assert_eq!(a.source_text.text(&a.source).as_deref(), Some("uart_cnt"));
    key(&mut a, KeyCode::F(9), KeyModifiers::NONE, Some(&engine));
    assert_eq!(requests.try_recv().unwrap().params["location"], "test.c:1");
}

#[test]
fn selection_highlight_handles_tabs_unicode_comments_and_partial_tokens() {
    let line = "\t/* 中文 */ uart_cnt++;";
    let spans = highlight::selected_syntax(line, &mut false, Some((3, line.len() - 2)));
    assert_eq!(
        spans.iter().map(|s| s.content.as_ref()).collect::<String>(),
        line.replace('\t', "    ")
    );
    let selected = spans
        .iter()
        .filter(|s| s.style.bg == Some(theme::ACCENT))
        .map(|s| s.content.as_ref())
        .collect::<String>();
    assert_eq!(selected, &line[3..line.len() - 2]);
}

#[test]
fn keyboard_collapse_find_and_hidden_source_do_not_reuse_stale_selection() {
    let (engine, requests) = session::test_channel();
    let mut a = app(&["uart_cnt", "return value;"]);
    select(&mut a, 0, 0, 8);
    key(&mut a, KeyCode::Left, KeyModifiers::NONE, Some(&engine));
    assert_eq!(a.source_text.caret.byte, 0);
    assert!(a.source_text.range().is_none());
    select(&mut a, 0, 0, 8);
    a.command(Some(&engine), ":find value");
    assert_eq!(a.source_text.caret.row, 1);
    assert!(a.source_text.range().is_none());
    select(&mut a, 1, 7, 12);
    a.help = true;
    key(&mut a, KeyCode::Enter, KeyModifiers::CONTROL, Some(&engine));
    assert!(requests.try_recv().is_err());
    a.help = false;
    a.main_pane = 8;
    render(&mut a);
    assert!(a.source_text.area.is_empty());
    assert!(a.source_text.buttons.is_empty());
    assert!(!a.source_text_key(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
        Some(&engine)
    ));
    assert!(requests.try_recv().is_err());
}

#[test]
fn context_menu_and_keyboard_selection_work_in_narrow_layouts() {
    let mut a = app(&["uart_cnt"]);
    for (width, height) in [(45, 12), (80, 24), (120, 36)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| draw(f, &mut a)).unwrap();
        mouse(&mut a, MouseEventKind::Down(Right), 3, 0, None);
        terminal.draw(|f| draw(f, &mut a)).unwrap();
        assert!(
            a.source_text
                .menu_hits
                .iter()
                .all(|(r, _)| r.right() <= width && r.bottom() <= height)
        );
        key(&mut a, KeyCode::Char('c'), KeyModifiers::NONE, None);
        crate::clipboard::COPIED.with(|v| assert_eq!(*v.borrow(), "uart_cnt"));
        key(&mut a, KeyCode::Esc, KeyModifiers::NONE, None);
    }
}
