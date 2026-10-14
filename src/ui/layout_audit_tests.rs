//! Whole-workspace geometry and interaction checks, with optional color evidence.
use super::*;

const SIZES: &[(u16, u16)] = &[
    (45, 12),
    (60, 18),
    (80, 24),
    (100, 30),
    (120, 36),
    (160, 24),
    (180, 50),
    (80, 44),
    (240, 60),
];

fn render(a: &mut App, width: u16, height: u16) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
    t.draw(|f| draw(f, a)).unwrap();
    t
}

fn evidence(a: &mut App, name: &str, w: u16, h: u16) {
    if let Ok(root) = std::env::var("DEBUGTUI_RENDER_DIR") {
        fs::create_dir_all(&root).unwrap();
        visual_tests::capture(Path::new(&root), name, a, w, h);
    }
}

fn assert_rect(rect: Rect, w: u16, h: u16, label: &str) {
    if rect.width > 0 && rect.height > 0 {
        assert!(
            rect.right() <= w && rect.bottom() <= h,
            "{label}: {rect:?} outside {w}x{h}"
        );
    }
}

#[test]
fn layout_audit_workspace_geometry_and_keyboard_pane_reachability() {
    for &(w, h) in SIZES {
        let mut a = App::new(Project::default(), true);
        a.fx.mode = crate::config::Motion::Off;
        a.project.ui.animations = crate::config::Motion::Off;
        for zoom in [false, true] {
            a.zoom = zoom;
            for (order, &pane) in PANE_ORDER.iter().enumerate() {
                let shortcut = "1234567890-".chars().nth(order).unwrap();
                a.key(
                    KeyEvent::new(KeyCode::Char(shortcut), KeyModifiers::ALT),
                    None,
                );
                assert_eq!(a.pane, pane);
                let _ = render(&mut a, w, h);
                for (rect, command) in &a.action_hits {
                    assert_rect(*rect, w, h, command);
                }
                for (rect, p) in &a.pane_hits {
                    assert_rect(*rect, w, h, PANES[*p]);
                }
                for rect in a.view_rects.into_iter().chain(a.scrollbars) {
                    assert_rect(rect, w, h, "view");
                }
                for item in &a.formats.hits {
                    assert_rect(item.rect, w, h, "value");
                }
                assert_rect(a.watch_input_rect, w, h, "watch input");
                assert_rect(a.console_input_rect, w, h, "console input");
                assert!(
                    a.pane_hits
                        .iter()
                        .any(|(r, p)| *p == pane && r.width > 2 && r.height > 0),
                    "Selected {} tab missing at {w}x{h}, zoom={zoom}",
                    PANES[pane]
                );
                assert!(
                    a.view_rects[pane].height > 0 && a.view_rects[pane].width > 0,
                    "Selected {} has no data area at {w}x{h}, zoom={zoom}",
                    PANES[pane]
                );
                if !zoom && [(45, 12), (80, 24), (120, 36), (160, 24), (180, 50)].contains(&(w, h))
                {
                    evidence(&mut a, &format!("audit-pane-{pane}-{w}x{h}"), w, h);
                }
            }
        }
        for state in [state::READY, state::RUNNING, state::STOPPED, state::FAULT] {
            a.snapshot.state = state.into();
            a.select_pane(pane::SOURCE);
            a.zoom = false;
            a.snapshot.cores = (0..8)
                .map(|index| session::CoreStatus {
                    index,
                    name: format!("processor.{index}"),
                    endpoint: format!("localhost:{}", 3333 + index),
                    state: state.into(),
                })
                .collect();
            a.snapshot.core = a.snapshot.cores.last().cloned();
            let _ = render(&mut a, w, h);
            for (rect, _) in &a.core_hits {
                assert_rect(*rect, w, h, "core");
            }
            assert!(!a.core_hits.is_empty());
            if w == 80 || w == 180 {
                evidence(&mut a, &format!("audit-{state}-cores-{w}x{h}"), w, h);
            }
        }
        let _ = render(&mut a, 20, 6);
        assert!(a.action_hits.is_empty() && a.pane_hits.is_empty() && a.core_hits.is_empty());
        assert!(a.scrollbars.iter().all(|rect| rect.height == 0));
    }
}

#[test]
fn layout_audit_wrapped_peripheral_actions_remain_visible() {
    for (w, h) in [(100, 30), (120, 36), (160, 24), (180, 50)] {
        let mut a = App::new(Project::default(), true);
        a.select_pane(pane::PERIPHERALS);
        let _ = render(&mut a, w, h);
        let panel = a
            .write_editor
            .entry_hits
            .iter()
            .find(|(_, p)| *p == pane::PERIPHERALS)
            .unwrap()
            .0;
        for expected in ["peripheral-access", "peripheral-refresh", "edit-value"] {
            assert!(
                a.action_hits
                    .iter()
                    .any(|(r, cmd)| *cmd == expected && panel.contains((r.x, r.y).into())),
                "{expected} missing from peripheral toolbar at {w}x{h}"
            );
        }
        assert!(a.view_rects[pane::PERIPHERALS].height >= 2);
    }
}

