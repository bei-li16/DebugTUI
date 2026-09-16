use crate::{
    config::Project,
    launch::{Document, Launch, Setup},
    session::{self, EngineHandle, Event, Frame, Request, Snapshot, Variable},
};
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event as Input, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame as UiFrame, Terminal,
    backend::{CrosstermBackend, TestBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs, Wrap},
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    fs,
    io::{self, IsTerminal},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const PANES: [&str; 9] = [
    "Source", "Watch", "Stack", "Regs", "Memory", "Asm", "Breaks", "Files", "Log",
];
const COMMANDS: [&str; 24] = [
    "setup",
    "connect",
    "run",
    "continue",
    "pause",
    "step",
    "next",
    "stepi",
    "finish",
    "restart",
    "download",
    "disconnect",
    "watch EXPRESSION",
    "unwatch EXPRESSION",
    "break LOCATION",
    "data-break EXPRESSION",
    "delete NUMBER",
    "memory ADDRESS [COUNT]",
    "disasm [ADDRESS]",
    "files",
    "open PATH",
    "frame NUMBER",
    "build",
    "quit",
];
const DEMO_SOURCE: &str = "/* DebugTUI demo: no target connected */\n#include <stdint.h>\n\n\nvolatile uint32_t counter;\nvolatile uint8_t flag = 1;\n\nvoid process_items(void)\n{\n    for (unsigned i = 0; i < 100; ++i) {\n        update_value(\"sample\\n\");\n        counter++;\n    }\n}\n\nvoid update_value(const char *format)\n{\n    uint8_t ret = 0;\n    flag = 0;\n    /* Place a data breakpoint on flag. */\n}\n";
const HELP: &str = "DebugTUI — keyboard-first GDB debugger\n\nF2 Launch setup / switch project\nF5 Continue      F6 Pause          F9 Toggle line breakpoint\nF10 Next         F11 Step          Shift+F11 Finish\nTab Next panel   Shift+Tab Previous panel\nArrows / PgUp / PgDn Scroll or select\nEnter Open selected file / select stack frame\n: or / Focus command line         Ctrl+P Command palette\nCtrl+C Pause target               Ctrl+Q End session and quit\n? Help           Esc Close dialog / cancel input\n\nWorkstation commands start with ':'\n:setup  :connect  :run  :disconnect  :restart  :download\n:watch counter        variable display; no hardware slot\n:data-break flag    hardware data breakpoint\n:break main   :delete 2  :frame 1\n:memory $sp 256   :disasm $pc\n:files  :open source.c   :elf app.elf\n:refresh  :build  :quit\n\nOther input is sent to the GDB console, e.g. p/x variable,\nx/8wx address, set variable name=value, info registers.\nRunning views show the last stopped snapshot.\nRestart/download behavior comes from the environment profile.\nExit behavior is controlled by session.on_exit.\n\nEsc / ? closes this help.";

