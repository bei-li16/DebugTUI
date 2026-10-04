//! Offline project overrides for explicit memory access channels.
use super::*;
use crate::config::{Core, MemoryAccess, validate_memory_access};

const LABELS: [&str; 11] = [
    "Channel",
    "Id",
    "Label",
    "TCL endpoint",
    "Target",
    "Read while running",
    "Core restriction",
    "Add channel",
    "Remove channel",
    "Apply to draft",
    "Cancel",
];

pub(super) enum Action {
    None,
    Cancel,
    Apply(Vec<MemoryAccess>),
}

pub(super) struct Channels {
    channels: Vec<MemoryAccess>,
    cores: Vec<Core>,
    selected: usize,
    field: usize,
    editor: Option<Editor>,
    source: String,
    pub message: String,
    hits: Vec<(Rect, usize)>,
}
impl Channels {
    pub fn new(project: &Project, overridden: bool) -> Self {
        Self {
            channels: project.memory_access.clone(), cores: project.cores.clone(),
            selected: 0, field: 0, editor: None, hits: vec![],
            source: if overridden { "project override".into() } else { format!("inherited from {}", project.tools.profile.display()) },
            message: "Apply stores a project override. Save config / Start writes it; the tools profile is preserved.".into(),
        }
    }
    fn enabled(&self, field: usize) -> bool {
        !self.channels.is_empty() || !(1..=6).contains(&field) && field != 8
    }
    fn value(&self, field: usize) -> String {
        let Some(channel) = self.channels.get(self.selected) else {
            return String::new();
        };
        match field {
            0 => format!(
                "{} / {} · {}",
                self.selected + 1,
                self.channels.len(),
                channel.id
            ),
            1 => channel.id.clone(),
            2 => channel.label.clone(),
            3 => channel.tcl_endpoint.clone(),
            4 => channel.target.clone(),
            5 => if channel.while_running { "Yes" } else { "No" }.into(),
            6 => {
                if channel.cores.is_empty() {
                    "All configured cores".into()
                } else {
                    channel.cores.join(", ")
                }
            }
            _ => String::new(),
        }
    }
    fn commit_editor(&mut self) {
        let Some(editor) = self.editor.take() else {
            return;
        };
        let Some(channel) = self.channels.get_mut(self.selected) else {
            return;
        };
        let value = editor.text.trim().to_owned();
        match self.field {
            1 => channel.id = value,
            2 => channel.label = value,
            3 => channel.tcl_endpoint = value,
            4 => channel.target = value,
            6 => {
                channel.cores = value
                    .split([',', ' '])
                    .filter(|core| !core.is_empty())
                    .map(str::to_owned)
                    .collect()
            }
            _ => {}
        }
    }
    fn apply(&mut self) -> Action {
        self.commit_editor();
        match validate_memory_access(&self.channels, &self.cores) {
            Ok(()) => Action::Apply(self.channels.clone()),
            Err(error) => {
                self.message = error;
                Action::None
            }
        }
    }
    pub fn paste(&mut self, text: &str) {
        if let Some(editor) = &mut self.editor {
            editor.insert(text);
        }
    }
    pub fn key(&mut self, key: KeyEvent) -> Action {
        if key.kind == KeyEventKind::Release {
            return Action::None;
        }
        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return self.apply();
        }
        if let Some(editor) = &mut self.editor {
            match key.code {
                KeyCode::Esc => self.editor = None,
                KeyCode::Enter => self.commit_editor(),
                _ => editor.key(key),
            }
            return Action::None;
        }
        match key.code {
            KeyCode::Esc => return Action::Cancel,
            KeyCode::Up | KeyCode::BackTab | KeyCode::Down | KeyCode::Tab => {
                let backwards = matches!(key.code, KeyCode::Up | KeyCode::BackTab);
                loop {
                    self.field =
                        (self.field + if backwards { LABELS.len() - 1 } else { 1 }) % LABELS.len();
                    if self.enabled(self.field) {
                        break;
                    }
                }
            }
            KeyCode::PageUp | KeyCode::PageDown | KeyCode::Left | KeyCode::Right
                if !self.channels.is_empty() =>
            {
                let backwards = matches!(key.code, KeyCode::PageUp | KeyCode::Left);
                self.selected = (self.selected
                    + if backwards {
                        self.channels.len() - 1
                    } else {
                        1
                    })
                    % self.channels.len();
            }
            KeyCode::Enter | KeyCode::Char(' ') if self.enabled(self.field) => match self.field {
                0 if !self.channels.is_empty() => {
                    self.selected = (self.selected + 1) % self.channels.len()
                }
                1..=4 | 6 => {
                    let value = if self.field == 6 {
                        self.channels[self.selected].cores.join(", ")
                    } else {
                        self.value(self.field)
                    };
                    self.editor = Some(Editor::new(value));
                }
                5 => {
                    self.channels[self.selected].while_running =
                        !self.channels[self.selected].while_running
                }
                7 => {
                    let mut index = 1;
                    while self
                        .channels
                        .iter()
                        .any(|channel| channel.id == format!("channel-{index}"))
                    {
                        index += 1;
                    }
                    self.channels.push(MemoryAccess {
                        id: format!("channel-{index}"),
                        tcl_endpoint: "127.0.0.1:6666".into(),
                        ..Default::default()
                    });
                    self.selected = self.channels.len() - 1;
                    self.field = 4;
                    self.message = "Enter the target from your OpenOCD configuration. Running reads must be supported by that target.".into();
                }
                8 => {
                    self.channels.remove(self.selected);
                    self.selected = self.selected.min(self.channels.len().saturating_sub(1));
                }
                9 => return self.apply(),
                10 => return Action::Cancel,
                _ => {}
            },
            _ => {}
        }
        Action::None
    }
    pub fn mouse(&mut self, mouse: MouseEvent) -> Action {
        if mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && let Some((_, field)) = self
                .hits
                .iter()
                .find(|(rect, _)| rect.contains((mouse.column, mouse.row).into()))
        {
            self.field = *field;
            return self.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        }
        Action::None
    }
    pub fn draw(&mut self, frame: &mut Frame) {
        self.hits.clear();
        let screen = frame.area();
        frame.render_widget(Block::default().style(theme::base()), screen);
        let width = screen.width.min(100);
        let height = screen.height.min(22);
        let area = Rect::new(
            screen.x + (screen.width - width) / 2,
            screen.y + (screen.height - height) / 2,
            width,
            height,
        );
        let card = Block::bordered()
            .title(" Memory access channels · project draft ")
            .border_style(Style::default().fg(theme::ACCENT));
        let inner = card.inner(area);
        frame.render_widget(card, area);
        if inner.height < 4 {
            return;
        }
        frame.render_widget(
            Paragraph::new(format!("Source: {}", self.source))
                .style(Style::default().fg(theme::MUTED)),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        let height = usize::from(inner.height.saturating_sub(4));
        let top = self.field.saturating_sub(height.saturating_sub(1));
        for (row, field) in (top..LABELS.len()).take(height).enumerate() {
            let enabled = self.enabled(field);
            let value = if field == self.field {
                self.editor
                    .as_ref()
                    .map(|editor| editor.text.clone())
                    .unwrap_or_else(|| self.value(field))
            } else {
                self.value(field)
            };
            let hit = Rect::new(inner.x, inner.y + 1 + row as u16, inner.width, 1);
            let text = if field >= 7 {
                format!("  [ {} ]", LABELS[field])
            } else {
                format!(
                    "{} {:<20} {}",
                    if self.field == field { "›" } else { " " },
                    LABELS[field],
                    visible_tail(&value, usize::from(inner.width.saturating_sub(24)))
                )
            };
            let style = if !enabled {
                Style::default().fg(theme::DIM)
            } else {
                theme::selected(self.field == field)
            };
            frame.render_widget(Paragraph::new(text).style(style), hit);
            if enabled {
                self.hits.push((hit, field));
            }
        }
        frame.render_widget(
            Paragraph::new(&*self.message)
                .style(Style::default().fg(theme::MUTED))
                .wrap(Wrap { trim: false }),
            Rect::new(inner.x, inner.bottom() - 3, inner.width, 2),
        );
        frame.render_widget(Paragraph::new("Tab / ↑↓: fields · ←→ / PgUp/PgDn: channel · Enter: edit · Ctrl+S: apply · Esc: cancel").style(Style::default().fg(theme::MUTED)), Rect::new(inner.x, inner.bottom() - 1, inner.width, 1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn unknown_core_duplicate_ids_and_incomplete_targets_are_rejected_before_applying() {
        let mut project = Project::default();
        project.cores.push(Core {
            name: "core.0".into(),
            ..Default::default()
        });
        project.memory_access.push(MemoryAccess {
            id: "ap".into(),
            target: "bus0".into(),
            tcl_endpoint: "localhost:6666".into(),
            ..Default::default()
        });
        let mut dialog = Channels::new(&project, false);
        dialog.channels[0].cores.push("core.1".into());
        assert!(matches!(dialog.apply(), Action::None));
        assert!(dialog.message.contains("unknown"));
        dialog.channels[0].cores.clear();
        dialog.channels.push(dialog.channels[0].clone());
        assert!(matches!(dialog.apply(), Action::None));
        dialog.channels.pop();
        dialog.channels[0].target.clear();
        assert!(matches!(dialog.apply(), Action::None));
        dialog.channels[0].target = "bus0".into();
        assert!(matches!(dialog.apply(), Action::Apply(_)));
    }

    #[test]
    fn all_controls_remain_reachable_in_narrow_and_wide_terminals() {
        let mut project = Project::default();
        project.memory_access.push(MemoryAccess {
            id: "ap".into(),
            target: "bus0".into(),
            tcl_endpoint: "localhost:6666".into(),
            ..Default::default()
        });
        let mut dialog = Channels::new(&project, false);
        for (width, height) in [(45, 12), (100, 25)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            for (field, label) in LABELS.iter().enumerate() {
                dialog.field = field;
                terminal.draw(|frame| dialog.draw(frame)).unwrap();
                let text: String = terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|cell| cell.symbol())
                    .collect();
                assert!(text.contains(label), "missing {label} at {width}x{height}");
                assert!(
                    dialog
                        .hits
                        .iter()
                        .all(|(rect, _)| rect.right() <= width && rect.bottom() <= height)
                );
            }
        }
    }
}
