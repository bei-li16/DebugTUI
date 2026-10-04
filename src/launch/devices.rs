use super::*;
use crate::devices::{Catalogue, Device, Selection};

pub(super) enum Action {
    None,
    Cancel,
    Legacy,
    Select(Selection),
}
enum Mode {
    Chips,
    Cores {
        chip: String,
        available: Vec<u32>,
        checked: Vec<u32>,
    },
    Add {
        fields: [Editor; 3],
    },
}
pub(super) struct Devices {
    catalogue: Catalogue,
    path: PathBuf,
    previous: Selection,
    mode: Mode,
    selected: usize,
    error: String,
    hits: Vec<(Rect, usize)>,
}
const APPLY: usize = usize::MAX;
const ADD: usize = usize::MAX - 1;
const CANCEL: usize = usize::MAX - 2;
const LEGACY: usize = usize::MAX - 3;
const ALL: usize = usize::MAX - 4;
const NONE: usize = usize::MAX - 5;
impl Devices {
    pub fn new(previous: Selection, cores: bool) -> Result<Self, String> {
        let path = crate::devices::ensure_user_catalogue()?;
        Self::at(path, previous, cores)
    }
    fn at(path: PathBuf, previous: Selection, cores: bool) -> Result<Self, String> {
        let catalogue = Catalogue::load(&path)?;
        let selected = catalogue
            .devices
            .keys()
            .position(|n| n == &previous.chip)
            .unwrap_or(0);
        let mut view = Self {
            catalogue,
            path,
            previous,
            mode: Mode::Chips,
            selected,
            error: String::new(),
            hits: vec![],
        };
        if cores {
            view.choose()?;
        }
        Ok(view)
    }
    fn choose(&mut self) -> Result<(), String> {
        let (chip, device) = self
            .catalogue
            .devices
            .iter()
            .nth(self.selected)
            .ok_or("Add a chip first")?;
        let checked = if self.previous.chip == *chip {
            self.previous
                .cores
                .iter()
                .filter(|id| device.cores.contains(id))
                .copied()
                .collect::<Vec<_>>()
        } else {
            vec![device.cores[0]]
        };
        self.mode = Mode::Cores {
            chip: chip.clone(),
            available: device.cores.clone(),
            checked,
        };
        self.selected = 0;
        Ok(())
    }
    pub fn paste(&mut self, text: &str) {
        if let Mode::Add { fields } = &mut self.mode
            && self.selected < 3
        {
            fields[self.selected].insert(text);
        }
    }
    fn activate(&mut self, button: usize) -> Result<Action, String> {
        self.error.clear();
        match button {
            CANCEL => return Ok(Action::Cancel),
            LEGACY => return Ok(Action::Legacy),
            ADD => {
                self.mode = Mode::Add {
                    fields: [
                        Editor::new(String::new()),
                        Editor::new("0".into()),
                        Editor::new("generic".into()),
                    ],
                };
                self.selected = 0;
            }
            ALL | NONE => {
                if let Mode::Cores {
                    available, checked, ..
                } = &mut self.mode
                {
                    *checked = if button == ALL {
                        available.clone()
                    } else {
                        vec![]
                    };
                }
            }
            APPLY => match &self.mode {
                Mode::Chips => self.choose()?,
                Mode::Cores { chip, checked, .. } => {
                    let mut cores = checked.clone();
                    cores.sort_unstable();
                    let selection = Selection {
                        chip: chip.clone(),
                        cores,
                    };
                    self.catalogue.selection(&selection)?;
                    return Ok(Action::Select(selection));
                }
                Mode::Add { fields } => {
                    let name = fields[0].text.trim().to_ascii_lowercase();
                    let device = Device {
                        cores: crate::devices::parse_ids(&fields[1].text)?,
                        backend: fields[2].text.trim().to_owned(),
                        cpu: String::new(),
                    };
                    crate::devices::add(&self.path, &name, device)?;
                    self.catalogue = Catalogue::load(&self.path)?;
                    self.mode = Mode::Chips;
                    self.selected = self
                        .catalogue
                        .devices
                        .keys()
                        .position(|n| n == &name)
                        .unwrap();
                }
            },
            _ => {}
        }
        Ok(Action::None)
    }
    fn result(&mut self, result: Result<Action, String>) -> Action {
        match result {
            Ok(action) => action,
            Err(e) => {
                self.error = e;
                Action::None
            }
        }
    }
    pub fn key(&mut self, key: KeyEvent) -> Action {
        if key.code == KeyCode::Esc {
            if matches!(self.mode, Mode::Add { .. }) {
                self.mode = Mode::Chips;
                self.selected = 0;
                return Action::None;
            }
            return Action::Cancel;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Enter | KeyCode::Char('s'))
        {
            let result = self.activate(APPLY);
            return self.result(result);
        }
        let len = match &self.mode {
            Mode::Chips => self.catalogue.devices.len(),
            Mode::Cores { available, .. } => available.len(),
            Mode::Add { .. } => 5,
        };
        match key.code {
            KeyCode::Up | KeyCode::BackTab => {
                self.selected = (self.selected + len.saturating_sub(1)) % len.max(1)
            }
            KeyCode::Down | KeyCode::Tab => self.selected = (self.selected + 1) % len.max(1),
            _ => match &mut self.mode {
                Mode::Chips => {
                    let result = match key.code {
                        KeyCode::Enter => self.activate(APPLY),
                        KeyCode::Char('n' | 'N') => self.activate(ADD),
                        KeyCode::Char('l' | 'L') => self.activate(LEGACY),
                        _ => Ok(Action::None),
                    };
                    return self.result(result);
                }
                Mode::Cores {
                    available, checked, ..
                } => match key.code {
                    KeyCode::Char(' ') => {
                        if let Some(id) = available.get(self.selected) {
                            if checked.contains(id) {
                                checked.retain(|x| x != id);
                            } else {
                                checked.push(*id);
                            }
                        }
                    }
                    KeyCode::Char('a' | 'A') => *checked = available.clone(),
                    KeyCode::Char('n' | 'N') => checked.clear(),
                    KeyCode::Enter => {
                        let result = self.activate(APPLY);
                        return self.result(result);
                    }
                    _ => {}
                },
                Mode::Add { fields } => {
                    if key.code == KeyCode::Enter {
                        if self.selected < 3 {
                            self.selected += 1;
                        } else {
                            let result =
                                self.activate(if self.selected == 3 { APPLY } else { CANCEL });
                            return self.result(result);
                        }
                    } else if self.selected < 3 {
                        fields[self.selected].key(key);
                    }
                }
            },
        }
        Action::None
    }
    pub fn mouse(&mut self, mouse: MouseEvent) -> Action {
        if matches!(
            mouse.kind,
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
        ) {
            return self.key(KeyEvent::new(
                if mouse.kind == MouseEventKind::ScrollUp {
                    KeyCode::Up
                } else {
                    KeyCode::Down
                },
                KeyModifiers::NONE,
            ));
        }
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return Action::None;
        }
        let hit = self
            .hits
            .iter()
            .find(|(r, _)| r.contains((mouse.column, mouse.row).into()))
            .map(|(_, id)| *id);
        if let Some(id) = hit {
            if id >= NONE {
                let result = self.activate(id);
                return self.result(result);
            }
            self.selected = id;
            if matches!(self.mode, Mode::Cores { .. }) {
                return self.key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
            }
        }
        Action::None
    }
    pub fn help(&self) -> String {
        let instruction = match &self.mode {
            Mode::Chips => {
                "Enter: choose chip. N: add chip to your local catalogue. L: use legacy project cores. Esc: cancel."
            }
            Mode::Cores { .. } => {
                "Space / click: toggle core. A: all. N: none. Enter: apply. At least one core is required. Esc: cancel."
            }
            Mode::Add { .. } => {
                "Enter / Tab: next field. Ctrl+S / Save: append chip. Core IDs: 0,1,2,3. Backend must match Tools / profile; generic reuses a conventional environment. Esc: cancel."
            }
        };
        let error = if self.error.is_empty() {
            String::new()
        } else {
            format!("Error: {}\n", self.error)
        };
        format!(
            "{error}{instruction}\nCatalogue: {}",
            portable_path(&self.path)
        )
    }
    pub fn draw(&mut self, f: &mut Frame, area: Rect) {
        self.hits.clear();
        let title = match &self.mode {
            Mode::Chips => "  Chip catalogue  ".into(),
            Mode::Cores { chip, .. } => format!("  Debug cores: {chip}  "),
            Mode::Add { .. } => "  Add chip to local profile  ".into(),
        };
        let block = theme::card(title, true);
        let inner = block.inner(area);
        f.render_widget(block, area);
        if inner.height < 1 {
            return;
        }
        let labels: Vec<String> = match &self.mode {
            Mode::Chips => self
                .catalogue
                .devices
                .iter()
                .map(|(name, device)| {
                    format!(
                        "{name}   cores {:?}   backend {}",
                        device.cores, device.backend
                    )
                })
                .collect(),
            Mode::Cores {
                available, checked, ..
            } => available
                .iter()
                .map(|id| {
                    format!(
                        "[{}] core.{id}",
                        if checked.contains(id) { "x" } else { " " }
                    )
                })
                .collect(),
            Mode::Add { fields } => ["Chip name", "Core IDs", "Backend"]
                .iter()
                .enumerate()
                .map(|(i, label)| {
                    let value = if i == self.selected {
                        let e = &fields[i];
                        format!("{}|{}", &e.text[..e.cursor], &e.text[e.cursor..])
                    } else {
                        fields[i].text.clone()
                    };
                    format!("{label:<12} {value}")
                })
                .chain(["Save chip".into(), "Cancel".into()])
                .collect(),
        };
        let height = inner.height.saturating_sub(1) as usize;
        let start = self.selected.saturating_sub(height.saturating_sub(1));
        for (row, (i, label)) in labels
            .iter()
            .enumerate()
            .skip(start)
            .take(height)
            .enumerate()
        {
            let hit = Rect::new(inner.x, inner.y + row as u16, inner.width, 1);
            f.render_widget(
                Paragraph::new(format!(
                    "{} {}",
                    if i == self.selected { ">" } else { " " },
                    visible_tail(label, inner.width.saturating_sub(2) as usize)
                ))
                .style(theme::selected(i == self.selected)),
                hit,
            );
            self.hits.push((
                hit,
                if matches!(self.mode, Mode::Add { .. }) && i >= 3 {
                    if i == 3 { APPLY } else { CANCEL }
                } else {
                    i
                },
            ));
        }
        let buttons = match self.mode {
            Mode::Chips => vec![
                ("Select", APPLY),
                ("Add", ADD),
                ("Legacy", LEGACY),
                ("Cancel", CANCEL),
            ],
            Mode::Cores { .. } => vec![
                ("Apply", APPLY),
                ("All", ALL),
                ("None", NONE),
                ("Cancel", CANCEL),
            ],
            Mode::Add { .. } => vec![("Save", APPLY), ("Cancel", CANCEL)],
        };
        let mut x = inner.x;
        for (label, id) in buttons {
            let width = (label.len() as u16 + 2).min(inner.right().saturating_sub(x));
            let hit = Rect::new(x, inner.bottom() - 1, width, 1);
            f.render_widget(
                Paragraph::new(format!(" {label} ")).style(Style::default().bg(theme::RAISED)),
                hit,
            );
            self.hits.push((hit, id));
            x += width + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    #[test]
    fn add_chip_reopen_toggle_cores_and_mouse_apply() {
        let dir = env::temp_dir().join(format!("debugtui-catalogue-ui-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("devices.toml");
        fs::write(&path, crate::devices::DEFAULTS).unwrap();
        let mut view = Devices::at(path.clone(), Selection::default(), false).unwrap();
        view.key(key(KeyCode::Char('n')));
        view.paste("s32k144");
        view.key(key(KeyCode::Enter)); // core IDs default to 0
        view.key(key(KeyCode::Enter)); // backend defaults to generic
        view.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert!(view.error.is_empty(), "{}", view.error);
        let catalogue = Catalogue::load(&path).unwrap();
        assert_eq!(catalogue.devices["s32k144"].cores, [0]);
        assert_eq!(catalogue.devices["s32k144"].backend, "generic");
        assert_eq!(catalogue.devices.len(), 5);
        // Reload the persisted catalogue and choose the physical core, not a list index.
        let selected = Selection {
            chip: "tha6412".into(),
            cores: vec![1],
        };
        let mut view = Devices::at(path.clone(), selected, true).unwrap();
        for (width, height) in [(45, 12), (80, 24), (120, 36)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| view.draw(f, f.area())).unwrap();
            assert!(
                view.hits
                    .iter()
                    .all(|(r, _)| r.right() <= width && r.bottom() <= height)
            );
            assert!(
                terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|c| c.symbol())
                    .collect::<String>()
                    .contains("[x] core.1")
            );
        }
        view.key(key(KeyCode::Char('n')));
        assert!(matches!(view.key(key(KeyCode::Enter)), Action::None));
        assert!(view.error.contains("at least one"));
        view.key(key(KeyCode::Char('a')));
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| view.draw(f, f.area())).unwrap();
        let hit = view.hits.iter().find(|(_, id)| *id == 2).unwrap().0;
        let click = |hit: Rect| MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: hit.x,
            row: hit.y,
            modifiers: KeyModifiers::NONE,
        };
        view.mouse(click(hit));
        let apply = view.hits.iter().find(|(_, id)| *id == APPLY).unwrap().0;
        let Action::Select(selection) = view.mouse(click(apply)) else {
            panic!("Expected selection");
        };
        assert_eq!(selection.chip, "tha6412");
        assert_eq!(selection.cores, [0, 1, 3]);
        let before = fs::read(&path).unwrap();
        view.key(key(KeyCode::Esc));
        assert_eq!(before, fs::read(&path).unwrap());
        fs::remove_dir_all(dir).unwrap();
    }
}