#[test]
fn layout_audit_hover_feedback_does_not_select_or_send_commands() {
    let (engine, requests) = session::test_channel();
    for menu in ["commands", "appearance", "format"] {
        let mut a = App::new(Project::default(), true);
        a.fx.mode = crate::config::Motion::Off;
        let _ = render(&mut a, 120, 36);
        match menu {
            "commands" => a.palette = true,
            "appearance" => a.formats.appearance = true,
            _ => {
                let item = a
                    .formats
                    .hits
                    .iter()
                    .find(|i| i.pane == pane::WATCH)
                    .unwrap()
                    .clone();
                a.open_format(Some(item));
            }
        }
        let initial = render(&mut a, 120, 36);
        let rect = match menu {
            "commands" => a.palette_hits[2].0,
            "appearance" => a.formats.appearance_hits[2].0,
            _ => {
                a.formats
                    .menu_hits
                    .iter()
                    .find(|(_, index)| *index != a.formats.index)
                    .unwrap()
                    .0
            }
        };
        let point = (rect.x + 1, rect.y);
        let state = (
            a.pane,
            a.selection,
            a.palette_index,
            a.formats.index,
            a.project.ui.animations,
        );
        let before = a.pointer;
        a.mouse(
            MouseEvent {
                kind: MouseEventKind::Moved,
                column: point.0,
                row: point.1,
                modifiers: KeyModifiers::NONE,
            },
            Some(&engine),
        );
        assert!(
            !a.hover_unchanged(before),
            "{menu}: hover must request redraw"
        );
        let hovered = render(&mut a, 120, 36);
        assert_eq!(hovered.backend().buffer()[point].bg, theme::HOVER, "{menu}");
        assert_ne!(initial.backend().buffer()[point].bg, theme::HOVER);
        assert_eq!(
            state,
            (
                a.pane,
                a.selection,
                a.palette_index,
                a.formats.index,
                a.project.ui.animations
            )
        );
        assert!(requests.try_recv().is_err());
        evidence(&mut a, &format!("audit-hover-{menu}"), 120, 36);
    }
}

#[test]
fn layout_audit_modal_resize_and_escape() {
    for &(w, h) in SIZES {
        for command in [
            "appearance",
            "commandlist",
            "help",
            "memory-access",
            "watch-access",
            "break-new",
            "break-data",
            "symbols",
            "edit-value",
        ] {
            let mut a = App::new(Project::default(), true);
            a.snapshot.state = state::STOPPED.into();
            a.select_pane(pane::WATCH);
            let _ = render(&mut a, w, h);
            if command == "commandlist" {
                a.open_help(false);
            } else {
                a.command(None, &format!(":{command}"));
            }
            match command {
                "memory-access" => assert!(a.memory_panel.modal()),
                "watch-access" => assert!(a.monitor.modal()),
                "edit-value" => assert!(a.write_editor.modal()),
                "break-new" | "break-data" => assert!(a.breaks.modal()),
                "appearance" => assert!(a.formats.appearance),
                "symbols" => assert!(a.symbol_search.open),
                _ => assert!(a.help || a.palette),
            }
            let _ = render(&mut a, w, h);
            if [(45, 12), (80, 24), (120, 36)].contains(&(w, h)) {
                evidence(&mut a, &format!("audit-modal-{command}-{w}x{h}"), w, h);
            }
            for _ in 0..12 {
                a.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), None);
                let _ = render(&mut a, w, h);
            }
            a.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), None);
            assert!(!a.memory_panel.modal() && !a.write_editor.modal() && !a.monitor.modal());
        }
    }
}

#[test]
fn layout_audit_small_terminal_does_not_activate_hidden_dialog_controls() {
    let mut a = App::new(Project::default(), true);
    a.open_memory_access();
    let t = render(&mut a, 45, 12);
    let cancel = t
        .backend()
        .buffer()
        .content
        .chunks(45)
        .enumerate()
        .find_map(|(y, row)| {
            let text: String = row.iter().map(|cell| cell.symbol()).collect();
            text.find("Cancel").map(|x| (x as u16, y as u16))
        })
        .unwrap();
    let _ = render(&mut a, 44, 12);
    a.mouse(
        MouseEvent {
            kind: MouseEventKind::Down(event::MouseButton::Left),
            column: cancel.0,
            row: cancel.1,
            modifiers: KeyModifiers::NONE,
        },
        None,
    );
    assert!(
        a.memory_panel.modal(),
        "Hidden Cancel hit remained active after resize"
    );
    let _ = render(&mut a, 80, 24);
    assert!(a.memory_panel.modal());
    a.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), None);
    assert!(!a.memory_panel.modal());
}

