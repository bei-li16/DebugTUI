//! Stable core identity in navigation, source and inspector surfaces.
use super::*;

pub(super) fn color(index: usize) -> Color {
    let colors = [
        (217, 160, 105),
        (157, 190, 126),
        (193, 151, 197),
        (132, 176, 211),
        (217, 151, 156),
        (191, 191, 118),
    ];
    let (r, g, b) = colors[index % colors.len()];
    Color::Rgb(r, g, b)
}
fn mix(base: Color, accent: Color) -> Color {
    if let (Color::Rgb(r, g, b), Color::Rgb(ar, ag, ab)) = (base, accent) {
        Color::Rgb(
            ((r as u16 * 7 + ar as u16) / 8) as u8,
            ((g as u16 * 7 + ag as u16) / 8) as u8,
            ((b as u16 * 7 + ab as u16) / 8) as u8,
        )
    } else {
        base
    }
}
pub(super) fn tint(f: &mut UiFrame, a: &App, rect: Rect) {
    let Some(core) = &a.snapshot.core else {
        return;
    };
    let accent = color(core.index);
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            let cell = &mut f.buffer_mut()[(x, y)];
            if matches!(cell.bg, theme::PANEL | theme::CANVAS | theme::RAISED) {
                cell.set_bg(mix(cell.bg, accent));
            }
            if matches!(cell.fg, theme::BORDER | theme::ACCENT) {
                cell.set_fg(accent);
            }
        }
    }
}
pub(super) fn draw(f: &mut UiFrame, a: &mut App, rect: Rect) {
    a.core_hits.clear();
    let Some(active) = a.snapshot.core.as_ref().map(|c| c.index) else {
        return;
    };
    if rect.width < 16 {
        return;
    }
    let count = a.snapshot.cores.len();
    if count == 0 {
        return;
    }
    theme::surface(f, rect, theme::CANVAS);
    f.render_widget(
        Paragraph::new(" Cores ").style(Style::default().fg(color(active))),
        Rect::new(rect.x, rect.y, 7, 1),
    );
    let previous = Rect::new(rect.x + 7, rect.y, 3, 1);
    f.render_widget(
        Paragraph::new(" ‹ ").style(theme::chip(false, false)),
        previous,
    );
    a.core_hits.push((previous, (active + count - 1) % count));
    let next = Rect::new(rect.right() - 3, rect.y, 3, 1);
    f.render_widget(Paragraph::new(" › ").style(theme::chip(false, false)), next);
    a.core_hits.push((next, (active + 1) % count));
    let slots = ((rect.width.saturating_sub(13)) / 21).max(1) as usize;
    let start = active
        .saturating_sub(slots / 2)
        .min(count.saturating_sub(slots));
    let mut x = rect.x + 10;
    for core in a.snapshot.cores.iter().skip(start).take(slots) {
        let width = 20.min(next.x.saturating_sub(x));
        if width == 0 {
            break;
        }
        let badge = match core.state.as_str() {
            "RUNNING" => "RUN",
            "STOPPED" => "STOP",
            "READY" => "READY",
            _ => "OFF",
        };
        let label = format!(" {} · {badge} ", core.name);
        let hit = Rect::new(x, rect.y, width, 1);
        f.render_widget(
            Paragraph::new(label).style(
                Style::default()
                    .fg(color(core.index))
                    .bg(if core.index == active {
                        mix(theme::RAISED, color(core.index))
                    } else {
                        theme::PANEL
                    })
                    .add_modifier(if core.index == active {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
            hit,
        );
        a.core_hits.push((hit, core.index));
        x += width + 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn core_buttons_and_tint_follow_identity_in_narrow_and_wide_windows() {
        let (engine, rx) = session::test_channel();
        let mut a = App::new(Project::default(), false);
        a.snapshot.cores = (0..12)
            .map(|i| session::CoreStatus {
                index: i,
                name: format!("core{i}"),
                endpoint: i.to_string(),
                state: "STOPPED".into(),
            })
            .collect();
        a.snapshot.core = Some(a.snapshot.cores[8].clone());
        a.snapshot.state = "STOPPED".into();
        for width in [45, 80, 160] {
            let mut t = Terminal::new(TestBackend::new(width, 32)).unwrap();
            t.draw(|f| super::super::draw(f, &mut a)).unwrap();
            assert!(a.core_hits.iter().any(|(_, i)| *i == 8));
            assert!(a.core_hits.iter().all(|(r, _)| r.right() <= width));
            let hit = a.core_hits[1].0;
            a.mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(event::MouseButton::Left),
                    column: hit.x,
                    row: hit.y,
                    modifiers: KeyModifiers::NONE,
                },
                Some(&engine),
            );
            assert_eq!(rx.try_recv().unwrap().params["index"], 9);
        }
        assert_ne!(color(0), color(1));
    }
    #[test]
    fn core_switch_updates_source_assembly_registers_and_surface_color() {
        let mut a = App::new(Project::default(), true);
        a.snapshot.state = "STOPPED".into();
        a.snapshot.cores = (0..2)
            .map(|i| session::CoreStatus {
                index: i,
                name: format!("core{i}"),
                endpoint: format!("localhost:{}", 3333 + i),
                state: "STOPPED".into(),
            })
            .collect();
        let mut colors = vec![];
        for index in 0..2 {
            a.demo = true;
            let mut s = a.snapshot.clone();
            s.core = Some(s.cores[index].clone());
            s.frame.file = format!("demo/core{index}.c");
            s.frame.line = 2 + index as u32;
            s.registers = vec![Variable {
                name: "pc".into(),
                value: format!("0x800{index}000"),
                ..Default::default()
            }];
            s.assembly = vec![format!("core{index}: mov r0, r{index}")];
            a.update(Event::Snapshot {
                snapshot: Box::new(s),
            });
            a.demo = false;
            assert!(a.source_file.ends_with(&format!("core{index}.c")));
            assert_eq!(a.source_line, 1 + index);
            let mut t = Terminal::new(TestBackend::new(160, 45)).unwrap();
            t.draw(|f| super::super::draw(f, &mut a)).unwrap();
            colors.push(
                t.backend().buffer()[(a.source_rect.right() - 2, a.source_rect.bottom() - 1)].bg,
            );
            if let Ok(root) = std::env::var("DEBUGTUI_RENDER_DIR") {
                fs::create_dir_all(&root).unwrap();
                super::super::visual_tests::capture(
                    Path::new(&root),
                    &format!("core{index}"),
                    &mut a,
                    160,
                    45,
                );
            }
            a.select_pane(5);
            t.draw(|f| super::super::draw(f, &mut a)).unwrap();
            assert!(
                t.backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|c| c.symbol())
                    .collect::<String>()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .contains(&format!("core{index}: mov"))
            );
            a.select_pane(0);
        }
        assert_ne!(colors[0], colors[1]);
    }
    #[test]
    fn group_scope_labels_and_mixed_state_buttons_are_usable() {
        let mut p = Project {
            cores: vec![
                crate::config::Core::default(),
                crate::config::Core::default(),
            ],
            ..Default::default()
        };
        p.multicore.scope = crate::config::ControlScope::All;
        p.multicore.restart = vec!["monitor chipreset".into()];
        let mut a = App::new(p, false);
        a.snapshot.state = "STOPPED".into();
        a.snapshot.cores = (0..2)
            .map(|i| session::CoreStatus {
                index: i,
                name: format!("core.{i}"),
                endpoint: i.to_string(),
                state: if i == 0 { "STOPPED" } else { "RUNNING" }.into(),
            })
            .collect();
        a.snapshot.core = Some(a.snapshot.cores[0].clone());
        assert!(a.action_enabled("pause"));
        assert!(a.action_enabled("continue"));
        assert!(a.action_enabled("restart"));
        for width in [45, 80, 160] {
            let mut t = Terminal::new(TestBackend::new(width, 32)).unwrap();
            t.draw(|f| super::super::draw(f, &mut a)).unwrap();
            assert!(a.action_hits.iter().any(|(_, m)| *m == "scope-toggle"));
            assert!(a.action_hits.iter().all(|(r, _)| r.right() <= width));
            let text = t
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(text.contains("Scope: All"));
        }
        a.snapshot.control_scope = Some(crate::config::ControlScope::Core);
        assert!(!a.action_enabled("pause"));
        assert!(a.action_enabled("continue"));
    }
}
