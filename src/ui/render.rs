use super::*;
use ratatui::widgets::Borders;

const ACTIONS: [(&str, &str); 10] = [
    ("▶ Run", "run"),
    ("▷ Continue", "continue"),
    ("Ⅱ Pause", "pause"),
    ("↺ Reset", "restart"),
    ("↔ Reconnect", "reconnect"),
    ("↓ Step In", "step"),
    ("→ Step Over", "next"),
    ("↑ Step Out", "finish"),
    ("× Exit", "quit"),
    ("? Help", "commandlist"),
];
const PROJECT_ACTIONS: [(&str, &str); 3] = [
    ("← Setup", "setup"),
    ("◆ Build", "build"),
    ("↓ Download", "download"),
];

fn project_bar(f: &mut UiFrame, a: &mut App, rect: Rect) {
    theme::surface(f, rect, theme::CANVAS);
    f.render_widget(
        Paragraph::new(" Project ").style(Style::default().fg(theme::DIM)),
        rect,
    );
    let buttons = Rect::new(rect.x + 9, rect.y, 35.min(rect.width.saturating_sub(9)), 1);
    toolbar(f, a, buttons, &PROJECT_ACTIONS);
    let (note, color) = a.fx.task_label(&a.snapshot.state).unwrap_or_else(|| {
        (
            "Setup: edit & restart · output in Console".into(),
            theme::DIM,
        )
    });
    if rect.width >= 85 {
        f.render_widget(
            Paragraph::new(note).style(Style::default().fg(color)),
            Rect::new(rect.x + 46, rect.y, rect.width - 46, 1),
        );
    }
}

fn wrapped_height(labels: &[&str], width: u16) -> u16 {
    let mut rows = 1;
    let mut x = 0;
    for label in labels {
        let len = unicode_width::UnicodeWidthStr::width(*label) as u16 + 3;
        if x > 0 && x + len > width {
            rows += 1;
            x = 0;
        }
        x += len;
    }
    rows
}

fn hovered(a: &App, rect: Rect) -> bool {
    a.pointer.is_some_and(|p| rect.contains(p))
}

fn tabs(f: &mut UiFrame, a: &mut App, rect: Rect, panes: &[usize], selected: usize) {
    theme::surface(f, rect, theme::PANEL);
    let mut x = rect.x;
    let mut y = rect.y;
    for &pane in panes {
        let label = format!(" {} ", PANES[pane]);
        let width = label.len() as u16;
        if x > rect.x && x + width > rect.right() {
            y += 1;
            x = rect.x;
        }
        if y >= rect.bottom() {
            break;
        }
        let hit = Rect::new(x, y, width.min(rect.right().saturating_sub(x)), 1);
        let style = theme::chip(pane == selected, hovered(a, hit));
        f.render_widget(Paragraph::new(label).style(style), hit);
        a.pane_hits.push((hit, pane));
        x += width + 1;
    }
}

