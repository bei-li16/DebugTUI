use super::*;

fn terminal(a: &mut App, width: u16, height: u16) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
    t.draw(|f| draw(f, a)).unwrap();
    t
}

#[test]
fn theme_preserves_hit_areas_focus_and_execution_context() {
    let (engine, requests) = session::test_channel();
    let mut a = App::new(Project::default(), true);
    // Static theme assertions are independent of transient hover interpolation.
    a.project.ui.animations = crate::config::Motion::Off;
    a.fx.mode = crate::config::Motion::Off;
    a.snapshot.state = "STOPPED".into();
    for (w, h) in [(45, 12), (80, 24), (120, 36), (180, 50)] {
        let t = terminal(&mut a, w, h);
        assert_eq!(a.action_hits.len(), if h == 12 { 6 } else { 12 });
        assert!(a.source_rect.height > 0);
        for (hit, _) in &a.action_hits {
            assert!(hit.right() <= w && hit.bottom() <= h);
        }
        assert!(a.console_input_rect.height > 0);
        let input = a.console_input_rect;
        a.editing = true;
        let focused = terminal(&mut a, w, h);
        assert_ne!(
            t.backend().buffer()[(input.x, input.y)].bg,
            focused.backend().buffer()[(input.x, input.y)].bg
        );
        a.editing = false;
        let hit = a
            .action_hits
            .iter()
            .find(|(_, cmd)| *cmd == "continue")
            .unwrap()
            .0;
        a.mouse(
            MouseEvent {
                kind: MouseEventKind::Moved,
                column: hit.x,
                row: hit.y,
                modifiers: KeyModifiers::NONE,
            },
            Some(&engine),
        );
        let hover = terminal(&mut a, w, h);
        assert_eq!(hover.backend().buffer()[(hit.x, hit.y)].bg, theme::HOVER);
        assert!(requests.try_recv().is_err()); // Hover feedback never sends a debug command.
        a.pointer = None;
    }
    let t = terminal(&mut a, 160, 45);
    let y = a.source_rect.y + a.snapshot.frame.line as u16 - 1 - a.source_top as u16;
    assert_eq!(
        t.backend().buffer()[(a.source_rect.right() - 2, y)].bg,
        theme::PC
    );
    a.palette = true;
    let t = terminal(&mut a, 160, 45);
    assert_eq!(t.backend().buffer()[(0, 0)].fg, theme::DIM);
    let row = a.palette_hits[0].0;
    assert_eq!(t.backend().buffer()[(row.x, row.y)].bg, theme::SELECTED);
    let right = row.right() - 2;
    assert_eq!(t.backend().buffer()[(right, row.y + 1)].symbol(), " ");
    assert_eq!(t.backend().buffer()[(right, row.y + 1)].bg, theme::PANEL);
}

// Optional color-preserving output for visual QA. Not included in runtime packages.
pub(super) fn capture(root: &Path, name: &str, a: &mut App, width: u16, height: u16) {
    let t = terminal(a, width, height);
    let cells: Vec<_> = t.backend().buffer().content.iter().map(|c| {
        let color = |color| match color { Color::Rgb(r,g,b) => format!("#{r:02x}{g:02x}{b:02x}"), _ => "inherit".into() };
        json!({"s":c.symbol(), "fg":color(c.fg), "bg":color(c.bg), "bold":c.modifier.contains(Modifier::BOLD)})
    }).collect();
    fs::write(
        root.join(format!("{name}.json")),
        serde_json::to_vec(&json!({"width":width,"height":height,"cells":cells})).unwrap(),
    )
    .unwrap();
}

#[test]
fn export_color_previews_when_requested() {
    let Ok(root) = std::env::var("DEBUGTUI_RENDER_DIR") else {
        return;
    };
    let root = Path::new(&root);
    fs::create_dir_all(root).unwrap();
    let mut a = App::new(Project::default(), true);
    a.load_source("Core/Src/main.c");
    a.load_source("BSP/Src/led.c");
    a.activate_source(0);
    a.snapshot.registers = (0..13)
        .map(|i| Variable {
            name: format!("r{i}"),
            value: format!("0x{:08x}", i * 256),
            changed: i == 2,
            ..Default::default()
        })
        .chain(a.snapshot.registers.clone())
        .collect();
    a.log("[gdb] Breakpoint 1, update_value at sample.c:18".into());
    a.log("[result 4] Target stopped. Registers and variables updated.".into());
    a.notice = "Breakpoint hit · sample.c:18".into();
    capture(root, "workspace", &mut a, 160, 42);
    capture(root, "narrow", &mut a, 80, 24);
    capture(root, "compact", &mut a, 45, 12);
    a.editing = true;
    a.input = "p/x counter".into();
    capture(root, "console", &mut a, 160, 42);
    a.editing = false;
    a.palette = true;
    capture(root, "commands", &mut a, 120, 36);
    a.palette = false;
    a.help = true;
    capture(root, "shortcuts", &mut a, 120, 36);
    a.help = false;
    a.open_source_list();
    capture(root, "files", &mut a, 120, 36);
    a.sources.list_open = false;
    a.select_pane(5);
    capture(root, "assembly", &mut a, 160, 42);
    a.open_setup();
    capture(root, "setup", &mut a, 140, 40);
    a.setup = None;
    a.project.ui.animations = crate::config::Motion::Full;
    a.fx.mode = crate::config::Motion::Full;
    a.select_pane(0);
    a.activate_source(0);
    a.source_top = 8;
    a.fx.request(400, "step");
    let mut running = a.snapshot.clone();
    running.state = "RUNNING".into();
    a.update(Event::Snapshot {
        snapshot: Box::new(running),
    });
    capture(root, "running", &mut a, 160, 42);
    let mut stopped = a.snapshot.clone();
    stopped.state = "STOPPED".into();
    stopped.generation += 1;
    stopped.frame.line = 19;
    stopped.stop_reason = "breakpoint-hit".into();
    stopped.watches[0].value = "12346".into();
    stopped.watches[0].changed = true;
    a.update(Event::Snapshot {
        snapshot: Box::new(stopped),
    });
    capture(root, "effects-initial", &mut a, 160, 42);
    for ms in [0, 80, 160, 240, 320, 400, 550, 750, 1000] {
        a.fx.age_for_preview(ms);
        capture(root, &format!("effects-{ms:04}"), &mut a, 160, 42);
    }
    a.formats.appearance = true;
    capture(root, "appearance", &mut a, 100, 25);
    a.formats.appearance = false;
    let item = a.formats.hits.iter().find(|i| i.pane == 1).unwrap().clone();
    a.open_format(Some(item));
    a.formats.index = 0;
    capture(root, "format-menu", &mut a, 110, 28);
}