pub struct App {
    project: Project,
    document: Document,
    setup: Option<Setup>,
    launch: Option<Launch>,
    snapshot: Snapshot,
    pane: usize,
    selection: usize,
    source: Vec<String>,
    source_file: String,
    source_line: usize,
    source_top: usize,
    logs: VecDeque<String>,
    console: VecDeque<String>,
    input: String,
    editing: bool,
    history: Vec<String>,
    history_index: usize,
    palette: bool,
    palette_index: usize,
    help: bool,
    confirm: Option<Request>,
    next_id: u64,
    notice: String,
    demo: bool,
    quitting: bool,
    source_rect: Rect,
    tab_rect: Rect,
    visible_tabs: Vec<usize>,
    help_scroll: u16,
}
impl App {
    pub fn new(project: Project, demo: bool) -> Self {
        let mut a = Self {
            document: Document::empty(project.path.clone().unwrap_or_else(|| {
                std::env::current_dir()
                    .unwrap_or_default()
                    .join("debug.toml")
            })),
            setup: None,
            launch: None,
            project,
            snapshot: Snapshot::default(),
            pane: 0,
            selection: 0,
            source: vec![],
            source_file: String::new(),
            source_line: 0,
            source_top: 0,
            logs: VecDeque::new(),
            console: VecDeque::new(),
            input: String::new(),
            editing: false,
            history: vec![],
            history_index: 0,
            palette: false,
            palette_index: 0,
            help: false,
            confirm: None,
            next_id: 1,
            notice: "F2 / :setup selects a project and debug environment; ? for help".into(),
            demo,
            quitting: false,
            source_rect: Rect::default(),
            tab_rect: Rect::default(),
            visible_tabs: vec![],
            help_scroll: 0,
        };
        if demo {
            a.source = DEMO_SOURCE.lines().map(str::to_owned).collect();
            a.source_file = "demo/sample.c".into();
            a.source_line = 17;
            a.source_top = 9;
            a.snapshot.state = "DEMO / STOPPED".into();
            a.snapshot.stop_reason = "breakpoint-hit".into();
            a.snapshot.frame = Frame {
                level: 0,
                function: "update_value".into(),
                file: a.source_file.clone(),
                line: 18,
                address: "0x0800068c".into(),
            };
            a.snapshot.stack = vec![
                a.snapshot.frame.clone(),
                Frame {
                    level: 1,
                    function: "process_items".into(),
                    line: 11,
                    file: a.source_file.clone(),
                    address: "0x080007e2".into(),
                },
            ];
            a.snapshot.watches = vec![
                Variable {
                    name: "counter".into(),
                    value: "12345".into(),
                    ..Default::default()
                },
                Variable {
                    name: "flag".into(),
                    value: "1".into(),
                    changed: true,
                    error: false,
                },
                Variable {
                    name: "checksum".into(),
                    value: "0xef4018".into(),
                    ..Default::default()
                },
            ];
            a.snapshot.locals = vec![
                Variable {
                    name: "ret".into(),
                    value: "0".into(),
                    ..Default::default()
                },
                Variable {
                    name: "format".into(),
                    value: "0x08006824 \"process_items\\n\"".into(),
                    ..Default::default()
                },
            ];
            a.snapshot.registers = vec![
                Variable {
                    name: "pc".into(),
                    value: "0x0800068c".into(),
                    ..Default::default()
                },
                Variable {
                    name: "sp".into(),
                    value: "0x20001868".into(),
                    ..Default::default()
                },
            ];
            a.log("[demo] Terminal UI preview; commands will not access hardware.".into());
            a.notice = "DEMO — Tab switches panels; Ctrl+P opens commands; q exits".into();
        }
        a
    }
    fn log(&mut self, text: String) {
        let text: String = text.chars().take(4000).collect();
        if !text.starts_with("[server]") && !text.starts_with("[diagnostic]") {
            if self.console.len() >= 500 {
                self.console.pop_front();
            }
            self.console.push_back(text.clone());
        }
        if self.logs.len() >= 1000 {
            self.logs.pop_front();
        }
        self.logs.push_back(text);
    }
    fn load_source(&mut self, file: &str) {
        self.source_file = file.into();
        self.source_top = 0;
        self.source = if let Some(path) = self.project.source_path(file) {
            match fs::metadata(&path) {
                Ok(meta) if meta.len() <= 2 * 1024 * 1024 => fs::read(&path)
                    .map(|bytes| {
                        String::from_utf8_lossy(&bytes)
                            .lines()
                            .take(30000)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_else(|e| vec![format!("Cannot read source: {e}")]),
                _ => vec!["Source file exceeds 2 MiB; use an external editor.".into()],
            }
        } else {
            vec![
                format!("Source not found: {file}"),
                "Add [[source_map]] to debug.toml or use :open PATH.".into(),
                "Use the Assembly panel (:disasm) when source is unavailable.".into(),
            ]
        };
    }
    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::Snapshot { snapshot } => {
                let moved = snapshot.generation != self.snapshot.generation
                    || snapshot.frame.file != self.snapshot.frame.file
                    || snapshot.frame.line != self.snapshot.frame.line;
                if moved && !snapshot.frame.file.is_empty() {
                    if self.source_file != snapshot.frame.file {
                        self.load_source(&snapshot.frame.file);
                    }
                    self.source_line = snapshot.frame.line.saturating_sub(1) as usize;
                    self.source_top = self.source_line.saturating_sub(8);
                }
                self.snapshot = *snapshot;
            }
            Event::Log { channel, text } => {
                if channel != "mi>" && channel != "mi<" {
                    for line in text.lines() {
                        self.log(format!("[{channel}] {line}"));
                    }
                }
            }
            Event::Response {
                id,
                ok,
                result,
                error,
            } => {
                self.notice = if ok {
                    format!("Command {id} completed")
                } else {
                    format!("Error: {}", error.unwrap_or_default())
                };
                if !result.is_null() && result.to_string().len() < 3000 {
                    self.log(format!("[result {id}] {result}"));
                }
            }
            Event::Exit => return true,
        }
        false
    }
    fn submit(&mut self, engine: Option<&EngineHandle>, method: &str, params: Value) {
        if self.demo {
            self.notice = format!("DEMO: {method} (no hardware action)");
            self.log(self.notice.clone());
            return;
        }
        if (method == "download" && self.project.actions.download.is_empty())
            || (method == "restart" && self.project.actions.restart.is_empty())
        {
            self.notice = format!("{method} is not configured by this environment");
            return;
        }
        let request = Request::new(self.next_id, method, params);
        self.next_id += 1;
        if method == "download" {
            self.confirm = Some(request);
            return;
        }
        self.send(engine, request);
    }
    fn send(&mut self, engine: Option<&EngineHandle>, request: Request) {
        if request.method == "quit" {
            self.quitting = true;
        }
        self.notice = format!("{}…", request.method);
        if let Some(engine) = engine
            && let Err(e) = engine.send(request)
        {
            self.notice = format!("Session worker unavailable: {e}");
        }
    }
    fn command(&mut self, engine: Option<&EngineHandle>, input: &str) {
        let input = input.trim();
        if input.is_empty() {
            return;
        }
        self.log(format!("> {input}"));
        if !input.starts_with(':') {
            self.submit(engine, "console", json!({"command":input}));
            return;
        }
        let (name, arg) = input[1..]
            .split_once(char::is_whitespace)
            .unwrap_or((&input[1..], ""));
        let arg = arg.trim();
        let unquote = |s: &str| s.trim().trim_matches('"').to_string();
        match name {
            "setup" => self.open_setup(),
            "connect" | "run" | "disconnect" | "continue" | "pause" | "step" | "next" | "stepi"
            | "finish" | "restart" | "download" | "refresh" | "build" | "quit" => {
                self.submit(engine, name, json!({}))
            }
            "watch" | "unwatch" => self.submit(engine, name, json!({"expression":arg})),
            "data-break" => self.submit(engine, "data_break", json!({"expression":arg})),
            "break" => self.submit(engine, "break", json!({"location":unquote(arg)})),
            "delete" => self.submit(engine, "delete_break", json!({"number":arg})),
            "frame" => self.submit(
                engine,
                "frame",
                json!({"level":arg.parse::<u64>().unwrap_or(0)}),
            ),
            "memory" => {
                let mut parts = arg.split_whitespace();
                let address = parts.next().unwrap_or("$sp");
                let count = parts
                    .next()
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(256);
                self.pane = 4;
                self.selection = 0;
                self.submit(engine, "memory", json!({"address":address,"count":count}));
            }
            "disasm" => {
                self.pane = 5;
                self.selection = 0;
                self.submit(engine, "disassemble", json!({"address":arg}));
            }
            "files" => {
                self.pane = 7;
                self.selection = 0;
                self.submit(engine, "files", json!({}));
            }
            "open" => {
                let path = unquote(arg);
                self.load_source(&path);
                self.source_line = 0;
                self.pane = 0;
            }
            "find" => {
                if !arg.is_empty() {
                    let count = self.source.len();
                    if let Some(index) = (1..=count)
                        .map(|n| (self.source_line + n) % count)
                        .find(|&i| self.source[i].contains(arg))
                    {
                        self.source_line = index;
                        self.source_top = index.saturating_sub(4);
                        self.pane = 0;
                        self.notice = format!("Found {arg} at line {}", index + 1);
                    } else {
                        self.notice = format!("No match: {arg}");
                    }
                }
            }
            "elf" => {
                let path = unquote(arg);
                self.project.program.elf = PathBuf::from(&path);
                self.submit(engine, "set_elf", json!({"path":path}));
            }
            "help" => self.help = true,
            _ => self.notice = format!("Unknown workstation command: {name}. Use ? for help."),
        }
    }
    fn toggle_break(&mut self, engine: Option<&EngineHandle>) {
        if self.source_file.is_empty() {
            self.notice = "Open a source file first".into();
            return;
        }
        let location = format!(
            "{}:{}",
            self.source_file.replace('\\', "/"),
            self.source_line + 1
        );
        if let Some(b) = self.snapshot.breakpoints.iter().find(|b| {
            b.location
                .replace('\\', "/")
                .eq_ignore_ascii_case(&location)
                || (b.line as usize == self.source_line + 1
                    && b.file
                        .replace('\\', "/")
                        .eq_ignore_ascii_case(&self.source_file.replace('\\', "/")))
        }) {
            let number = b.id.clone();
            self.submit(engine, "delete_break", json!({"number":number}));
        } else {
            self.submit(engine, "break", json!({"location":location}));
        }
    }
    fn move_selection(&mut self, delta: isize) {
        if self.pane == 0 {
            self.source_line = self
                .source_line
                .saturating_add_signed(delta)
                .min(self.source.len().saturating_sub(1));
            if self.source_line < self.source_top {
                self.source_top = self.source_line;
            }
            let height = self.source_rect.height.saturating_sub(2).max(1) as usize;
            if self.source_line >= self.source_top + height {
                self.source_top = self.source_line + 1 - height;
            }
        } else {
            let max = match self.pane {
                1 => self.snapshot.watches.len(),
                2 => self.snapshot.stack.len(),
                3 => self.snapshot.registers.len(),
                4 => self.snapshot.memory.len(),
                5 => self.snapshot.assembly.len(),
                6 => self.snapshot.breakpoints.len(),
                7 => self.snapshot.files.len(),
                _ => self.logs.len(),
            };
            self.selection = self
                .selection
                .saturating_add_signed(delta)
                .min(max.saturating_sub(1));
        }
    }
    fn key(&mut self, key: KeyEvent, engine: Option<&EngineHandle>) -> bool {
        if key.kind == KeyEventKind::Release {
            return false;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('q') {
            if self.demo {
                return true;
            }
            self.submit(engine, "quit", json!({}));
            return false;
        }
        if let Some(setup) = &mut self.setup {
            if key.code == KeyCode::Esc && !setup.is_editing() && !setup.pending {
                self.document = setup.document.clone();
                self.setup = None;
            } else {
                self.launch = setup.key(key);
            }
            return false;
        }
        if self.help {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?') => self.help = false,
                KeyCode::Down => self.help_scroll = self.help_scroll.saturating_add(1).min(30),
                KeyCode::Up => self.help_scroll = self.help_scroll.saturating_sub(1),
                KeyCode::PageDown => self.help_scroll = self.help_scroll.saturating_add(8).min(30),
                KeyCode::PageUp => self.help_scroll = self.help_scroll.saturating_sub(8),
                _ => {}
            }
            return false;
        }
        if self.confirm.is_some() {
            if key.code == KeyCode::Char('y') {
                let request = self.confirm.take().unwrap();
                self.send(engine, request);
            } else if matches!(key.code, KeyCode::Esc | KeyCode::Char('n')) {
                self.confirm = None;
            }
            return false;
        }
        if self.palette {
            match key.code {
                KeyCode::Esc => self.palette = false,
                KeyCode::Down => self.palette_index = (self.palette_index + 1) % COMMANDS.len(),
                KeyCode::Up => {
                    self.palette_index = (self.palette_index + COMMANDS.len() - 1) % COMMANDS.len()
                }
                KeyCode::Enter => {
                    let command = COMMANDS[self.palette_index];
                    self.palette = false;
                    if command.contains(' ') {
                        self.input = format!(":{} ", command.split_whitespace().next().unwrap());
                        self.editing = true;
                    } else {
                        self.command(engine, &format!(":{command}"));
                    }
                }
                _ => {}
            }
            return false;
        }
        if self.editing {
            match key.code {
                KeyCode::Esc => {
                    self.editing = false;
                    self.input.clear();
                }
                KeyCode::Enter => {
                    let input = std::mem::take(&mut self.input);
                    self.editing = false;
                    self.history.push(input.clone());
                    self.history_index = self.history.len();
                    self.command(engine, &input);
                }
                KeyCode::Backspace => {
                    self.input.pop();
                }
                KeyCode::Up => {
                    if self.history_index > 0 {
                        self.history_index -= 1;
                        self.input = self.history[self.history_index].clone();
                    }
                }
                KeyCode::Down => {
                    if self.history_index + 1 < self.history.len() {
                        self.history_index += 1;
                        self.input = self.history[self.history_index].clone();
                    } else {
                        self.history_index = self.history.len();
                        self.input.clear();
                    }
                }
                KeyCode::Tab => {
                    let prefix = self.input.trim_start_matches(':');
                    if let Some(command) = COMMANDS.iter().find(|c| c.starts_with(prefix)) {
                        self.input = format!(":{}", command.split_whitespace().next().unwrap());
                    }
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.input.clear();
                    self.editing = false;
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.input.push(c)
                }
                _ => {}
            }
            return false;
        }
        match key.code {
            KeyCode::F(2) => self.open_setup(),
            KeyCode::F(5) => self.submit(engine, "continue", json!({})),
            KeyCode::F(6) => self.submit(engine, "pause", json!({})),
            KeyCode::F(9) => self.toggle_break(engine),
            KeyCode::F(10) => self.submit(engine, "next", json!({})),
            KeyCode::F(11) => self.submit(
                engine,
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    "finish"
                } else {
                    "step"
                },
                json!({}),
            ),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.submit(engine, "pause", json!({}))
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.palette = true;
                self.palette_index = 0;
            }
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.editing = true;
                self.input = ":find ".into();
            }
            KeyCode::Char(':') => {
                self.editing = true;
                self.input = ":".into();
            }
            KeyCode::Char('/') => {
                self.editing = true;
                self.input.clear();
            }
            KeyCode::Char('?') => self.help = true,
            KeyCode::Char('q') => {
                if self.demo {
                    return true;
                }
                self.submit(engine, "quit", json!({}));
            }
            KeyCode::Tab => {
                self.pane = (self.pane + 1) % PANES.len();
                self.selection = 0;
            }
            KeyCode::BackTab => {
                self.pane = (self.pane + PANES.len() - 1) % PANES.len();
                self.selection = 0;
            }
            KeyCode::Down => self.move_selection(1),
            KeyCode::Up => self.move_selection(-1),
            KeyCode::PageDown => self.move_selection(12),
            KeyCode::PageUp => self.move_selection(-12),
            KeyCode::Home => {
                self.selection = 0;
                self.source_line = 0;
                self.source_top = 0;
            }
            KeyCode::Enter => {
                if self.pane == 7 {
                    if let Some(file) = self.snapshot.files.get(self.selection).cloned() {
                        self.load_source(&file);
                        self.source_line = 0;
                        self.pane = 0;
                    }
                } else if self.pane == 2
                    && let Some(frame) = self.snapshot.stack.get(self.selection)
                {
                    let level = frame.level;
                    self.submit(engine, "frame", json!({"level":level}));
                }
            }
            KeyCode::Delete if self.pane == 6 => {
                if let Some(b) = self.snapshot.breakpoints.get(self.selection) {
                    let number = b.id.clone();
                    self.submit(engine, "delete_break", json!({"number":number}));
                }
            }
            _ => {}
        }
        false
    }
    fn open_setup(&mut self) {
        let mut setup = Setup::new(self.document.clone());
        if !matches!(self.snapshot.state.as_str(), "DISCONNECTED" | "FAULT") {
            setup.message =
                "Current session stays active until F5. Starting ends it and switches projects."
                    .into();
        }
        self.setup = Some(setup);
    }
}

