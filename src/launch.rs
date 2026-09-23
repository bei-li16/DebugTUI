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

mod choices;
use choices::{Choice, Picker};

const LABELS: [&str; 12] = [
    "Project",
    "Tools / profile",
    "Program / ELF",
    "Source root",
    "Build command",
    "Download command",
    "On exit",
    "Log directory",
    "SVD file",
    "Save to project",
    "Start debugging",
    "Save configuration",
];
const HINTS: [&str; 12] = [
    "Project TOML stores launch settings, Watch and breakpoints. Selecting a file reloads all fields.\nRelative to the startup directory; default: ./debug.toml. F3: project list. F2: browse.\nEnter: type a file or directory. A directory uses its debug.toml; a missing file stays a draft until saved.",
    "Profile stores tool defaults (GDB/OpenOCD); project fields override it.\nProject owns ELF/build/Watch/exit policy. Profiles are loaded, never rewritten.\nEdit shared tools in debug-env.toml. Tool parameters are edited in that file, outside Setup. F2: select profile.",
    "ELF / executable provides symbols for C source, variables and breakpoints. A HEX file has no debug symbols.\nExample: ./build/firmware.elf, relative to Project. F2: browse. Use the ELF matching the flashed firmware.\nOptional for remote attachment; needed for source debugging. Selecting it does not flash the device.",
    "Local source lookup root and working directory for Build / Download commands.\nExample: . or ./firmware, relative to Project; blank uses the project directory. F2: browse.\nCI absolute source paths may also need [[source_map]] in the project TOML.",
    "Shell command used by the workspace Build action; runs in Source root, not the tools directory.\nExamples: .\\build.bat or cmake --build build. Quote paths containing spaces.\nOptional: blank uses legacy [build] if present. Starting debugging does not run this command.",
    "Shell command used by Download; runs in Source root. Example: .\\flash.bat or .\\scripts\\flash.ps1.\nOptional: blank uses the profile's actions.download; without either, Download is unavailable.\nBuild/Download release debug connections and owned services before running the command.",
    "Project policy: [session].on_exit; applies to every configured core on session cleanup.\ndetach: detach GDB. resume: resume and release GDB (remote disconnect / local detach).\ndisconnect: release the connection without resuming. Final target state depends on the server/board.",
    "Directory for GDB/MI and server diagnostic logs. Example: ./debug_log, relative to Project.\nBlank disables session file logging. Enable logs when reporting connection or multicore problems.\nLogs are written during a debug session; saving Setup only stores this path.",
    "Optional CMSIS-SVD XML file describing peripheral registers and fields; it is not an ELF or source file.\nExample: ./.vscode/THA6206/tha6206.svd, relative to Project. F2: browse.\nChoose the device's matching SVD. Blank disables peripheral descriptions, not CPU debugging.",
    "Yes (default): save the selected Project TOML before starting. No: use a temporary session.\nCtrl+S explicitly saves even when this option is No. Opening Setup alone creates no file.\nRelative resource paths are based on the selected Project TOML directory.",
    "Start with the reviewed settings: Enter, F5 or Ctrl+R. The previous session is closed first.\nSave to project controls whether this draft is written before starting.\nFor a new project, choose Examples to fill a starting configuration, then adjust project paths and select Tools / profile.",
    "Ctrl+S saves the draft to Project without starting GDB or connecting to hardware.\nA missing Project TOML is created; the referenced tools profile is never rewritten.\nUse Exit to leave without saving draft edits.",
];
// Only project fields are editable. Tool defaults remain in the selected profile;
// legacy project tool overrides are still loaded and preserved by Document.
const ON_EXIT: usize = 6;
const LOG_DIR: usize = 7;
const SVD: usize = 8;
const SAVE_TO_PROJECT: usize = 9;
const START: usize = 10;
const SAVE: usize = 11;
const WORKSPACE: usize = 12;
const PROJECTS: usize = 13;
const EXAMPLES: usize = 14;
const EXIT: usize = 15;

fn displayed_path(base: &Path, path: &Path) -> String {
    let value = relative_path(base, path);
    if Path::new(&value).is_absolute() || value.starts_with('.') {
        value
    } else {
        format!("./{value}")
    }
}

