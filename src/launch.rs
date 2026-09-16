//! Project selection and launch configuration. No debugger processes are started here.
use crate::config::{Project, portable_path};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
};
use std::{
    env, fs,
    path::{Component, Path, PathBuf},
};

const LABELS: [&str; 15] = [
    "Project",
    "Tools / profile",
    "Program / ELF",
    "Source root",
    "GDB executable",
    "GDB arguments",
    "Target mode",
    "Endpoint",
    "Start service",
    "Timeout (ms)",
    "On exit",
    "Log directory",
    "Save to project",
    "Start debugging",
    "Save configuration",
];
const HINTS: [&str; 15] = [
    "Select a project directory or a debug.toml file. F2 browses files.",
    "Select project-local tools or an environment TOML; blank uses standalone GDB.",
    "Executable with debug symbols. Optional for a remote target. F2 browses files.",
    "Source directory; source mappings in debug.toml are preserved.",
    "GDB executable path or a command on PATH. F2 browses files.",
    "Additional arguments as a JSON array, e.g. [\"--data-directory=C:/gdb/data\"].",
    "Left/Right or Enter: remote, extended-remote, local.",
    "Remote target address, e.g. localhost:3333. Local mode does not use it.",
    "Use the profile's service, or connect to a service started externally.",
    "Timeout for a GDB command; must be a positive number.",
    "Detach, resume then detach, or disconnect. Final behavior depends on the server.",
    "Optional directory for MI and server logs. Paths are relative to the project.",
    "Save launch settings in the project before connecting; No keeps this session temporary.",
    "F5 starts debugging. Switching projects ends the current session first.",
    "Ctrl+S saves settings without connecting. Tools profiles are never rewritten.",
];

fn absolute(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        base.join(path)
    }
}

fn visible_tail(text: &str, cells: usize) -> String {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
    if text.width() <= cells {
        return text.to_owned();
    }
    let mut used = 1;
    let tail: String = text
        .chars()
        .rev()
        .take_while(|c| {
            used += c.width().unwrap_or(0);
            used <= cells
        })
        .collect();
    format!("…{}", tail.chars().rev().collect::<String>())
}

/// Prefer paths relative to the project so it can be moved with its tools.
pub fn relative_path(base: &Path, path: &Path) -> String {
    let base = PathBuf::from(portable_path(base));
    let path = PathBuf::from(portable_path(path));
    let a: Vec<_> = base.components().collect();
    let b: Vec<_> = path.components().collect();
    let common = a.iter().zip(&b).take_while(|(a, b)| a == b).count();
    if common == 0
        || a.iter()
            .chain(&b)
            .any(|c| matches!(c, Component::ParentDir))
    {
        return portable_path(&path);
    }
    let mut result = PathBuf::new();
    for _ in &a[common..] {
        result.push("..");
    }
    for c in &b[common..] {
        result.push(c.as_os_str());
    }
    if result.as_os_str().is_empty() {
        ".".into()
    } else {
        portable_path(&result)
    }
}

