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
        Rect::new(rect.x, rect.y + rect.height / 2, rect.width, 1),
    );
    let buttons = Rect::new(
        rect.x + 9,
        rect.y,
        35.min(rect.width.saturating_sub(9)),
        rect.height,
    );
    toolbar(f, a, buttons, &PROJECT_ACTIONS, rect.height);
    let (note, color) = a.fx.task_label(&a.snapshot.state).unwrap_or_else(|| {
        (
            "Setup: edit & restart · output in Console".into(),
            theme::DIM,
        )
    });
    if rect.width >= 85 {
        f.render_widget(
            Paragraph::new(note).style(Style::default().fg(color)),
            Rect::new(rect.x + 46, rect.y + rect.height / 2, rect.width - 46, 1),
        );
    }
}

// Full frames need three terminal rows. Keep the existing compact layout when
// the terminal is short so the source and inspector still have usable space.
fn control_height(f: &UiFrame) -> u16 {
    if f.area().height >= 36 { 3 } else { 1 }
}

fn wrapped_height(labels: &[&str], width: u16, row_height: u16) -> u16 {
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
    rows * row_height
}

fn hovered(a: &App, rect: Rect) -> bool {
    a.pointer_over(rect)
}

fn pane_label(pane: usize, width: u16) -> &'static str {
    // Keep all Inspector tabs on one row in the ordinary 160-column layout.
    // The section title below still spells out the selected view's full name.
    match pane {
        pane::REGS if width < 60 => "Regs",
        pane::PERIPHERALS if width < 60 => "Periph",
        _ => PANES[pane],
    }
}

fn tabs(f: &mut UiFrame, a: &mut App, rect: Rect, panes: &[usize], selected: usize, height: u16) {
    theme::surface(f, rect, theme::PANEL);
    let mut x = rect.x;
    let mut y = rect.y;
    let height = height.min(rect.height);
    for &pane in panes {
        let label = pane_label(pane, rect.width);
        let width = unicode_width::UnicodeWidthStr::width(label) as u16 + 2;
        if x > rect.x && x + width > rect.right() {
            y += height;
            x = rect.x;
        }
        if y >= rect.bottom() {
            break;
        }
        let hit = Rect::new(
            x,
            y,
            width.min(rect.right().saturating_sub(x)),
            height.min(rect.bottom() - y),
        );
        let style = theme::chip(pane == selected, hovered(a, hit));
        theme::button(f, hit, label, style);
        a.pane_hits.push((hit, pane));
        x += width + 1;
    }
}