fn help_line_count(text: &str, width: u16) -> u16 {
    let width = usize::from(width.max(1));
    text.lines()
        .map(|line| {
            let mut lines = 1usize;
            let mut used = 0usize;
            for word in line.split_whitespace() {
                let size = unicode_width::UnicodeWidthStr::width(word);
                let gap = usize::from(used > 0);
                if used + gap + size > width && used > 0 {
                    lines += 1;
                    used = 0;
                }
                if size > width {
                    lines += (size - 1) / width;
                    used = (size - 1) % width + 1;
                } else {
                    used += usize::from(used > 0) + size;
                }
            }
            lines
        })
        .sum::<usize>()
        .min(u16::MAX as usize) as u16
}

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
    toml_only: bool,
}
impl Browser {
    fn open(path: &Path, toml_only: bool) -> Result<Self, String> {
        let directory = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(Path::new("."))
        };
        let mut b = Self {
            directory: fs::canonicalize(directory).map_err(|e| e.to_string())?,
            entries: vec![],
            selected: 0,
            toml_only,
        };
        b.reload()?;
        Ok(b)
    }
    fn reload(&mut self) -> Result<(), String> {
        let mut entries = fs::read_dir(&self.directory)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|path| {
                !self.toml_only
                    || path.is_dir()
                    || path
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("toml"))
            })
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
    pub quit_requested: bool,
    working_directory: PathBuf,
    selected: usize,
    editor: Option<Editor>,
    browser: Option<Browser>,
    picker: Option<Picker>,
    action_hits: Vec<(Rect, usize)>,
    row_hits: Vec<(Rect, usize)>,
}
impl Setup {
    pub fn new(document: Document) -> Self {
        let working_directory = env::current_dir().unwrap_or_else(|_| document.base().to_owned());
        let working_directory = fs::canonicalize(&working_directory).unwrap_or(working_directory);
        let projects = Picker::projects(document.base()).ok();
        // Offer discovery on a fresh default draft, not whenever an edited,
        // unsaved configuration is reopened from the workspace.
        let fresh = document.raw.as_table().is_some_and(|table| {
            table
                .keys()
                .all(|key| matches!(key.as_str(), "version" | "tools"))
        });
        let picker = if !document.path.exists() && fresh {
            projects.filter(|p| !p.choices.is_empty())
        } else {
            None
        };
        let message = if picker.is_some() {
            "Choose a project TOML to load its settings, or Esc to keep the new debug.toml draft."
        } else if !document.path.exists() {
            "New project: choose Examples for a starting configuration, or enter your own settings."
        } else {
            "Review the configuration. Projects switches TOML files; Start begins debugging."
        }
        .into();
        Self {
            document,
            message,
            save: true,
            pending: false,
            selected: START,
            workspace_requested: false,
            quit_requested: false,
            working_directory,
            editor: None,
            browser: None,
            picker,
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
            displayed_path(&self.working_directory, &self.document.path),
            profile,
            path(&p.program.elf),
            path(&p.program.source_root),
            p.tasks.build,
            p.tasks.download,
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
            let doc = Document::open(&absolute(&self.working_directory, Path::new(value)))?;
            // Validate before replacing the draft, so a mistaken Cargo.toml or broken
            // profile cannot discard the configuration the user was editing.
            doc.project()?;
            self.document = doc;
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
            ON_EXIT => doc.set("session", "on_exit", value.into()),
            LOG_DIR => {
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
            SVD => {
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
        if self.selected == SAVE_TO_PROJECT {
            self.save = !self.save;
            return Ok(());
        }
        let values: &[&str] = match self.selected {
            ON_EXIT => &["detach", "resume", "disconnect"],
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
        if key.kind != KeyEventKind::Release
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('q')
        {
            self.quit_requested = true;
            return None;
        }
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
        if key.code == KeyCode::F(3) || key.code == KeyCode::F(4) {
            self.commit_editor()?;
            self.browser = None;
            if key.code == KeyCode::F(3) {
                self.open_projects()?;
            } else {
                self.picker = Some(Picker::examples(self.document.base()));
            }
            return Ok(None);
        }
        if let Some(picker) = &mut self.picker {
            match key.code {
                KeyCode::Esc => self.picker = None,
                KeyCode::Up | KeyCode::BackTab => {
                    picker.selected = picker.selected.saturating_sub(1)
                }
                KeyCode::Down | KeyCode::Tab => {
                    picker.selected =
                        (picker.selected + 1).min(picker.choices.len().saturating_sub(1))
                }
                KeyCode::Home => picker.selected = 0,
                KeyCode::End => picker.selected = picker.choices.len().saturating_sub(1),
                KeyCode::Enter => self.apply_choice()?,
                KeyCode::F(2) => {
                    self.picker = None;
                    self.selected = 0;
                    self.open_browser()?;
                }
                _ => {}
            }
            return Ok(None);
        }
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
                        *browser = Browser::open(parent, browser.toml_only)?;
                    }
                }
                KeyCode::Enter => {
                    if let Some(path) = browser.entries.get(browser.selected).cloned() {
                        if path.is_dir() {
                            *browser = Browser::open(&path, browser.toml_only)?;
                        } else {
                            self.choose_path(&path)?;
                        }
                    }
                }
                KeyCode::Char(' ') if matches!(self.selected, 0 | 1 | 3 | LOG_DIR) => {
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
            KeyCode::Up | KeyCode::BackTab => self.selected = (self.selected + EXIT) % (EXIT + 1),
            KeyCode::Down | KeyCode::Tab => self.selected = (self.selected + 1) % (EXIT + 1),
            KeyCode::F(2) if matches!(self.selected, 0..=3 | LOG_DIR | SVD) => {
                self.open_browser()?;
            }
            KeyCode::Left | KeyCode::Right => self.cycle(key.code == KeyCode::Left)?,
            KeyCode::Enter if matches!(self.selected, ON_EXIT | SAVE_TO_PROJECT) => {
                self.cycle(false)?
            }
            KeyCode::Enter if self.selected < START => {
                self.editor = Some(Editor::new(self.values()[self.selected].clone()))
            }
            KeyCode::Enter if self.selected == START => return self.start(),
            KeyCode::Enter if self.selected == SAVE => self.save_document()?,
            KeyCode::Enter if self.selected == WORKSPACE => self.workspace_requested = true,
            KeyCode::Enter if self.selected == PROJECTS => self.open_projects()?,
            KeyCode::Enter if self.selected == EXAMPLES => {
                self.picker = Some(Picker::examples(self.document.base()))
            }
            KeyCode::Enter if self.selected == EXIT => self.quit_requested = true,
            KeyCode::Esc => self.workspace_requested = true,
            _ => {}
        }
        Ok(None)
    }
    fn open_browser(&mut self) -> Result<(), String> {
        let value = self.values()[self.selected].clone();
        let base = if self.selected == 0 {
            &self.working_directory
        } else {
            self.document.base()
        };
        let path = absolute(base, Path::new(&value));
        self.browser = Some(Browser::open(
            if path.exists() {
                &path
            } else {
                self.document.base()
            },
            matches!(self.selected, 0 | 1),
        )?);
        Ok(())
    }
    fn open_projects(&mut self) -> Result<(), String> {
        let picker = Picker::projects(self.document.base())?;
        if picker.choices.is_empty() {
            self.message = "No project TOML found. Use the current draft, choose Examples, or F2 on Project to browse elsewhere.".into();
            self.selected = 0;
            self.picker = None;
        } else {
            self.picker = Some(picker);
        }
        Ok(())
    }
    fn apply_choice(&mut self) -> Result<(), String> {
        let Some(choice) = self.picker.as_ref().and_then(|p| p.choices.get(p.selected)) else {
            return Ok(());
        };
        let (document, message) = match choice {
            Choice::Project(path) => (
                Document::open(path)?,
                "Project loaded. All fields refreshed; review settings before Start.",
            ),
            Choice::Example { raw, .. } => {
                let mut document = self.document.clone();
                document.raw = raw.clone();
                // Examples replace project defaults, not an existing toolchain.
                // Keep legacy inline overrides as well as the selected profile.
                for key in ["tools", "gdb", "target", "service"] {
                    if document.raw.get(key).is_none()
                        && let Some(value) = self.document.raw.get(key)
                    {
                        document
                            .raw
                            .as_table_mut()
                            .unwrap()
                            .insert(key.into(), value.clone());
                    }
                }
                if let Some(timeout) = self
                    .document
                    .raw
                    .get("session")
                    .and_then(|s| s.get("timeout_ms"))
                {
                    document.set("session", "timeout_ms", timeout.clone());
                }
                document.discovered = false;
                (
                    document,
                    "Example applied to draft. Review project paths and Tools / profile; Ctrl+S saves. No file written yet.",
                )
            }
        };
        document.project()?;
        self.document = document;
        self.message = message.into();
        self.picker = None;
        self.editor = None;
        self.selected = 0;
        Ok(())
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
        project.prepare_workspace().map_err(|error| {
            if project.target.mode != "local"
                && project.target.endpoint.is_empty()
                && project.cores.is_empty()
            {
                format!("{error}. Select Tools / profile with a configured [target], or configure project [[cores]].")
            } else {
                error
            }
        })?;
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
        // Exit must remain available even for an invalid editor or an open picker.
        if mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && self
                .action_hits
                .iter()
                .any(|(rect, id)| *id == EXIT && rect.contains((mouse.column, mouse.row).into()))
        {
            self.quit_requested = true;
            return None;
        }
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
            if (self.browser.is_some() || self.picker.is_some())
                && !matches!(action, PROJECTS | EXAMPLES)
            {
                return None;
            }
            self.browser = None;
            self.picker = None;
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
            if let Some(picker) = &mut self.picker {
                picker.selected = index;
            } else if let Some(browser) = &mut self.browser {
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
        let value = if self.selected == 0 {
            portable_path(path)
        } else {
            relative_path(self.document.base(), path)
        };
        self.browser = None;
        self.set_value(&value)?;
        self.message = "Selected. Click Start or press Ctrl+R; Ctrl+S saves.".into();
        Ok(())
    }
    fn core_summary(&self) -> String {
        match self.document.project() {
            Ok(project) if project.cores.is_empty() => "Single core".into(),
            Ok(project) => format!(
                "Cores: {}",
                project
                    .cores
                    .iter()
                    .map(|core| core.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Err(_) => "Check Tools / profile".into(),
        }
    }
    fn help_text(&self) -> String {
        if let Some(picker) = &self.picker {
            return picker
                .choices
                .get(picker.selected)
                .map(Choice::description)
                .unwrap_or_default();
        }
        match self.selected {
            WORKSPACE => "Return to the workspace without restarting. Draft edits apply on Start; use Save to keep them on disk.".into(),
            PROJECTS => "Choose a project TOML in the selected project directory (F3). Its settings replace the displayed draft.\nF2 on Project browses other directories. Selecting a file never starts debugging or saves it.".into(),
            EXAMPLES => "Choose an example to fill the draft (F4), then adjust project paths and select Tools / profile.\nTemplates cover single-core, local and multicore projects; current tool settings are retained.\nApplying an example does not write files or start GDB; Save / Start controls persistence.".into(),
            EXIT => "Exit DebugTUI without saving draft edits. Works even when configuration is incomplete or invalid.\nIf a session is active, normal disconnect and owned-process cleanup still run. Shortcut: Ctrl+Q.".into(),
            _ => format!("{}: {}", LABELS[self.selected], HINTS[self.selected]),
        }
    }
    pub fn draw(&mut self, f: &mut Frame) {
        self.action_hits.clear();
        self.row_hits.clear();
        let screen = f.area();
        f.render_widget(Block::default().style(theme::base()), screen);
        let width = screen.width.min(122);
        let height = screen.height.min(38);
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
        let compact = area.width < 95;
        let actions = [
            (
                if compact {
                    "Start"
                } else {
                    "▶ Start debugging"
                },
                START,
            ),
            (if compact { "Save" } else { "Save · Ctrl+S" }, SAVE),
            (
                if compact {
                    "Projects"
                } else {
                    "Projects · F3"
                },
                PROJECTS,
            ),
            (
                if compact {
                    "Examples"
                } else {
                    "Examples · F4"
                },
                EXAMPLES,
            ),
            (if compact { "Back" } else { "← Workspace" }, WORKSPACE),
            (if compact { "Exit" } else { "Exit · Ctrl+Q" }, EXIT),
        ];
        let mut action_rows = 1;
        let mut used = 1;
        for (label, _) in actions {
            let width = unicode_width::UnicodeWidthStr::width(label) as u16 + 2;
            if used + width > area.width {
                action_rows += 1;
                used = 1;
            }
            used += width + 1;
        }
        let help_height = if area.height < 20 {
            1
        } else {
            // Keep room for scrolling fields, while allowing the full selected
            // field description to wrap on ordinary 80-column terminals.
            let limit = area
                .height
                .saturating_sub(3 + action_rows + 6 + 2 + 2)
                .clamp(2, 8);
            help_line_count(&self.help_text(), area.width).clamp(2, limit)
        };
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(if area.height < 20 { 2 } else { 3 }),
                Constraint::Length(action_rows),
                Constraint::Min(3),
                Constraint::Length(help_height),
                Constraint::Length(if area.height < 20 { 1 } else { 2 }),
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
                Line::raw(format!(
                    " Configure project settings; select tools via profile. {}",
                    self.core_summary()
                )),
            ]),
            rows[0],
        );
        let mut x = rows[1].x + 1;
        let mut y = rows[1].y;
        for (label, action) in actions {
            let text = format!(" {label} ");
            let width = unicode_width::UnicodeWidthStr::width(text.as_str()) as u16;
            if x + width > rows[1].right() {
                x = rows[1].x + 1;
                y += 1;
            }
            let hit = Rect::new(x, y, width.min(rows[1].right().saturating_sub(x)), 1);
            let enabled = action == EXIT
                || (!self.pending
                    && ((self.browser.is_none() && self.picker.is_none())
                        || matches!(action, PROJECTS | EXAMPLES)));
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
            } else if action == EXIT {
                Style::default().fg(theme::RED).bg(theme::RAISED)
            } else {
                Style::default().fg(theme::TEXT).bg(theme::RAISED)
            };
            f.render_widget(Paragraph::new(text).style(style), hit);
            self.action_hits.push((hit, action));
            x += width + 1;
        }
        if let Some(picker) = &self.picker {
            let block = theme::card(format!("  {}  ", picker.title), true);
            let inner = block.inner(rows[2]);
            f.render_widget(block, rows[2]);
            let start = picker
                .selected
                .saturating_sub((inner.height as usize).saturating_sub(1));
            let lines = picker
                .choices
                .iter()
                .enumerate()
                .skip(start)
                .take(inner.height as usize)
                .map(|(i, choice)| {
                    Line::styled(
                        format!(
                            " {} {}",
                            if i == picker.selected { "›" } else { " " },
                            visible_tail(&choice.label(), inner.width.saturating_sub(4) as usize)
                        ),
                        theme::selected(i == picker.selected),
                    )
                })
                .collect();
            theme::lines(f, lines, inner);
            self.row_hits.extend(
                (start..picker.choices.len())
                    .take(inner.height as usize)
                    .enumerate()
                    .map(|(row, index)| {
                        (
                            Rect::new(inner.x, inner.y + row as u16, inner.width, 1),
                            index,
                        )
                    }),
            );
            f.render_widget(
                Paragraph::new(self.help_text())
                    .wrap(Wrap { trim: false })
                    .style(Style::default().fg(theme::MUTED)),
                rows[3],
            );
        } else if let Some(browser) = &self.browser {
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
                    let shown = if value.is_empty() {
                        match i {
                            1 => "(select debug-env.toml; legacy tools supported)",
                            2 => "(select ELF for source debugging)",
                            4 => "(optional; no build command)",
                            5 => "(optional; uses profile download)",
                            LOG_DIR => "(optional; logging disabled)",
                            SVD => "(optional; no peripheral descriptions)",
                            _ => "(not set)",
                        }
                    } else {
                        value
                    };
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
            let block = theme::card(
                if self.document.path.is_file() {
                    "  ◇  Project configuration  "
                } else {
                    "  ◇  New project / unsaved draft  "
                },
                true,
            );
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
                Paragraph::new(self.help_text())
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
        f.render_widget(Paragraph::new(if self.picker.is_some() {
            " Up/Down: select  Enter / click: apply  Esc: cancel\n F2: browse files  F3: projects  F4: examples  Ctrl+Q: exit"
        } else if self.editor.is_some() {
            " Enter: apply  Esc: cancel  Ctrl+U: clear\n Ctrl+R / F5: apply and start  Ctrl+S: apply and save"
        } else {
            " Enter: edit  F2: browse  F3: projects  F4: examples\n F5: start  Ctrl+S: save  Esc: workspace  Ctrl+Q: exit"
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
    fn setup_browses_project_selects_profile_and_returns_launch() {
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
        fs::write(
            fixture.0.join("debug-env.toml"),
            "[target]\nendpoint='localhost:3333'\n",
        )
        .unwrap();
        setup.selected = 1;
        setup.key(key(KeyCode::Enter));
        setup.paste("debug-env.toml");
        setup.key(key(KeyCode::Enter));
        setup.selected = SAVE_TO_PROJECT;
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
        setup.selected = SVD;
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
        setup.working_directory = fs::canonicalize(&fixture.0).unwrap();
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
            setup.selected = SAVE_TO_PROJECT;
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| setup.draw(f)).unwrap();
            let button = setup
                .action_hits
                .iter()
                .find(|(_, id)| *id == START)
                .unwrap()
                .0;
            assert!(setup.row_hits.iter().all(|(rect, _)| rect.y > button.y));
            assert_eq!(setup.action_hits.len(), 6);
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
        fs::write(
            fixture.0.join("debug-env.toml"),
            "[target]\nendpoint='localhost:4444'\n",
        )
        .unwrap();
        setup.selected = 1;
        setup.key(key(KeyCode::Enter));
        setup.paste("debug-env.toml");
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
        fs::write(
            fixture.0.join("debug-env.toml"),
            "[target]\nendpoint='localhost:3333'\n",
        )
        .unwrap();
        setup.selected = 1;
        setup.key(key(KeyCode::Enter));
        setup.paste("debug-env.toml");
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

    #[test]
    fn exit_button_works_with_invalid_edits_and_in_every_picker() {
        let fixture = Fixture::new();
        for (width, height) in [(45, 12), (80, 24), (120, 36)] {
            for state in 0..4 {
                let mut setup = Setup::new(Document::open(&fixture.0).unwrap());
                match state {
                    0 => {
                        setup.selected = 1;
                        setup.editor = Some(Editor::new("missing-profile.toml".into()));
                    }
                    1 => setup.picker = Some(Picker::examples(setup.document.base())),
                    2 => {
                        setup.selected = 0;
                        setup.open_browser().unwrap();
                    }
                    _ => setup.pending = true,
                }
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal.draw(|f| setup.draw(f)).unwrap();
                let hit = setup
                    .action_hits
                    .iter()
                    .find(|(_, id)| *id == EXIT)
                    .unwrap()
                    .0;
                assert!(hit.width >= 4 && hit.right() <= width && hit.bottom() <= height);
                assert!(
                    setup
                        .mouse(MouseEvent {
                            kind: MouseEventKind::Down(MouseButton::Left),
                            column: hit.x,
                            row: hit.y,
                            modifiers: KeyModifiers::NONE,
                        })
                        .is_none()
                );
                assert!(setup.quit_requested, "{width}x{height} state {state}");
                assert!(!setup.document.path.exists());
            }
        }
    }

    #[test]
    fn project_paths_round_trip_relative_to_startup_after_switching_directories() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("nested")).unwrap();
        let mut setup = Setup::new(Document::open(&fixture.0).unwrap());
        setup.working_directory = fs::canonicalize(&fixture.0).unwrap();
        setup.selected = 0;
        assert_eq!(setup.values()[0], "./debug.toml");
        setup.set_value("./nested/debug-core1.toml").unwrap();
        assert_eq!(setup.values()[0], "./nested/debug-core1.toml");
        let path = setup.document.path.clone();
        setup.key(key(KeyCode::Enter));
        setup.key(key(KeyCode::Enter));
        assert_eq!(setup.document.path, path); // No nested/nested path on re-edit.
        setup.key(key(KeyCode::F(2)));
        assert_eq!(
            setup.browser.as_ref().unwrap().directory,
            fs::canonicalize(fixture.0.join("nested")).unwrap()
        );
        setup.key(key(KeyCode::Esc));
        setup.set_value("./debug.toml").unwrap();
        assert_eq!(setup.document.base(), setup.working_directory);
        assert!(!setup.document.path.exists());
    }

    #[test]
    fn discovered_projects_load_all_settings_without_writing_or_losing_a_valid_draft() {
        let fixture = Fixture::new();
        fs::write(
            fixture.0.join("Cargo.toml"),
            "[package]\nname='unrelated'\nversion='1.0.0'\n[target.'cfg(windows)'.dependencies]\n",
        )
        .unwrap();
        fs::write(
            fixture.0.join("debug-env.toml"),
            "[gdb]\nexecutable='profile-gdb'\n",
        )
        .unwrap();
        fs::write(fixture.0.join("debug-broken.toml"), "[invalid").unwrap();
        fs::write(fixture.0.join("debug-core0.toml"), "version=2\n[gdb]\nexecutable='first-gdb'\nargs=['--first']\n[target]\nendpoint='localhost:3333'\n[program]\nelf='first.elf'\n").unwrap();
        fs::write(fixture.0.join("custom.toml"), "[gdb]\nexecutable='second-gdb'\n[target]\nmode='extended-remote'\nendpoint='localhost:3334'\n[program]\nelf='second.elf'\nsource_root='src'\n").unwrap();
        let before = fs::read(fixture.0.join("debug-core0.toml")).unwrap();
        let mut setup = Setup::new(Document::open(&fixture.0).unwrap());
        assert_eq!(setup.picker.as_ref().unwrap().choices.len(), 3);
        for name in ["debug-core0.toml", "custom.toml"] {
            setup.open_projects().unwrap();
            let picker = setup.picker.as_mut().unwrap();
            picker.selected = picker
                .choices
                .iter()
                .position(|c| c.label() == name)
                .unwrap();
            assert!(setup.key(key(KeyCode::Enter)).is_none());
            assert!(setup.picker.is_none());
            assert_eq!(setup.document.path.file_name().unwrap(), name);
        }
        let values = setup.values();
        assert_eq!(values[2], "second.elf");
        assert_eq!(values[3], "src");
        let project = setup.document.project().unwrap();
        assert_eq!(project.gdb.executable, PathBuf::from("second-gdb"));
        assert!(project.gdb.args.is_empty());
        assert_eq!(project.target.mode, "extended-remote");
        assert_eq!(project.target.endpoint, "localhost:3334");
        setup.open_projects().unwrap();
        let picker = setup.picker.as_mut().unwrap();
        picker.selected = picker
            .choices
            .iter()
            .position(|c| c.label() == "debug-broken.toml")
            .unwrap();
        setup.key(key(KeyCode::Enter));
        assert!(setup.message.starts_with("Error:"));
        assert_eq!(setup.values(), values);
        assert!(setup.picker.is_some());
        assert!(!fixture.0.join("debug.toml").exists());
        assert_eq!(
            fs::read(fixture.0.join("debug-core0.toml")).unwrap(),
            before
        );
    }

    #[test]
    fn examples_are_opt_in_refresh_fields_and_persist_only_on_save() {
        let fixture = Fixture::new();
        let doc = Document::open(&fixture.0).unwrap();
        let mut setup = Setup::new(doc);
        assert!(setup.picker.is_none());
        assert!(setup.values()[2].is_empty());
        setup.key(key(KeyCode::F(4)));
        setup.key(key(KeyCode::Esc));
        assert!(setup.values()[2].is_empty());
        let count = Picker::examples(setup.document.base()).choices.len();
        for i in 0..count {
            setup.key(key(KeyCode::F(4)));
            setup.picker.as_mut().unwrap().selected = i;
            setup.key(key(KeyCode::Enter));
            assert!(setup.picker.is_none(), "{}", setup.message);
            let project = setup.document.project().unwrap();
            assert!(!project.program.elf.as_os_str().is_empty());
            assert!(project.session.log_dir.is_some());
            assert!(project.service.is_none());
            for key in ["gdb", "target", "service"] {
                assert!(setup.document.raw.get(key).is_none());
            }
            assert!(setup.document.raw["session"].get("timeout_ms").is_none());
            assert!(!setup.document.path.exists());
        }
        assert_eq!(setup.document.project().unwrap().cores.len(), 2);
        setup.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert!(setup.document.path.exists());
        assert_eq!(
            Document::open(&fixture.0)
                .unwrap()
                .project()
                .unwrap()
                .cores
                .len(),
            2
        );
    }

    #[test]
    fn local_tools_example_inherits_profile_and_can_be_saved_without_rewriting_it() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join(".vscode")).unwrap();
        let profile = fixture.0.join(".vscode/debug-env.toml");
        let content = "[gdb]\nexecutable='./gdb.exe'\n[target]\nendpoint='localhost:4444'\n[service]\ncommand='./server.exe'\n";
        fs::write(&profile, content).unwrap();
        let mut setup = Setup::new(Document::open(&fixture.0).unwrap());
        setup.key(key(KeyCode::F(4)));
        assert!(
            setup.picker.as_ref().unwrap().choices[0]
                .label()
                .contains(".vscode")
        );
        setup.key(key(KeyCode::Enter));
        assert_eq!(setup.values()[1], ".vscode/debug-env.toml");
        let project = setup.document.project().unwrap();
        assert_eq!(project.target.endpoint, "localhost:4444");
        assert!(project.service.as_ref().unwrap().enabled);
        setup.document.save().unwrap();
        assert_eq!(fs::read_to_string(&profile).unwrap(), content);
        assert!(
            !fs::read_to_string(&setup.document.path)
                .unwrap()
                .contains("executable")
        );
    }

    #[test]
    fn setup_help_and_pickers_render_at_supported_terminal_sizes() {
        let fixture = Fixture::new();
        for (width, height) in [(45, 12), (80, 24), (120, 36)] {
            let mut setup = Setup::new(Document::open(&fixture.0).unwrap());
            setup.working_directory = fs::canonicalize(&fixture.0).unwrap();
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            for mode in ["empty", "examples", "applied", "help"] {
                match mode {
                    "examples" => {
                        setup.key(key(KeyCode::F(4)));
                    }
                    "applied" => {
                        setup.key(key(KeyCode::Enter));
                    }
                    "help" => setup.selected = 1,
                    _ => {}
                }
                terminal.draw(|f| setup.draw(f)).unwrap();
                let text = terminal
                    .backend()
                    .buffer()
                    .content
                    .chunks(width as usize)
                    .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(text.contains("Exit"));
                if mode == "help" && width >= 120 {
                    assert!(text.contains("project fields override it"));
                    assert!(text.contains("Profiles are loaded, never rewritten"));
                }
                if let Some(directory) = env::var_os("DEBUGTUI_SETUP_SNAPSHOTS") {
                    let directory = PathBuf::from(directory);
                    fs::create_dir_all(&directory).unwrap();
                    fs::write(directory.join(format!("{mode}-{width}x{height}.txt")), text)
                        .unwrap();
                }
            }
        }
    }

    #[test]
    fn project_form_preserves_plain_core0_core1_and_dual_core_configuration() {
        let fixture = Fixture::new();
        let profile = fixture.0.join("debug-env.toml");
        let tools = "[gdb]\nexecutable='fixture-gdb'\nargs=['--quiet']\n[target]\nmode='extended-remote'\nendpoint='localhost:3333'\n[service]\nenabled=false\ncommand='fixture-openocd'\n[session]\ntimeout_ms=12345\non_exit='disconnect'\n";
        fs::write(&profile, tools).unwrap();
        for names in [vec![], vec!["core0"], vec!["core1"], vec!["core0", "core1"]] {
            let path = fixture.0.join("debug.toml");
            let mut text = "version=2\nwatch=['counter']\nbreakpoints=['main']\n[tools]\nprofile='./debug-env.toml'\n[program]\nelf='old.elf'\nsource_root='.'\n[actions]\nrun=['break main','continue']\n[multicore]\nscope='core'\nhalt_peers=true\n[[source_map]]\nfrom='/ci/build'\nto='.'\n".to_owned();
            for name in &names {
                let index = usize::from(*name == "core1");
                text += &format!(
                    "[[cores]]\nname='{name}'\nendpoint='localhost:{}'\nstartup_order={index}\ninit=['set pagination off']\nafter_connect=['monitor halt']\nrun=['break main']\nwatch=['{name}_counter']\nbreakpoints=['main']\n",
                    3333 + index
                );
            }
            fs::write(&path, text).unwrap();
            let mut setup = Setup::new(Document::open(&path).unwrap());
            let before = setup.document.raw.clone();
            assert_eq!(setup.values()[ON_EXIT], "disconnect"); // Legacy profile inheritance.
            setup.selected = ON_EXIT;
            setup.key(key(KeyCode::Enter)); // Writes only the project policy.
            assert_eq!(setup.values()[ON_EXIT], "detach");
            setup.selected = 2;
            setup.set_value("new.elf").unwrap();
            // A live core may save Watch/breakpoints while Setup is open.
            let mut expected_cores = before.get("cores").cloned();
            if let Some(cores) = expected_cores.as_mut() {
                cores[0]["watch"] = toml::Value::Array(vec!["runtime_counter".into()]);
                cores[0]["breakpoints"] = toml::Value::Array(vec![]);
                let mut disk = before.clone();
                disk["cores"] = cores.clone();
                fs::write(&path, toml::to_string(&disk).unwrap()).unwrap();
            }
            setup.save_document().unwrap();
            let saved = Document::open(&path).unwrap();
            assert_eq!(saved.raw.get("cores"), expected_cores.as_ref());
            for key in [
                "tools",
                "multicore",
                "source_map",
                "actions",
                "watch",
                "breakpoints",
            ] {
                assert_eq!(
                    saved.raw.get(key),
                    before.get(key),
                    "{key} changed for {names:?}"
                );
            }
            for key in ["gdb", "target", "service"] {
                assert!(
                    saved.raw.get(key).is_none(),
                    "profile leaked into project: {key}"
                );
            }
            assert!(saved.raw["session"].get("timeout_ms").is_none());
            let project = saved.project().unwrap();
            assert_eq!(project.cores.len(), names.len());
            assert_eq!(project.session.on_exit, "detach");
            assert_eq!(project.session.timeout_ms, 12345);
            assert_eq!(project.gdb.args, ["--quiet"]);
            assert_eq!(fs::read_to_string(&profile).unwrap(), tools);
            // Switching tools must retain the project's core mapping and policy.
            fs::write(fixture.0.join("other-env.toml"), "[target]\nmode='extended-remote'\nendpoint='localhost:5555'\n[session]\ntimeout_ms=9000\non_exit='resume'\n").unwrap();
            setup.selected = 1;
            setup.set_value("other-env.toml").unwrap();
            setup.save_document().unwrap();
            let switched = setup.document.project().unwrap();
            assert_eq!(switched.target.endpoint, "localhost:5555");
            assert_eq!(switched.session.timeout_ms, 9000);
            assert_eq!(switched.session.on_exit, "detach");
            assert_eq!(setup.document.raw.get("cores"), expected_cores.as_ref());
        }
    }

    #[test]
    fn legacy_inline_tools_survive_form_edits_and_project_examples() {
        let fixture = Fixture::new();
        let path = fixture.0.join("debug.toml");
        fs::write(&path, "version=2\n[gdb]\nexecutable='legacy-gdb'\nargs=['--quiet']\n[target]\nmode='extended-remote'\nendpoint='localhost:3334'\n[service]\nenabled=false\ncommand='legacy-server'\n[session]\ntimeout_ms=5432\non_exit='resume'\n").unwrap();
        let mut setup = Setup::new(Document::open(&path).unwrap());
        let original = setup.document.raw.clone();
        setup.selected = ON_EXIT;
        setup.cycle(false).unwrap();
        setup.save_document().unwrap();
        for key in ["gdb", "target", "service"] {
            assert_eq!(setup.document.raw[key], original[key]);
        }
        assert_eq!(
            setup.document.raw["session"]["timeout_ms"],
            original["session"]["timeout_ms"]
        );
        setup.key(key(KeyCode::F(4)));
        setup.key(key(KeyCode::Enter));
        assert!(setup.picker.is_none(), "{}", setup.message);
        for key in ["gdb", "target", "service"] {
            assert_eq!(setup.document.raw[key], original[key]);
        }
        assert_eq!(setup.document.project().unwrap().session.timeout_ms, 5432);
    }

    #[test]
    fn rendered_setup_has_project_fields_only_and_exit_policy_has_no_browser() {
        let fixture = Fixture::new();
        let mut setup = Setup::new(Document::open(&fixture.0).unwrap());
        let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
        terminal.draw(|f| setup.draw(f)).unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        for label in [
            "GDB executable",
            "GDB arguments",
            "Target mode",
            "Endpoint",
            "Start service",
            "Timeout (ms)",
        ] {
            assert!(!text.contains(label), "tool editor still visible: {label}");
        }
        assert!(text.contains("Single core"));
        assert!(text.contains("On exit"));
        setup.selected = ON_EXIT;
        setup.key(key(KeyCode::F(2)));
        assert!(setup.browser.is_none());
        setup.key(key(KeyCode::Right));
        assert_eq!(setup.document.project().unwrap().session.on_exit, "resume");
        setup.key(key(KeyCode::Left));
        assert_eq!(setup.document.project().unwrap().session.on_exit, "detach");
    }
}