#[derive(Clone)]
pub struct Document {
    pub path: PathBuf,
    raw: toml::Value,
    original: Option<toml::Value>,
    pub discovered: bool,
}
impl Document {
    pub fn empty(path: PathBuf) -> Self {
        let mut doc = Self {
            path,
            raw: toml::Value::Table(toml::Table::new()),
            original: None,
            discovered: false,
        };
        doc.raw
            .as_table_mut()
            .unwrap()
            .insert("version".into(), toml::Value::Integer(2));
        doc
    }
    pub fn open(path: &Path) -> Result<Self, String> {
        let path = absolute(&env::current_dir().map_err(|e| e.to_string())?, path);
        let path = if path.is_dir() {
            path.join("debug.toml")
        } else {
            path
        };
        let parent = fs::canonicalize(path.parent().ok_or("Project needs a parent directory")?)
            .map_err(|e| format!("Project directory: {e}"))?;
        let path = parent.join(path.file_name().ok_or("Project needs a file name")?);
        let mut doc = Self::empty(path);
        if doc.path.exists() {
            let raw = fs::read_to_string(&doc.path).map_err(|e| e.to_string())?;
            doc.raw = toml::from_str(raw.trim_start_matches('\u{feff}'))
                .map_err(|e| format!("Project: {e}"))?;
            doc.original = Some(doc.raw.clone());
        }
        // Discover only the selected project's tools, never the TUI installation's tools.
        if doc.raw.get("tools").is_none()
            && doc.raw.get("gdb").is_none()
            && doc.base().join("tools/debug-env.toml").is_file()
        {
            doc.set("tools", "profile", "./tools/debug-env.toml".into());
            doc.discovered = true;
        }
        Ok(doc)
    }
    pub fn base(&self) -> &Path {
        self.path.parent().unwrap_or(Path::new("."))
    }
    pub fn project(&self) -> Result<Project, String> {
        Project::from_document(self.raw.clone(), Some(self.path.clone()), None)
    }
    pub fn set(&mut self, section: &str, name: &str, value: toml::Value) {
        let table = self.raw.as_table_mut().unwrap();
        let target = table
            .entry(section)
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        if !target.is_table() {
            *target = toml::Value::Table(toml::Table::new());
        }
        target.as_table_mut().unwrap().insert(name.into(), value);
    }
    pub fn set_path(&mut self, section: &str, key: &str, path: &Path) {
        let mut value = relative_path(self.base(), path);
        if key == "executable" && !value.contains(['/', '\\']) {
            value = format!("./{value}");
        }
        self.set(section, key, value.into());
    }
    pub fn environment(&mut self, input: &str) {
        self.discovered = false;
        self.raw
            .as_table_mut()
            .unwrap()
            .insert("tools".into(), toml::Value::Table(toml::Table::new()));
        if !input.is_empty() {
            let path = absolute(self.base(), Path::new(input));
            let path = if path.is_dir() {
                path.join("debug-env.toml")
            } else {
                path
            };
            self.set_path("tools", "profile", &path);
        }
    }
    pub fn save(&mut self) -> Result<(), String> {
        self.project()?;
        if self.path.exists() {
            let text = fs::read_to_string(&self.path).map_err(|e| e.to_string())?;
            let current: toml::Value =
                toml::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
            let mut current_settings = current.clone();
            let mut original_settings = self
                .original
                .clone()
                .ok_or("Project file appeared on disk; reload it before saving")?;
            for key in ["watch", "breakpoints"] {
                current_settings
                    .as_table_mut()
                    .ok_or("Project must be a table")?
                    .remove(key);
                original_settings
                    .as_table_mut()
                    .ok_or("Project must be a table")?
                    .remove(key);
                if let Some(value) = current.get(key) {
                    self.raw
                        .as_table_mut()
                        .unwrap()
                        .insert(key.into(), value.clone());
                }
            }
            if current_settings != original_settings {
                return Err("Project changed on disk; select it again before saving".into());
            }
        }
        let text = toml::to_string_pretty(&self.raw).map_err(|e| e.to_string())?;
        fs::write(&self.path, text).map_err(|e| format!("Save {}: {e}", self.path.display()))?;
        self.original = Some(self.raw.clone());
        Ok(())
    }
}

pub struct Launch {
    pub document: Document,
    pub save: bool,
}

