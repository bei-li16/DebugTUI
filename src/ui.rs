use crate::{
    config::Project,
    launch::{Document, Launch, Setup},
    session::{self, EngineHandle, Event, Frame, Request, Snapshot, Variable},
};
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event as Input, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame as UiFrame, Terminal,
    backend::{CrosstermBackend, TestBackend},
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use serde_json::{Value, json};
use std::{
    collections::{HashSet, VecDeque},
    fs,
    io::{self, IsTerminal},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const PANES: [&str; 10] = [
    "Source", "Watch", "Stack", "Regs", "Memory", "Asm", "Breaks", "Files", "Log", "Locals",
];
mod render;
mod source_tabs;
pub use render::draw;
use source_tabs::SourceTabs;

const MAIN_PANES: [usize; 4] = [0, 5, 7, 8];
const SIDE_PANES: [usize; 4] = [3, 2, 4, 6];
const VARIABLE_PANES: [usize; 2] = [1, 9];
const COMMANDS: [&str; 29] = [
    "setup",
    "connect",
    "reconnect",
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
    "find TEXT",
    "elf PATH",
    "refresh",
    "build",
    "help",
    "quit",
];
const DEMO_SOURCE: &str = "/* DebugTUI demo: no target connected */\n#include <stdint.h>\n\n\nvolatile uint32_t counter;\nvolatile uint8_t flag = 1;\n\nvoid process_items(void)\n{\n    for (unsigned i = 0; i < 100; ++i) {\n        update_value(\"sample\\n\");\n        counter++;\n    }\n}\n\nvoid update_value(const char *format)\n{\n    uint8_t ret = 0;\n    flag = 0;\n    /* Place a data breakpoint on flag. */\n}\n";
const HELP: &str = r#"DebugTUI — GDB debugging workspace

Left: Source / Asm / Files / Log
Right top: Regs / Stack / Memory / Breaks
Right bottom: Watch / Locals
Tab / Shift+Tab switches view and keyboard focus.
Click tabs to change only that group; source stays visible.
Wheel over a view or drag its scrollbar to browse content.
Click Stack rows to select a frame; Delete removes a breakpoint.
Narrow terminals show the focused group; Tab reaches all views.
Source files stay open in tabs; click a name or × to close.
< / > and the tab-strip wheel browse hidden tabs; [Files N] lists all.
Ctrl+PgUp / Ctrl+PgDn switches files; Ctrl+W closes; Ctrl+O lists.

Toolbar buttons: Run, Continue, Pause, Reset, Reconnect,
Step, Next, Finish, CommandList. Unavailable actions are dimmed.
CommandList / Ctrl+P lists commands; click or Enter selects.
Commands with arguments open the command line for editing.
Asm loads at $pc on entry and updates after each stop.
Memory loads at $sp; :memory ADDRESS [COUNT] reads another range.

F2 Setup         F5 Continue      F6 Pause
F9 Breakpoint    F10 Next         F11 Step / Shift+F11 Finish
Arrows / PgUp / PgDn / Home / End scroll or select
Enter opens a file / selects a stack frame
: Command line   / GDB console   Ctrl+F Find source
Click the Console input to type GDB commands or :commands.
Enter submits; Up / Down recalls history; Esc leaves input.
Ctrl+C Pause     Ctrl+Q Quit      Esc Cancel / close dialog

:setup :connect :reconnect :run :continue :pause :disconnect
:step :next :stepi :finish :restart :download :refresh
:watch counter   :unwatch counter   :data-break flag
:break main   :delete 2   :frame 1
:memory $sp 256   :disasm $pc   :files   :open source.c
:find text   :elf app.elf   :build   :help   :quit

Other input goes to GDB, e.g. p/x variable or info registers.
Running views show the last stopped snapshot.
Run / reset / download behavior comes from the environment.
Exit behavior is controlled by session.on_exit.

Esc / ? closes this help."#;
pub struct App {
    project: Project,
    document: Document,
    setup: Option<Setup>,
    launch: Option<Launch>,
    snapshot: Snapshot,
    pane: usize,
    main_pane: usize,
    side_pane: usize,
    variable_pane: usize,
    selections: [usize; 10],
    selection: usize,
    source: Vec<String>,
    source_file: String,
    source_line: usize,
    source_top: usize,
    sources: SourceTabs,
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
    side_rect: Rect,
    console_input_rect: Rect,
    view_rects: [Rect; 10],
    view_tops: [usize; 10],
    log_follow: bool,
    pane_hits: Vec<(Rect, usize)>,
    action_hits: Vec<(Rect, &'static str)>,
    palette_hits: Vec<(Rect, usize)>,
    scrollbars: [Rect; 10],
    scroll_drag: Option<(usize, u16)>,
    view_stamps: [Option<String>; 10],
    view_errors: [Option<String>; 10],
    pending_view: Option<(u64, usize)>,
    pending_commands: HashSet<u64>,
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
            main_pane: 0,
            side_pane: 3,
            variable_pane: 1,
            selections: [0; 10],
            selection: 0,
            source: vec![],
            source_file: String::new(),
            source_line: 0,
            source_top: 0,
            sources: SourceTabs::default(),
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
            side_rect: Rect::default(),
            console_input_rect: Rect::default(),
            view_rects: [Rect::default(); 10],
            view_tops: [0; 10],
            log_follow: true,
            pane_hits: vec![],
            action_hits: vec![],
            palette_hits: vec![],
            scrollbars: [Rect::default(); 10],
            scroll_drag: None,
            view_stamps: Default::default(),
            view_errors: Default::default(),
            pending_view: None,
            pending_commands: HashSet::new(),
            help_scroll: 0,
        };
        if demo {
            a.source = DEMO_SOURCE.lines().map(str::to_owned).collect();
            a.source_file = "demo/sample.c".into();
            a.source_line = 17;
            a.source_top = 9;
            a.remember_demo_source();
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
            a.snapshot.assembly = vec![
                "0x0800068c  push {r7, lr}".into(),
                "0x0800068e  mov r7, sp".into(),
                "0x08000690  movs r3, #0".into(),
                "0x08000692  strb r3, [r7, #7]".into(),
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
            self.view_tops[8] = self.view_tops[8].saturating_sub(1);
        }
        self.logs.push_back(text);
    }
    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::Snapshot { snapshot } => {
                if matches!(
                    snapshot.state.as_str(),
                    "DISCONNECTED" | "STARTING GDB" | "FAULT"
                ) {
                    self.view_stamps.fill(None);
                }
                let moved = snapshot.generation != self.snapshot.generation
                    || snapshot.frame.file != self.snapshot.frame.file
                    || snapshot.frame.line != self.snapshot.frame.line
                    || snapshot.frame.level != self.snapshot.frame.level;
                if moved && snapshot.state == "STOPPED" && !snapshot.frame.file.is_empty() {
                    self.sources.frame_key = self.source_key(&snapshot.frame.file);
                    self.load_source(&snapshot.frame.file);
                    self.source_line = snapshot.frame.line.saturating_sub(1) as usize;
                    self.source_top = self.source_line.saturating_sub(8);
                } else if moved && snapshot.state == "STOPPED" {
                    // Preserve open tabs, but never present the old source as the
                    // current stop when the new PC has no source information.
                    self.sources.frame_key.clear();
                    self.hide_source();
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
                let background_view = self.pending_view.is_some_and(|(pending, _)| pending == id);
                self.pending_commands.remove(&id);
                if let Some((pending, pane)) = self.pending_view
                    && pending == id
                {
                    self.view_errors[pane] = if ok { None } else { error.clone() };
                    self.pending_view = None;
                }
                self.notice = if ok {
                    format!("Command {id} completed")
                } else {
                    format!("Error: {}", error.unwrap_or_default())
                };
                if !background_view && !result.is_null() && result.to_string().len() < 3000 {
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
        if let Some(engine) = engine {
            let id = request.id;
            self.pending_commands.insert(id);
            if let Err(e) = engine.send(request) {
                self.pending_commands.remove(&id);
                self.notice = format!("Session worker unavailable: {e}");
            }
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
            "connect" | "reconnect" | "run" | "disconnect" | "continue" | "pause" | "step"
            | "next" | "stepi" | "finish" | "restart" | "download" | "refresh" | "build"
            | "quit" => {
                if name == "refresh" {
                    self.view_stamps.fill(None);
                }
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
                self.select_pane(4);
                self.view_stamps[4] = Some(self.view_stamp());
                self.submit(engine, "memory", json!({"address":address,"count":count}));
            }
            "disasm" => {
                self.select_pane(5);
                self.view_stamps[5] = Some(self.view_stamp());
                self.submit(engine, "disassemble", json!({"address":arg}));
            }
            "files" => {
                self.select_pane(7);
                self.view_stamps[7] = Some("files".into());
                self.submit(engine, "files", json!({}));
            }
            "open" => {
                let path = unquote(arg);
                self.load_source(&path);
                self.select_pane(0);
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
                        self.select_pane(0);
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
        let source_key = self.source_key(&self.source_file);
        if let Some(b) = self.snapshot.breakpoints.iter().find(|b| {
            b.location
                .replace('\\', "/")
                .eq_ignore_ascii_case(&location)
                || (b.line as usize == self.source_line + 1
                    && self.source_key(&b.file) == source_key)
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
            let height = self.source_rect.height.max(1) as usize;
            if self.source_line >= self.source_top + height {
                self.source_top = self.source_line + 1 - height;
            }
        } else if matches!(self.pane, 2 | 6 | 7) {
            let max = self.view_len(self.pane);
            self.selection = self
                .selection
                .saturating_add_signed(delta)
                .min(max.saturating_sub(1));
            let height = self.view_rects[self.pane].height.max(1) as usize;
            let top = self.view_tops[self.pane];
            if self.selection < top {
                self.view_tops[self.pane] = self.selection;
            } else if self.selection >= top + height {
                self.view_tops[self.pane] = self.selection + 1 - height;
            }
        } else {
            self.set_view_top(
                self.pane,
                self.view_tops[self.pane].saturating_add_signed(delta),
            );
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
        if self.sources.list_open {
            self.source_list_key(key);
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
                    self.activate_palette(engine);
                }
                _ => {}
            }
            return false;
        }
        if !self.editing && key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::PageUp => {
                    self.cycle_source(-1);
                    return false;
                }
                KeyCode::PageDown => {
                    self.cycle_source(1);
                    return false;
                }
                KeyCode::Char('w') if self.main_pane == 0 => {
                    if let Some(index) = self.sources.active {
                        self.close_source(index);
                    }
                    return false;
                }
                KeyCode::Char('o') => {
                    self.open_source_list();
                    return false;
                }
                _ => {}
            }
        }
        let workspace_shortcut = matches!(key.code, KeyCode::F(_))
            || (key.code == KeyCode::Char('p') && key.modifiers.contains(KeyModifiers::CONTROL));
        if self.editing && !workspace_shortcut {
            match key.code {
                KeyCode::Esc => {
                    self.editing = false;
                    self.input.clear();
                }
                KeyCode::Enter => {
                    let input = std::mem::take(&mut self.input);
                    if !input.trim().is_empty() {
                        self.history.push(input.clone());
                        self.history_index = self.history.len();
                        self.command(engine, &input);
                    }
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
                self.cycle_pane(1);
            }
            KeyCode::BackTab => {
                self.cycle_pane(-1);
            }
            KeyCode::Down => self.move_selection(1),
            KeyCode::Up => self.move_selection(-1),
            KeyCode::PageDown => self.move_selection(12),
            KeyCode::PageUp => self.move_selection(-12),
            KeyCode::Home => {
                self.selection = 0;
                self.set_view_top(self.pane, 0);
                if self.pane == 8 {
                    self.log_follow = false;
                }
                if self.pane == 0 {
                    self.source_line = 0;
                    self.source_top = 0;
                }
            }
            KeyCode::End => self.move_selection(isize::MAX),
            KeyCode::Enter => {
                if self.pane == 7 {
                    if let Some(file) = self.snapshot.files.get(self.selection).cloned() {
                        self.load_source(&file);
                        self.select_pane(0);
                    }
                } else if self.pane == 2
                    && let Some(frame) = self.snapshot.stack.get(self.selection)
                {
                    let level = frame.level;
                    self.submit(engine, "frame", json!({"level":level}));
                } else if !matches!(self.pane, 2 | 7) {
                    self.editing = true;
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
    fn select_pane(&mut self, pane: usize) {
        self.selections[self.pane] = self.selection;
        self.pane = pane;
        self.selection = self.selections[pane];
        if MAIN_PANES.contains(&pane) {
            self.main_pane = pane;
        } else if SIDE_PANES.contains(&pane) {
            self.side_pane = pane;
        } else {
            self.variable_pane = pane;
        }
    }
    fn selected(&self, pane: usize) -> usize {
        if self.pane == pane {
            self.selection
        } else {
            self.selections[pane]
        }
    }
    fn cycle_pane(&mut self, delta: isize) {
        let panes = [0, 5, 7, 8, 3, 2, 4, 6, 1, 9];
        let index = panes.iter().position(|&p| p == self.pane).unwrap_or(0);
        self.select_pane(panes[(index as isize + delta).rem_euclid(panes.len() as isize) as usize]);
    }
    fn view_stamp(&self) -> String {
        format!(
            "{}:{}:{}",
            self.snapshot.generation, self.snapshot.frame.address, self.snapshot.frame.level
        )
    }
    // Fetch expensive views only when visible, once per stopped location. Responses
    // arrive through the same worker as user actions; rendering never blocks on GDB.
    fn ensure_visible_data(&mut self, engine: Option<&EngineHandle>) -> bool {
        if self.demo
            || self.setup.is_some()
            || self.quitting
            || self.pending_view.is_some()
            || !self.pending_commands.is_empty()
            || self.snapshot.state != "STOPPED"
        {
            return false;
        }
        let mut panes = vec![];
        if self.source_rect.width > 0 {
            panes.push(self.main_pane);
        }
        if self.side_rect.width > 0 {
            panes.push(self.side_pane);
        }
        for pane in panes {
            let (method, params) = match pane {
                5 => ("disassemble", json!({"address":"$pc"})),
                4 => ("memory", json!({"address":"$sp","count":256})),
                7 => ("files", json!({})),
                _ => continue,
            };
            let stamp = if pane == 7 {
                "files".into()
            } else {
                self.view_stamp()
            };
            if self.view_stamps[pane].as_ref() == Some(&stamp) {
                continue;
            }
            self.view_stamps[pane] = Some(stamp);
            self.view_errors[pane] = None;
            match pane {
                5 => {
                    self.snapshot.assembly.clear();
                    self.view_tops[5] = 0;
                }
                4 => {
                    self.snapshot.memory.clear();
                    self.view_tops[4] = 0;
                }
                _ => {}
            }
            self.pending_view = Some((self.next_id, pane));
            self.submit(engine, method, params);
            return true;
        }
        false
    }
    fn activate_palette(&mut self, engine: Option<&EngineHandle>) {
        let command = COMMANDS[self.palette_index];
        self.palette = false;
        if command.contains(' ') {
            self.input = format!(":{} ", command.split_whitespace().next().unwrap());
            self.editing = true;
        } else {
            self.command(engine, &format!(":{command}"));
        }
    }
    fn action_enabled(&self, command: &str) -> bool {
        if self.quitting {
            return false;
        }
        if self.demo {
            return true;
        }
        match command {
            "commandlist" => true,
            "reconnect" => {
                !self.snapshot.state.starts_with("STARTING") && self.snapshot.state != "CONNECTING"
            }
            "pause" => self.snapshot.state == "RUNNING",
            "run" | "continue" => matches!(self.snapshot.state.as_str(), "STOPPED" | "READY"),
            "restart" => {
                self.snapshot.state == "STOPPED" && !self.project.actions.restart.is_empty()
            }
            _ => self.snapshot.state == "STOPPED",
        }
    }
    fn view_len(&self, pane: usize) -> usize {
        match pane {
            0 => self.source.len(),
            1 => self.snapshot.watches.len() * 2,
            2 => self.snapshot.stack.len(),
            3 => self.snapshot.registers.len(),
            4 => self.snapshot.memory.len(),
            5 => self.snapshot.assembly.len(),
            6 => self.snapshot.breakpoints.len(),
            7 => self.snapshot.files.len(),
            8 => self.logs.len(),
            _ => self.snapshot.locals.len() * 2,
        }
    }
    fn set_view_top(&mut self, pane: usize, top: usize) {
        let visible = self.view_rects[pane].height.max(1) as usize;
        let len = self.view_len(pane);
        let max = len.saturating_sub(visible);
        let top = top.min(max);
        if pane == 0 {
            self.source_top = top;
            self.source_line = self
                .source_line
                .clamp(top, (top + visible - 1).min(len.saturating_sub(1)));
        } else {
            self.view_tops[pane] = top;
            if matches!(pane, 2 | 6 | 7) {
                let selected = self
                    .selected(pane)
                    .clamp(top, (top + visible - 1).min(len.saturating_sub(1)));
                self.selections[pane] = selected;
                if self.pane == pane {
                    self.selection = selected;
                }
            }
            if pane == 8 {
                self.log_follow = top == max;
            }
        }
    }
    fn scrollbar_thumb(&self, pane: usize) -> (usize, usize, usize) {
        let height = self.scrollbars[pane].height as usize;
        let visible = self.view_rects[pane].height as usize;
        let len = self.view_len(pane);
        let max = len.saturating_sub(visible);
        let thumb = (height * visible / len.max(1)).clamp(1, height.max(1));
        let travel = height.saturating_sub(thumb);
        let position = if pane == 0 {
            self.source_top
        } else {
            self.view_tops[pane]
        };
        let top = (position.min(max) * travel).checked_div(max).unwrap_or(0);
        (top, thumb, max)
    }
    fn drag_scrollbar(&mut self, pane: usize, row: u16, offset: u16) {
        let (_, thumb, max) = self.scrollbar_thumb(pane);
        let travel = (self.scrollbars[pane].height as usize).saturating_sub(thumb);
        let position = row
            .saturating_sub(self.scrollbars[pane].y)
            .saturating_sub(offset) as usize;
        let top = (position.min(travel) * max)
            .checked_div(travel)
            .unwrap_or(0);
        self.set_view_top(pane, top);
    }
    fn mouse(&mut self, mouse: MouseEvent, engine: Option<&EngineHandle>) {
        if self.setup.is_some() || self.help || self.confirm.is_some() || self.quitting {
            return;
        }
        let point = (mouse.column, mouse.row).into();
        if self.source_tabs_mouse(mouse) {
            return;
        }
        if self.palette {
            match mouse.kind {
                MouseEventKind::ScrollDown => {
                    self.palette_index = (self.palette_index + 1).min(COMMANDS.len() - 1)
                }
                MouseEventKind::ScrollUp => {
                    self.palette_index = self.palette_index.saturating_sub(1)
                }
                MouseEventKind::Down(event::MouseButton::Left) => {
                    if let Some((_, index)) = self
                        .palette_hits
                        .iter()
                        .find(|(rect, _)| rect.contains(point))
                    {
                        self.palette_index = *index;
                        self.activate_palette(engine);
                    }
                }
                _ => {}
            }
            return;
        }
        match mouse.kind {
            MouseEventKind::Up(event::MouseButton::Left) => self.scroll_drag = None,
            MouseEventKind::Drag(event::MouseButton::Left) => {
                if let Some((pane, offset)) = self.scroll_drag {
                    self.drag_scrollbar(pane, mouse.row, offset);
                }
            }
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                let delta = if mouse.kind == MouseEventKind::ScrollDown {
                    3
                } else {
                    -3
                };
                if let Some(pane) = (0..PANES.len()).find(|&pane| {
                    self.view_rects[pane].contains(point) || self.scrollbars[pane].contains(point)
                }) {
                    self.select_pane(pane);
                    let top = if pane == 0 {
                        self.source_top
                    } else {
                        self.view_tops[pane]
                    };
                    self.set_view_top(pane, top.saturating_add_signed(delta));
                }
            }
            MouseEventKind::Down(event::MouseButton::Left) => {
                if self.console_input_rect.contains(point) {
                    self.editing = true;
                    self.history_index = self.history.len();
                    return;
                }
                self.editing = false;
                if let Some((_, pane)) =
                    self.pane_hits.iter().find(|(rect, _)| rect.contains(point))
                {
                    self.select_pane(*pane);
                } else if let Some((_, action)) = self
                    .action_hits
                    .iter()
                    .find(|(rect, _)| rect.contains(point))
                {
                    let action = *action;
                    if self.action_enabled(action) {
                        if action == "commandlist" {
                            self.palette = true;
                            self.palette_index = 0;
                        } else {
                            self.command(engine, &format!(":{action}"));
                        }
                    }
                } else if let Some(pane) =
                    self.scrollbars.iter().position(|rect| rect.contains(point))
                {
                    self.select_pane(pane);
                    let (top, thumb, _) = self.scrollbar_thumb(pane);
                    let click = mouse.row.saturating_sub(self.scrollbars[pane].y) as usize;
                    let offset = if (top..top + thumb).contains(&click) {
                        click - top
                    } else {
                        thumb / 2
                    } as u16;
                    self.scroll_drag = Some((pane, offset));
                    self.drag_scrollbar(pane, mouse.row, offset);
                } else if let Some(pane) =
                    self.view_rects.iter().position(|rect| rect.contains(point))
                {
                    self.select_pane(pane);
                    let rect = self.view_rects[pane];
                    if pane == 0 {
                        self.source_line = (self.source_top
                            + mouse.row.saturating_sub(self.source_rect.y) as usize)
                            .min(self.source.len().saturating_sub(1));
                        if mouse.column < self.source_rect.x + 7 {
                            self.toggle_break(engine);
                        }
                    } else if matches!(pane, 2 | 6 | 7) {
                        let clicked =
                            self.view_tops[pane] + mouse.row.saturating_sub(rect.y) as usize;
                        if clicked >= self.view_len(pane) {
                            return;
                        }
                        self.selection = clicked;
                        if matches!(pane, 2 | 7) {
                            self.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), engine);
                        }
                    }
                }
            }
            _ => {}
        }
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
                    app.sources = SourceTabs::default();
                    app.selection = 0;
                    app.pane = 0;
                    app.main_pane = 0;
                    app.side_pane = 3;
                    app.variable_pane = 1;
                    app.selections.fill(0);
                    app.view_tops.fill(0);
                    app.log_follow = true;
                    app.scroll_drag = None;
                    app.view_stamps.fill(None);
                    app.view_errors.fill(None);
                    app.pending_view = None;
                    app.pending_commands.clear();
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
        if app.ensure_visible_data(engine.as_ref()) {
            dirty = true;
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
                    } else if app.sources.list_open {
                        app.sources
                            .query
                            .extend(text.chars().filter(|c| !c.is_control()));
                        app.sources.list_index = 0;
                        dirty = true;
                    } else if app.editing {
                        app.input.push_str(&text);
                        dirty = true;
                    }
                }
                Input::Mouse(mouse) => {
                    app.mouse(mouse, engine.as_ref());
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
    fn render(a: &mut App, w: u16, h: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| draw(f, a)).unwrap();
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                    .collect::<String>()
                    + "\n"
            })
            .collect()
    }
    fn mouse_at(a: &mut App, kind: MouseEventKind, x: u16, y: u16, engine: Option<&EngineHandle>) {
        a.mouse(
            MouseEvent {
                kind,
                column: x,
                row: y,
                modifiers: KeyModifiers::NONE,
            },
            engine,
        );
    }
    #[test]
    fn inspector_tabs_keep_source_visible_without_duplicate_stack() {
        let mut a = App::new(Project::default(), true);
        for pane in SIDE_PANES {
            render(&mut a, 160, 45);
            let rect = a.pane_hits.iter().find(|(_, i)| *i == pane).unwrap().0;
            mouse_at(
                &mut a,
                MouseEventKind::Down(event::MouseButton::Left),
                rect.x,
                rect.y,
                None,
            );
            let text = render(&mut a, 160, 45);
            assert_eq!(a.main_pane, 0);
            assert_eq!(a.side_pane, pane);
            assert!(text.contains("counter++;"));
            assert!(!text.contains("Call Stack"));
            assert!(a.side_rect.x > a.source_rect.right());
        }
    }
    #[test]
    fn variable_tabs_are_below_registers_and_keep_independent_focus() {
        let mut a = App::new(Project::default(), true);
        let text = render(&mut a, 160, 45);
        assert!(text.contains(&format!("DebugTUI v{}", env!("CARGO_PKG_VERSION"))));
        let hit = |app: &App, pane| app.pane_hits.iter().find(|(_, id)| *id == pane).unwrap().0;
        let regs = hit(&a, 3);
        assert!(regs.x < hit(&a, 2).x);
        assert!(hit(&a, 1).y > regs.y);
        assert!(hit(&a, 1).x < hit(&a, 9).x);
        assert!(text.contains("12345"));
        let locals = hit(&a, 9);
        mouse_at(
            &mut a,
            MouseEventKind::Down(event::MouseButton::Left),
            locals.x,
            locals.y,
            None,
        );
        let text = render(&mut a, 160, 45);
        assert!(text.contains("process_items\\n"));
        assert!(!text.contains("12345"));
        assert_eq!(a.side_pane, 3);
        assert_eq!(a.main_pane, 0);
        assert!(a.view_rects[9].height > 0);
        assert_eq!(a.view_rects[1].height, 0);
        // A short or narrow terminal must still let Tab reach the variables.
        for (w, h) in [(80, 24), (120, 12)] {
            render(&mut a, w, h);
            assert!(a.view_rects[9].height > 0);
            assert!(a.console_input_rect.height > 0);
        }
    }
    #[test]
    fn assembly_files_and_log_scrollbars_browse_all_rows_and_remember_position() {
        let mut a = App::new(Project::default(), true);
        a.snapshot.assembly = (0..200).map(|i| format!("instruction-{i:03}")).collect();
        a.snapshot.files = (0..200).map(|i| format!("file-{i:03}.c")).collect();
        a.logs = (0..200).map(|i| format!("log-{i:03}")).collect();
        for (pane, first, last) in [
            (5, "instruction-000", "instruction-199"),
            (7, "file-000.c", "file-199.c"),
            (8, "log-000", "log-199"),
        ] {
            a.select_pane(pane);
            render(&mut a, 140, 40);
            let bar = a.scrollbars[pane];
            assert_eq!(bar.height, a.view_rects[pane].height);
            assert!(bar.height > 0);
            mouse_at(
                &mut a,
                MouseEventKind::Down(event::MouseButton::Left),
                bar.x,
                bar.y,
                None,
            );
            mouse_at(
                &mut a,
                MouseEventKind::Drag(event::MouseButton::Left),
                bar.x,
                0,
                None,
            );
            assert!(render(&mut a, 140, 40).contains(first));
            mouse_at(
                &mut a,
                MouseEventKind::Drag(event::MouseButton::Left),
                bar.x,
                bar.bottom() + 20,
                None,
            );
            let text = render(&mut a, 140, 40);
            assert!(text.contains(last));
            assert!(!text.contains(first));
            mouse_at(
                &mut a,
                MouseEventKind::Up(event::MouseButton::Left),
                bar.x,
                bar.y,
                None,
            );
            let top = a.view_tops[pane];
            mouse_at(&mut a, MouseEventKind::ScrollUp, bar.x, bar.y, None);
            assert_eq!(a.view_tops[pane], top - 3);
            a.select_pane(0);
            render(&mut a, 140, 40);
            a.select_pane(pane);
            render(&mut a, 140, 40);
            assert_eq!(a.view_tops[pane], top - 3);
            assert_eq!(a.source_top, 0); // browsing other views never moves source
        }
        a.select_pane(7);
        let text = render(&mut a, 140, 40);
        let rect = a.view_rects[7];
        let clicked = a.view_tops[7] + 2;
        assert!(text.contains(&format!("file-{clicked:03}.c")));
        mouse_at(
            &mut a,
            MouseEventKind::Down(event::MouseButton::Left),
            rect.x + 3,
            rect.y + 2,
            None,
        );
        assert_eq!(a.source_file, format!("file-{clicked:03}.c"));
        assert_eq!(a.main_pane, 0);
    }
    #[test]
    fn log_scrollback_stays_put_until_end_resumes_following() {
        let mut a = App::new(Project::default(), true);
        a.logs = (0..1000).map(|i| format!("record-{i:04}")).collect();
        a.select_pane(8);
        assert!(render(&mut a, 120, 36).contains("record-0999"));
        a.key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE), None);
        assert!(render(&mut a, 120, 36).contains("record-0000"));
        a.move_selection(20);
        let text = render(&mut a, 120, 36);
        assert!(text.contains("record-0020"));
        a.log("record-1000".into());
        assert!(render(&mut a, 120, 36).contains("record-0020"));
        assert!(!a.log_follow);
        a.key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE), None);
        a.log("record-1001".into());
        assert!(render(&mut a, 120, 36).contains("record-1001"));
        assert!(a.log_follow);
    }
    #[test]
    fn console_input_click_submit_history_and_debug_shortcuts() {
        let (engine, commands) = session::test_channel();
        let mut a = App::new(Project::default(), false);
        a.snapshot.state = "STOPPED".into();
        assert!(render(&mut a, 120, 36).contains("gdb>"));
        let input = a.console_input_rect;
        mouse_at(
            &mut a,
            MouseEventKind::Down(event::MouseButton::Left),
            input.x + 8,
            input.y,
            Some(&engine),
        );
        for ch in "p/x counter".chars() {
            a.key(
                KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
                Some(&engine),
            );
        }
        a.key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            Some(&engine),
        );
        let request = commands.try_recv().unwrap();
        assert_eq!(request.method, "console");
        assert_eq!(request.params["command"], "p/x counter");
        assert!(a.editing);
        assert!(a.input.is_empty());
        for ch in ":watch counter".chars() {
            a.key(
                KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
                Some(&engine),
            );
        }
        a.key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            Some(&engine),
        );
        let request = commands.try_recv().unwrap();
        assert_eq!(request.method, "watch");
        assert_eq!(request.params["expression"], "counter");
        a.key(
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            Some(&engine),
        );
        assert_eq!(a.input, ":watch counter");
        a.key(
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            Some(&engine),
        );
        assert_eq!(a.input, "p/x counter");
        a.key(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            Some(&engine),
        );
        a.key(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            Some(&engine),
        );
        assert!(a.input.is_empty());
        a.key(
            KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE),
            Some(&engine),
        );
        assert_eq!(commands.try_recv().unwrap().method, "next");
        assert!(a.editing);
        a.input = "中".repeat(150) + " tail";
        assert!(render(&mut a, 80, 24).contains("tail"));
        a.key(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            Some(&engine),
        );
        assert!(!a.editing);
        assert!(commands.try_recv().is_err());
    }
    #[test]
    fn stop_without_symbols_does_not_keep_previous_source_location() {
        let mut a = App::new(Project::default(), true);
        let mut stopped = a.snapshot.clone();
        stopped.state = "STOPPED".into();
        stopped.generation += 1;
        stopped.stop_reason = "signal-received".into();
        stopped.frame = Frame {
            address: "0x00006978".into(),
            function: "??".into(),
            ..Default::default()
        };
        a.update(Event::Snapshot {
            snapshot: Box::new(stopped),
        });
        let text = render(&mut a, 160, 40);
        assert!(text.contains("PC 0x00006978"));
        assert!(text.contains("No source location"));
        assert!(!text.contains("counter++;"));
        assert!(!text.contains("sample.c:0"));
        // Browsing another file must not change the displayed stopped location.
        a.source_file = "browsed.c".into();
        a.source = vec!["void browsed(void);".into()];
        let text = render(&mut a, 160, 40);
        assert!(text.contains("void browsed"));
        assert!(text.lines().nth(1).unwrap().contains("PC 0x00006978"));
    }
    #[test]
    fn scrollbar_click_drag_and_wheel_reach_both_ends() {
        let mut a = App::new(Project::default(), true);
        a.source = (0..500).map(|i| format!("line {i}")).collect();
        a.source_top = 0;
        a.source_line = 0;
        render(&mut a, 140, 40);
        let bar = a.scrollbars[0];
        let max = a.source.len() - a.source_rect.height as usize;
        mouse_at(
            &mut a,
            MouseEventKind::Down(event::MouseButton::Left),
            bar.x,
            bar.y,
            None,
        );
        mouse_at(
            &mut a,
            MouseEventKind::Drag(event::MouseButton::Left),
            bar.x,
            bar.bottom() + 20,
            None,
        );
        assert_eq!(a.source_top, max);
        mouse_at(
            &mut a,
            MouseEventKind::Drag(event::MouseButton::Left),
            bar.x,
            0,
            None,
        );
        assert_eq!(a.source_top, 0);
        mouse_at(
            &mut a,
            MouseEventKind::Up(event::MouseButton::Left),
            bar.x,
            0,
            None,
        );
        assert!(a.scroll_drag.is_none());
        let source = a.source_rect;
        mouse_at(
            &mut a,
            MouseEventKind::ScrollDown,
            source.x + 12,
            source.y,
            None,
        );
        assert_eq!(a.source_top, 3);
        mouse_at(
            &mut a,
            MouseEventKind::ScrollUp,
            source.x + 12,
            source.y,
            None,
        );
        assert_eq!(a.source_top, 0);
        assert!(render(&mut a, 140, 40).contains("line 0"));
    }
    #[test]
    fn toolbar_dispatches_commands_and_palette_is_clickable() {
        let (engine, commands) = session::test_channel();
        let mut a = App::new(Project::default(), false);
        a.project.actions.restart = vec!["monitor reset".into()];
        a.snapshot.state = "STOPPED".into();
        render(&mut a, 120, 36);
        for action in [
            "run",
            "continue",
            "restart",
            "reconnect",
            "step",
            "next",
            "finish",
        ] {
            let hit = a
                .action_hits
                .iter()
                .find(|(_, id)| *id == action)
                .unwrap()
                .0;
            mouse_at(
                &mut a,
                MouseEventKind::Down(event::MouseButton::Left),
                hit.x,
                hit.y,
                Some(&engine),
            );
            assert_eq!(commands.try_recv().unwrap().method, action);
        }
        let pause = a
            .action_hits
            .iter()
            .find(|(_, id)| *id == "pause")
            .unwrap()
            .0;
        mouse_at(
            &mut a,
            MouseEventKind::Down(event::MouseButton::Left),
            pause.x,
            pause.y,
            Some(&engine),
        );
        assert!(commands.try_recv().is_err());
        a.snapshot.state = "RUNNING".into();
        mouse_at(
            &mut a,
            MouseEventKind::Down(event::MouseButton::Left),
            pause.x,
            pause.y,
            Some(&engine),
        );
        assert_eq!(commands.try_recv().unwrap().method, "pause");
        let hit = a
            .action_hits
            .iter()
            .find(|(_, id)| *id == "commandlist")
            .unwrap()
            .0;
        mouse_at(
            &mut a,
            MouseEventKind::Down(event::MouseButton::Left),
            hit.x,
            hit.y,
            Some(&engine),
        );
        assert!(a.palette);
        render(&mut a, 120, 36);
        let index = COMMANDS
            .iter()
            .position(|c| *c == "watch EXPRESSION")
            .unwrap();
        let hit = a.palette_hits.iter().find(|(_, i)| *i == index).unwrap().0;
        mouse_at(
            &mut a,
            MouseEventKind::Down(event::MouseButton::Left),
            hit.x,
            hit.y,
            Some(&engine),
        );
        assert!(!a.palette);
        assert!(a.editing);
        assert_eq!(a.input, ":watch ");
    }
    #[test]
    fn assembly_loads_on_entry_and_stop_without_repeated_requests() {
        let (engine, commands) = session::test_channel();
        let mut a = App::new(Project::default(), false);
        a.snapshot.state = "STOPPED".into();
        a.snapshot.frame.address = "0x08000000".into();
        render(&mut a, 120, 36);
        assert!(!a.ensure_visible_data(Some(&engine)));
        a.select_pane(5);
        render(&mut a, 120, 36);
        assert!(a.ensure_visible_data(Some(&engine)));
        let request = commands.try_recv().unwrap();
        assert_eq!(request.method, "disassemble");
        assert_eq!(request.params["address"], "$pc");
        assert!(!a.ensure_visible_data(Some(&engine)));
        a.update(Event::Response {
            id: request.id,
            ok: true,
            result: json!({}),
            error: None,
        });
        assert!(!a.ensure_visible_data(Some(&engine)));
        a.snapshot.generation += 1;
        a.snapshot.frame.address = "0x08000004".into();
        assert!(a.ensure_visible_data(Some(&engine)));
        let request = commands.try_recv().unwrap();
        a.update(Event::Response {
            id: request.id,
            ok: false,
            result: Value::Null,
            error: Some("Cannot access memory".into()),
        });
        assert!(render(&mut a, 120, 36).contains("Cannot access memory"));
        assert!(!a.ensure_visible_data(Some(&engine)));
        a.snapshot.state = "RUNNING".into();
        a.snapshot.generation += 1;
        assert!(!a.ensure_visible_data(Some(&engine)));
    }
    #[test]
    fn lazy_views_wait_for_reset_to_finish_before_reading_pc() {
        let (engine, commands) = session::test_channel();
        let mut a = App::new(Project::default(), false);
        a.project.actions.restart = vec!["monitor reset".into()];
        a.snapshot.state = "STOPPED".into();
        a.select_pane(5);
        render(&mut a, 120, 36);
        a.command(Some(&engine), ":restart");
        let reset = commands.try_recv().unwrap();
        assert!(!a.ensure_visible_data(Some(&engine)));
        a.snapshot.frame.address = "0x08000100".into();
        a.update(Event::Response {
            id: reset.id,
            ok: true,
            result: json!({}),
            error: None,
        });
        assert!(a.ensure_visible_data(Some(&engine)));
        assert_eq!(commands.try_recv().unwrap().method, "disassemble");
        assert!(!a.ensure_visible_data(Some(&engine)));
    }
    #[test]
    fn mouse_selects_sidebar_frame_and_does_not_toggle_source_breakpoint() {
        let (engine, commands) = session::test_channel();
        let mut a = App::new(Project::default(), true);
        a.demo = false;
        a.snapshot.state = "STOPPED".into();
        a.select_pane(2);
        render(&mut a, 140, 40);
        let side = a.side_rect;
        mouse_at(
            &mut a,
            MouseEventKind::Down(event::MouseButton::Left),
            side.x,
            side.y + 1,
            Some(&engine),
        );
        let request = commands.try_recv().unwrap();
        assert_eq!(request.method, "frame");
        assert_eq!(request.params["level"], 1);
        assert_eq!(a.main_pane, 0);
        assert!(commands.try_recv().is_err());
    }
    #[test]
    fn write_layout_previews_when_requested() {
        let Ok(path) = std::env::var("DEBUGTUI_RENDER_DIR") else {
            return;
        };
        fs::create_dir_all(&path).unwrap();
        let mut a = App::new(Project::default(), true);
        for (name, pane, w, h) in [
            ("workspace", 0, 160, 45),
            ("assembly", 5, 120, 36),
            ("stack", 2, 120, 36),
            ("compact", 0, 80, 24),
        ] {
            a.select_pane(pane);
            fs::write(
                Path::new(&path).join(format!("{name}.txt")),
                render(&mut a, w, h),
            )
            .unwrap();
        }
        a.palette = true;
        fs::write(
            Path::new(&path).join("commands.txt"),
            render(&mut a, 120, 36),
        )
        .unwrap();
    }
    #[test]
    fn renders_narrow_and_wide_layouts() {
        for (w, h) in [(45, 12), (80, 24), (120, 36), (180, 50)] {
            let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
            let mut a = App::new(Project::default(), true);
            for pane in 0..PANES.len() {
                a.select_pane(pane);
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