fn toolbar(f: &mut UiFrame, a: &mut App, rect: Rect, actions: &[(&str, &'static str)]) {
    theme::surface(f, rect, theme::PANEL);
    let mut x = rect.x;
    let mut y = rect.y;
    for &(label, command) in actions {
        let text = format!(" {label} ");
        let width = unicode_width::UnicodeWidthStr::width(text.as_str()) as u16;
        if x > rect.x && x + width > rect.right() {
            y += 1;
            x = rect.x;
        }
        if y >= rect.bottom() {
            break;
        }
        let hit = Rect::new(x, y, width.min(rect.right().saturating_sub(x)), 1);
        let enabled = a.action_enabled(command);
        let (fg, bg) = if !enabled {
            (theme::DIM, theme::PANEL)
        } else if hovered(a, hit) {
            (theme::TEXT, theme::HOVER)
        } else if command == "continue" || command == "run" {
            (theme::GREEN, theme::PC)
        } else if command == "pause" {
            (theme::AMBER, theme::CHANGE)
        } else if command == "quit" {
            (theme::RED, theme::RAISED)
        } else {
            (theme::TEXT, theme::RAISED)
        };
        f.render_widget(
            Paragraph::new(text).style(Style::default().fg(fg).bg(bg)),
            hit,
        );
        a.action_hits.push((hit, command));
        x += width + 1;
    }
}

fn viewport(a: &mut App, pane: usize, rect: Rect) -> Rect {
    a.scrollbars[pane] = Rect::new(
        rect.right().saturating_sub(1),
        rect.y,
        u16::from(rect.width > 0),
        rect.height,
    );
    let inner = Rect {
        width: rect.width.saturating_sub(1),
        ..rect
    };
    a.view_rects[pane] = inner;
    inner
}

fn scrollbar(f: &mut UiFrame, a: &App, pane: usize) {
    let rect = a.scrollbars[pane];
    let (top, thumb, max) = a.scrollbar_thumb(pane);
    let track = (0..rect.height as usize)
        .map(|row| {
            let active = max > 0 && (top..top + thumb).contains(&row);
            Line::styled(
                if active { "┃" } else { "│" },
                Style::default().fg(if active { theme::ACCENT } else { theme::DIM }),
            )
        })
        .collect::<Vec<_>>();
    f.render_widget(Paragraph::new(track), rect);
}

fn view(f: &mut UiFrame, a: &mut App, pane: usize, rect: Rect) {
    let rect = viewport(a, pane, rect);
    let max = a.view_len(pane).saturating_sub(rect.height as usize);
    a.view_tops[pane] = if pane == 8 && a.log_follow {
        max
    } else {
        a.view_tops[pane].min(max)
    };
    let selection = a.selected(pane);
    let start = a.view_tops[pane];
    scrollbar(f, a, pane);
    if pane == peripherals::PANE {
        a.draw_peripherals(f, rect);
        return;
    }
    if let Some(error) = &a.view_errors[pane] {
        f.render_widget(
            Paragraph::new(format!("{error}\n\n:refresh retries this view."))
                .style(Style::default().fg(theme::AMBER))
                .wrap(Wrap { trim: false }),
            rect,
        );
        return;
    }
    if a.view_len(pane) == 0 {
        let (title, hint) = match pane {
            1 => (
                "No watches yet",
                "Enter a variable below, then click + Add.",
            ),
            9 => (
                "No locals in this frame",
                "Select a stopped frame to inspect its variables.",
            ),
            6 => ("No breakpoints", "+ Code / + Data above, or F9 in Source"),
            7 if !a.snapshot.files.is_empty() => (
                "No matching files",
                "Edit Find above; Ctrl+U clears the filter.",
            ),
            7 => (
                "Source files",
                "Connect to GDB to load the source file list.",
            ),
            2 => ("Call stack", "Pause the target to inspect its frames."),
            3 => ("Registers", "Connect and pause to inspect register values."),
            4 | 5 if a.snapshot.state == "RUNNING" => {
                ("Target running", "Pause the target to load this view.")
            }
            4 | 5 if a.snapshot.state == "STOPPED" => (
                "Reading from GDB…",
                "The view will update when the request completes.",
            ),
            4 | 5 => (
                "Waiting for target",
                "Connect and stop the target to load this view.",
            ),
            _ => ("Session log", "Debugger events will appear here."),
        };
        theme::empty(f, rect, title, hint);
        return;
    }
    if matches!(pane, 1 | 3 | 9) {
        a.numeric_view(f, pane, rect);
        return;
    }
    if pane == 4 {
        a.memory_view(f, rect);
        return;
    }
    let lines: Vec<Line> = match pane {
        2 => a
            .snapshot
            .stack
            .iter()
            .enumerate()
            .skip(start)
            .take(rect.height as usize)
            .map(|(i, s)| {
                Line::styled(
                    format!(
                        "{} #{} {} :{}",
                        if i == selection { "›" } else { " " },
                        s.level,
                        s.function,
                        s.line
                    ),
                    theme::selected(i == selection),
                )
            })
            .collect(),
        4 | 5 => {
            let values = if pane == 4 {
                &a.snapshot.memory
            } else {
                &a.snapshot.assembly
            };
            if values.is_empty() {
                let message = if a.snapshot.state == "RUNNING" {
                    "Pause the target to load this view."
                } else if a.snapshot.state == "STOPPED" {
                    "Loading from GDB…"
                } else {
                    "Connect and stop the target to load this view."
                };
                vec![Line::raw(message)]
            } else {
                values
                    .iter()
                    .skip(start)
                    .take(rect.height as usize)
                    .map(|s| {
                        let current = pane == 5
                            && s.split_whitespace().next().is_some_and(|address| {
                                u64::from_str_radix(address.trim_start_matches("0x"), 16)
                                    .ok()
                                    .zip(
                                        u64::from_str_radix(
                                            a.snapshot.frame.address.trim_start_matches("0x"),
                                            16,
                                        )
                                        .ok(),
                                    )
                                    .is_some_and(|(left, right)| left == right)
                            });
                        data_line(s, current, pane == 5)
                    })
                    .collect()
            }
        }
        6 => breakpoints::rows(a, start, rect.height as usize),
        7 => a
            .filtered_files()
            .into_iter()
            .enumerate()
            .skip(start)
            .take(rect.height as usize)
            .map(|(i, index)| {
                Line::styled(
                    format!(
                        "{} {}",
                        if i == selection { "›" } else { " " },
                        source_tabs::fit(
                            &a.snapshot.files[index],
                            rect.width.saturating_sub(2) as usize
                        )
                    ),
                    theme::selected(i == selection),
                )
            })
            .collect(),
        _ => a
            .logs
            .iter()
            .skip(start)
            .take(rect.height as usize)
            .map(|s| log_line(s))
            .collect(),
    };
    theme::lines(f, lines, rect);
}

fn source(f: &mut UiFrame, a: &mut App, rect: Rect) {
    a.source_rect = viewport(a, 0, rect);
    let max = a.source.len().saturating_sub(rect.height as usize);
    a.source_top = a.source_top.min(max);
    if rect.height > 0 {
        if a.source_line < a.source_top {
            a.source_top = a.source_line;
        }
        if a.source_line >= a.source_top + rect.height as usize {
            a.source_top = (a.source_line + 1)
                .saturating_sub(rect.height as usize)
                .min(max);
        }
    }
    if a.source.is_empty() {
        scrollbar(f, a, 0);
        let message = if a.sources.active.is_some() {
            "This source file is empty.".into()
        } else if a.snapshot.state == "STOPPED"
            && a.snapshot.frame.file.is_empty()
            && !a.snapshot.frame.address.is_empty()
        {
            format!(
                "No source location for PC {} ({}).\n\nInspect Asm / Regs and the GDB / server log.\nStep In / Step Over need function debug information.\nUse :stepi only when the address contains valid code.\nFor an unexpected address, check target state before continuing.",
                a.snapshot.frame.address, a.snapshot.frame.function
            )
        } else {
            "No source file selected.\n\nChoose an open tab, Files, or :open PATH.\nA pause or breakpoint opens its source file.\nF2 selects a project and debugging environment.".into()
        };
        f.render_widget(
            Paragraph::new(message).wrap(Wrap { trim: false }),
            a.source_rect,
        );
        return;
    }
    let current_file = a.source_is_frame();
    let source_key = a.source_key(&a.source_file);
    let breakpoint_lines: HashSet<usize> = a
        .snapshot
        .breakpoints
        .iter()
        .filter(|b| b.enabled && a.source_key(&b.file) == source_key)
        .map(|b| b.line as usize)
        .collect();
    let disabled_lines: HashSet<usize> = a
        .snapshot
        .breakpoints
        .iter()
        .filter(|b| !b.enabled && a.source_key(&b.file) == source_key)
        .map(|b| b.line as usize)
        .collect();
    let gutter = source_text::gutter(a.source.len()).min(a.source_rect.width);
    a.source_text.area = Rect::new(
        a.source_rect.x + gutter,
        a.source_rect.y,
        a.source_rect.width.saturating_sub(gutter),
        a.source_rect.height,
    );
    let range = a.source_text.range();
    for (row, (i, line)) in a
        .source
        .iter()
        .enumerate()
        .skip(a.source_top)
        .take(rect.height as usize)
        .enumerate()
    {
        let pc = i + 1 == a.snapshot.frame.line as usize && current_file;
        let selected = i == a.source_line;
        let bp = breakpoint_lines.contains(&(i + 1));
        let bg = if pc {
            theme::PC
        } else if selected {
            theme::SELECTED
        } else {
            theme::PANEL
        };
        let y = a.source_rect.y + row as u16;
        let prefix = format!(
            "{}{}{:>digits$} │ ",
            if pc {
                "▶"
            } else if selected {
                "›"
            } else {
                " "
            },
            if bp {
                "●"
            } else if disabled_lines.contains(&(i + 1)) {
                "○"
            } else {
                " "
            },
            i + 1,
            digits = source_text::gutter(a.source.len()).saturating_sub(5) as usize
        );
        f.render_widget(
            Paragraph::new(prefix).style(Style::default().bg(theme::CANVAS).fg(if pc {
                theme::GREEN
            } else if bp {
                theme::RED
            } else if selected {
                theme::ACCENT
            } else {
                theme::DIM
            })),
            Rect::new(a.source_rect.x, y, gutter, 1),
        );
        let mut comment = a.source_comments.get(i).copied().unwrap_or(false);
        let selection = range.and_then(|(start, end)| {
            (i >= start.row && i <= end.row).then_some((
                if i == start.row { start.byte } else { 0 },
                if i == end.row { end.byte } else { line.len() },
            ))
        });
        let text_rect = Rect::new(a.source_text.area.x, y, a.source_text.area.width, 1);
        f.buffer_mut().set_style(text_rect, Style::default().bg(bg));
        f.render_widget(
            Paragraph::new(Line::from(highlight::selected_syntax(
                line,
                &mut comment,
                selection,
            )))
            .style(Style::default().bg(bg))
            .scroll((0, a.source_text.left.min(u16::MAX as usize) as u16)),
            text_rect,
        );
        if a.pane == 0
            && !a.input_active()
            && !a.console_view.focused
            && range.is_none()
            && a.source_text.caret.row == i
        {
            let col = source_text::column(line, a.source_text.caret.byte);
            if col >= a.source_text.left && col - a.source_text.left < text_rect.width as usize {
                f.buffer_mut()[(text_rect.x + (col - a.source_text.left) as u16, y)]
                    .set_style(Style::default().add_modifier(Modifier::UNDERLINED));
            }
        }
    }
    scrollbar(f, a, 0);
}

fn main_panel(f: &mut UiFrame, a: &mut App, rect: Rect) {
    theme::surface(f, rect, theme::PANEL);
    // Keep source visible in very short terminals; every action remains in Help.
    let compact = [ACTIONS[1], ACTIONS[2], ACTIONS[8], ACTIONS[9]];
    let mut actions = if rect.height < 8 {
        compact.to_vec()
    } else {
        ACTIONS.to_vec()
    };
    if !a.project.cores.is_empty() {
        let group = a.group_control();
        for (label, command) in &mut actions {
            *label = match *command {
                "run" => "▶ Run All",
                "continue" => {
                    if group {
                        "▷ Continue All"
                    } else {
                        "▷ Continue Core"
                    }
                }
                "pause" => {
                    if group {
                        "Ⅱ Pause All"
                    } else {
                        "Ⅱ Pause Core"
                    }
                }
                "restart" if !a.project.multicore.restart.is_empty() => "↺ Reset Chip",
                "step" => "↓ Step Core",
                "next" => "→ Next Core",
                "finish" => "↑ Finish Core",
                _ => label,
            };
        }
        actions.insert(
            0,
            (
                if group { "Scope: All" } else { "Scope: Core" },
                "scope-toggle",
            ),
        );
    }
    let labels: Vec<_> = actions.iter().map(|(name, _)| *name).collect();
    let toolbar_height = wrapped_height(&labels, rect.width);
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(toolbar_height),
        Constraint::Min(1),
    ])
    .split(rect);
    tabs(f, a, rows[0], &MAIN_PANES, a.main_pane);
    toolbar(f, a, rows[1], &actions);
    source_text::buttons(f, a, rows[0]);
    let title = match a.main_pane {
        0 => String::new(),
        5 => " Assembly · follows $pc · :disasm ADDRESS ".into(),
        7 => format!(
            " Files · {} / {} · Enter opens ",
            a.filtered_files().len(),
            a.snapshot.files.len()
        ),
        _ => if a.log_follow {
            " Log · following output "
        } else {
            " Log · scrollback · End follows output "
        }
        .into(),
    };
    let block = section(title).border_style(Style::default().fg(
        if MAIN_PANES.contains(&a.pane) && !a.input_active() {
            theme::ACCENT
        } else {
            theme::BORDER
        },
    ));
    let inner = block.inner(rows[2]);
    f.render_widget(block, rows[2]);
    if a.main_pane == 0 {
        source_tabs::draw_tabs(
            f,
            a,
            Rect {
                height: rows[2].height.min(1),
                ..rows[2]
            },
        );
        source(f, a, inner);
    } else if a.main_pane == 7 {
        let height = if inner.height >= 10 { 3 } else { 1 };
        let rows = Layout::vertical([Constraint::Length(height), Constraint::Min(0)]).split(inner);
        search::file_bar(f, a, rows[0]);
        view(f, a, 7, rows[1]);
        a.source_rect = a.view_rects[7];
    } else {
        view(f, a, a.main_pane, inner);
        a.source_rect = a.view_rects[a.main_pane];
    }
}