#[test]
fn layout_audit_register_profiles_details_and_mpu_fit_supported_sizes() {
    let (engine, requests) = session::test_channel();
    for profile in ["stm32f429", "cortex-r52", "cortex-m7"] {
        for &(w, h) in SIZES {
            let mut project = Project::default();
            if profile == "stm32f429" {
                project.registers.catalogue =
                    Path::new(env!("CARGO_MANIFEST_DIR")).join("profiles/registers/stm32f429.toml");
            } else {
                project.registers.cpu = profile.into();
            }
            let mut a = App::new(project, false);
            a.snapshot.state = state::STOPPED.into();
            a.select_pane(pane::REGS);
            a.zoom = true;
            let _ = render(&mut a, w, h);
            assert!(a.register_view.enabled());
            assert!(a.view_rects[pane::REGS].height > 0, "{profile} at {w}x{h}");
            for command in [
                "register-status",
                "mpu",
                if profile == "cortex-m7" {
                    "cache"
                } else {
                    "register-status"
                },
            ] {
                a.command(Some(&engine), &format!(":{command}"));
                let t = render(&mut a, w, h);
                let text: String = t
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|c| c.symbol())
                    .collect();
                let title = match command {
                    "mpu" => "MPU regions",
                    "cache" => "M7 cache",
                    _ => "Register status",
                };
                assert!(text.contains(title), "{profile} {command} failed to open");
                if [(45, 12), (80, 24), (120, 36)].contains(&(w, h)) {
                    evidence(&mut a, &format!("audit-{profile}-{command}-{w}x{h}"), w, h);
                }
                a.key(
                    KeyEvent::new(KeyCode::End, KeyModifiers::NONE),
                    Some(&engine),
                );
                let _ = render(&mut a, w, h);
                a.key(
                    KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                    Some(&engine),
                );
            }
            assert!(
                requests.try_recv().is_err(),
                "Rendering and browsing must not access the target"
            );
        }
    }
}

#[test]
fn layout_audit_core_status_and_essential_footer_shortcuts_are_not_truncated() {
    for &(w, h) in SIZES {
        let mut a = App::new(Project::default(), true);
        a.snapshot.cores = vec![session::CoreStatus {
            index: 0,
            name: "很长的处理器名称.processor.0".into(),
            endpoint: "localhost:3333".into(),
            state: state::STOPPED.into(),
        }];
        a.snapshot.core = a.snapshot.cores.first().cloned();
        let t = render(&mut a, w, h);
        let footer: String = (0..w)
            .map(|x| t.backend().buffer()[(x, h - 1)].symbol())
            .collect();
        assert!(
            footer.contains("Ctrl+Q Exit")
                && footer.contains("? Help")
                && footer.contains("F2 Setup")
        );
        let core = a.core_hits.iter().find(|(r, _)| r.width > 3).unwrap().0;
        let status: String = (core.x..core.right())
            .map(|x| t.backend().buffer()[(x, core.y + core.height / 2)].symbol())
            .collect();
        assert!(
            status.contains("STOP"),
            "core status truncated at {w}x{h}: {status}"
        );
    }
}

#[test]
fn layout_audit_resize_keeps_the_visible_selection_in_view() {
    for pane in [pane::SOURCE, pane::WATCH, pane::REGS] {
        let mut project = Project::default();
        project.registers.catalogue =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("profiles/registers/stm32f429.toml");
        let mut a = App::new(project, true);
        a.snapshot.watches = (0..60)
            .map(|i| Variable {
                name: format!("value_{i}"),
                value: i.to_string(),
                ..Default::default()
            })
            .collect();
        a.select_pane(pane);
        a.zoom = true;
        let _ = render(&mut a, 120, 36);
        a.key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE), None);
        let _ = render(&mut a, 120, 36);
        let selected = if pane == pane::SOURCE {
            a.source_line
        } else {
            a.selected(pane)
        };
        for (w, h) in [(80, 24), (45, 12), (180, 50)] {
            let _ = render(&mut a, w, h);
            let top = if pane == pane::SOURCE {
                a.source_top
            } else {
                a.view_tops[pane]
            };
            let height = usize::from(a.view_rects[pane].height);
            assert!(
                (top..top + height).contains(&selected),
                "{} selection {selected} lost on resize to {w}x{h}: top {top}, height {height}",
                PANES[pane]
            );
        }
    }
}
