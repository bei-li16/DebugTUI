//! Project selection and launch configuration. No debugger processes are started here.
use crate::{
    config::{Project, portable_path},
    theme,
};
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
};
use std::{
    env, fs,
    path::{Component, Path, PathBuf},
};

const LABELS: [&str; 18] = [
    "Project",
    "Tools / profile",
    "Program / ELF",
    "Source root",
    "Build command",
    "Download command",
    "GDB executable",
    "GDB arguments",
    "Target mode",
    "Endpoint",
    "Start service",
    "Timeout (ms)",
    "On exit",
    "Log directory",
    "SVD file",
    "Save to project",
    "Start debugging",
    "Save configuration",
];
const HINTS: [&str; 18] = [
    "Select a project directory or a debug.toml file. F2 browses files.",
    "Select project-local tools or an environment TOML; blank uses standalone GDB.",
    "Executable with debug symbols. Optional for a remote target. F2 browses files.",
    "Source directory and working directory for Build / Download commands. Blank uses the project directory.",
    "Shell command in Source root, e.g. build.bat or cmake --build build. Blank uses legacy [build], if configured.",
    "Shell command in Source root, e.g. flash.bat. Blank uses the tools profile's GDB download action.",
    "GDB executable path or a command on PATH. F2 browses files.",
    "Additional arguments as a JSON array, e.g. [\"--data-directory=C:/gdb/data\"].",
    "Left/Right or Enter: remote, extended-remote, local.",
    "Remote target address, e.g. localhost:3333. Local mode does not use it.",
    "Use the profile's service, or connect to a service started externally.",
    "Timeout for a GDB command; must be a positive number.",
    "Detach, resume then detach, or disconnect. Final behavior depends on the server.",
    "Optional directory for MI and server logs. Paths are relative to the project.",
    "Optional CMSIS-SVD for peripheral registers. F2 browses files; relative to the project. Blank disables it.",
    "Save launch settings in the project before connecting; No keeps this session temporary.",
    "Click Start debugging, Enter, Ctrl+R or F5. The previous session ends before the new one starts.",
    "Ctrl+S saves settings without connecting. Tools profiles are never rewritten.",
];
const START: usize = 16;
const SAVE: usize = 17;
const WORKSPACE: usize = 18;

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
    pub fn is_modified(&self) -> bool {
        self.original.as_ref() != Some(&self.raw)
    }
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
        let _guard = crate::config::PREFERENCE_WRITE
            .lock()
            .map_err(|e| e.to_string())?;
        if self.path.exists() {
            let text = fs::read_to_string(&self.path).map_err(|e| e.to_string())?;
            let current: toml::Value =
                toml::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
            let mut current_settings = current.clone();
            let mut original_settings = self
                .original
                .clone()
                .ok_or("Project file appeared on disk; reload it before saving")?;
            // Runtime display preferences can change while the setup form is open.
            for key in ["watch", "breakpoints", "ui"] {
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
            // A worker saves its own Watch/breakpoint list under [[cores]].
            // Treat those lists like the root runtime preferences above.
            for settings in [&mut current_settings, &mut original_settings] {
                if let Some(cores) = settings
                    .get_mut("cores")
                    .and_then(toml::Value::as_array_mut)
                {
                    for core in cores {
                        if let Some(core) = core.as_table_mut() {
                            core.remove("watch");
                            core.remove("breakpoints");
                        }
                    }
                }
            }
            if let Some(cores) = self
                .raw
                .get_mut("cores")
                .and_then(toml::Value::as_array_mut)
            {
                for core in cores {
                    let current_core = current
                        .get("cores")
                        .and_then(toml::Value::as_array)
                        .and_then(|list| list.iter().find(|c| c.get("name") == core.get("name")));
                    if let Some(current_core) = current_core {
                        for key in ["watch", "breakpoints"] {
                            if let Some(value) = current_core.get(key) {
                                core.as_table_mut()
                                    .unwrap()
                                    .insert(key.into(), value.clone());
                            }
                        }
                    }
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
    pub workspace_requested: bool,
    selected: usize,
    editor: Option<Editor>,
    browser: Option<Browser>,
    action_hits: Vec<(Rect, usize)>,
    row_hits: Vec<(Rect, usize)>,
}
impl Setup {
    pub fn new(document: Document) -> Self {
        Self {
            document,
            message: "Review the configuration, then click Start debugging or press Enter.".into(),
            save: true,
            pending: false,
            selected: START,
            workspace_requested: false,
            editor: None,
            browser: None,
            action_hits: vec![],
            row_hits: vec![],
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
            p.tasks.build,
            p.tasks.download,
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
            path(&p.program.svd),
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
        // Shell commands must retain their executable/argument quotes verbatim.
        let value = if matches!(self.selected, 4 | 5) {
            value.trim()
        } else {
            value.trim().trim_matches('"')
        };
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
            4 | 5 => doc.set(
                "tasks",
                if self.selected == 4 {
                    "build"
                } else {
                    "download"
                },
                value.into(),
            ),
            6 => {
                if value.contains(['/', '\\']) {
                    let path = absolute(doc.base(), Path::new(value));
                    doc.set_path("gdb", "executable", &path);
                } else {
                    doc.set("gdb", "executable", value.into());
                }
            }
            7 => {
                let args: Vec<String> =
                    serde_json::from_str(value).map_err(|e| format!("GDB arguments: {e}"))?;
                doc.set(
                    "gdb",
                    "args",
                    toml::Value::Array(args.into_iter().map(toml::Value::String).collect()),
                );
            }
            8 => {
                doc.set("target", "mode", value.into());
                if value == "local" {
                    doc.set("service", "enabled", false.into());
                }
            }
            9 => doc.set("target", "endpoint", value.into()),
            10 => doc.set("service", "enabled", (value == "Yes").into()),
            11 => doc.set(
                "session",
                "timeout_ms",
                toml::Value::Integer(
                    value
                        .parse::<i64>()
                        .map_err(|_| "Timeout must be a positive integer")?,
                ),
            ),
            12 => doc.set("session", "on_exit", value.into()),
            13 => {
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
            14 => {
                if value.is_empty() {
                    doc.set("program", "svd", "".into());
                } else {
                    let path = absolute(doc.base(), Path::new(value));
                    doc.set_path("program", "svd", &path);
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
        if self.selected == 15 {
            self.save = !self.save;
            return Ok(());
        }
        let values: &[&str] = match self.selected {
            8 => &["remote", "extended-remote", "local"],
            10 => &["No", "Yes"],
            12 => &["detach", "resume", "disconnect"],
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
    pub fn key(&mut self, key: KeyEvent) -> Option<Launch> {
        if self.pending || key.kind == KeyEventKind::Release {
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
        if self.browser.is_none() {
            if key.code == KeyCode::F(5)
                || (key.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(key.code, KeyCode::Char('r') | KeyCode::Enter))
            {
                return self.start();
            }
            if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
                self.commit_editor()?;
                self.save_document()?;
                return Ok(None);
            }
        }
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
                KeyCode::Char(' ') if matches!(self.selected, 0 | 1 | 3 | 13) => {
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
                    self.message = "Updated. Click Start or press Ctrl+R; Ctrl+S saves.".into();
                }
                _ => editor.key(key),
            }
            return Ok(None);
        }
        match key.code {
            KeyCode::Up | KeyCode::BackTab => {
                self.selected = (self.selected + WORKSPACE) % (WORKSPACE + 1)
            }
            KeyCode::Down | KeyCode::Tab => self.selected = (self.selected + 1) % (WORKSPACE + 1),
            KeyCode::F(2) if matches!(self.selected, 0..=3 | 6 | 13 | 14) => {
                let value = self.values()[self.selected].clone();
                let path = absolute(self.document.base(), Path::new(&value));
                self.browser = Some(Browser::open(if path.exists() {
                    &path
                } else {
                    self.document.base()
                })?);
            }
            KeyCode::Left | KeyCode::Right => self.cycle(key.code == KeyCode::Left)?,
            KeyCode::Enter if matches!(self.selected, 8 | 10 | 12 | 15) => self.cycle(false)?,
            KeyCode::Enter if self.selected < START => {
                self.editor = Some(Editor::new(self.values()[self.selected].clone()))
            }
            KeyCode::Enter if self.selected == START => return self.start(),
            KeyCode::Enter if self.selected == SAVE => self.save_document()?,
            KeyCode::Enter if self.selected == WORKSPACE => self.workspace_requested = true,
            KeyCode::Esc => self.workspace_requested = true,
            _ => {}
        }
        Ok(None)
    }
    fn commit_editor(&mut self) -> Result<(), String> {
        if let Some(editor) = &self.editor {
            let value = editor.text.clone();
            self.set_value(&value)?;
            self.editor = None;
        }
        Ok(())
    }
    fn start(&mut self) -> Result<Option<Launch>, String> {
        self.commit_editor()?;
        let mut project = self.document.project()?;
        project.prepare_workspace()?;
        if !project.program.svd.as_os_str().is_empty() {
            crate::svd::Device::load(&project.program.svd)?;
        }
        self.pending = true;
        self.message = "Closing the previous session and preparing the selected project...".into();
        Ok(Some(Launch {
            document: self.document.clone(),
            save: self.save,
        }))
    }
    pub fn mouse(&mut self, mouse: MouseEvent) -> Option<Launch> {
        if self.pending {
            return None;
        }
        let point = (mouse.column, mouse.row).into();
        if matches!(
            mouse.kind,
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
        ) {
            if self.row_hits.iter().any(|(rect, _)| rect.contains(point)) {
                let code = if mouse.kind == MouseEventKind::ScrollDown {
                    KeyCode::Down
                } else {
                    KeyCode::Up
                };
                return self.key(KeyEvent::new(code, KeyModifiers::NONE));
            }
            return None;
        }
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return None;
        }
        if let Some((_, action)) = self
            .action_hits
            .iter()
            .find(|(rect, _)| rect.contains(point))
            .copied()
        {
            if self.browser.is_some() {
                return None;
            }
            if let Err(error) = self.commit_editor() {
                self.message = format!("Error: {error}");
                return None;
            }
            self.selected = action;
            return self.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        }
        if let Some((_, index)) = self
            .row_hits
            .iter()
            .find(|(rect, _)| rect.contains(point))
            .copied()
        {
            if let Some(browser) = &mut self.browser {
                browser.selected = index;
            } else {
                if let Err(error) = self.commit_editor() {
                    self.message = format!("Error: {error}");
                    return None;
                }
                self.selected = index;
            }
            return self.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        }
        None
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
        if self.selected == 6 && !value.contains(['/', '\\']) {
            value = format!("./{value}");
        }
        self.browser = None;
        self.set_value(&value)?;
        self.message = "Selected. Click Start or press Ctrl+R; Ctrl+S saves.".into();
        Ok(())
    }
    pub fn draw(&mut self, f: &mut Frame) {
        self.action_hits.clear();
        self.row_hits.clear();
        let screen = f.area();
        f.render_widget(Block::default().style(theme::base()), screen);
        let width = screen.width.min(122);
        let height = screen.height.min(31);
        let area = Rect::new(
            screen.x + (screen.width - width) / 2,
            screen.y + (screen.height - height) / 2,
            width,
            height,
        );
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
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(if area.height < 20 { 2 } else { 3 }),
                Constraint::Length(1),
                Constraint::Min(3),
                Constraint::Length(if area.height < 20 { 1 } else { 2 }),
                Constraint::Length(2),
                Constraint::Length(if area.height < 20 { 1 } else { 2 }),
            ])
            .split(area);
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    format!(
                        " ◈ DebugTUI v{}  /  Launch setup",
                        env!("CARGO_PKG_VERSION")
                    ),
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::raw(" Select a project, configure its environment, start debugging."),
            ]),
            rows[0],
        );
        let compact = area.width < 75;
        let actions = [
            (
                if compact {
                    "▶ Start"
                } else {
                    "▶ Start debugging"
                },
                START,
            ),
            (if compact { "Save" } else { "Save · Ctrl+S" }, SAVE),
            ("← Workspace", WORKSPACE),
        ];
        let mut x = rows[1].x + 1;
        for (label, action) in actions {
            let text = format!(" {label} ");
            let width = unicode_width::UnicodeWidthStr::width(text.as_str()) as u16;
            let hit = Rect::new(
                x,
                rows[1].y,
                width.min(rows[1].right().saturating_sub(x)),
                1,
            );
            let enabled = !self.pending && self.browser.is_none();
            let style = if !enabled {
                Style::default().fg(theme::DIM)
            } else if self.selected == action {
                theme::selected(true)
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else if action == START {
                Style::default()
                    .fg(theme::GREEN)
                    .bg(theme::PC)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT).bg(theme::RAISED)
            };
            f.render_widget(Paragraph::new(text).style(style), hit);
            self.action_hits.push((hit, action));
            x += width + 1;
        }
        if rows[1].right().saturating_sub(x) >= 20 {
            f.render_widget(
                Paragraph::new("Ctrl+R / F5 starts").style(Style::default().fg(theme::MUTED)),
                Rect::new(x + 1, rows[1].y, rows[1].right() - x - 1, 1),
            );
        }
        if let Some(browser) = &self.browser {
            let height = rows[2].height.saturating_sub(2) as usize;
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
                            if i == browser.selected { "›" } else { " " },
                            if p.is_dir() { "▸ dir " } else { "· file" }
                        ),
                        theme::selected(i == browser.selected),
                    )
                })
                .collect::<Vec<_>>();
            let block = theme::card(
                format!("  Files / {}  ", portable_path(&browser.directory)),
                true,
            );
            let inner = block.inner(rows[2]);
            f.render_widget(block, rows[2]);
            theme::lines(f, lines, inner);
            self.row_hits.extend(
                (start..browser.entries.len())
                    .take(inner.height as usize)
                    .enumerate()
                    .map(|(row, index)| {
                        (
                            Rect::new(inner.x, inner.y + row as u16, inner.width, 1),
                            index,
                        )
                    }),
            );
            f.render_widget(Paragraph::new(" Enter: open directory / select file   Space: select current directory\n Backspace: parent   Esc: cancel").wrap(Wrap { trim: false }), rows[3]);
        } else {
            let height = rows[2].height.saturating_sub(2) as usize;
            let start = if self.selected < START {
                self.selected.saturating_sub(height.saturating_sub(1))
            } else {
                0
            };
            let values = self.values();
            let lines = LABELS
                .iter()
                .enumerate()
                .take(START)
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
                        if i == self.selected { "›" } else { " " }
                    );
                    let available = rows[2].width.saturating_sub(
                        unicode_width::UnicodeWidthStr::width(prefix.as_str()) as u16 + 2,
                    ) as usize;
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
                        theme::selected(i == self.selected).fg(if i == self.selected {
                            theme::ACCENT
                        } else {
                            theme::TEXT
                        }),
                    )
                })
                .collect::<Vec<_>>();
            let block = theme::card("  ◇  Session configuration  ", true);
            let inner = block.inner(rows[2]);
            f.render_widget(block, rows[2]);
            theme::lines(f, lines, inner);
            self.row_hits
                .extend((start..START).take(inner.height as usize).enumerate().map(
                    |(row, index)| {
                        (
                            Rect::new(inner.x, inner.y + row as u16, inner.width, 1),
                            index,
                        )
                    },
                ));
            f.render_widget(
                Paragraph::new(format!(" {}", if self.selected == WORKSPACE { "Return to the workspace without restarting. Edited settings apply on Start." } else { HINTS[self.selected] }))
                    .wrap(Wrap { trim: false })
                    .style(Style::default().fg(theme::MUTED)),
                rows[3],
            );
        }
        f.render_widget(
            Paragraph::new(format!(" {}", self.message))
                .wrap(Wrap { trim: false })
                .style(Style::default().fg(if self.message.starts_with("Error") {
                    theme::RED
                } else {
                    theme::GREEN
                })),
            rows[4],
        );
        f.render_widget(Paragraph::new(if self.editor.is_some() {
            " Enter: apply  Esc: cancel  Ctrl+U: clear\n Ctrl+R / F5: apply and start  Ctrl+S: apply and save"
        } else {
            " Click / Tab / ↑ ↓: select  Enter: activate  F2: browse\n Ctrl+R / F5: start  Ctrl+S: save  Esc: workspace  Ctrl+Q: quit"
        }).style(Style::default().fg(theme::MUTED)), rows[5]);
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
        assert_eq!(p.breakpoints, [crate::config::BreakpointSpec::from("main")]);
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
        setup.selected = 0;
        setup.key(key(KeyCode::F(2)));
        assert!(setup.browser.is_some());
        setup.key(key(KeyCode::Char(' '))); // select current directory
        assert!(setup.browser.is_none());
        assert!(setup.key(key(KeyCode::F(5))).is_none());
        assert!(setup.message.starts_with("Error")); // no endpoint
        setup.selected = 9;
        setup.key(key(KeyCode::Enter));
        setup.paste("localhost:3333");
        setup.key(key(KeyCode::Enter));
        setup.selected = 15;
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
    fn svd_field_browses_saves_relative_and_clears_without_changing_tools() {
        let root = std::env::temp_dir().join(format!("debugtui-svd-{}", std::process::id()));
        fs::create_dir_all(root.join("chip")).unwrap();
        fs::write(
            root.join("chip/test.svd"),
            include_str!("../tests/fixtures/peripherals.svd"),
        )
        .unwrap();
        let mut setup = Setup::new(Document::empty(root.join("debug.toml")));
        setup.document.set("target", "mode", "local".into());
        setup.selected = 14;
        setup.key(key(KeyCode::F(2)));
        assert!(setup.browser.is_some());
        setup.key(key(KeyCode::Esc));
        setup.choose_path(&root.join("chip/test.svd")).unwrap();
        setup.save_document().unwrap();
        let saved = Document::open(&root).unwrap();
        assert_eq!(saved.raw["program"]["svd"].as_str(), Some("chip/test.svd"));
        assert_eq!(
            saved.project().unwrap().program.svd,
            fs::canonicalize(root.join("chip/test.svd")).unwrap()
        );
        setup.set_value("chip/missing.svd").unwrap();
        assert!(setup.key(key(KeyCode::F(5))).is_none());
        assert!(setup.message.contains("SVD"));
        setup.set_value("").unwrap();
        setup.save_document().unwrap();
        assert!(
            Document::open(&root)
                .unwrap()
                .project()
                .unwrap()
                .program
                .svd
                .as_os_str()
                .is_empty()
        );
        fs::remove_file(root.join("debug.toml")).unwrap();
        fs::remove_file(root.join("chip/test.svd")).unwrap();
        fs::remove_dir(root.join("chip")).unwrap();
        fs::remove_dir(root).unwrap();
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
        setup.selected = 0;
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
    fn task_commands_preserve_quotes_source_root_and_profile_fallback() {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.0.join("source root")).unwrap();
        fs::write(
            fixture.0.join("tools/debug-env.toml"),
            "[actions]\ndownload=['load']\n",
        )
        .unwrap();
        let mut setup = Setup::new(Document::open(&fixture.0).unwrap());
        setup.selected = 3;
        setup.set_value("source root").unwrap();
        setup.selected = 4;
        let command = r#""scripts/build app.bat" "argument with spaces""#;
        setup.set_value(command).unwrap();
        setup.selected = 5;
        setup.set_value("flash.bat && echo done").unwrap();
        setup.document.save().unwrap();
        let project = Document::open(&setup.document.path)
            .unwrap()
            .project()
            .unwrap();
        assert_eq!(project.tasks.build, command);
        assert_eq!(project.tasks.download, "flash.bat && echo done");
        assert_eq!(
            project.program.source_root,
            setup.document.base().join("source root")
        );
        assert_eq!(project.actions.download, ["load"]);
        setup.set_value("").unwrap();
        assert!(setup.document.project().unwrap().has_download());
        setup.selected = 2;
        setup.set_value("not-built.elf").unwrap();
        assert!(setup.key(key(KeyCode::F(5))).is_some());
        assert!(
            !setup
                .document
                .project()
                .unwrap()
                .prepare_workspace()
                .unwrap()
        );
    }
    #[test]
    fn setup_renders_all_fields_and_browser_in_small_and_large_terminals() {
        let fixture = Fixture::new();
        for (width, height) in [(45, 12), (80, 24), (120, 36), (180, 50)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let mut setup = Setup::new(Document::open(&fixture.0).unwrap());
            for (selected, label) in LABELS.iter().enumerate().take(START) {
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

    #[test]
    fn setup_actions_stay_at_top_and_start_by_click_from_any_field() {
        let fixture = Fixture::new();
        for (width, height) in [(45, 12), (80, 24), (120, 36)] {
            let mut doc = Document::open(&fixture.0).unwrap();
            doc.set("target", "endpoint", "localhost:3333".into());
            let mut setup = Setup::new(doc);
            assert_eq!(setup.selected, START);
            setup.selected = 15;
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| setup.draw(f)).unwrap();
            let button = setup
                .action_hits
                .iter()
                .find(|(_, id)| *id == START)
                .unwrap()
                .0;
            assert!(setup.row_hits.iter().all(|(rect, _)| rect.y > button.y));
            assert_eq!(setup.action_hits.len(), 3);
            assert!(
                setup
                    .action_hits
                    .iter()
                    .all(|(rect, _)| rect.right() <= width && rect.bottom() <= height)
            );
            let click = MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: button.x,
                row: button.y,
                modifiers: KeyModifiers::NONE,
            };
            assert!(setup.mouse(click).is_some());
            assert!(setup.pending);
            assert!(setup.mouse(click).is_none());
        }
    }
    #[test]
    fn start_commits_edited_field_and_invalid_settings_keep_configuration_open() {
        let fixture = Fixture::new();
        let mut setup = Setup::new(Document::open(&fixture.0).unwrap());
        assert!(setup.key(key(KeyCode::Enter)).is_none());
        assert!(!setup.pending);
        assert!(setup.message.starts_with("Error"));
        setup.selected = 9;
        setup.key(key(KeyCode::Enter));
        setup.paste("localhost:4444");
        let launch = setup
            .key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL))
            .unwrap();
        assert_eq!(
            launch.document.project().unwrap().target.endpoint,
            "localhost:4444"
        );
        assert!(setup.editor.is_none());
        assert!(!launch.document.path.exists());
    }
    #[test]
    fn setup_workspace_button_and_escape_do_not_launch_or_save() {
        let fixture = Fixture::new();
        let mut setup = Setup::new(Document::open(&fixture.0).unwrap());
        setup.selected = 9;
        setup.key(key(KeyCode::Enter));
        setup.paste("localhost:3333");
        setup.key(key(KeyCode::Esc));
        assert!(!setup.workspace_requested);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| setup.draw(f)).unwrap();
        let hit = setup
            .action_hits
            .iter()
            .find(|(_, id)| *id == WORKSPACE)
            .unwrap()
            .0;
        assert!(
            setup
                .mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: hit.x,
                    row: hit.y,
                    modifiers: KeyModifiers::NONE
                })
                .is_none()
        );
        assert!(setup.workspace_requested);
        assert!(!setup.pending);
        assert!(!setup.document.path.exists());
    }
}