fn side_panel(f: &mut UiFrame, a: &mut App, rect: Rect, compact: bool) {
    theme::surface(f, rect, theme::PANEL);
    if (compact || rect.height <= 12) && VARIABLE_PANES.contains(&a.pane) {
        variable_panel(f, a, rect);
        return;
    }
    let labels = SIDE_PANES.map(|i| PANES[i]);
    let tab_height = wrapped_height(&labels, rect.width);
    let local_height = if !compact && rect.height > 12 {
        let height = (rect.height / 3).clamp(5, 12);
        // Reserve the input frame's two border rows without hiding Watch values.
        height + if height >= 8 { 2 } else { 0 }
    } else {
        0
    };
    let rows = Layout::vertical([
        Constraint::Length(tab_height),
        Constraint::Min(1),
        Constraint::Length(local_height),
    ])
    .split(rect);
    tabs(f, a, rows[0], &SIDE_PANES, a.side_pane);
    let title = match a.side_pane {
        2 => " Stack · click / Enter selects frame ",
        3 => " System registers ",
        4 => " Memory · :memory ADDRESS [COUNT] ",
        _ => " Breakpoints · Space toggle · e edit ",
    };
    let title = if a.side_pane == peripherals::PANE {
        a.peripheral_title()
    } else {
        title.to_owned()
    };
    let block = section(title).border_style(Style::default().fg(
        if SIDE_PANES.contains(&a.pane) && !a.input_active() {
            theme::ACCENT
        } else {
            theme::BORDER
        },
    ));
    let mut inner = block.inner(rows[1]);
    f.render_widget(block, rows[1]);
    if a.side_pane == 6 && inner.height > 2 {
        let labels: Vec<_> = breakpoints::ACTIONS
            .iter()
            .map(|(label, _)| *label)
            .collect();
        let height = wrapped_height(&labels, inner.width).min(inner.height.saturating_sub(2));
        toolbar(
            f,
            a,
            Rect::new(inner.x, inner.y, inner.width, height),
            breakpoints::ACTIONS,
        );
        inner.y += height;
        inner.height -= height;
        let footer = Rect::new(inner.x, inner.bottom() - 1, inner.width, 1);
        f.render_widget(
            Paragraph::new(breakpoints::detail(a)).style(Style::default().fg(theme::MUTED)),
            footer,
        );
        inner.height -= 1;
    }
    if a.side_pane == peripherals::PANE && inner.height > 1 {
        toolbar(
            f,
            a,
            Rect::new(inner.x, inner.y, inner.width, 1),
            &[("↻ Refresh selected", "peripheral-refresh")],
        );
        inner.y += 1;
        inner.height -= 1;
    }
    view(f, a, a.side_pane, inner);
    a.side_rect = a.view_rects[a.side_pane];
    if local_height > 0 {
        variable_panel(f, a, rows[2]);
    }
}