fn toolbar(
    f: &mut UiFrame,
    a: &mut App,
    rect: Rect,
    actions: &[(&str, &'static str)],
    height: u16,
) {
    theme::surface(f, rect, theme::PANEL);
    let mut x = rect.x;
    let mut y = rect.y;
    let height = height.min(rect.height);
    for &(label, command) in actions {
        let text = format!(" {label} ");
        let width = unicode_width::UnicodeWidthStr::width(text.as_str()) as u16;
        if x > rect.x && x + width > rect.right() {
            y += height;
            x = rect.x;
        }
        if y >= rect.bottom() {
            break;
        }
        let hit = Rect::new(
            x,
            y,
            width.min(rect.right().saturating_sub(x)),
            height.min(rect.bottom() - y),
        );
        let enabled = command == "edit-value" || a.action_enabled(command);
        let tone = if command == "continue" || command == "run" {
            theme::GREEN
        } else if command == "pause" {
            theme::AMBER
        } else if command == "quit" {
            theme::RED
        } else {
            theme::TEXT
        };
        theme::button(
            f,
            hit,
            label,
            theme::control(enabled, false, hovered(a, hit), tone),
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
    a.view_tops[pane] = if pane == pane::LOG && a.log_follow {
        max
    } else {
        a.view_tops[pane].min(max)
    };
    let selection = a.selected(pane);
    let start = a.view_tops[pane];
    scrollbar(f, a, pane);
    if pane == pane::PERIPHERALS {
        a.draw_peripherals(f, rect);
        return;
    }
    if pane == pane::REGS && a.register_view.enabled() {
        a.draw_registers(f, rect);
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
            pane::WATCH => (
                "No watches yet",
                "Enter a variable below, then click + Add.",
            ),
            pane::LOCALS => (
                "No locals in this frame",
                "Select a stopped frame to inspect its variables.",
            ),
            pane::BREAKS => ("No breakpoints", "+ Code / + Data above, or F9 in Source"),
            pane::FILES if !a.snapshot.files.is_empty() => (
                "No matching files",
                "Edit Find above; Ctrl+U clears the filter.",
            ),
            pane::FILES => (
                "Source files",
                "Connect to GDB to load the source file list.",
            ),
            pane::STACK => ("Call stack", "Pause the target to inspect its frames."),
            pane::REGS => ("Registers", "Connect and pause to inspect register values."),
            pane::MEMORY => (
                "Memory range",
                "Memory access selects the range and channel; Read obtains a sample.",
            ),
            pane::ASM if a.snapshot.state == state::RUNNING => {
                ("Target running", "Pause the target to load this view.")
            }
            pane::ASM if a.snapshot.state == state::STOPPED => (
                "Reading from GDB…",
                "The view will update when the request completes.",
            ),
            pane::ASM => (
                "Waiting for target",
                "Connect and stop the target to load this view.",
            ),
            _ => ("Session log", "Debugger events will appear here."),
        };
        theme::empty(f, rect, title, hint);
        return;
    }
    if matches!(pane, pane::WATCH | pane::REGS | pane::LOCALS) {
        a.numeric_view(f, pane, rect);
        return;
    }
    if pane == pane::MEMORY {
        a.memory_view(f, rect);
        return;
    }
    let lines: Vec<Line> = match pane {
        pane::STACK => a
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
        pane::MEMORY | pane::ASM => {
            let values = if pane == pane::MEMORY {
                &a.snapshot.memory
            } else {
                &a.snapshot.assembly
            };
            if values.is_empty() {
                let message = if a.snapshot.state == state::RUNNING {
                    "Pause the target to load this view."
                } else if a.snapshot.state == state::STOPPED {
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
                        let current = pane == pane::ASM
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
                        data_line(s, current, pane == pane::ASM)
                    })
                    .collect()
            }
        }
        pane::BREAKS => breakpoints::rows(a, start, rect.height as usize),
        pane::FILES => a
            .filtered_files()
            .iter()
            .copied()
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
    a.source_rect = viewport(a, pane::SOURCE, rect);
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
        scrollbar(f, a, pane::SOURCE);
        let message = if a.sources.active.is_some() {
            "This source file is empty.".into()
        } else if a.snapshot.state == state::STOPPED
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
        if a.pane == pane::SOURCE
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
    scrollbar(f, a, pane::SOURCE);
}

fn execution_actions(a: &App, short: bool) -> Vec<(&'static str, &'static str)> {
    let compact = [ACTIONS[1], ACTIONS[2], ACTIONS[8], ACTIONS[9]];
    let mut actions = if short {
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
    actions
}

fn main_panel(f: &mut UiFrame, a: &mut App, rect: Rect, shared_actions: bool) {
    theme::surface(f, rect, theme::PANEL);
    // Wide layouts share one execution toolbar above both Source and Inspector.
    // Short terminals keep the compact in-panel controls.
    let actions = execution_actions(a, rect.height < 8);
    let labels: Vec<_> = actions.iter().map(|(name, _)| *name).collect();
    let button_height = if rect.height >= 18 {
        control_height(f)
    } else {
        1
    };
    let toolbar_height = if shared_actions {
        0
    } else {
        wrapped_height(&labels, rect.width, button_height)
    };
    let rows = Layout::vertical([
        Constraint::Length(button_height),
        Constraint::Length(toolbar_height),
        Constraint::Min(1),
    ])
    .split_cached(rect);
    // Source text actions share the tab row; reserve their space before drawing
    // tabs so their frames never cover another control's mouse target.
    let tab_area = Rect {
        width: rows[0]
            .width
            .saturating_sub(source_text::button_width(a, rows[0])),
        ..rows[0]
    };
    tabs(f, a, tab_area, &MAIN_PANES, a.main_pane, button_height);
    if !shared_actions {
        toolbar(f, a, rows[1], &actions, button_height);
    }
    source_text::buttons(f, a, rows[0]);
    let title = match a.main_pane {
        pane::SOURCE => String::new(),
        pane::ASM => " Assembly · follows $pc · :disasm ADDRESS ".into(),
        pane::FILES => format!(
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
    if a.main_pane == pane::SOURCE {
        source_tabs::draw_tabs(
            f,
            a,
            Rect {
                height: rows[2].height.min(1),
                ..rows[2]
            },
        );
        source(f, a, inner);
    } else if a.main_pane == pane::FILES {
        let height = if inner.height >= 10 { 3 } else { 1 };
        let rows =
            Layout::vertical([Constraint::Length(height), Constraint::Min(0)]).split_cached(inner);
        search::file_bar(f, a, rows[0]);
        view(f, a, pane::FILES, rows[1]);
        a.source_rect = a.view_rects[pane::FILES];
    } else {
        view(f, a, a.main_pane, inner);
        a.source_rect = a.view_rects[a.main_pane];
    }
}

/// The inspector, with Watch/Locals below it (or instead of it when compact
/// and focused) unless `variables` is false because they are drawn elsewhere.
fn side_panel(f: &mut UiFrame, a: &mut App, rect: Rect, compact: bool, variables: bool) {
    theme::surface(f, rect, theme::PANEL);
    if variables && (compact || rect.height <= 12) && VARIABLE_PANES.contains(&a.pane) {
        variable_panel(f, a, rect);
        return;
    }
    let labels = SIDE_PANES.map(|i| pane_label(i, rect.width));
    let button_height = if rect.height >= 18 {
        control_height(f)
    } else {
        1
    };
    let tab_height = wrapped_height(&labels, rect.width, button_height);
    let local_height = if variables && !compact && rect.height > 12 {
        let height = (rect.height / 3).clamp(5, 12);
        // Reserve the input frame's two border rows without hiding Watch values.
        height + if height >= 8 { 2 } else { 0 } + button_height - 1
            + u16::from(button_height == 3)
            + u16::from(a.variable_pane == pane::WATCH)
    } else {
        0
    };
    let rows = Layout::vertical([
        Constraint::Length(tab_height),
        Constraint::Min(1),
        Constraint::Length(local_height),
    ])
    .split_cached(rect);
    tabs(f, a, rows[0], &SIDE_PANES, a.side_pane, button_height);
    let title = match a.side_pane {
        pane::STACK => " Stack · click / Enter selects frame ",
        pane::REGS => " System registers ",
        pane::MEMORY => " Memory ",
        _ => " Breakpoints · Space toggle · e edit ",
    };
    let title = if a.side_pane == pane::PERIPHERALS {
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
    if a.side_pane == pane::MEMORY && inner.height > 0 {
        let access = if inner.width < 27 {
            "Access"
        } else {
            "Memory access"
        };
        let height = wrapped_height(&[access, "↻ Read", "Edit value"], inner.width, 1)
            .min(inner.height.saturating_sub(1).max(1));
        a.write_editor
            .entry_hits
            .push((Rect::new(inner.x, inner.y, inner.width, height), 4));
        toolbar(
            f,
            a,
            Rect::new(inner.x, inner.y, inner.width, height),
            &[
                (access, "memory-access"),
                ("↻ Read", "memory-refresh"),
                ("Edit value", "edit-value"),
            ],
            1,
        );
        inner.y += height;
        inner.height -= height;
        let detail_height = 2.min(inner.height.saturating_sub(1));
        f.render_widget(
            Paragraph::new(a.memory_caption()).style(Style::default().fg(theme::MUTED)),
            Rect::new(inner.x, inner.y, inner.width, detail_height),
        );
        inner.y += detail_height;
        inner.height -= detail_height;
    }
    if a.side_pane == pane::BREAKS && inner.height > 2 {
        let labels: Vec<_> = breakpoints::ACTIONS
            .iter()
            .map(|(label, _)| *label)
            .collect();
        let row_height = if inner.height >= 8 { button_height } else { 1 };
        let height =
            wrapped_height(&labels, inner.width, row_height).min(inner.height.saturating_sub(2));
        toolbar(
            f,
            a,
            Rect::new(inner.x, inner.y, inner.width, height),
            breakpoints::ACTIONS,
            row_height,
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
    if a.side_pane == pane::PERIPHERALS && inner.height > 1 {
        let height = wrapped_height(
            &["Memory access", "↻ Read", "Edit value"],
            inner.width,
            button_height,
        )
        .min(inner.height.saturating_sub(1));
        a.write_editor.entry_hits.push((
            Rect::new(inner.x, inner.y, inner.width, height),
            pane::PERIPHERALS,
        ));
        toolbar(
            f,
            a,
            Rect::new(inner.x, inner.y, inner.width, height),
            &[
                ("Memory access", "peripheral-access"),
                ("↻ Read", "peripheral-refresh"),
                ("Edit value", "edit-value"),
            ],
            height,
        );
        inner.y += height;
        inner.height -= height;
    }
    if a.side_pane == pane::PERIPHERALS && inner.height > 1 {
        let selected = if a.pane == pane::PERIPHERALS {
            a.selection
        } else {
            a.selections[pane::PERIPHERALS]
        };
        f.render_widget(
            Paragraph::new(a.access_caption(pane::PERIPHERALS, selected))
                .style(Style::default().fg(theme::MUTED)),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        inner.y += 1;
        inner.height -= 1;
    }
    if a.side_pane == pane::REGS && a.register_view.enabled() && inner.height > 2 {
        let height = wrapped_height(&a.register_action_labels(), inner.width, 1)
            .min(inner.height.saturating_sub(2));
        a.write_editor
            .entry_hits
            .push((Rect::new(inner.x, inner.y, inner.width, height), 3));
        toolbar(
            f,
            a,
            Rect::new(inner.x, inner.y, inner.width, height),
            registers::ACTIONS,
            1,
        );
        inner.y += height;
        inner.height -= height;
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
    let watch = a.variable_pane == pane::WATCH;
    let tab_height = if inner.height >= 11 + u16::from(watch) {
        control_height(f)
    } else {
        1
    };
    let rows = Layout::vertical([
        Constraint::Length(tab_height),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(if !watch {
            0
        } else if inner.height >= tab_height + 8 + u16::from(watch) {
            3
        } else {
            1
        }),
    ])
    .split_cached(inner);
    let remove_width = if watch && rows[0].width >= 30 { 12 } else { 0 };
    let tab_area = Rect {
        width: rows[0].width.saturating_sub(remove_width),
        ..rows[0]
    };
    tabs(f, a, tab_area, &VARIABLE_PANES, a.variable_pane, tab_height);
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
        theme::button(
            f,
            hit,
            "Del Remove",
            theme::control(enabled, false, hovered(a, hit), theme::TEXT),
        );
    }
    a.write_editor.entry_hits.push((rows[1], a.variable_pane));
    if watch {
        let width = rows[1].width.min(30);
        toolbar(
            f,
            a,
            Rect::new(rows[1].x, rows[1].y, width, rows[1].height),
            &[
                ("Memory access", "watch-access"),
                ("Edit value", "edit-value"),
            ],
            1,
        );
        let selected = if a.pane == pane::WATCH {
            a.selection
        } else {
            a.selections[pane::WATCH]
        };
        f.render_widget(
            Paragraph::new(a.access_caption(pane::WATCH, selected))
                .style(Style::default().fg(theme::MUTED)),
            Rect::new(
                rows[1].x + width,
                rows[1].y,
                rows[1].width - width,
                rows[1].height,
            ),
        );
    } else {
        toolbar(f, a, rows[1], &[("Edit value", "edit-value")], 1);
    }
    view(f, a, a.variable_pane, rows[2]);
    if watch {
        let add_width = 9.min(rows[3].width);
        a.watch_input_rect = Rect {
            width: rows[3].width.saturating_sub(add_width),
            ..rows[3]
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
            rows[3].right() - add_width,
            rows[3].y,
            add_width,
            rows[3].height,
        );
        a.watch.add_rect = hit;
        let enabled = a.pending_watch.is_none() && a.pending_task.is_none();
        theme::button(
            f,
            hit,
            "+ Add",
            theme::control(enabled, false, hovered(a, hit), theme::TEXT),
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
        // As with buttons, leave the frame on the parent surface and apply the
        // field background only inside the rounded border.
        .style(Style::default());
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
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(input_height)])
        .split_cached(inner);
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

/// The Shortcuts page for `width` columns: group titles, then keys and
/// actions in two columns with actions wrapped in their own column. Narrow
/// pages put the keys above their action instead.
fn shortcut_lines(width: u16) -> Vec<Line<'static>> {
    use unicode_width::UnicodeWidthStr;
    let width = (width as usize).saturating_sub(1);
    let column = SHORTCUTS
        .iter()
        .flat_map(|(_, rows)| rows.iter())
        .map(|(keys, _)| keys.width())
        .max()
        .unwrap_or(0);
    let stacked = width < column + 30;
    let title = Style::default()
        .fg(theme::ACCENT)
        .add_modifier(Modifier::BOLD);
    let key = Style::default()
        .fg(theme::TEXT)
        .add_modifier(Modifier::BOLD);
    let note = Style::default().fg(theme::MUTED);
    let mut lines = vec![];
    for (group, rows) in SHORTCUTS {
        if !lines.is_empty() {
            lines.push(Line::default());
        }
        lines.push(Line::styled(format!(" {group}"), title));
        for (keys, action) in *rows {
            if keys.is_empty() {
                for text in wrap_words(action, width.saturating_sub(3)) {
                    lines.push(Line::styled(format!("   {text}"), note));
                }
            } else if stacked {
                lines.push(Line::styled(format!("   {keys}"), key));
                for text in wrap_words(action, width.saturating_sub(5)) {
                    lines.push(Line::from(format!("     {text}")));
                }
            } else {
                let indent = 3 + column + 2;
                for (i, text) in wrap_words(action, width.saturating_sub(indent))
                    .into_iter()
                    .enumerate()
                {
                    let keys = if i == 0 { *keys } else { "" };
                    let pad = " ".repeat(column - keys.width() + 2);
                    lines.push(Line::from(vec![
                        Span::styled(format!("   {keys}{pad}"), key),
                        Span::raw(text),
                    ]));
                }
            }
        }
    }
    lines
}
/// Greedy word wrap by display width; a word longer than the line keeps its
/// own line.
fn wrap_words(text: &str, width: usize) -> Vec<String> {
    use unicode_width::UnicodeWidthStr;
    let mut lines = vec![];
    let mut line = String::new();
    for word in text.split(' ') {
        if !line.is_empty() && line.width() + 1 + word.width() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    lines.push(line);
    lines
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
        "memory-access" => "Select Memory address, byte count and channel",
        "edit-value" => "Edit the selected value with explicit Preview, Apply and Cancel",
        "memory-refresh" => "Read the selected Memory range and channel once",
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
        "zoom" => "Let the focused panel group fill the workspace (z)",
        "cores" => "List debug cores and their states",
        "core" => "Select a core by name or index (Ctrl+T: next core)",
        "break-new" => "Open the code breakpoint editor",
        "break-data" => "Open the data watchpoint editor",
        "break-edit" => "Edit the selected breakpoint: enabled, condition, ignore count",
        "break-toggle" => "Enable or disable the selected breakpoint",
        "enable" => "Enable a breakpoint by number, or all",
        "disable" => "Disable a breakpoint by number, or all",
        "register-probe" => "Probe which catalogue registers this core implements",
        "register-cancel" => "Cancel the pending register read",
        "register-status" => "Register read status and reasons",
        "register-bank-read" => "Read the selected banked register group",
        "mpu" => "MPU region overview",
        "cache" => "Cache and TCM overview",
        "peripheral-refresh" => "Read the selected peripheral register once",
        "commandlist" => "Help: commands and keyboard shortcuts (Ctrl+P)",
        "quit" => "End session and close TUI (Ctrl+Q)",
        _ => "",
    }
}

fn header(f: &mut UiFrame, a: &App, rect: Rect) {
    let state = a.snapshot.state.as_str();
    let color = if state == state::FAULT {
        theme::RED
    } else if state == state::RUNNING {
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
                .bg(if state.contains(state::STOPPED) {
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
                match a.fx.task_label(&a.snapshot.state) {
                    Some((label, color)) => {
                        Span::styled(format!("  · {label}"), Style::default().fg(color))
                    }
                    None => Span::styled(
                        format!("  {}", a.snapshot.stop_reason),
                        Style::default().fg(theme::DIM),
                    ),
                },
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
    a.hover_probes.get_mut().clear();
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
    a.watch.local_expand_hits.clear();
    a.write_editor.entry_hits.clear();
    a.core_hits.clear();
    a.completion.hits.clear();
    a.completion.area = Rect::default();
    a.scrollbars.fill(Rect::default());
    let setup_register_context = a.setup.as_ref().map(|_| a.register_context());
    if let Some(setup) = &mut a.setup {
        setup.observe_register_target(
            &a.project,
            a.snapshot.register_probe.as_ref(),
            &setup_register_context.unwrap(),
            a.snapshot.state == state::STOPPED,
        );
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
    let button_height = control_height(f);
    let header_height = if area.height >= 24 { 2 } else { 1 };
    let core_height = if a.snapshot.core.is_some() {
        button_height
    } else {
        0
    };
    let shared_actions = area.width >= 150 && area.height >= 36;
    let inline_header = area.width >= if core_height > 0 { 150 } else { 100 };
    let navigation_height = if inline_header {
        button_height.max(search_height)
    } else {
        button_height + core_height + search_height
    };
    let actions = execution_actions(a, false);
    let labels: Vec<_> = actions.iter().map(|(label, _)| *label).collect();
    let execution_height = if shared_actions {
        wrapped_height(&labels, area.width, button_height)
    } else {
        0
    };
    let rows = Layout::vertical([
        Constraint::Length(header_height + navigation_height + execution_height),
        Constraint::Min(5),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split_cached(area);
    let header_rect = Rect {
        height: header_height,
        ..rows[0]
    };
    header(f, a, header_rect);
    let navigation_y = header_rect.bottom();
    let project_width = if inline_header { 44 } else { area.width };
    project_bar(
        f,
        a,
        Rect::new(
            header_rect.x,
            header_rect.bottom(),
            project_width,
            button_height,
        ),
    );
    let core_width = if inline_header && core_height > 0 {
        (14 + a.snapshot.cores.len().min(3) as u16 * 18).min(area.width.saturating_sub(88))
    } else {
        area.width
    };
    if core_height > 0 {
        cores::draw(
            f,
            a,
            Rect::new(
                rows[0].x + if inline_header { 46 } else { 0 },
                navigation_y + if inline_header { 0 } else { button_height },
                core_width,
                core_height,
            ),
        );
    }
    let search_x = if inline_header {
        rows[0].x + 46 + if core_height > 0 { core_width + 2 } else { 0 }
    } else {
        rows[0].x
    };
    search::symbol_bar(
        f,
        a,
        Rect::new(
            search_x,
            navigation_y
                + if inline_header {
                    0
                } else {
                    button_height + core_height
                },
            rows[0].right().saturating_sub(search_x).min(118),
            if inline_header {
                navigation_height
            } else {
                search_height
            },
        ),
    );
    if shared_actions {
        toolbar(
            f,
            a,
            Rect::new(
                rows[0].x,
                rows[0].bottom() - execution_height,
                rows[0].width,
                execution_height,
            ),
            &actions,
            button_height,
        );
    }

    // Fund the search field from Console height to preserve source/inspector space.
    // Tiny terminals retain the one-row field and the existing minimum layout.
    let console_height =
        (((rows[1].height + search_height) / 4 + 1).clamp(3, 10) - search_height).max(2);
    let body = Layout::vertical([Constraint::Min(3), Constraint::Length(console_height)])
        .split_cached(rows[1]);
    if a.zoom {
        // The focused group alone: Source group, or Inspector / Variables.
        if MAIN_PANES.contains(&a.pane) {
            main_panel(f, a, body[0], shared_actions);
        } else {
            side_panel(f, a, body[0], true, true);
        }
    } else if area.width >= 160 && body[0].height <= 16 {
        // Wide and short, like an editor's bottom panel: three columns, so
        // Watch/Locals stay visible beside Source and the inspector.
        let columns = Layout::horizontal([
            Constraint::Percentage(50),
            Constraint::Length(1),
            Constraint::Percentage(25),
            Constraint::Length(1),
            Constraint::Percentage(25),
        ])
        .split_cached(body[0]);
        main_panel(f, a, columns[0], shared_actions);
        side_panel(f, a, columns[2], true, false);
        theme::surface(f, columns[4], theme::PANEL);
        variable_panel(f, a, columns[4]);
    } else if area.width >= 100 {
        let columns = Layout::horizontal([
            Constraint::Percentage(68),
            Constraint::Length(1),
            Constraint::Percentage(32),
        ])
        .split_cached(body[0]);
        main_panel(f, a, columns[0], shared_actions);
        side_panel(f, a, columns[2], false, true);
    } else if body[0].height >= 20 {
        // Narrow and tall, like an editor's side panel: Source stays above;
        // below shows the focused inspector, otherwise Watch/Locals.
        let rows = Layout::vertical([
            Constraint::Percentage(58),
            Constraint::Length(1),
            Constraint::Percentage(42),
        ])
        .split_cached(body[0]);
        main_panel(f, a, rows[0], false);
        if SIDE_PANES.contains(&a.pane) {
            side_panel(f, a, rows[2], true, false);
        } else {
            theme::surface(f, rows[2], theme::PANEL);
            variable_panel(f, a, rows[2]);
        }
    } else if MAIN_PANES.contains(&a.pane) {
        main_panel(f, a, body[0], false);
    } else {
        side_panel(f, a, body[0], true, true);
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
        // Notices carry real status ("ELF not built…"); DIM measured 3.8:1
        // against the canvas, below the 4.5:1 body-text contrast minimum.
        .style(Style::default().fg(if a.notice.starts_with("Error") {
            theme::RED
        } else {
            theme::MUTED
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
            " F2 Setup  / Console  Ctrl+P Help  Tab Views{core_hint}  F5 Continue  F6 Pause  F10 Step Over  F11 Step In  Ctrl+Q Exit  f Format  z Zoom"
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
        let label = format!(" ◇ {focus}{}  ", if a.zoom { " · zoom" } else { "" });
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
        .split_cached(inner);
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
        if !a.help && x < parts[0].right() {
            let filter = Rect::new(x + 2, parts[0].y, parts[0].right().saturating_sub(x + 2), 1);
            f.render_widget(
                if a.palette_query.is_empty() {
                    Paragraph::new("Type to filter").style(Style::default().fg(theme::MUTED))
                } else {
                    Paragraph::new(format!("Filter: {}▏", a.palette_query))
                        .style(Style::default().fg(theme::ACCENT))
                },
                filter,
            );
        }
        let inner = parts[1];
        let footer = if a.help {
            let lines = shortcut_lines(inner.width);
            let max = lines.len().saturating_sub(inner.height as usize);
            a.help_scroll = a.help_scroll.min(max as u16);
            let top = a.help_scroll as usize;
            f.render_widget(Paragraph::new(lines).scroll((a.help_scroll, 0)), inner);
            let keys = if inner.width >= 70 {
                " ↑ ↓ PgUp PgDn scroll · Tab commands · Esc close"
            } else {
                " ↑↓ scroll · Esc close"
            };
            if max > 0 {
                let bottom = top + inner.height as usize;
                format!(
                    "{keys}   {}-{bottom}/{}",
                    top + 1,
                    max + inner.height as usize
                )
            } else {
                keys.into()
            }
        } else {
            " type to filter · ↑ ↓ Enter · Tab shortcuts · Esc close".into()
        };
        f.render_widget(
            Paragraph::new(footer).style(Style::default().fg(theme::MUTED)),
            parts[2],
        );
        if !a.help {
            let commands = a.palette_commands();
            let selected = commands
                .iter()
                .position(|&i| i == a.palette_index)
                .unwrap_or(0);
            let start = selected.saturating_sub(inner.height.saturating_sub(1) as usize);
            if commands.is_empty() {
                f.render_widget(
                    Paragraph::new("  No command matches the filter")
                        .style(Style::default().fg(theme::MUTED)),
                    inner,
                );
            }
            for (row, (i, command)) in commands
                .iter()
                .map(|&i| (i, COMMANDS[i]))
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
    registers::draw_mpu(f, a);
    registers::draw_status(f, a);
    monitor::draw(f, a);
    memory::draw(f, a);
    writes::draw(f, a);
    breakpoints::popup(f, a);
}
