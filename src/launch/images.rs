//! Local-only symbol image draft editor. Applying never connects to a target.
use super::*;
use crate::images::{Domain, Image};
use unicode_width::UnicodeWidthChar;

const FIELDS: [&str; 13] = [
    "Image",
    "Id",
    "ELF file",
    "Source root",
    "Cores (comma separated)",
    "Program domain",
    "Section offset",
    "Source maps (from => to | ...)",
    "Global memory channel",
    "Add image",
    "Remove image",
    "Apply to draft",
    "Cancel",
];
pub(super) enum Action {
    None,
    Cancel,
    Apply(Vec<Image>),
}
pub(super) struct Images {
    images: Vec<Image>,
    project: Project,
    selected: usize,
    field: usize,
    editor: Option<Editor>,
    hits: Vec<(Rect, usize)>,
    pub message: String,
}
impl Images {
    pub fn new(project: Project) -> Self {
        Self {
            images: project.images.clone(),
            project,
            selected: 0,
            field: 0,
            editor: None,
            hits: vec![],
            message: String::new(),
        }
    }
    fn help(&self) -> &'static str {
        match self.field {
            0 => {
                "Image: Left/Right chooses the image. Resident symbols load before connecting; browsing never changes CPU execution."
            }
            1 => {
                "Id: Enter edits the name used in @image:expression. Type or paste; Ctrl+U clears the field."
            }
            2 => {
                "ELF file: Enter edits the symbol file path, relative to Project. Use the ELF matching the target firmware."
            }
            3 => {
                "Source root: Enter edits the local source directory, relative to Project. This does not relocate the image."
            }
            4 => {
                "Cores: Enter edits core names, such as core.0, core.1. Blank applies this image to all selected cores."
            }
            5 => {
                "Program domain: Enter cycles Any, Hyp and Guest. This constrains identification; it never changes CPU mode."
            }
            6 => {
                "Section offset: Enter edits a signed decimal or positive 0x offset. Already linked runtime addresses use 0."
            }
            7 => {
                "Source maps: Enter edits from => to rules, separated with |. Local destinations are relative to Project."
            }
            8 => {
                "Global memory channel: Enter edits the configured channel name, for example ahb, used for image-owned reads."
            }
            9 => {
                "Add image: creates a new symbol image draft. Configure its Id, ELF and core binding before applying."
            }
            10 => {
                "Remove image: removes the selected draft entry. Changes are saved only after Apply and Save config."
            }
            11 => {
                "Apply to draft: validates these images and returns to Setup. Save config persists them; Start loads symbols."
            }
            _ => "Cancel: discards this image dialog's changes and returns to Setup.",
        }
    }
    fn enabled(&self, field: usize) -> bool {
        !self.images.is_empty() || field == 9 || field >= 11
    }
    fn value(&self, field: usize) -> String {
        let Some(image) = self.images.get(self.selected) else {
            return String::new();
        };
        match field {
            0 => format!(
                "{} / {} · {}",
                self.selected + 1,
                self.images.len(),
                image.id
            ),
            1 => image.id.clone(),
            2 => portable_path(&image.elf),
            3 => portable_path(&image.source_root),
            4 => image.cores.join(", "),
            5 => match image.domain {
                Domain::Any => "Any",
                Domain::Hyp => "Hyp / EL2",
                Domain::Guest => "Guest / EL0–EL1",
            }
            .into(),
            6 => image.offset.to_string(),
            7 => image
                .source_map
                .iter()
                .map(|m| format!("{} => {}", m.from, portable_path(&m.to)))
                .collect::<Vec<_>>()
                .join(" | "),
            8 => image.memory_channel.clone(),
            _ => String::new(),
        }
    }
    fn commit(&mut self) -> Result<(), String> {
        let Some(editor) = self.editor.as_ref() else {
            return Ok(());
        };
        let text = editor.text.trim();
        let image = &mut self.images[self.selected];
        match self.field {
            1 => image.id = text.into(),
            2 => image.elf = text.into(),
            3 => image.source_root = text.into(),
            4 => {
                image.cores = text
                    .split([',', ' '])
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect()
            }
            6 => {
                image.offset = if let Some(hex) = text.strip_prefix("0x") {
                    i64::from_str_radix(hex, 16)
                } else {
                    text.parse()
                }
                .map_err(|_| "Enter a signed decimal offset or positive 0x hex offset")?
            }
            7 => {
                image.source_map = if text.is_empty() {
                    vec![]
                } else {
                    text.split('|')
                        .map(|rule| {
                            let (from, to) = rule
                                .split_once("=>")
                                .ok_or("Use from => to; separate rules with |")?;
                            Ok(crate::config::SourceMap {
                                from: from.trim().into(),
                                to: to.trim().into(),
                            })
                        })
                        .collect::<Result<_, String>>()?
                }
            }
            8 => image.memory_channel = text.into(),
            _ => {}
        }
        self.editor = None;
        Ok(())
    }
    fn apply(&mut self) -> Action {
        if let Err(error) = self.commit() {
            self.message = error;
            return Action::None;
        }
        let mut project = self.project.clone();
        project.images = self.images.clone();
        if let Err(error) = crate::images::validate(&project) {
            self.message = error;
            return Action::None;
        }
        Action::Apply(self.images.clone())
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
                KeyCode::Esc => {
                    self.editor = None;
                    self.message.clear();
                }
                KeyCode::Enter => {
                    if let Err(error) = self.commit() {
                        self.message = error;
                    } else {
                        self.message.clear();
                    }
                }
                _ => editor.key(key),
            }
            return Action::None;
        }
        match key.code {
            KeyCode::Esc => return Action::Cancel,
            KeyCode::Up | KeyCode::Down | KeyCode::Tab | KeyCode::BackTab => {
                self.message.clear();
                loop {
                    self.field = (self.field
                        + if matches!(key.code, KeyCode::Up | KeyCode::BackTab) {
                            FIELDS.len() - 1
                        } else {
                            1
                        })
                        % FIELDS.len();
                    if self.enabled(self.field) {
                        break;
                    }
                }
            }
            KeyCode::Left | KeyCode::Right | KeyCode::PageUp | KeyCode::PageDown
                if !self.images.is_empty() =>
            {
                self.message.clear();
                self.selected = (self.selected
                    + if matches!(key.code, KeyCode::Left | KeyCode::PageUp) {
                        self.images.len() - 1
                    } else {
                        1
                    })
                    % self.images.len();
            }
            KeyCode::Enter | KeyCode::Char(' ') if self.enabled(self.field) => match self.field {
                0 => {
                    self.message.clear();
                    self.selected = (self.selected + 1) % self.images.len();
                }
                1..=4 | 6 | 7 | 8 => {
                    self.message.clear();
                    self.editor = Some(Editor::new(self.value(self.field)));
                }
                5 => {
                    self.message.clear();
                    let image = &mut self.images[self.selected];
                    image.domain = match image.domain {
                        Domain::Any => Domain::Hyp,
                        Domain::Hyp => Domain::Guest,
                        Domain::Guest => Domain::Any,
                    };
                }
                9 => {
                    self.message.clear();
                    let mut n = 1;
                    while self.images.iter().any(|i| i.id == format!("image-{n}")) {
                        n += 1;
                    }
                    self.images.push(Image {
                        id: format!("image-{n}"),
                        source_root: self.project.program.source_root.clone(),
                        ..Default::default()
                    });
                    self.selected = self.images.len() - 1;
                    self.field = 2;
                }
                10 => {
                    self.message.clear();
                    self.images.remove(self.selected);
                    self.selected = self.selected.min(self.images.len().saturating_sub(1));
                }
                11 => return self.apply(),
                12 => return Action::Cancel,
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
                .find(|(r, _)| r.contains((mouse.column, mouse.row).into()))
        {
            let field = *field;
            if self.editor.is_some() {
                if field == self.field {
                    return Action::None;
                }
                if field == 12 {
                    return Action::Cancel;
                }
                // Commit against the original field before moving focus.
                if let Err(error) = self.commit() {
                    self.message = error;
                    return Action::None;
                }
            }
            self.field = field;
            self.message.clear();
            return self.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        }
        Action::None
    }
    pub fn draw(&mut self, frame: &mut Frame) {
        self.hits.clear();
        let screen = frame.area();
        frame.render_widget(Block::default().style(theme::base()), screen);
        let width = screen.width.min(112);
        let height = screen.height.min(23);
        let rect = Rect::new(
            screen.x + (screen.width - width) / 2,
            screen.y + (screen.height - height) / 2,
            width,
            height,
        );
        let title = if self.editor.is_some() {
            format!(" Symbol images · editing {} ", FIELDS[self.field])
        } else {
            " Symbol images · project draft ".into()
        };
        let block = Block::bordered()
            .title(title)
            .border_style(Style::default().fg(theme::ACCENT));
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        if inner.height < 4 {
            return;
        }
        let height = usize::from(inner.height.saturating_sub(3));
        let top = self.field.saturating_sub(height.saturating_sub(1));
        for (row, field) in (top..FIELDS.len()).take(height).enumerate() {
            let hit = Rect::new(inner.x, inner.y + row as u16, inner.width, 1);
            let text = if field >= 9 {
                Line::from(format!("  [ {} ]", FIELDS[field]))
            } else {
                let prefix = format!(
                    "{} {:<30} ",
                    if field == self.field { "›" } else { " " },
                    FIELDS[field],
                );
                let available = usize::from(inner.width)
                    .saturating_sub(unicode_width::UnicodeWidthStr::width(prefix.as_str()));
                let editing = if field == self.field {
                    self.editor.as_ref()
                } else {
                    None
                };
                let (shown, style) = if let Some(editor) = editing {
                    (
                        editing_value(editor, available),
                        Style::default()
                            .fg(theme::AMBER)
                            .add_modifier(Modifier::UNDERLINED),
                    )
                } else {
                    (
                        visible_tail(&self.value(field), available),
                        Style::default(),
                    )
                };
                Line::from(vec![Span::raw(prefix), Span::styled(shown, style)])
            };
            frame.render_widget(
                Paragraph::new(text).style(if self.enabled(field) {
                    theme::row(field == self.field, false)
                } else {
                    Style::default().fg(theme::DIM)
                }),
                hit,
            );
            if self.enabled(field) {
                self.hits.push((hit, field));
            }
        }
        frame.render_widget(
            Paragraph::new(if self.message.is_empty() {
                self.help()
            } else {
                &self.message
            })
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(if self.message.is_empty() {
                theme::MUTED
            } else {
                theme::RED
            })),
            Rect::new(inner.x, inner.bottom() - 3, inner.width, 2),
        );
        frame.render_widget(
            Paragraph::new(if self.editor.is_some() {
                if inner.width < 75 {
                    "Editing · Enter: accept · Esc: undo"
                } else {
                    "Editing · Enter: accept · Esc: undo · Ctrl+U: clear · Ctrl+S: apply"
                }
            } else {
                "Tab/↑↓: fields  ←→: images  Enter: edit  Ctrl+S: apply  Esc: cancel"
            })
            .style(Style::default().fg(theme::MUTED)),
            Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
        );
    }
}

