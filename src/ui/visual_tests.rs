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
    a.snapshot.state = state::STOPPED.into();
    for (w, h) in [(45, 12), (80, 24), (120, 36), (180, 50)] {
        let t = terminal(&mut a, w, h);
        assert_eq!(
            a.action_hits
                .iter()
                .filter(|(_, command)| !matches!(*command, "watch-access" | "edit-value"))
                .count(),
            if h == 12 { 7 } else { 13 }
        );
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
        let buffer = t.backend().buffer();
        if h >= 36 {
            assert_eq!(hit.height, 3);
            assert_eq!(buffer[(hit.x, hit.y)].symbol(), "╭");
            assert_eq!(buffer[(hit.right() - 1, hit.y)].symbol(), "╮");
            assert_eq!(buffer[(hit.x, hit.bottom() - 1)].symbol(), "╰");
            assert_eq!(buffer[(hit.right() - 1, hit.bottom() - 1)].symbol(), "╯");
        } else {
            assert_eq!(hit.height, 1);
            assert_eq!(buffer[(hit.x, hit.y)].symbol(), "│");
            assert_eq!(buffer[(hit.right() - 1, hit.y)].symbol(), "│");
        }
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
        assert_eq!(
            hover.backend().buffer()[(hit.x + 1, hit.y + hit.height / 2)].bg,
            theme::BUTTON_HOVER
        );
        assert_eq!(
            hover.backend().buffer()[(hit.x, hit.y)].bg,
            buffer[(hit.x, hit.y)].bg
        );
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

#[test]
fn rounded_controls_keep_parent_background_during_hover_and_press_animation() {
    let mut a = App::new(Project::default(), true);
    a.fx.mode = crate::config::Motion::Full;
    let before = terminal(&mut a, 180, 50);
    let hit = a
        .action_hits
        .iter()
        .find(|(_, cmd)| *cmd == "continue")
        .unwrap()
        .0;
    a.pointer = Some(ratatui::layout::Position::new(hit.x, hit.y));
    a.fx.hover = Some((hit, Instant::now() - Duration::from_millis(80)));
    a.fx.pressed = Some(hit);
    a.fx.trigger("hover", 250);
    a.fx.trigger("press", 180);
    let animated = terminal(&mut a, 180, 50);
    for y in hit.y..hit.bottom() {
        for x in hit.x..hit.right() {
            if y == hit.y || y == hit.bottom() - 1 || x == hit.x || x == hit.right() - 1 {
                assert_eq!(
                    animated.backend().buffer()[(x, y)].bg,
                    before.backend().buffer()[(x, y)].bg
                );
            }
        }
    }
    assert_ne!(
        animated.backend().buffer()[(hit.x + 1, hit.y + 1)].bg,
        before.backend().buffer()[(hit.x + 1, hit.y + 1)].bg
    );
}

#[test]
fn wide_workspace_aligns_project_core_search_and_shared_execution_toolbar() {
    let mut a = App::new(Project::default(), true);
    a.snapshot.cores = (0..2)
        .map(|index| session::CoreStatus {
            index,
            name: format!("core.{index}"),
            endpoint: format!("localhost:{}", 3333 + index),
            state: state::STOPPED.into(),
        })
        .collect();
    a.snapshot.core = Some(a.snapshot.cores[0].clone());
    a.project.cores = vec![crate::config::Core::default(); 2];
    for width in [150, 180, 240] {
        let _ = terminal(&mut a, width, 50);
        let project = a
            .action_hits
            .iter()
            .find(|(_, cmd)| *cmd == "setup")
            .unwrap()
            .0;
        assert_eq!(project.y, a.symbol_search.bar.y);
        assert!(a.core_hits.iter().all(|(hit, _)| hit.y == project.y));
        assert!(
            a.core_hits
                .iter()
                .all(|(hit, _)| hit.x >= project.right() && hit.right() <= a.symbol_search.bar.x)
        );
        assert!(
            a.action_hits
                .iter()
                .filter(|(_, command)| !matches!(*command, "watch-access" | "edit-value"))
                .all(|(hit, _)| hit.bottom() <= a.source_rect.y)
        );
        let source_tab = a
            .pane_hits
            .iter()
            .find(|(_, pane)| *pane == pane::SOURCE)
            .unwrap()
            .0;
        let register_tab = a
            .pane_hits
            .iter()
            .find(|(_, pane)| *pane == pane::REGS)
            .unwrap()
            .0;
        assert_eq!(source_tab.y, register_tab.y);
        assert!(a.source_rect.height >= 20);
    }
    if let Ok(root) = std::env::var("DEBUGTUI_RENDER_DIR") {
        capture(Path::new(&root), "multicore", &mut a, 180, 50);
    }
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
    a.select_pane(pane::ASM);
    capture(root, "assembly", &mut a, 160, 42);
    a.open_setup();
    capture(root, "setup", &mut a, 140, 40);
    capture(root, "setup-narrow", &mut a, 80, 24);
    capture(root, "setup-compact", &mut a, 45, 12);
    a.setup = None;
    a.project.ui.animations = crate::config::Motion::Full;
    a.fx.mode = crate::config::Motion::Full;
    a.select_pane(pane::SOURCE);
    a.activate_source(0);
    a.source_top = 8;
    a.fx.request(400, method::STEP);
    let mut running = a.snapshot.clone();
    running.state = state::RUNNING.into();
    a.update(Event::Snapshot {
        snapshot: Box::new(running),
    });
    capture(root, "running", &mut a, 160, 42);
    let mut stopped = a.snapshot.clone();
    stopped.state = state::STOPPED.into();
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
    let item = a
        .formats
        .hits
        .iter()
        .find(|i| i.pane == pane::WATCH)
        .unwrap()
        .clone();
    a.open_format(Some(item));
    a.formats.index = 0;
    capture(root, "format-menu", &mut a, 110, 28);
}