struct Editor {
    text: String,
    cursor: usize,
}
impl Editor {
    fn new(text: String) -> Self {
        let cursor = text.len();
        Self { text, cursor }
    }
    fn insert(&mut self, text: &str) {
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        self.text.insert_str(self.cursor, &text);
        self.cursor += text.len();
    }
    fn key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.text.len(),
            KeyCode::Left => {
                self.cursor = self.text[..self.cursor]
                    .char_indices()
                    .last()
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            }
            KeyCode::Right => {
                self.cursor += self.text[self.cursor..]
                    .chars()
                    .next()
                    .map(char::len_utf8)
                    .unwrap_or(0)
            }
            KeyCode::Backspace if self.cursor > 0 => {
                let previous = self.text[..self.cursor].char_indices().last().unwrap().0;
                self.text.replace_range(previous..self.cursor, "");
                self.cursor = previous;
            }
            KeyCode::Delete if self.cursor < self.text.len() => {
                let end = self.cursor + self.text[self.cursor..].chars().next().unwrap().len_utf8();
                self.text.replace_range(self.cursor..end, "");
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.text.clear();
                self.cursor = 0;
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.insert(&c.to_string())
            }
            _ => {}
        }
    }
}

struct Browser {
    directory: PathBuf,
    entries: Vec<PathBuf>,
    selected: usize,
}
impl Browser {
    fn open(path: &Path) -> Result<Self, String> {
        let directory = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(Path::new("."))
        };
        let mut b = Self {
            directory: fs::canonicalize(directory).map_err(|e| e.to_string())?,
            entries: vec![],
            selected: 0,
        };
        b.reload()?;
        Ok(b)
    }
    fn reload(&mut self) -> Result<(), String> {
        let mut entries = fs::read_dir(&self.directory)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .collect::<Vec<_>>();
        entries.sort_by_key(|p| {
            (
                !p.is_dir(),
                p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase(),
            )
        });
        if let Some(parent) = self.directory.parent() {
            entries.insert(0, parent.to_owned());
        }
        self.entries = entries;
        self.selected = 0;
        Ok(())
    }
}