fn variable_panel(f: &mut UiFrame, a: &mut App, rect: Rect) {
    theme::surface(f, rect, theme::PANEL);
    let divider = section("");
    let inner = divider.inner(rect);
    f.render_widget(divider, rect);
    let watch = a.variable_pane == 1;
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(if !watch {
            0
        } else if inner.height >= 9 {
            3
        } else {
            1
        }),
    ])
    .split(inner);
    let remove_width = if watch && rows[0].width >= 30 { 12 } else { 0 };
    let tab_area = Rect {
        width: rows[0].width.saturating_sub(remove_width),
        ..rows[0]
    };
    tabs(f, a, tab_area, &VARIABLE_PANES, a.variable_pane);
    if remove_width > 0 {
        let hit = Rect::new(
            rows[0].right() - remove_width,
            rows[0].y,
            remove_width,
            rows[0].height,
        );
        a.watch.remove_rect = hit;
        let enabled =
            a.watch_removable() && a.watch.pending_remove.is_none() && a.pending_task.is_none();
        f.render_widget(
            Paragraph::new(" Del Remove ").style(
                Style::default()
                    .fg(if enabled { theme::TEXT } else { theme::DIM })
                    .bg(if enabled && hovered(a, hit) {
                        theme::HOVER
                    } else {
                        theme::PANEL
                    }),
            ),
            hit,
        );
    }
    view(f, a, a.variable_pane, rows[1]);
    if watch {
        let add_width = 9.min(rows[2].width);
        a.watch_input_rect = Rect {
            width: rows[2].width.saturating_sub(add_width),
            ..rows[2]
        };
        input_box(
            f,
            a.watch_input_rect,
            "Watch expression",
            &a.watch_input,
            a.watch_editing,
            "Variable / expression...",
            "Tab complete · Enter add",
        );
        let hit = Rect::new(
            rows[2].right() - add_width,
            rows[2].y,
            add_width,
            rows[2].height,
        );
        a.watch.add_rect = hit;
        let enabled = a.pending_watch.is_none() && a.pending_task.is_none();
        let button = Block::bordered()
            .border_type(ratatui::widgets::BorderType::Rounded)
            .borders(if hit.height >= 3 {
                Borders::ALL
            } else {
                Borders::LEFT | Borders::RIGHT
            })
            .border_style(Style::default().fg(if enabled {
                theme::ACCENT
            } else {
                theme::BORDER
            }))
            .style(Style::default().bg(if enabled && hovered(a, hit) {
                theme::HOVER
            } else {
                theme::RAISED
            }));
        let button_inner = button.inner(hit);
        f.render_widget(button, hit);
        f.render_widget(
            Paragraph::new(" + Add ").style(
                Style::default()
                    .fg(if enabled { theme::ACCENT } else { theme::DIM })
                    .bg(if enabled && hovered(a, hit) {
                        theme::HOVER
                    } else {
                        theme::RAISED
                    })
                    .add_modifier(Modifier::BOLD),
            ),
            button_inner,
        );
    }
}