fn section(title: impl Into<Line<'static>>) -> Block<'static> {
    Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(title)
        .title_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
}
fn variables(vars: &[Variable]) -> Vec<Line<'static>> {
    vars.iter()
        .flat_map(|v| {
            vec![
                Line::from(Span::styled(
                    v.name.clone(),
                    Style::default().fg(Color::Cyan),
                )),
                Line::from(Span::styled(
                    format!("  {}{}", if v.changed { "* " } else { "" }, v.value),
                    Style::default().fg(if v.error {
                        Color::Red
                    } else if v.changed {
                        Color::Yellow
                    } else {
                        Color::Reset
                    }),
                )),
            ]
        })
        .collect()
}
fn syntax(line: &str) -> Vec<Span<'static>> {
    if line.trim_start().starts_with("//") || line.trim_start().starts_with("/*") {
        return vec![Span::styled(
            line.to_owned(),
            Style::default().fg(Color::DarkGray),
        )];
    }
    let mut spans = Vec::new();
    let mut word = String::new();
    let flush = |word: &mut String, spans: &mut Vec<Span<'static>>| {
        if !word.is_empty() {
            let color = if matches!(
                word.as_str(),
                "void"
                    | "const"
                    | "static"
                    | "volatile"
                    | "uint8_t"
                    | "uint16_t"
                    | "uint32_t"
                    | "int"
                    | "char"
                    | "return"
                    | "if"
                    | "else"
                    | "for"
                    | "while"
                    | "struct"
                    | "typedef"
                    | "sizeof"
            ) {
                Color::Cyan
            } else if word.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                Color::Yellow
            } else {
                Color::Reset
            };
            spans.push(Span::styled(
                std::mem::take(word),
                Style::default().fg(color),
            ));
        }
    };
    for c in line.replace('\t', "    ").chars() {
        if c.is_alphanumeric() || c == '_' {
            word.push(c);
        } else {
            flush(&mut word, &mut spans);
            spans.push(Span::raw(c.to_string()));
        }
    }
    flush(&mut word, &mut spans);
    spans
}
pub fn draw(f: &mut UiFrame, a: &mut App) {
    if let Some(setup) = &a.setup {
        setup.draw(f);
        return;
    }
    let area = f.area();
    if area.width < 45 || area.height < 12 {
        f.render_widget(
            Paragraph::new("DebugTUI\nEnlarge terminal to at least 45 x 12.\nCtrl+Q exits.")
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Min(5),
            Constraint::Length(1),
            Constraint::Length(2),
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
    let header = Line::from(vec![
        Span::styled(
            " DebugTUI ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            " {} · {}  ",
            a.project.target.mode, a.project.target.endpoint
        )),
        Span::styled(
            a.snapshot.state.clone(),
            Style::default()
                .fg(state_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(if a.quitting {
            "  Closing session…"
        } else {
            ""
        }),
    ]);
    f.render_widget(
        Paragraph::new(vec![
            header,
            Line::from(Span::styled(
                format!(
                    " {}",
                    if a.source_file.is_empty() {
                        a.project.program.elf.to_string_lossy().into_owned()
                    } else {
                        format!(
                            "{}:{}  {}",
                            a.source_file
                                .rsplit(['/', '\\'])
                                .next()
                                .unwrap_or(&a.source_file),
                            a.snapshot.frame.line,
                            a.snapshot.stop_reason
                        )
                    }
                ),
                Style::default().fg(Color::DarkGray),
            )),
        ]),
        rows[0],
    );
    a.tab_rect = rows[1];
    a.visible_tabs = if area.width >= 80 {
        (0..PANES.len()).collect()
    } else {
        let start = a.pane.saturating_sub(1).min(PANES.len() - 3);
        (start..start + 3).collect()
    };
    let labels: Vec<Line> = a
        .visible_tabs
        .iter()
        .map(|&i| Line::from(PANES[i]))
        .collect();
    let selected = a
        .visible_tabs
        .iter()
        .position(|&i| i == a.pane)
        .unwrap_or(0);
    f.render_widget(
        Tabs::new(labels)
            .select(selected)
            .highlight_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .divider("│"),
        rows[1],
    );
    let body = rows[2];
    match a.pane {
        0 => {
            let split = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(4),
                    Constraint::Length(if body.height > 15 { 6 } else { 3 }),
                ])
                .split(body);
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(if body.width >= 90 {
                    vec![Constraint::Percentage(68), Constraint::Percentage(32)]
                } else {
                    vec![Constraint::Percentage(100), Constraint::Length(0)]
                })
                .split(split[0]);
            a.source_rect = columns[0];
            if a.source.is_empty() {
                f.render_widget(Paragraph::new("\n  F2  Choose project and debug environment\n\n  Browse tools, GDB and program files.\n  Save settings and start with F5.\n\n  Ctrl+P  Command palette\n  ?       Keyboard help\n\n  GDB comes from --gdb / PATH or an environment profile.").block(section(" Source ")),columns[0]);
            } else {
                let count = columns[0].height.saturating_sub(1) as usize;
                let lines: Vec<Line> = a
                    .source
                    .iter()
                    .enumerate()
                    .skip(a.source_top)
                    .take(count)
                    .map(|(i, line)| {
                        let pc = i + 1 == a.snapshot.frame.line as usize
                            && a.source_file == a.snapshot.frame.file;
                        let selected = i == a.source_line;
                        let bp = a.snapshot.breakpoints.iter().any(|b| {
                            b.enabled
                                && b.line as usize == i + 1
                                && b.file
                                    .replace('\\', "/")
                                    .eq_ignore_ascii_case(&a.source_file.replace('\\', "/"))
                        });
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
                        let mut row = Line::from(spans);
                        if pc {
                            row = row.style(Style::default().add_modifier(Modifier::BOLD));
                        }
                        row
                    })
                    .collect();
                f.render_widget(
                    Paragraph::new(lines).block(section(" Source · F9 breakpoint ")),
                    columns[0],
                );
            }
            if columns[1].width > 0 {
                let side = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
                    .split(columns[1]);
                f.render_widget(
                    Paragraph::new(variables(&a.snapshot.watches))
                        .block(section(" Watch · stopped snapshot "))
                        .wrap(Wrap { trim: false }),
                    side[0],
                );
                f.render_widget(
                    Paragraph::new(variables(&a.snapshot.locals))
                        .block(section(" Locals "))
                        .wrap(Wrap { trim: false }),
                    side[1],
                );
            }
            let bottom = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
                .split(split[1]);
            f.render_widget(
                Paragraph::new(
                    a.snapshot
                        .stack
                        .iter()
                        .map(|s| Line::raw(format!(" #{} {} :{}", s.level, s.function, s.line)))
                        .collect::<Vec<_>>(),
                )
                .block(section(" Call Stack ")),
                bottom[0],
            );
            let height = bottom[1].height.saturating_sub(1) as usize;
            let logs = a
                .console
                .iter()
                .skip(a.console.len().saturating_sub(height))
                .map(|s| Line::raw(s.clone()))
                .collect::<Vec<_>>();
            f.render_widget(Paragraph::new(logs).block(section(" Console ")), bottom[1]);
        }
        1 => f.render_widget(
            Paragraph::new(variables(&a.snapshot.watches))
                .block(section(" Watch · :watch EXPR / :unwatch EXPR "))
                .scroll((
                    a.selection.saturating_mul(2).min(u16::MAX as usize) as u16,
                    0,
                ))
                .wrap(Wrap { trim: false }),
            body,
        ),
        2 => {
            let items: Vec<ListItem> = a
                .snapshot
                .stack
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    ListItem::new(format!(
                        "{} #{} {:20} {}:{}",
                        if i == a.selection { "›" } else { " " },
                        s.level,
                        s.function,
                        s.file,
                        s.line
                    ))
                    .style(if i == a.selection {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default()
                    })
                })
                .collect();
            f.render_widget(
                List::new(items).block(section(" Call Stack · Enter selects frame ")),
                body,
            );
        }
        3 => f.render_widget(
            Paragraph::new(
                a.snapshot
                    .registers
                    .iter()
                    .skip(a.selection)
                    .map(|v| {
                        Line::from(Span::styled(
                            format!(
                                " {:6} {} {}",
                                v.name,
                                v.value,
                                if v.changed { "*" } else { "" }
                            ),
                            Style::default().fg(if v.changed {
                                Color::Yellow
                            } else {
                                Color::Reset
                            }),
                        ))
                    })
                    .collect::<Vec<_>>(),
            )
            .block(section(" Registers · last stopped snapshot ")),
            body,
        ),
        4 | 5 => {
            let lines = if a.pane == 4 {
                &a.snapshot.memory
            } else {
                &a.snapshot.assembly
            };
            let title = if a.pane == 4 {
                " Memory · :memory ADDRESS [COUNT] "
            } else {
                " Assembly · :disasm $pc "
            };
            f.render_widget(
                Paragraph::new(
                    lines
                        .iter()
                        .skip(a.selection)
                        .map(|s| Line::raw(s.clone()))
                        .collect::<Vec<_>>(),
                )
                .block(section(title)),
                body,
            );
        }
        6 => f.render_widget(
            Paragraph::new(
                a.snapshot
                    .breakpoints
                    .iter()
                    .enumerate()
                    .skip(
                        a.selection
                            .saturating_sub(body.height.saturating_sub(2) as usize),
                    )
                    .map(|(i, b)| {
                        Line::from(Span::styled(
                            format!(
                                "{} {:3} {:3} {:20} {}",
                                if i == a.selection { "›" } else { " " },
                                b.id,
                                if b.enabled { "on" } else { "off" },
                                b.kind,
                                b.location
                            ),
                            Style::default().fg(if i == a.selection {
                                Color::Cyan
                            } else {
                                Color::Reset
                            }),
                        ))
                    })
                    .collect::<Vec<_>>(),
            )
            .block(section(" Breakpoints · Delete removes selected ")),
            body,
        ),
        7 => f.render_widget(
            Paragraph::new(
                a.snapshot
                    .files
                    .iter()
                    .enumerate()
                    .skip(
                        a.selection
                            .saturating_sub(body.height.saturating_sub(2) as usize),
                    )
                    .map(|(i, s)| {
                        Line::from(Span::styled(
                            format!("{} {s}", if i == a.selection { "›" } else { " " }),
                            Style::default().fg(if i == a.selection {
                                Color::Cyan
                            } else {
                                Color::Reset
                            }),
                        ))
                    })
                    .collect::<Vec<_>>(),
            )
            .block(section(" Source Files · :files loads list · Enter opens ")),
            body,
        ),
        _ => {
            let height = body.height.saturating_sub(1) as usize;
            let start = if a.selection == 0 {
                a.logs.len().saturating_sub(height)
            } else {
                a.selection
            };
            f.render_widget(
                Paragraph::new(
                    a.logs
                        .iter()
                        .skip(start)
                        .map(|s| Line::raw(s.clone()))
                        .collect::<Vec<_>>(),
                )
                .block(section(" Debug Console · / enters GDB command ")),
                body,
            );
        }
    }
    f.render_widget(
        Paragraph::new(format!(" {}", a.notice)).style(Style::default().fg(
            if a.notice.starts_with("Error") {
                Color::Red
            } else {
                Color::DarkGray
            },
        )),
        rows[3],
    );
    let input = if a.editing {
        format!(" › {}", a.input)
    } else {
        " › F2 Setup   : Commands   / GDB   Ctrl+P Actions".into()
    };
    f.render_widget(
        Paragraph::new(input).block(Block::default().borders(Borders::TOP).border_style(
            Style::default().fg(if a.editing {
                Color::Cyan
            } else {
                Color::DarkGray
            }),
        )),
        rows[4],
    );
    if a.editing {
        let width = unicode_width::UnicodeWidthStr::width(a.input.as_str()) as u16;
        f.set_cursor_position((
            rows[4].x + (3 + width).min(rows[4].width.saturating_sub(1)),
            rows[4].y + 1,
        ));
    }
    f.render_widget(
        Paragraph::new(" F5 Continue  F6 Pause  F9 Break  F10 Next  F11 Step  Tab Panels  ? Help")
            .style(Style::default().fg(Color::DarkGray)),
        rows[5],
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
        let r = center(area, 55, 26);
        f.render_widget(Clear, r);
        let start = a
            .palette_index
            .saturating_sub(r.height.saturating_sub(3) as usize);
        let lines: Vec<Line> = COMMANDS
            .iter()
            .enumerate()
            .skip(start)
            .map(|(i, s)| {
                Line::from(Span::styled(
                    format!("{} {s}", if i == a.palette_index { "›" } else { " " }),
                    Style::default().fg(if i == a.palette_index {
                        Color::Cyan
                    } else {
                        Color::Reset
                    }),
                ))
            })
            .collect();
        f.render_widget(
            Paragraph::new(lines).block(
                Block::bordered()
                    .title(" Commands · ↑ ↓ Enter · Esc ")
                    .border_style(Style::default().fg(Color::Cyan)),
            ),
            r,
        );
    }
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
fn center(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width.saturating_sub(2));
    let h = h.min(area.height.saturating_sub(2));
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            DisableBracketedPaste,
            DisableMouseCapture,
            LeaveAlternateScreen,
            crossterm::cursor::Show
        );
    }
}
pub fn run(
    document: Document,
    demo: bool,
    auto_connect: bool,
    initial_error: Option<String>,
) -> Result<(), String> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(
            "Interactive mode requires a terminal. Use --headless --stdio for automation.".into(),
        );
    }
    enable_raw_mode().map_err(|e| e.to_string())?;
    let _guard = TerminalGuard;
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )
    .map_err(|e| e.to_string())?;
    let mut terminal =
        Terminal::new(CrosstermBackend::new(io::stdout())).map_err(|e| e.to_string())?;
    let (mut project, error) = match document.project() {
        Ok(project) => (project, initial_error),
        Err(e) => (Project::default(), Some(e)),
    };
    if !document.path.is_file() {
        project.path = None;
    }
    let ready = project.clone().prepare().is_ok();
    let mut app = App::new(project.clone(), demo);
    app.document = document;
    if !demo && (!auto_connect || !ready || error.is_some()) {
        app.open_setup();
        if let Some(e) = error {
            app.setup.as_mut().unwrap().message = format!("Error: {e}");
        }
    }
    let mut engine = if demo {
        None
    } else {
        Some(session::spawn(project.clone()))
    };
    if !demo && app.setup.is_none() {
        app.submit(engine.as_ref(), "connect", json!({}));
    }
    const SWITCH_QUIT: u64 = u64::MAX - 1;
    let mut pending_launch: Option<Launch> = None;
    let mut switch_error = None;
    let mut dirty = true;
    let mut last_draw = Instant::now() - Duration::from_secs(1);
    loop {
        let mut worker_exited = false;
        if let Some(engine) = &engine {
            for _ in 0..128 {
                match engine.events.try_recv() {
                    Ok(event) => {
                        if let Event::Response {
                            id,
                            ok: false,
                            error,
                            ..
                        } = &event
                            && *id == SWITCH_QUIT
                        {
                            switch_error = error.clone();
                        }
                        if matches!(event, Event::Exit) && pending_launch.is_some() && !app.quitting
                        {
                            worker_exited = true;
                            break;
                        }
                        if app.update(event) {
                            if app.snapshot.state == "FAULT" {
                                return Err(
                                    "Session ended with errors; target state must be checked"
                                        .into(),
                                );
                            }
                            return Ok(());
                        }
                        dirty = true;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        return Err("Debug session worker exited unexpectedly".into());
                    }
                }
            }
        }
        if worker_exited {
            engine.take();
            let mut launch = pending_launch.take().unwrap();
            let prepared = (|| -> Result<Project, String> {
                if let Some(e) = switch_error.take() {
                    return Err(e);
                }
                if launch.save {
                    launch.document.save()?;
                }
                let mut project = launch.document.project()?;
                project.prepare()?;
                if !launch.save {
                    project.path = None;
                }
                Ok(project)
            })();
            match prepared {
                Ok(project) => {
                    app.project = project.clone();
                    app.document = launch.document;
                    app.setup = None;
                    app.snapshot = Snapshot::default();
                    app.source.clear();
                    app.source_file.clear();
                    app.source_line = 0;
                    app.source_top = 0;
                    app.selection = 0;
                    app.pane = 0;
                    app.editing = false;
                    app.confirm = None;
                    engine = Some(session::spawn(project));
                    app.submit(engine.as_ref(), "connect", json!({}));
                }
                Err(e) => {
                    if let Some(setup) = &mut app.setup {
                        setup.pending = false;
                        setup.message = format!("Error: {e}");
                    }
                    engine = Some(session::spawn(app.project.clone()));
                }
            }
            dirty = true;
        }
        if let Some(launch) = app.launch.take() {
            if let Some(engine) = &engine {
                engine.send(Request::new(SWITCH_QUIT, "quit", json!({})))?;
                pending_launch = Some(launch);
            } else if let Some(setup) = &mut app.setup {
                setup.pending = false;
                setup.message = "DEMO: restart without --demo to connect a debugger.".into();
            }
        }
        if dirty && last_draw.elapsed() >= Duration::from_millis(25) {
            terminal
                .draw(|f| draw(f, &mut app))
                .map_err(|e| e.to_string())?;
            dirty = false;
            last_draw = Instant::now();
        }
        if event::poll(Duration::from_millis(20)).map_err(|e| e.to_string())? {
            match event::read().map_err(|e| e.to_string())? {
                Input::Key(key) => {
                    if app.key(key, engine.as_ref()) {
                        return Ok(());
                    }
                    dirty = true;
                }
                Input::Resize(_, _) => dirty = true,
                Input::Paste(text) => {
                    if let Some(setup) = &mut app.setup {
                        setup.paste(&text);
                        dirty = true;
                    } else if app.editing {
                        app.input.push_str(&text);
                        dirty = true;
                    }
                }
                Input::Mouse(mouse) => {
                    if app.setup.is_some() {
                        continue;
                    }
                    match mouse.kind {
                        MouseEventKind::ScrollDown => app.move_selection(3),
                        MouseEventKind::ScrollUp => app.move_selection(-3),
                        MouseEventKind::Down(event::MouseButton::Left) => {
                            if mouse.row == app.tab_rect.y {
                                let mut x = app.tab_rect.x + 1;
                                for &i in &app.visible_tabs {
                                    let end = x + PANES[i].len() as u16 + 2;
                                    if mouse.column >= x && mouse.column < end {
                                        app.pane = i;
                                        app.selection = 0;
                                        break;
                                    }
                                    x = end + 1;
                                }
                            } else if app.pane == 0
                                && app.source_rect.contains((mouse.column, mouse.row).into())
                            {
                                app.source_line = (app.source_top
                                    + mouse.row.saturating_sub(app.source_rect.y + 1) as usize)
                                    .min(app.source.len().saturating_sub(1));
                                if mouse.column < app.source_rect.x + 7 {
                                    app.toggle_break(engine.as_ref());
                                }
                            }
                        }
                        _ => {}
                    }
                    dirty = true;
                }
                _ => {}
            }
        }
    }
}
pub fn snapshot(path: &Path) -> Result<(), String> {
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).map_err(|e| e.to_string())?;
    let mut app = App::new(Project::default(), true);
    terminal
        .draw(|f| draw(f, &mut app))
        .map_err(|e| e.to_string())?;
    let b = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..36 {
        for x in 0..120 {
            text.push_str(b[(x, y)].symbol());
        }
        text.push('\n');
    }
    fs::write(path, text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn renders_narrow_and_wide_layouts() {
        for (w, h) in [(45, 12), (80, 24), (120, 36), (180, 50)] {
            let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
            let mut a = App::new(Project::default(), true);
            for pane in 0..PANES.len() {
                a.pane = pane;
                t.draw(|f| draw(f, &mut a)).unwrap();
                let text = t
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|c| c.symbol())
                    .collect::<String>();
                assert!(text.contains("DebugTUI"));
            }
            a.help = true;
            t.draw(|f| draw(f, &mut a)).unwrap();
        }
    }
    #[test]
    fn unicode_input_backspace_and_history() {
        let mut a = App::new(Project::default(), true);
        a.editing = true;
        a.input = "中文x".into();
        a.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE), None);
        assert_eq!(a.input, "中文");
        a.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), None);
        assert!(!a.editing);
    }
    #[test]
    fn server_traffic_does_not_hide_gdb_console_results() {
        let mut a = App::new(Project::default(), false);
        a.log("[gdb] $1 = 0xef4018".into());
        for _ in 0..1100 {
            a.log("[server] Read register 'pc'".into());
        }
        let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
        t.draw(|f| draw(f, &mut a)).unwrap();
        let rendered = t
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(rendered.contains("$1 = 0xef4018"));
        assert_eq!(a.logs.len(), 1000);
    }
}
