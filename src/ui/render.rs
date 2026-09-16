use super::*;

const ACTIONS: [(&str, &str); 9] = [
    ("Run", "run"),
    ("Continue", "continue"),
    ("Pause", "pause"),
    ("Reset", "restart"),
    ("Reconnect", "reconnect"),
    ("Step", "step"),
    ("Next", "next"),
    ("Finish", "finish"),
    ("CommandList", "commandlist"),
];

fn wrapped_height(labels: &[&str], width: u16) -> u16 {
    let mut rows = 1;
    let mut x = 0;
    for label in labels {
        let len = label.len() as u16 + 3;
        if x > 0 && x + len > width {
            rows += 1;
            x = 0;
        }
        x += len;
    }
    rows
}

fn tabs(f: &mut UiFrame, a: &mut App, rect: Rect, panes: &[usize], selected: usize) {
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
        let style = if pane == selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default().fg(Color::Gray)
        };
        f.render_widget(Paragraph::new(label).style(style), hit);
        a.pane_hits.push((hit, pane));
        x += width + 1;
    }
}

fn toolbar(f: &mut UiFrame, a: &mut App, rect: Rect) {
    let mut x = rect.x;
    let mut y = rect.y;
    for (label, command) in ACTIONS {
        let text = format!("[{label}]");
        let width = text.len() as u16;
        if x > rect.x && x + width > rect.right() {
            y += 1;
            x = rect.x;
        }
        if y >= rect.bottom() {
            break;
        }
        let hit = Rect::new(x, y, width.min(rect.right().saturating_sub(x)), 1);
        let color = if a.action_enabled(command) {
            Color::Cyan
        } else {
            Color::DarkGray
        };
        f.render_widget(Paragraph::new(text).style(Style::default().fg(color)), hit);
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
                if active { "█" } else { "│" },
                Style::default().fg(if active { Color::Cyan } else { Color::DarkGray }),
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
    if let Some(error) = &a.view_errors[pane] {
        f.render_widget(
            Paragraph::new(format!("{error}\n\n:refresh retries this view."))
                .style(Style::default().fg(Color::Yellow))
                .wrap(Wrap { trim: false }),
            rect,
        );
        return;
    }
    let lines: Vec<Line> = match pane {
        1 | 9 => {
            let vars = if pane == 1 {
                &a.snapshot.watches
            } else {
                &a.snapshot.locals
            };
            if vars.is_empty() {
                if pane == 1 {
                    vec![
                        Line::raw("No watches yet."),
                        Line::raw(":watch EXPRESSION adds a variable."),
                    ]
                } else {
                    vec![Line::raw("No locals in this frame.")]
                }
            } else {
                variables(vars)
                    .into_iter()
                    .skip(start)
                    .take(rect.height as usize)
                    .collect()
            }
        }
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
                    Style::default().fg(if i == selection {
                        Color::Cyan
                    } else {
                        Color::Reset
                    }),
                )
            })
            .collect(),
        3 => a
            .snapshot
            .registers
            .iter()
            .skip(start)
            .take(rect.height as usize)
            .map(|v| {
                Line::styled(
                    format!(
                        " {:8} {}{}",
                        v.name,
                        v.value,
                        if v.changed { " *" } else { "" }
                    ),
                    Style::default().fg(if v.changed {
                        Color::Yellow
                    } else {
                        Color::Reset
                    }),
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
                        Line::styled(
                            format!("{}{s}", if current { "▶ " } else { "  " }),
                            Style::default().fg(if current { Color::Green } else { Color::Reset }),
                        )
                    })
                    .collect()
            }
        }
        6 => {
            if a.snapshot.breakpoints.is_empty() {
                vec![Line::raw("No breakpoints. F9 or :break LOCATION")]
            } else {
                a.snapshot
                    .breakpoints
                    .iter()
                    .enumerate()
                    .skip(start)
                    .take(rect.height as usize)
                    .map(|(i, b)| {
                        Line::styled(
                            format!(
                                "{} {} {} {}",
                                if i == selection { "›" } else { " " },
                                b.id,
                                if b.enabled { "on" } else { "off" },
                                b.location
                            ),
                            Style::default().fg(if i == selection {
                                Color::Cyan
                            } else {
                                Color::Reset
                            }),
                        )
                    })
                    .collect()
            }
        }
        7 => a
            .snapshot
            .files
            .iter()
            .enumerate()
            .skip(start)
            .take(rect.height as usize)
            .map(|(i, s)| {
                Line::styled(
                    format!("{} {s}", if i == selection { "›" } else { " " }),
                    Style::default().fg(if i == selection {
                        Color::Cyan
                    } else {
                        Color::Reset
                    }),
                )
            })
            .collect(),
        _ => a
            .logs
            .iter()
            .skip(start)
            .take(rect.height as usize)
            .map(|s| Line::raw(s.clone()))
            .collect(),
    };
    f.render_widget(Paragraph::new(lines), rect);
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
                "No source location for PC {} ({}).\n\nInspect Asm / Regs and the GDB / server log.\nStep / Next need function debug information.\nUse :stepi only when the address contains valid code.\nFor an unexpected address, check target state before continuing.",
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
    let lines = a
        .source
        .iter()
        .enumerate()
        .skip(a.source_top)
        .take(rect.height as usize)
        .map(|(i, line)| {
            let pc = i + 1 == a.snapshot.frame.line as usize && current_file;
            let selected = i == a.source_line;
            let bp = breakpoint_lines.contains(&(i + 1));
            let mut spans = vec![Span::styled(
                format!(
                    "{}{}{:>4} ",
                    if pc {
                        "▶"
                    } else if selected {
                        "›"
                    } else {
                        " "
                    },
                    if bp { "●" } else { " " },
                    i + 1
                ),
                Style::default().fg(if pc {
                    Color::Green
                } else if bp {
                    Color::Red
                } else if selected {
                    Color::Cyan
                } else {
                    Color::DarkGray
                }),
            )];
            spans.extend(syntax(line));
            Line::from(spans)
        })
        .collect::<Vec<_>>();
    f.render_widget(Paragraph::new(lines), a.source_rect);
    scrollbar(f, a, 0);
}