/// Give editable fields a visible boundary even before they receive focus.
/// A one-row version keeps all controls usable in small terminals.
pub(super) fn input_box(
    f: &mut UiFrame,
    area: Rect,
    label: &str,
    text: &str,
    focused: bool,
    placeholder: &str,
    hint: &str,
) {
    let full = area.height >= 3;
    let background = if focused {
        theme::SELECTED
    } else {
        theme::RAISED
    };
    let mut border = Block::bordered()
        .border_type(ratatui::widgets::BorderType::Rounded)
        .borders(if full {
            Borders::ALL
        } else {
            Borders::LEFT | Borders::RIGHT
        })
        .border_style(Style::default().fg(if focused { theme::ACCENT } else { theme::MUTED }))
        .style(Style::default().bg(background));
    if full {
        border = border.title(format!(" {label} ")).title_style(
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        );
    }
    let inner = border.inner(area);
    f.render_widget(border, area);
    let prefix = if full {
        " ".into()
    } else {
        format!(" {label}{} ", if label.contains(':') { "" } else { ":" })
    };
    input_line(f, inner, &prefix, text, focused, placeholder, hint);
    // Keep hints legible and distinguish the input surface from the surrounding panel.
    for y in inner.y..inner.bottom() {
        for x in inner.x..inner.right() {
            let cell = &mut f.buffer_mut()[(x, y)];
            cell.bg = background;
            if cell.fg == theme::DIM {
                cell.fg = theme::MUTED;
            }
        }
    }
}

pub(super) fn input_line(
    f: &mut UiFrame,
    area: Rect,
    prefix: &str,
    text: &str,
    focused: bool,
    placeholder: &str,
    hint: &str,
) {
    theme::surface(f, area, if focused { theme::RAISED } else { theme::PANEL });
    let hint_width = if area.width >= 85 {
        hint.len() as u16 + 1
    } else {
        0
    };
    let available = area
        .width
        .saturating_sub(prefix.len() as u16 + 1 + hint_width) as usize;
    let mut start = 0;
    let mut width = unicode_width::UnicodeWidthStr::width(text);
    for (index, ch) in text.char_indices() {
        if width <= available {
            break;
        }
        width = width.saturating_sub(unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0));
        start = index + ch.len_utf8();
    }
    let empty = text.is_empty();
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                prefix,
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                if empty { placeholder } else { &text[start..] },
                Style::default().fg(if empty { theme::DIM } else { theme::TEXT }),
            ),
        ])),
        Rect {
            width: area.width.saturating_sub(hint_width),
            ..area
        },
    );
    if hint_width > 0 {
        f.render_widget(
            Paragraph::new(hint).style(Style::default().fg(theme::DIM)),
            Rect::new(area.right() - hint_width, area.y, hint_width, 1),
        );
    }
    if focused && area.width > 0 && area.height > 0 {
        f.set_cursor_position((
            area.x + (prefix.len() as u16 + width as u16).min(area.width - 1),
            area.y,
        ));
    }
}

fn completion_popup(f: &mut UiFrame, a: &mut App) {
    if !a.input_active() || a.help || a.palette || a.confirm.is_some() || a.sources.list_open {
        return;
    }
    let anchor = if a.watch_editing {
        a.watch_input_rect
    } else {
        a.console_input_rect
    };
    if anchor.height == 0 || anchor.y < 4 || a.completion.items.is_empty() {
        return;
    }
    let count = a.completion.items.len();
    let visible = count.min(6).min(anchor.y.saturating_sub(3) as usize);
    let width = (a
        .completion
        .items
        .iter()
        .map(|s| unicode_width::UnicodeWidthStr::width(s.as_str()))
        .max()
        .unwrap_or(0) as u16
        + 4)
    .clamp(38, 90)
    .min(anchor.width);
    let height = visible as u16 + 3;
    let rect = Rect::new(anchor.x, anchor.y - height, width, height);
    a.completion.area = rect;
    theme::surface(f, rect, theme::PANEL);
    let block = theme::card(format!(" {} suggestions ", count), true);
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let start = a
        .completion
        .selected
        .saturating_sub(visible.saturating_sub(1));
    for (row, (index, text)) in a
        .completion
        .items
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .enumerate()
    {
        let hit = Rect::new(inner.x, inner.y + row as u16, inner.width, 1);
        let selected = index == a.completion.selected;
        f.render_widget(
            Paragraph::new(format!(" {}", text)).style(
                Style::default()
                    .bg(if selected {
                        theme::SELECTED
                    } else {
                        theme::PANEL
                    })
                    .fg(if selected { theme::TEXT } else { theme::MUTED }),
            ),
            hit,
        );
        a.completion.hits.push((hit, index));
    }
    f.render_widget(
        Paragraph::new(" ↑↓ select · Tab / click fill").style(Style::default().fg(theme::DIM)),
        Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
    );
}

fn console_panel(f: &mut UiFrame, a: &mut App, rect: Rect) {
    let block = section("  ›_ Console  ");
    theme::surface(f, rect, theme::CANVAS);
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let input_height = if rect.height >= 6 { 3 } else { 1 };
    let rows =
        Layout::vertical([Constraint::Min(0), Constraint::Length(input_height)]).split(inner);
    a.console_view.layout(rows[0], a.console.len());
    let latest = if a.console_view.follow {
        " LIVE · Latest ".into()
    } else {
        format!(" Latest (+{}) ", a.console_view.unread)
    };
    let button_width = (latest.len() as u16).min(rect.width.saturating_sub(16));
    a.console_view.latest = Rect::new(
        rect.right().saturating_sub(button_width),
        rect.y,
        button_width,
        rect.height.min(1),
    );
    if rect.width >= 75 {
        let status = if a.console_view.follow {
            "GDB / :commands · Shift+PgUp history".into()
        } else {
            format!(
                "History {}–{} / {} · End: latest",
                a.console_view.top + 1,
                (a.console_view.top + rows[0].height as usize).min(a.console.len()),
                a.console.len()
            )
        };
        f.render_widget(
            Paragraph::new(status).style(Style::default().fg(theme::DIM)),
            Rect::new(
                rect.x + 16,
                rect.y,
                rect.width.saturating_sub(button_width + 17),
                1,
            ),
        );
    }
    f.render_widget(
        Paragraph::new(latest).style(
            Style::default()
                .fg(if a.console_view.follow {
                    theme::GREEN
                } else {
                    theme::ACCENT
                })
                .bg(theme::RAISED),
        ),
        a.console_view.latest,
    );
    let lines = a
        .console
        .iter()
        .skip(a.console_view.top)
        .take(rows[0].height as usize)
        .map(|line| log_line(line))
        .collect::<Vec<_>>();
    f.render_widget(Paragraph::new(lines), a.console_view.rect);
    let (top, size, max) = a.console_view.thumb(a.console.len());
    let track = (0..a.console_view.bar.height as usize)
        .map(|row| {
            let active = max > 0 && (top..top + size).contains(&row);
            let glyph = if a.project.ui.unicode {
                if active { "┃" } else { "│" }
            } else if active {
                "#"
            } else {
                "|"
            };
            Line::styled(
                glyph,
                Style::default().fg(if active { theme::ACCENT } else { theme::DIM }),
            )
        })
        .collect::<Vec<_>>();
    f.render_widget(Paragraph::new(track), a.console_view.bar);
    let input_area = if input_height == 3 {
        let block = theme::card("", a.editing || hovered(a, rows[1]));
        let input = block.inner(rows[1]);
        f.render_widget(block, rows[1]);
        input
    } else {
        rows[1]
    };
    a.console_input_rect = input_area;
    input_line(
        f,
        input_area,
        " gdb> ",
        &a.input,
        a.editing,
        "GDB command or :action…",
        "Tab complete · Enter send",
    );
}