pub struct Setup {
    pub document: Document,
    pub message: String,
    pub save: bool,
    pub pending: bool,
    selected: usize,
    editor: Option<Editor>,
    browser: Option<Browser>,
}
impl Setup {
    pub fn new(document: Document) -> Self {
        Self {
            document,
            message: "Choose a project and environment, then press F5.".into(),
            save: true,
            pending: false,
            selected: 0,
            editor: None,
            browser: None,
        }
    }
    fn values(&self) -> Vec<String> {
        let p = self.document.project().unwrap_or_default();
        let path = |p: &Path| {
            if p.as_os_str().is_empty() {
                String::new()
            } else {
                relative_path(self.document.base(), p)
            }
        };
        let profile = self
            .document
            .raw
            .get("tools")
            .and_then(|t| t.get("profile").or_else(|| t.get("root")))
            .and_then(toml::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        vec![
            portable_path(&self.document.path),
            profile,
            path(&p.program.elf),
            path(&p.program.source_root),
            if p.gdb.executable.is_absolute() {
                path(&p.gdb.executable)
            } else {
                portable_path(&p.gdb.executable)
            },
            serde_json::to_string(&p.gdb.args).unwrap_or_default(),
            p.target.mode,
            p.target.endpoint,
            if p.service.as_ref().is_some_and(|s| s.enabled) {
                "Yes (profile service)"
            } else {
                "No (external / local)"
            }
            .into(),
            p.session.timeout_ms.to_string(),
            p.session.on_exit,
            p.session.log_dir.as_deref().map(path).unwrap_or_default(),
            if self.save {
                "Yes"
            } else {
                "No (connect once)"
            }
            .into(),
            "F5 / Enter".into(),
            "Ctrl+S / Enter".into(),
        ]
    }
    fn set_value(&mut self, value: &str) -> Result<(), String> {
        let value = value.trim().trim_matches('"');
        if self.selected == 0 {
            let doc = Document::open(&absolute(self.document.base(), Path::new(value)))?;
            self.document = doc;
            self.document.project()?;
            return Ok(());
        }
        let mut doc = self.document.clone();
        match self.selected {
            1 => doc.environment(value),
            2 | 3 => {
                let key = if self.selected == 2 {
                    "elf"
                } else {
                    "source_root"
                };
                if value.is_empty() {
                    doc.set("program", key, "".into());
                } else {
                    let path = absolute(doc.base(), Path::new(value));
                    doc.set_path("program", key, &path);
                }
            }
            4 => {
                if value.contains(['/', '\\']) {
                    let path = absolute(doc.base(), Path::new(value));
                    doc.set_path("gdb", "executable", &path);
                } else {
                    doc.set("gdb", "executable", value.into());
                }
            }
            5 => {
                let args: Vec<String> =
                    serde_json::from_str(value).map_err(|e| format!("GDB arguments: {e}"))?;
                doc.set(
                    "gdb",
                    "args",
                    toml::Value::Array(args.into_iter().map(toml::Value::String).collect()),
                );
            }
            6 => {
                doc.set("target", "mode", value.into());
                if value == "local" {
                    doc.set("service", "enabled", false.into());
                }
            }
            7 => doc.set("target", "endpoint", value.into()),
            8 => doc.set("service", "enabled", (value == "Yes").into()),
            9 => doc.set(
                "session",
                "timeout_ms",
                toml::Value::Integer(
                    value
                        .parse::<i64>()
                        .map_err(|_| "Timeout must be a positive integer")?,
                ),
            ),
            10 => doc.set("session", "on_exit", value.into()),
            11 => {
                if value.is_empty() {
                    if let Some(t) = doc
                        .raw
                        .get_mut("session")
                        .and_then(toml::Value::as_table_mut)
                    {
                        t.remove("log_dir");
                    }
                } else {
                    let path = absolute(doc.base(), Path::new(value));
                    doc.set_path("session", "log_dir", &path);
                }
            }
            _ => {}
        }
        // Retain an invalid profile selection so its error is visible and can be corrected.
        if self.selected == 1 {
            self.document = doc;
            self.document.project()?;
        } else {
            doc.project()?;
            self.document = doc;
        }
        Ok(())
    }
    fn cycle(&mut self, backwards: bool) -> Result<(), String> {
        if self.selected == 12 {
            self.save = !self.save;
            return Ok(());
        }
        let values: &[&str] = match self.selected {
            6 => &["remote", "extended-remote", "local"],
            8 => &["No", "Yes"],
            10 => &["detach", "resume", "disconnect"],
            _ => return Ok(()),
        };
        let current = self.values()[self.selected].clone();
        let n = values
            .iter()
            .position(|v| current.starts_with(v))
            .unwrap_or(0);
        self.set_value(values[(n + if backwards { values.len() - 1 } else { 1 }) % values.len()])
    }
    pub fn paste(&mut self, text: &str) {
        if let Some(e) = &mut self.editor {
            e.insert(text);
        }
    }
    pub fn is_editing(&self) -> bool {
        self.editor.is_some() || self.browser.is_some()
    }
    pub fn key(&mut self, key: KeyEvent) -> Option<Launch> {
        if self.pending {
            return None;
        }
        let result = self.handle_key(key);
        match result {
            Ok(launch) => launch,
            Err(e) => {
                self.message = format!("Error: {e}");
                None
            }
        }
    }
    fn handle_key(&mut self, key: KeyEvent) -> Result<Option<Launch>, String> {
        if let Some(browser) = &mut self.browser {
            match key.code {
                KeyCode::Esc => self.browser = None,
                KeyCode::Up => browser.selected = browser.selected.saturating_sub(1),
                KeyCode::Down => {
                    browser.selected =
                        (browser.selected + 1).min(browser.entries.len().saturating_sub(1))
                }
                KeyCode::PageUp => browser.selected = browser.selected.saturating_sub(10),
                KeyCode::PageDown => {
                    browser.selected =
                        (browser.selected + 10).min(browser.entries.len().saturating_sub(1))
                }
                KeyCode::Backspace => {
                    if let Some(parent) = browser.directory.parent() {
                        *browser = Browser::open(parent)?;
                    }
                }
                KeyCode::Enter => {
                    if let Some(path) = browser.entries.get(browser.selected).cloned() {
                        if path.is_dir() {
                            *browser = Browser::open(&path)?;
                        } else {
                            self.choose_path(&path)?;
                        }
                    }
                }
                KeyCode::Char(' ') if matches!(self.selected, 0 | 1 | 3 | 11) => {
                    let path = browser.directory.clone();
                    self.choose_path(&path)?;
                }
                _ => {}
            }
            return Ok(None);
        }
        if let Some(editor) = &mut self.editor {
            match key.code {
                KeyCode::Esc => self.editor = None,
                KeyCode::Enter => {
                    let value = editor.text.clone();
                    self.set_value(&value)?;
                    self.editor = None;
                    self.message = "Updated. F5 starts debugging; Ctrl+S saves.".into();
                }
                _ => editor.key(key),
            }
            return Ok(None);
        }
        match key.code {
            KeyCode::Up | KeyCode::BackTab => {
                self.selected = (self.selected + LABELS.len() - 1) % LABELS.len()
            }
            KeyCode::Down | KeyCode::Tab => self.selected = (self.selected + 1) % LABELS.len(),
            KeyCode::F(2) if matches!(self.selected, 0..=4 | 11) => {
                let value = self.values()[self.selected].clone();
                let path = absolute(self.document.base(), Path::new(&value));
                self.browser = Some(Browser::open(if path.exists() {
                    &path
                } else {
                    self.document.base()
                })?);
            }
            KeyCode::Left | KeyCode::Right => self.cycle(key.code == KeyCode::Left)?,
            KeyCode::Enter if matches!(self.selected, 6 | 8 | 10 | 12) => self.cycle(false)?,
            KeyCode::Enter if self.selected < 13 => {
                self.editor = Some(Editor::new(self.values()[self.selected].clone()))
            }
            KeyCode::F(5) | KeyCode::Enter if key.code == KeyCode::F(5) || self.selected == 13 => {
                let mut project = self.document.project()?;
                project.prepare()?;
                self.pending = true;
                self.message =
                    "Closing the previous session and preparing the selected project...".into();
                return Ok(Some(Launch {
                    document: self.document.clone(),
                    save: self.save,
                }));
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.save_document()?
            }
            KeyCode::Enter if self.selected == 14 => self.save_document()?,
            _ => {}
        }
        Ok(None)
    }
    fn save_document(&mut self) -> Result<(), String> {
        self.document.save()?;
        self.message = format!("Saved {}", self.document.path.display());
        Ok(())
    }
    fn choose_path(&mut self, path: &Path) -> Result<(), String> {
        let mut value = if self.selected == 0 {
            portable_path(path)
        } else {
            relative_path(self.document.base(), path)
        };
        if self.selected == 4 && !value.contains(['/', '\\']) {
            value = format!("./{value}");
        }
        self.browser = None;
        self.set_value(&value)?;
        self.message = "Selected. F5 starts debugging; Ctrl+S saves.".into();
        Ok(())
    }
    pub fn draw(&self, f: &mut Frame) {
        let area = f.area();
        if area.width < 45 || area.height < 12 {
            f.render_widget(
                Paragraph::new("DebugTUI\nEnlarge terminal to at least 45 x 12.\nCtrl+Q exits."),
                area,
            );
            return;
        }
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(3),
                Constraint::Length(3),
                Constraint::Length(2),
                Constraint::Length(2),
            ])
            .split(area);
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    " DebugTUI  /  Launch setup",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::raw(" Select a project, configure its environment, start debugging."),
            ]),
            rows[0],
        );
        if let Some(browser) = &self.browser {
            let height = rows[1].height.saturating_sub(2) as usize;
            let start = browser.selected.saturating_sub(height.saturating_sub(1));
            let lines = browser
                .entries
                .iter()
                .enumerate()
                .skip(start)
                .map(|(i, p)| {
                    let name = if Some(p.as_path()) == browser.directory.parent() {
                        "..".into()
                    } else {
                        p.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned()
                    };
                    Line::styled(
                        format!(
                            "{} {} {name}",
                            if i == browser.selected { ">" } else { " " },
                            if p.is_dir() { "[DIR] " } else { "[FILE]" }
                        ),
                        Style::default().fg(if i == browser.selected {
                            Color::Cyan
                        } else {
                            Color::Reset
                        }),
                    )
                })
                .collect::<Vec<_>>();
            f.render_widget(
                Paragraph::new(lines).block(
                    Block::bordered().title(format!(" {} ", portable_path(&browser.directory))),
                ),
                rows[1],
            );
            f.render_widget(Paragraph::new(" Enter: open directory / select file   Space: select current directory\n Backspace: parent   Esc: cancel").wrap(Wrap { trim: false }), rows[2]);
        } else {
            let height = rows[1].height.saturating_sub(2) as usize;
            let start = self.selected.saturating_sub(height.saturating_sub(1));
            let values = self.values();
            let lines = LABELS
                .iter()
                .enumerate()
                .skip(start)
                .map(|(i, label)| {
                    let value = if i == self.selected {
                        self.editor
                            .as_ref()
                            .map(|e| e.text.as_str())
                            .unwrap_or(&values[i])
                    } else {
                        &values[i]
                    };
                    let shown = if value.is_empty() { "(not set)" } else { value };
                    let prefix = format!(
                        "{} {label:<18} ",
                        if i == self.selected { ">" } else { " " }
                    );
                    let available = rows[1].width.saturating_sub(prefix.len() as u16 + 2) as usize;
                    let shown = if let Some(e) = &self.editor
                        && i == self.selected
                    {
                        let before = &e.text[..e.cursor];
                        let tail: String = before
                            .chars()
                            .rev()
                            .take(available.saturating_sub(1) / 2)
                            .collect::<String>()
                            .chars()
                            .rev()
                            .collect();
                        format!("{tail}▏{}", &e.text[e.cursor..])
                    } else {
                        visible_tail(shown, available)
                    };
                    Line::styled(
                        format!("{prefix}{shown}"),
                        Style::default().fg(if i == self.selected {
                            Color::Cyan
                        } else {
                            Color::Reset
                        }),
                    )
                })
                .collect::<Vec<_>>();
            f.render_widget(
                Paragraph::new(lines).block(Block::bordered().title(" Session configuration ")),
                rows[1],
            );
            f.render_widget(
                Paragraph::new(format!(" {}", HINTS[self.selected]))
                    .wrap(Wrap { trim: false })
                    .style(Style::default().fg(Color::DarkGray)),
                rows[2],
            );
        }
        f.render_widget(
            Paragraph::new(format!(" {}", self.message))
                .wrap(Wrap { trim: false })
                .style(Style::default().fg(if self.message.starts_with("Error") {
                    Color::Red
                } else {
                    Color::Green
                })),
            rows[3],
        );
        f.render_widget(Paragraph::new(if self.editor.is_some() {
            " Enter: apply  Esc: cancel  Home/End/Arrows: move  Ctrl+U: clear"
        } else {
            " Tab/Arrows: select  Enter: edit  F2: browse\n F5: start  Ctrl+S: save  Esc: workspace  Ctrl+Q: quit"
        }).style(Style::default().fg(Color::DarkGray)), rows[4]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = env::temp_dir().join(format!(
                "debugtui-launch-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(root.join("tools")).unwrap();
            Self(root)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn project_local_profile_stays_relative_and_preserves_project_settings() {
        let fixture = Fixture::new();
        fs::write(
            fixture.0.join("tools/debug-env.toml"),
            "[gdb]\nexecutable='./gdb.exe'\n[target]\nendpoint='localhost:3333'\n",
        )
        .unwrap();
        let mut doc = Document::open(&fixture.0).unwrap();
        assert!(doc.discovered);
        let project = doc.project().unwrap();
        assert!(project.gdb.executable.ends_with("tools/gdb.exe"));
        doc.set_path("program", "elf", &fixture.0.join("app.elf"));
        doc.save().unwrap();
        let text = fs::read_to_string(&doc.path).unwrap();
        assert!(text.contains("./tools/debug-env.toml"));
        assert!(text.contains("app.elf"));
        assert!(!text.contains("executable"));
        assert!(!text.contains("debugtui-launch-"));
        // The running session can save its preferences before a project switch.
        project
            .save_preferences(vec!["counter".into()], vec!["main".into()])
            .unwrap();
        doc.set("target", "endpoint", "localhost:4444".into());
        doc.save().unwrap();
        let p = Project::load(&doc.path).unwrap();
        assert_eq!(p.watch, ["counter"]);
        assert_eq!(p.breakpoints, ["main"]);
        assert_eq!(p.target.endpoint, "localhost:4444");
        // Unrelated external edits must not be overwritten by the form.
        fs::write(&doc.path, "version=2\nwatch=['changed']\n").unwrap();
        assert!(doc.save().unwrap_err().contains("changed on disk"));
    }

    #[test]
    fn setup_browses_project_edits_connection_and_returns_launch() {
        let fixture = Fixture::new();
        let doc = Document::open(&fixture.0).unwrap();
        let mut setup = Setup::new(doc);
        setup.key(key(KeyCode::F(2)));
        assert!(setup.browser.is_some());
        setup.key(key(KeyCode::Char(' '))); // select current directory
        assert!(setup.browser.is_none());
        assert!(setup.key(key(KeyCode::F(5))).is_none());
        assert!(setup.message.starts_with("Error")); // no endpoint
        setup.selected = 7;
        setup.key(key(KeyCode::Enter));
        setup.paste("localhost:3333");
        setup.key(key(KeyCode::Enter));
        setup.selected = 12;
        setup.key(key(KeyCode::Enter));
        let launch = setup.key(key(KeyCode::F(5))).unwrap();
        assert!(!launch.save);
        assert_eq!(
            launch.document.project().unwrap().target.endpoint,
            "localhost:3333"
        );
        assert!(!setup.document.path.exists()); // selecting / starting does not write early
    }

    #[test]
    fn selecting_a_new_project_drops_previous_environment_and_unicode_edit_is_safe() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("second")).unwrap();
        let mut first = Document::open(&fixture.0).unwrap();
        first.set(
            "gdb",
            "args",
            toml::Value::Array(vec!["--first-only".into()]),
        );
        let mut setup = Setup::new(first);
        setup.set_value("second").unwrap();
        assert!(setup.document.project().unwrap().gdb.args.is_empty());
        let mut editor = Editor::new("中文x".into());
        editor.key(key(KeyCode::Left));
        editor.key(key(KeyCode::Backspace));
        editor.insert("工程");
        assert_eq!(editor.text, "中工程x");
        editor.key(key(KeyCode::Home));
        editor.key(key(KeyCode::Delete));
        assert_eq!(editor.text, "工程x");
    }

    #[test]
    fn setup_renders_all_fields_and_browser_in_small_and_large_terminals() {
        let fixture = Fixture::new();
        for (width, height) in [(45, 12), (80, 24), (120, 36), (180, 50)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let mut setup = Setup::new(Document::open(&fixture.0).unwrap());
            for (selected, label) in LABELS.iter().enumerate() {
                setup.selected = selected;
                terminal.draw(|f| setup.draw(f)).unwrap();
                let text = terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|c| c.symbol())
                    .collect::<String>();
                assert!(
                    text.contains(label),
                    "missing field {} at {width}x{height}",
                    LABELS[selected]
                );
            }
            setup.selected = 0;
            setup.key(key(KeyCode::F(2)));
            terminal.draw(|f| setup.draw(f)).unwrap();
        }
    }
}