fn main_panel(f: &mut UiFrame, a: &mut App, rect: Rect) {
    let toolbar_height = wrapped_height(&ACTIONS.map(|(name, _)| name), rect.width);
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(toolbar_height),
        Constraint::Min(1),
    ])
    .split(rect);
    tabs(f, a, rows[0], &MAIN_PANES, a.main_pane);
    toolbar(f, a, rows[1]);
    let title = match a.main_pane {
        0 => String::new(),
        5 => " Assembly · follows $pc · :disasm ADDRESS ".into(),
        7 => " Files · click / Enter opens ".into(),
        _ => if a.log_follow {
            " Log · following output "
        } else {
            " Log · scrollback · End follows output "
        }
        .into(),
    };
    let block = section(title);
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
    } else {
        view(f, a, a.main_pane, inner);
        a.source_rect = a.view_rects[a.main_pane];
    }
}

fn side_panel(f: &mut UiFrame, a: &mut App, rect: Rect, compact: bool) {
    if (compact || rect.height <= 12) && VARIABLE_PANES.contains(&a.pane) {
        variable_panel(f, a, rect);
        return;
    }
    let labels = SIDE_PANES.map(|i| PANES[i]);
    let tab_height = wrapped_height(&labels, rect.width);
    let local_height = if !compact && rect.height > 12 {
        (rect.height / 3).clamp(5, 12)
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
        3 => " Registers ",
        4 => " Memory · :memory ADDRESS [COUNT] ",
        _ => " Breakpoints · Delete removes selected ",
    };
    let block = section(title);
    let inner = block.inner(rows[1]);
    f.render_widget(block, rows[1]);
    view(f, a, a.side_pane, inner);
    a.side_rect = a.view_rects[a.side_pane];
    if local_height > 0 {
        variable_panel(f, a, rows[2]);
    }
}

fn variable_panel(f: &mut UiFrame, a: &mut App, rect: Rect) {
    let divider = section("");
    let inner = divider.inner(rect);
    f.render_widget(divider, rect);
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(inner);
    tabs(f, a, rows[0], &VARIABLE_PANES, a.variable_pane);
    view(f, a, a.variable_pane, rows[1]);
}