fn log_line(text: &str) -> Line<'static> {
    let (clock, text) = text
        .split_once("] ")
        .filter(|(prefix, _)| prefix.len() == 13 && prefix.as_bytes().get(3) == Some(&b':'))
        .map(|(clock, text)| (format!("{clock}] "), text))
        .unwrap_or_else(|| (String::new(), text));
    let (label, body) = text
        .split_once(']')
        .filter(|(label, _)| label.starts_with('['))
        .map(|(label, body)| (format!("{label}]"), body.to_owned()))
        .unwrap_or_else(|| (String::new(), text.to_owned()));
    let color = if label.contains("error") || body.starts_with("Error:") {
        theme::RED
    } else if text.starts_with('>') {
        theme::ACCENT
    } else if label.contains("result") {
        theme::GREEN
    } else {
        theme::DIM
    };
    Line::from(vec![
        Span::raw(" "),
        Span::styled(clock, Style::default().fg(theme::DIM)),
        Span::styled(label, Style::default().fg(color)),
        Span::styled(
            body,
            Style::default().fg(if color == theme::RED {
                theme::RED
            } else {
                theme::MUTED
            }),
        ),
    ])
}

fn data_line(text: &str, current: bool, assembly: bool) -> Line<'static> {
    let (address, rest) = text.split_once(' ').unwrap_or((text, ""));
    let mut spans = vec![
        Span::styled(
            if current { "▶ " } else { "  " },
            Style::default().fg(theme::GREEN),
        ),
        Span::styled(address.to_owned(), Style::default().fg(theme::DIM)),
        Span::raw("  "),
    ];
    if assembly {
        let (op, args) = rest
            .trim_start()
            .split_once(' ')
            .unwrap_or((rest.trim(), ""));
        spans.push(Span::styled(
            format!("{op:8}"),
            Style::default().fg(theme::VIOLET),
        ));
        spans.push(Span::styled(
            args.to_owned(),
            Style::default().fg(theme::TEXT),
        ));
    } else {
        spans.push(Span::styled(
            rest.to_owned(),
            Style::default().fg(theme::ACCENT),
        ));
    }
    Line::from(spans).style(Style::default().bg(if current { theme::PC } else { theme::PANEL }))
}

fn hint(command: &str) -> &str {
    match command.split_whitespace().next().unwrap_or("") {
        "setup" => "Select project / environment",
        "connect" => "Connect debugger",
        "reconnect" => "Disconnect, then connect again",
        "scope" | "scope-toggle" => "Continue/Pause: all cores or current core",
        "run" => "Start using the environment's run action",
        "continue" => "Resume execution (F5)",
        "pause" => "Stop execution (F6)",
        "step" => "Step In: enter a function (F11)",
        "next" => "Step Over: execute the current line (F10)",
        "stepi" => "Step one instruction",
        "finish" => "Step Out: return to the caller (Shift+F11)",
        "restart" => "Reset using the environment action",
        "download" => "Download: Source root script / tools GDB action (F2 config)",
        "disconnect" => "Disconnect and release owned processes",
        "watch" => "Add a watched expression",
        "unwatch" => "Remove a watched expression",
        "break" => "Set a source / function breakpoint",
        "data-break" => "Set a hardware watchpoint",
        "delete" => "Delete breakpoint by number",
        "memory" => "Read memory bytes",
        "disasm" => "Disassemble at address (default $pc)",
        "files" => "List source files",
        "symbols" => "Search functions, variables and types; open source location",
        "open" => "Open local source",
        "frame" => "Select a stack frame",
        "find" => "Find text in source",
        "elf" => "Load symbols from ELF",
        "refresh" => "Refresh stopped views / retry loading",
        "build" => "Build: run configured command in Source root (F2 config)",
        "appearance" => "Animation intensity and compatible effect glyphs",
        "animations" => "Choose off, subtle or full animation",
        "format" => "Format selected value: binary / octal / decimal / hex",
        "help" => "Keyboard and command help",
        "commandlist" => "Help: commands and keyboard shortcuts (Ctrl+P)",
        "quit" => "End session and close TUI (Ctrl+Q)",
        _ => "",
    }
}

