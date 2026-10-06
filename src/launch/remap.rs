use super::*;
use crate::source::{Candidate, Inventory, Scan};

pub(super) enum Action {
    None,
    Apply(Candidate),
    Cancel,
}

pub(super) struct Mapping {
    project: Project,
    scan: Option<Scan>,
    inventory: Option<Inventory>,
    error: String,
    selected: usize,
    hits: Vec<(Rect, usize)>,
}
const APPLY: usize = usize::MAX;
const RESCAN: usize = usize::MAX - 1;
const CANCEL: usize = usize::MAX - 2;
impl Mapping {
    pub fn new(project: Project) -> Self {
        let mut view = Self {
            project,
            scan: None,
            inventory: None,
            error: String::new(),
            selected: 0,
            hits: vec![],
        };
        view.rescan();
        view
    }
    fn rescan(&mut self) {
        self.scan = None;
        self.inventory = None;
        self.error.clear();
        match Scan::start(self.project.clone()) {
            Ok(scan) => self.scan = Some(scan),
            Err(error) => self.error = error,
        }
    }
    pub fn scanning(&self) -> bool {
        self.scan.is_some()
    }
    pub fn tick(&mut self) -> bool {
        let Some(scan) = &self.scan else {
            return false;
        };
        let result = match scan.result.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return false,
            Err(_) => Err("ELF scan worker stopped. Press R to retry.".into()),
        };
        self.scan = None;
        match result {
            Ok(inventory) => {
                self.selected = inventory
                    .candidates
                    .iter()
                    .position(|c| {
                        c.from == crate::source::normalized(&self.project.source_remap.from)
                    })
                    .or_else(|| {
                        inventory
                            .candidates
                            .iter()
                            .enumerate()
                            .max_by_key(|(_, c)| (c.matched, std::cmp::Reverse(c.from.len())))
                            .map(|(i, _)| i)
                    })
                    .unwrap_or(0);
                self.inventory = Some(inventory);
            }
            Err(error) => self.error = error,
        }
        true
    }
    pub fn help(&self) -> String {
        if let Some(candidate) = self.current() {
            let mut text = format!(
                "{} -> {}\n{} / {} files exist locally (existence only; use the matching source revision).",
                candidate.from,
                portable_path(&self.project.program.source_root),
                candidate.matched,
                candidate.files
            );
            for (old, local, exists) in candidate.samples.iter().take(2) {
                text.push_str(&format!(
                    "\n{old}\n  -> {local} [{}]",
                    if *exists { "found" } else { "missing" }
                ));
            }
            text
        } else {
            format!(
                "ELF: {}\nLocal Source root: {}\nReads source metadata using the selected GDB; no target connection.\nEsc cancels. R retries. The directory suffix below your selection is preserved.",
                portable_path(&self.project.program.elf),
                portable_path(&self.project.program.source_root)
            )
        }
    }
    fn current(&self) -> Option<&Candidate> {
        self.inventory.as_ref()?.candidates.get(self.selected)
    }
    pub fn key(&mut self, key: KeyCode) -> Action {
        let len = self.inventory.as_ref().map_or(0, |i| i.candidates.len());
        match key {
            KeyCode::Esc => return Action::Cancel,
            KeyCode::Char('r' | 'R') => self.rescan(),
            KeyCode::Enter => {
                if let Some(c) = self.current() {
                    return Action::Apply(c.clone());
                }
            }
            KeyCode::Up | KeyCode::BackTab => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Tab => {
                self.selected = (self.selected + 1).min(len.saturating_sub(1))
            }
            KeyCode::PageUp => self.selected = self.selected.saturating_sub(10),
            KeyCode::PageDown => self.selected = (self.selected + 10).min(len.saturating_sub(1)),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = len.saturating_sub(1),
            KeyCode::Left => {
                if let Some(c) = self.current()
                    && let Some(inventory) = &self.inventory
                    && let Some((i, _)) = inventory
                        .candidates
                        .iter()
                        .enumerate()
                        .filter(|(_, parent)| {
                            parent.from != c.from
                                && crate::source::suffix(&c.from, &parent.from).is_some()
                        })
                        .max_by_key(|(_, parent)| parent.from.len())
                {
                    self.selected = i;
                }
            }
            KeyCode::Right => {
                if let Some(c) = self.current()
                    && let Some(inventory) = &self.inventory
                    && let Some((i, _)) =
                        inventory.candidates.iter().enumerate().find(|(_, child)| {
                            child.from != c.from
                                && crate::source::suffix(&child.from, &c.from).is_some()
                        })
                {
                    self.selected = i;
                }
            }
            _ => {}
        }
        Action::None
    }
    pub fn mouse(&mut self, mouse: MouseEvent) -> Action {
        if mouse.kind == MouseEventKind::ScrollDown {
            return self.key(KeyCode::Down);
        }
        if mouse.kind == MouseEventKind::ScrollUp {
            return self.key(KeyCode::Up);
        }
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return Action::None;
        }
        if let Some((_, index)) = self
            .hits
            .iter()
            .find(|(rect, _)| rect.contains((mouse.column, mouse.row).into()))
        {
            match *index {
                APPLY => return self.key(KeyCode::Enter),
                RESCAN => return self.key(KeyCode::Char('r')),
                CANCEL => return Action::Cancel,
                index => self.selected = index,
            }
        }
        Action::None
    }
    pub fn draw(&mut self, f: &mut Frame, area: Rect) {
        self.hits.clear();
        let block = theme::card("  ELF directories -> Source root  ", true);
        let inner = block.inner(area);
        f.render_widget(block, area);
        if inner.height == 0 {
            return;
        }
        let body_height = inner.height.saturating_sub(2);
        if let Some(inventory) = &self.inventory {
            let start = self
                .selected
                .saturating_sub(body_height.saturating_sub(1) as usize);
            let lines = inventory
                .candidates
                .iter()
                .enumerate()
                .skip(start)
                .take(body_height as usize)
                .map(|(i, c)| {
                    let count = format!("{:>5}/{:<5}", c.matched, c.files);
                    let label = format!(
                        "{} {count}  {}",
                        if i == self.selected { "›" } else { " " },
                        visible_tail(&c.from, inner.width.saturating_sub(16) as usize)
                    );
                    Line::styled(label, theme::selected(i == self.selected))
                })
                .collect();
            theme::lines(
                f,
                lines,
                Rect::new(inner.x, inner.y, inner.width, body_height),
            );
            self.hits.extend(
                (start..inventory.candidates.len())
                    .take(body_height as usize)
                    .enumerate()
                    .map(|(row, index)| {
                        (
                            Rect::new(inner.x, inner.y + row as u16, inner.width, 1),
                            index,
                        )
                    }),
            );
        } else {
            let text = if self.scan.is_some() {
                "Scanning ELF source directories... Esc cancels."
            } else {
                &self.error
            };
            f.render_widget(
                Paragraph::new(text)
                    .wrap(Wrap { trim: false })
                    .style(Style::default().fg(if self.error.is_empty() {
                        theme::TEXT
                    } else {
                        theme::RED
                    })),
                Rect::new(inner.x, inner.y, inner.width, body_height),
            );
        }
        if inner.height >= 2 {
            let summary = self.inventory.as_ref().map_or(String::new(), |i| {
                format!("{} source files; columns: found / covered", i.files)
            });
            f.render_widget(
                Paragraph::new(summary).style(Style::default().fg(theme::MUTED)),
                Rect::new(inner.x, inner.bottom() - 2, inner.width, 1),
            );
        }
        let mut x = inner.x;
        for (label, index) in [
            (" Apply · Enter ", APPLY),
            (" Rescan · R ", RESCAN),
            (" Cancel · Esc ", CANCEL),
        ] {
            let width = (unicode_width::UnicodeWidthStr::width(label) as u16)
                .min(inner.right().saturating_sub(x));
            let hit = Rect::new(x, inner.bottom() - 1, width, 1);
            f.render_widget(
                Paragraph::new(label).style(Style::default().bg(theme::RAISED)),
                hit,
            );
            self.hits.push((hit, index));
            x += width + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    #[test]
    fn directory_mouse_selection_preview_and_apply_have_distinct_actions() {
        let mut mapping = Mapping::new(Project::default());
        mapping.inventory = Some(Inventory {
            files: 1,
            candidates: ["/ci", "/ci/app", "/ci/app/src"]
                .into_iter()
                .map(|from| Candidate {
                    from: from.into(),
                    aliases: vec![],
                    files: 1,
                    matched: usize::from(from == "/ci/app"),
                    samples: vec![(
                        "/ci/app/src/main.c".into(),
                        "D:/local/src/main.c".into(),
                        true,
                    )],
                })
                .collect(),
        });
        mapping.error.clear();
        for (width, height) in [(45, 12), (80, 24), (120, 36)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| mapping.draw(f, f.area())).unwrap();
            let (_, index) = mapping.hits.iter().find(|(_, id)| *id == 1).unwrap();
            assert_eq!(*index, 1);
            let hit = mapping.hits.iter().find(|(_, id)| *id == 1).unwrap().0;
            let mouse = |rect: Rect| MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: rect.x,
                row: rect.y,
                modifiers: KeyModifiers::NONE,
            };
            assert!(matches!(mapping.mouse(mouse(hit)), Action::None));
            assert_eq!(mapping.current().unwrap().from, "/ci/app");
            assert!(mapping.help().contains("1 / 1 files"));
            mapping.key(KeyCode::Left);
            assert_eq!(mapping.current().unwrap().from, "/ci");
            mapping.key(KeyCode::Right);
            assert_eq!(mapping.current().unwrap().from, "/ci/app");
            let apply = mapping.hits.iter().find(|(_, id)| *id == APPLY).unwrap().0;
            assert!(matches!(mapping.mouse(mouse(apply)), Action::Apply(c) if c.from == "/ci/app"));
            let cancel = mapping.hits.iter().find(|(_, id)| *id == CANCEL).unwrap().0;
            assert!(matches!(mapping.mouse(mouse(cancel)), Action::Cancel));
        }
    }
}