fn console_panel(f: &mut UiFrame, a: &mut App, rect: Rect) {
    let block = section(" Console · GDB commands / :commands ");
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
    let lines = a
        .console
        .iter()
        .skip(a.console.len().saturating_sub(rows[0].height as usize))
        .map(|line| Line::raw(line.clone()))
        .collect::<Vec<_>>();
    f.render_widget(Paragraph::new(lines), rows[0]);
    a.console_input_rect = rows[1];
    let prefix = "gdb> ";
    let available = rows[1].width.saturating_sub(prefix.len() as u16 + 1) as usize;
    // Keep the insertion point visible for long commands, including wide characters.
    let mut start = 0;
    let mut width = unicode_width::UnicodeWidthStr::width(a.input.as_str());
    for (index, ch) in a.input.char_indices() {
        if width <= available {
            break;
        }
        width = width.saturating_sub(unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0));
        start = index + ch.len_utf8();
    }
    let placeholder = a.input.is_empty() && !a.editing;
    let content = if placeholder {
        "Click or / to type; Enter sends"
    } else {
        &a.input[start..]
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(prefix, Style::default().fg(Color::Cyan)),
            Span::styled(
                content,
                Style::default().fg(if placeholder {
                    Color::DarkGray
                } else {
                    Color::Reset
                }),
            ),
        ]))
        .style(Style::default().bg(if a.editing {
            Color::Rgb(24, 33, 40)
        } else {
            Color::Reset
        })),
        rows[1],
    );
    if a.editing && rows[1].width > 0 && rows[1].height > 0 {
        f.set_cursor_position((
            rows[1].x + (prefix.len() as u16 + width as u16).min(rows[1].width - 1),
            rows[1].y,
        ));
    }
}

fn hint(command: &str) -> &str {
    match command.split_whitespace().next().unwrap_or("") {
        "setup" => "Select project / environment",
        "connect" => "Connect debugger",
        "reconnect" => "Disconnect, then connect again",
        "run" => "Start using the environment's run action",
        "continue" => "Resume execution (F5)",
        "pause" => "Stop execution (F6)",
        "step" => "Step into source (F11)",
        "next" => "Step over source (F10)",
        "stepi" => "Step one instruction",
        "finish" => "Step out (Shift+F11)",
        "restart" => "Reset using the environment action",
        "download" => "Program firmware (confirmation required)",
        "disconnect" => "Disconnect and release owned processes",
        "watch" => "Add a watched expression",
        "unwatch" => "Remove a watched expression",
        "break" => "Set a source / function breakpoint",
        "data-break" => "Set a hardware watchpoint",
        "delete" => "Delete breakpoint by number",
        "memory" => "Read memory bytes",
        "disasm" => "Disassemble at address (default $pc)",
        "files" => "List source files",
        "open" => "Open local source",
        "frame" => "Select a stack frame",
        "find" => "Find text in source",
        "elf" => "Load symbols from ELF",
        "refresh" => "Refresh stopped views / retry loading",
        "build" => "Run configured build command",
        "help" => "Keyboard and command help",
        "quit" => "End session and exit",
        _ => "",
    }
}