/// Keep the insertion point visible; a tail-only view hides edits near Home.
fn editing_value(editor: &Editor, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let before = &editor.text[..editor.cursor];
    let after = &editor.text[editor.cursor..];
    let budget = if after.is_empty() {
        width - 1
    } else {
        (width - 1) / 2
    };
    let mut left = Vec::new();
    let mut used = 0;
    for c in before.chars().rev() {
        let cells = c.width().unwrap_or(0);
        if used + cells > budget {
            break;
        }
        left.push(c);
        used += cells;
    }
    let mut shown = left.into_iter().rev().collect::<String>();
    shown.push('▏');
    used += 1;
    for c in after.chars() {
        let cells = c.width().unwrap_or(0);
        if used + cells > width {
            break;
        }
        shown.push(c);
        used += cells;
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    fn dialog() -> Images {
        let mut project = Project::default();
        project.images.push(Image {
            id: "vm0".into(),
            elf: "guest.elf".into(),
            ..Default::default()
        });
        Images::new(project)
    }
    fn press(dialog: &mut Images, code: KeyCode) -> Action {
        dialog.key(KeyEvent::new(code, KeyModifiers::NONE))
    }
    fn screen(dialog: &mut Images, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| dialog.draw(f)).unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }
    #[test]
    fn enter_visibly_edits_a_name_and_accept_or_escape_have_distinct_effects() {
        let mut dialog = dialog();
        dialog.field = 1;
        press(&mut dialog, KeyCode::Enter);
        let rendered = screen(&mut dialog, 112, 23);
        assert!(rendered.contains("editing Id"));
        assert!(rendered.contains("vm0▏"));
        assert!(rendered.contains("Editing · Enter: accept"));
        press(&mut dialog, KeyCode::Char('x'));
        assert!(screen(&mut dialog, 45, 12).contains("vm0x▏"));
        press(&mut dialog, KeyCode::Esc);
        assert_eq!(dialog.images[0].id, "vm0");
        press(&mut dialog, KeyCode::Enter);
        press(&mut dialog, KeyCode::Char('x'));
        press(&mut dialog, KeyCode::Enter);
        assert_eq!(dialog.images[0].id, "vm0x");
        assert!(dialog.editor.is_none());
    }
    #[test]
    fn editor_view_keeps_home_middle_end_and_utf8_insertion_points_visible() {
        let text = "D:/很长的源码路径/更多目录/guest0.elf";
        for cursor in text.char_indices().map(|(at, _)| at).chain([text.len()]) {
            let editor = Editor {
                text: text.into(),
                cursor,
            };
            for width in [0, 1, 2, 3, 8, 12, 40] {
                let shown = editing_value(&editor, width);
                assert!(unicode_width::UnicodeWidthStr::width(shown.as_str()) <= width);
                assert_eq!(shown.contains('▏'), width > 0);
            }
        }
        assert!(
            editing_value(
                &Editor {
                    text: text.into(),
                    cursor: 0
                },
                8
            )
            .starts_with("▏D:/")
        );
        assert!(editing_value(&Editor::new(text.into()), 12).ends_with("guest0.elf▏"));
    }
    #[test]
    fn clicking_another_row_commits_the_original_field_and_invalid_input_keeps_focus() {
        let mut dialog = dialog();
        dialog.field = 1;
        press(&mut dialog, KeyCode::Enter);
        dialog.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        dialog.paste("renamed");
        screen(&mut dialog, 112, 23);
        let click = |dialog: &mut Images, field| {
            let hit = dialog
                .hits
                .iter()
                .find(|(_, index)| *index == field)
                .unwrap()
                .0;
            dialog.mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: hit.x,
                row: hit.y,
                modifiers: KeyModifiers::NONE,
            })
        };
        click(&mut dialog, 2);
        assert_eq!(dialog.images[0].id, "renamed");
        assert_eq!(dialog.images[0].elf, PathBuf::from("guest.elf"));
        assert_eq!(dialog.editor.as_ref().unwrap().text, "guest.elf");
        press(&mut dialog, KeyCode::Esc);
        dialog.field = 6;
        press(&mut dialog, KeyCode::Enter);
        dialog.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        dialog.paste("invalid");
        screen(&mut dialog, 112, 23);
        click(&mut dialog, 4);
        assert_eq!(dialog.field, 6);
        assert!(dialog.editor.is_some());
        assert_eq!(dialog.images[0].offset, 0);
        assert!(dialog.images[0].cores.is_empty());
        assert!(dialog.message.contains("signed decimal"));
        assert!(matches!(click(&mut dialog, 12), Action::Cancel));
    }
    #[test]
    fn corrected_errors_follow_focus_and_applying_preserves_advanced_image_settings() {
        let mut dialog = dialog();
        dialog.images[0].domain = Domain::Guest;
        dialog.images[0].sections.insert(".data".into(), 0x10021000);
        dialog.images[0].saved_context = Some(crate::images::SavedContext {
            provider: "bao_v1".into(),
            symbol: "bao_debug_guest_contexts".into(),
            channel: "ahb".into(),
            memory: vec![crate::images::MemoryRange {
                start: 0x10020000,
                size: 0x8000,
            }],
            ..Default::default()
        });
        let saved = serde_json::to_value(&dialog.images[0].saved_context).unwrap();
        dialog.field = 7;
        press(&mut dialog, KeyCode::Enter);
        dialog.paste("invalid");
        press(&mut dialog, KeyCode::Enter);
        assert!(dialog.message.contains("from => to"));
        press(&mut dialog, KeyCode::Esc);
        assert!(dialog.message.is_empty());
        dialog.field = 2;
        press(&mut dialog, KeyCode::Up);
        assert!(dialog.help().starts_with("Id:"));
        press(&mut dialog, KeyCode::Enter);
        press(&mut dialog, KeyCode::Char('x'));
        let Action::Apply(images) =
            dialog.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))
        else {
            panic!("valid editing should apply: {}", dialog.message)
        };
        assert_eq!(images[0].id, "vm0x");
        assert_eq!(
            serde_json::to_value(&images[0].saved_context).unwrap(),
            saved
        );
        assert_eq!(images[0].sections[".data"], 0x10021000);
        assert_eq!(dialog.project.images[0].id, "vm0");
    }
    #[test]
    fn symbol_draft_is_local_and_all_controls_remain_reachable_in_short_terminals() {
        let mut project = Project::default();
        project.images.push(Image {
            id: "vm".into(),
            elf: "guest.elf".into(),
            ..Default::default()
        });
        let mut dialog = Images::new(project);
        for (width, height) in [(45, 12), (112, 30)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            for field in 0..FIELDS.len() {
                dialog.field = field;
                terminal.draw(|f| dialog.draw(f)).unwrap();
                assert!(
                    dialog.hits.iter().any(|(_, index)| *index == field),
                    "field {field} at {width}x{height}"
                );
            }
        }
        dialog.images[0].cores = vec!["missing".into()];
        assert!(matches!(dialog.apply(), Action::None));
        dialog.images[0].cores.clear();
        assert!(matches!(dialog.apply(), Action::Apply(_)));
        dialog.images.push(dialog.images[0].clone());
        assert!(matches!(dialog.apply(), Action::None));
    }
}