fn header(f: &mut UiFrame, a: &App, rect: Rect) {
    let state = a.snapshot.state.as_str();
    let color = if state == "FAULT" {
        theme::RED
    } else if state == "RUNNING" {
        theme::GREEN
    } else {
        theme::ACCENT
    };
    let busy = !a.pending_commands.is_empty() || a.pending_view.is_some();
    let icon = a.fx.glyph(a.project.ui.unicode, state, busy);
    theme::surface(f, Rect { height: 1, ..rect }, theme::RAISED);
    let badge = format!(" {icon} {state} ");
    let width = unicode_width::UnicodeWidthStr::width(badge.as_str()) as u16;
    let brand_width = rect.width.saturating_sub(width);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" ◈ DebugTUI v{}", env!("CARGO_PKG_VERSION")),
                Style::default()
                    .fg(theme::TEXT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                if a.demo {
                    "  /  PREVIEW"
                } else {
                    "  /  DEBUG WORKSPACE"
                },
                Style::default().fg(theme::DIM),
            ),
        ])),
        Rect::new(rect.x, rect.y, brand_width, 1),
    );
    f.render_widget(
        Paragraph::new(badge).style(
            Style::default()
                .fg(color)
                .bg(if state.contains("STOPPED") {
                    theme::PC
                } else {
                    theme::RAISED
                })
                .add_modifier(Modifier::BOLD),
        ),
        Rect::new(
            rect.right().saturating_sub(width),
            rect.y,
            width.min(rect.width),
            1,
        ),
    );
    if rect.height > 1 {
        let location = if let Some(stage) = a.fx.connection.filter(|stage| *stage < 3) {
            let names = ["Environment", "GDB", "Target", "Ready"];
            names
                .iter()
                .enumerate()
                .map(|(i, n)| {
                    format!(
                        "{} {n}",
                        if i < stage {
                            "+"
                        } else if i == stage {
                            ">"
                        } else {
                            "·"
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join("  /  ")
        } else if a.snapshot.frame.file.is_empty() && !a.snapshot.frame.address.is_empty() {
            format!(
                "PC {} · {} · no source location",
                a.snapshot.frame.address, a.snapshot.frame.function
            )
        } else if a.snapshot.frame.file.is_empty() {
            "Choose a project with F2 to begin".into()
        } else {
            format!(
                "{}:{} · {}",
                a.snapshot
                    .frame
                    .file
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or(""),
                a.snapshot.frame.line,
                a.snapshot.frame.function
            )
        };
        let core_tag = a
            .core_info
            .as_ref()
            .map(|(name, idx, count)| format!(" [{name} · {}/{count}]", idx + 1))
            .unwrap_or_default();
        let endpoint = a
            .snapshot
            .core
            .as_ref()
            .map(|c| c.endpoint.as_str())
            .unwrap_or(&a.project.target.endpoint);
        let context = format!(" {} · {}{}", a.project.target.mode, endpoint, core_tag);
        let right_width = if rect.width >= 100 {
            (context.len() as u16 + 2).min(rect.width / 2)
        } else {
            0
        };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!(" {location}"), Style::default().fg(theme::MUTED)),
                Span::styled(
                    format!("  {}", a.snapshot.stop_reason),
                    Style::default().fg(theme::DIM),
                ),
            ])),
            Rect::new(rect.x, rect.y + 1, rect.width - right_width, 1),
        );
        if right_width > 0 {
            f.render_widget(
                Paragraph::new(context)
                    .style(Style::default().fg(theme::DIM))
                    .alignment(ratatui::layout::Alignment::Right),
                Rect::new(rect.right() - right_width, rect.y + 1, right_width, 1),
            );
        }
    }
}