pub fn draw(f: &mut UiFrame, a: &mut App) {
    source_tabs::reset_hits(a);
    a.pane_hits.clear();
    a.action_hits.clear();
    a.palette_hits.clear();
    a.source_rect = Rect::default();
    a.side_rect = Rect::default();
    a.console_input_rect = Rect::default();
    a.view_rects.fill(Rect::default());
    a.scrollbars.fill(Rect::default());
    if let Some(setup) = &a.setup {
        setup.draw(f);
        return;
    }
    let area = f.area();
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
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(5),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);
    let state_color = if a.snapshot.state.contains("STOPPED") {
        Color::Green
    } else if a.snapshot.state == "RUNNING" {
        Color::Yellow
    } else if a.snapshot.state == "FAULT" {
        Color::Red
    } else {
        Color::Cyan
    };
    f.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    format!(" DebugTUI v{} ", env!("CARGO_PKG_VERSION")),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(
                    " {} · {}  ",
                    a.project.target.mode, a.project.target.endpoint
                )),
                Span::styled(a.snapshot.state.clone(), Style::default().fg(state_color)),
                Span::raw(if a.quitting {
                    "  Closing session…"
                } else {
                    ""
                }),
            ]),
            Line::styled(
                if a.snapshot.frame.file.is_empty() && !a.snapshot.frame.address.is_empty() {
                    format!(
                        " PC {} · {} · no source location · {}",
                        a.snapshot.frame.address, a.snapshot.frame.function, a.snapshot.stop_reason
                    )
                } else {
                    format!(
                        " {}:{}  {}",
                        a.snapshot
                            .frame
                            .file
                            .rsplit(['/', '\\'])
                            .next()
                            .unwrap_or(""),
                        a.snapshot.frame.line,
                        a.snapshot.stop_reason
                    )
                },
                Style::default().fg(
                    if a.snapshot.state == "STOPPED" && a.snapshot.frame.file.is_empty() {
                        Color::Yellow
                    } else {
                        Color::DarkGray
                    },
                ),
            ),
        ]),
        rows[0],
    );
    let console_height = (rows[1].height / 4).clamp(3, 9);
    let body =
        Layout::vertical([Constraint::Min(3), Constraint::Length(console_height)]).split(rows[1]);
    if area.width >= 100 {
        let columns = Layout::horizontal([
            Constraint::Percentage(64),
            Constraint::Length(1),
            Constraint::Percentage(36),
        ])
        .split(body[0]);
        main_panel(f, a, columns[0]);
        side_panel(f, a, columns[2], false);
    } else if MAIN_PANES.contains(&a.pane) {
        main_panel(f, a, body[0]);
    } else {
        side_panel(f, a, body[0], true);
    }
    console_panel(f, a, body[1]);
    f.render_widget(
        Paragraph::new(format!(" {}", a.notice)).style(Style::default().fg(
            if a.notice.starts_with("Error") {
                Color::Red
            } else {
                Color::DarkGray
            },
        )),
        rows[2],
    );
    f.render_widget(
        Paragraph::new(format!(
            " F2 Setup · / Console · Ctrl+P Commands · Tab views · Focus: {} · F5 Continue F6 Pause F10 Next F11 Step",
            if a.editing { "Console" } else { PANES[a.pane] }
        ))
        .style(Style::default().fg(Color::DarkGray)),
        rows[3],
    );
    if a.help {
        let r = center(area, 90, 32);
        f.render_widget(Clear, r);
        f.render_widget(
            Paragraph::new(HELP)
                .block(
                    Block::bordered()
                        .title(" Help · ↑ ↓ scroll · Esc ")
                        .border_style(Style::default().fg(Color::Cyan)),
                )
                .scroll((a.help_scroll, 0))
                .wrap(Wrap { trim: false }),
            r,
        );
    }
    if a.palette {
        let r = center(area, 88, COMMANDS.len() as u16 + 3);
        f.render_widget(Clear, r);
        let block = Block::bordered()
            .title(" CommandList · click / ↑ ↓ Enter · Esc ")
            .border_style(Style::default().fg(Color::Cyan));
        let inner = block.inner(r);
        f.render_widget(block, r);
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
                Paragraph::new(line).style(Style::default().fg(if i == a.palette_index {
                    Color::Cyan
                } else {
                    Color::Reset
                })),
                hit,
            );
            a.palette_hits.push((hit, i));
        }
    }
    source_tabs::draw_list(f, a);
    if a.confirm.is_some() {
        let r = center(area, 70, 7);
        f.render_widget(Clear, r);
        f.render_widget(
            Paragraph::new(format!(
                "Execute the configured download action?\n{}\n\ny: Download    n / Esc: Cancel",
                a.project.program.elf.display()
            ))
            .wrap(Wrap { trim: false })
            .block(
                Block::bordered()
                    .title(" Download firmware ")
                    .border_style(Style::default().fg(Color::Yellow)),
            ),
            r,
        );
    }
}