pub fn draw(f: &mut UiFrame, a: &mut App) {
    a.file_search.area = Rect::default();
    a.symbol_search.bar = Rect::default();
    source_tabs::reset_hits(a);
    a.source_text.area = Rect::default();
    a.source_text.buttons.clear();
    a.formats.hits.clear();
    a.formats.refresh_rect = Rect::default();
    a.pane_hits.clear();
    a.action_hits.clear();
    a.palette_hits.clear();
    a.help_tab_hits.clear();
    a.source_rect = Rect::default();
    a.side_rect = Rect::default();
    a.console_input_rect = Rect::default();
    a.console_view.clear_hits();
    a.view_rects.fill(Rect::default());
    a.watch_input_rect = Rect::default();
    a.watch.add_rect = Rect::default();
    a.watch.remove_rect = Rect::default();
    a.watch.remove_hits.clear();
    a.watch.expand_hits.clear();
    a.core_hits.clear();
    a.completion.hits.clear();
    a.completion.area = Rect::default();
    a.scrollbars.fill(Rect::default());
    if let Some(setup) = &mut a.setup {
        setup.draw(f);
        return;
    }
    let area = f.area();
    f.render_widget(Block::default().style(theme::base()), area);
    if area.width < 45 || area.height < 12 {
        f.render_widget(
            Paragraph::new(format!(
                "DebugTUI v{}\nEnlarge terminal to at least 45 x 12.\nCtrl+Q exits.",
                env!("CARGO_PKG_VERSION")
            )),
            area,
        );
        return;
    }
    let search_height = if area.height >= 30 { 3 } else { 1 };
    let rows = Layout::vertical([
        Constraint::Length(
            (if area.height >= 24 { 3 } else { 2 })
                + search_height
                + u16::from(a.snapshot.core.is_some()),
        ),
        Constraint::Min(5),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);
    let mut header_rect = rows[0];
    header_rect.height = header_rect.height.saturating_sub(search_height);
    if a.snapshot.core.is_some() {
        header_rect.height = header_rect.height.saturating_sub(1);
    }
    header(f, a, header_rect);
    project_bar(
        f,
        a,
        Rect::new(
            header_rect.x,
            header_rect.bottom() - 1,
            header_rect.width,
            1,
        ),
    );
    if a.snapshot.core.is_some() {
        cores::draw(
            f,
            a,
            Rect::new(
                rows[0].x,
                rows[0].bottom() - search_height - 1,
                rows[0].width,
                1,
            ),
        );
    }
    search::symbol_bar(
        f,
        a,
        Rect::new(
            rows[0].x,
            rows[0].bottom() - search_height,
            rows[0].width.min(118),
            search_height,
        ),
    );

    // Fund the search field from Console height to preserve source/inspector space.
    // Tiny terminals retain the one-row field and the existing minimum layout.
    let console_height =
        (((rows[1].height + search_height) / 4 + 1).clamp(3, 10) - search_height).max(2);
    let body =
        Layout::vertical([Constraint::Min(3), Constraint::Length(console_height)]).split(rows[1]);
    if area.width >= 100 {
        let columns = Layout::horizontal([
            Constraint::Percentage(68),
            Constraint::Length(1),
            Constraint::Percentage(32),
        ])
        .split(body[0]);
        main_panel(f, a, columns[0]);
        side_panel(f, a, columns[2], false);
    } else if MAIN_PANES.contains(&a.pane) {
        main_panel(f, a, body[0]);
    } else {
        side_panel(f, a, body[0], true);
    }
    cores::tint(f, a, body[0]);
    console_panel(f, a, body[1]);
    theme::surface(f, rows[2], theme::CANVAS);
    f.render_widget(
        Paragraph::new(
            a.action_hits
                .iter()
                .find(|(rect, _)| hovered(a, *rect))
                .map(|(_, command)| format!(" {} · {}", command, hint(command)))
                .unwrap_or_else(|| {
                    if a.input_active()
                        && !a.completion.hint.is_empty()
                        && !a.notice.starts_with("Error")
                    {
                        format!(" {}", a.completion.hint)
                    } else {
                        format!(" {}", a.notice)
                    }
                }),
        )
        .style(Style::default().fg(if a.notice.starts_with("Error") {
            theme::RED
        } else {
            theme::DIM
        })),
        rows[2],
    );
    let focus = if a.editing {
        "Console"
    } else if a.watch_editing {
        "Watch input"
    } else if a.console_view.focused {
        "Console history"
    } else {
        PANES[a.pane]
    };
    theme::surface(f, rows[3], theme::RAISED);
    let core_hint = if a.core_info.is_some() {
        "  Ctrl+T Core"
    } else {
        ""
    };
    let keys = if area.width >= 110 {
        format!(
            " F2 Setup  / Console  Ctrl+P Help  Tab Views{core_hint}  F5 Continue  F6 Pause  F10 Step Over  F11 Step In  Ctrl+Q Exit  f Format"
        )
    } else if area.width >= 70 {
        format!(" F2 Setup  / Console  Ctrl+P Help  Tab Views{core_hint}  Ctrl+Q Exit")
    } else {
        format!(" F2 Setup  / Console  ? Help{core_hint}  Ctrl+Q Exit")
    };
    f.render_widget(
        Paragraph::new(keys).style(Style::default().fg(theme::MUTED)),
        rows[3],
    );
    if area.width >= 140 {
        let label = format!(" ◇ {focus}  ");
        f.render_widget(
            Paragraph::new(label.clone()).style(Style::default().fg(theme::ACCENT)),
            Rect::new(
                rows[3].right() - label.len() as u16,
                rows[3].y,
                label.len() as u16,
                1,
            ),
        );
    }

    effects::paint(f, a);
    completion_popup(f, a);
    effects::completion(f, a);
    if a.help || a.palette {
        let r = center(area, 90, COMMANDS.len() as u16 + 5);
        theme::overlay(f, r);
        let block = theme::card("  ?  Help  ", true);
        let inner = block.inner(r);
        f.render_widget(block, r);
        let parts = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);
        let mut x = parts[0].x;
        for (name, shortcuts) in [(" Commands ", false), (" Shortcuts ", true)] {
            let hit = Rect::new(x, parts[0].y, name.len() as u16, 1);
            f.render_widget(
                Paragraph::new(name).style(theme::chip(a.help == shortcuts, hovered(a, hit))),
                hit,
            );
            a.help_tab_hits.push((hit, shortcuts));
            x += hit.width + 1;
        }
        f.render_widget(
            Paragraph::new(if a.help {
                " ↑ ↓ scroll · Tab commands · Esc close"
            } else {
                " ↑ ↓ Enter · Tab shortcuts · Esc close"
            })
            .style(Style::default().fg(theme::MUTED)),
            parts[2],
        );
        let inner = parts[1];
        if a.help {
            f.render_widget(
                Paragraph::new(HELP)
                    .scroll((a.help_scroll, 0))
                    .wrap(Wrap { trim: false }),
                inner,
            );
        } else {
            let start = a
                .palette_index
                .saturating_sub(inner.height.saturating_sub(1) as usize);
            for (row, (i, command)) in COMMANDS
                .iter()
                .enumerate()
                .skip(start)
                .take(inner.height as usize)
                .enumerate()
            {
                let hit = Rect::new(inner.x, inner.y + row as u16, inner.width, 1);
                let line = if inner.width >= 65 {
                    format!(
                        "{} {:24} {}",
                        if i == a.palette_index { "›" } else { " " },
                        command,
                        hint(command)
                    )
                } else {
                    format!(
                        "{} {}",
                        if i == a.palette_index { "›" } else { " " },
                        command
                    )
                };
                f.render_widget(
                    Paragraph::new(line).style(theme::selected(i == a.palette_index)),
                    hit,
                );
                a.palette_hits.push((hit, i));
            }
        }
    }
    source_tabs::draw_list(f, a);
    search::draw_symbols(f, a);
    if a.confirm.is_some() {
        let r = center(area, 90, 10);
        theme::overlay(f, r);
        f.render_widget(
            Paragraph::new(format!(
                "Execute Download?\n{}\n\n{}\n\ny: Download    n / Esc: Cancel",
                if a.project.tasks.download.trim().is_empty() {
                    format!("GDB: {}", a.project.actions.download.join("; "))
                } else {
                    a.project.tasks.download.clone()
                },
                if a.project.tasks.download.trim().is_empty() {
                    format!("ELF: {}", a.project.program.elf.display())
                } else {
                    format!(
                        "Working directory: {}",
                        crate::config::portable_path(&a.project.program.source_root)
                    )
                }
            ))
            .wrap(Wrap { trim: false })
            .block(
                theme::card("  ↓  Download firmware  ", true)
                    .border_style(Style::default().fg(theme::AMBER)),
            ),
            r,
        );
    }
    if !a.help && !a.palette && !a.sources.list_open && a.confirm.is_none() {
        source_text::popup(f, a);
    }
    formats::popup(f, a);
    monitor::draw(f, a);
    breakpoints::popup(f, a);
}
