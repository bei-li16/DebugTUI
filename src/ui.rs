use crate::{
    config::Project,
    launch::{Document, Launch, Setup},
    session::{self, EngineHandle, Event, Frame, Request, Snapshot, Variable},
    theme,
};
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
        EnableFocusChange, EnableMouseCapture, Event as Input, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseEvent, MouseEventKind,
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
    widgets::{Block, Paragraph, Wrap},
};
use serde_json::{Value, json};
use std::{
    collections::{HashSet, VecDeque},
    fs,
    io::{self, IsTerminal},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const PANES: [&str; 11] = [
    "Source",
    "Watch",
    "Stack",
    "System Regs",
    "Memory",
    "Asm",
    "Breaks",
    "Files",
    "Log",
    "Locals",
    "Peripherals",
];
mod completion;
mod console;
mod effects;
mod formats;
mod highlight;
mod peripherals;
mod render;
mod source_tabs;
#[cfg(test)]
mod visual_tests;
use highlight::syntax;
pub use render::draw;
use source_tabs::SourceTabs;
use theme::section;

const MAIN_PANES: [usize; 4] = [0, 5, 7, 8];
const SIDE_PANES: [usize; 5] = [3, 10, 2, 4, 6];
const VARIABLE_PANES: [usize; 2] = [1, 9];
const COMMANDS: [&str; 33] = [
    "appearance",
    "animations MODE",
    "format",
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
    "peripheral-refresh",
    "build",
    "help",
    "quit",
];
const DEMO_SOURCE: &str = "/* DebugTUI demo: no target connected */\n#include <stdint.h>\n\n\nvolatile uint32_t counter;\nvolatile uint8_t flag = 1;\n\nvoid process_items(void)\n{\n    for (unsigned i = 0; i < 100; ++i) {\n        update_value(\"sample\\n\");\n        counter++;\n    }\n}\n\nvoid update_value(const char *format)\n{\n    uint8_t ret = 0;\n    flag = 0;\n    /* Place a data breakpoint on flag. */\n}\n";
const HELP: &str = r#"DebugTUI — GDB debugging workspace

Left: Source / Asm / Files / Log
Right top: System Regs / Peripherals / Stack / Memory / Breaks
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
Step In, Step Over, Step Out, Exit, Help. Unavailable actions are dimmed.
Help / Ctrl+P lists commands; click or Enter selects.
Help tabs / Tab switch between Commands and Shortcuts.
Commands with arguments open the command line for editing.
Asm loads at $pc on entry and updates after each stop.
Project bar: Build / Download (configure commands in F2 Setup).
Shell commands run in Source root; output appears in Console.
Connected sessions are released, then restored after success.
Memory loads at $sp; :memory ADDRESS [COUNT] reads another range.
Peripherals: configure SVD in F2 Setup. Click / Enter expands groups and fields.
Left / Right collapses / expands; r / Refresh reads the selected register.
Only visible registers inside expanded groups refresh after stops.
Write-only registers are skipped; read side effects require manual refresh.

F2 Setup         F5 Continue      F6 Pause
F9 Breakpoint    F10 Step Over    F11 Step In / Shift+F11 Step Out
Arrows / PgUp / PgDn / Home / End scroll or select
Enter opens a file / selects a stack frame
: Command line   / GDB console   Ctrl+F Find source
Click the Console input to type GDB commands or :commands.
f / right-click a value: binary, octal, decimal or hexadecimal for that item.
Data defaults to decimal; registers and SVD fields default to hexadecimal.
:appearance chooses Off / Subtle / Full animation and Unicode / ASCII effects.
Both inputs offer completion: Up / Down selects, Tab or click fills, Enter submits.
With no suggestions, Up / Down recalls Console history; Esc leaves input.
Watch: click watch> or select Watch and Enter; type a global variable to add it.
Variable and member completion uses the current ELF/GDB when stopped.
Ctrl+C Pause     Ctrl+Q Exit      Esc Cancel / close dialog
Step In / Over / Out send GDB step / next / finish respectively.

:setup :connect :reconnect :run :continue :pause :disconnect
:step :next :stepi :finish :restart :download :refresh
:watch counter   :unwatch counter   :data-break flag
:break main   :delete 2   :frame 1
:memory $sp 256   :disasm $pc   :files   :open source.c
:find text   :elf app.elf   :build   :help   :quit

Other input goes to GDB, e.g. p/x variable or info registers.
Running views show the last stopped snapshot.
Run / reset / download behavior comes from the environment.
Exit ends debugging and closes TUI, following session.on_exit.

Esc / ? closes this help."#;
pub struct App {
    project: Project,
    document: Document,
    setup: Option<Setup>,
    launch: Option<Launch>,
    snapshot: Snapshot,
    peripherals: peripherals::Peripherals,
    pane: usize,
    main_pane: usize,
    side_pane: usize,
    variable_pane: usize,
    selections: [usize; 11],
    selection: usize,
    source: Vec<String>,
    source_comments: Vec<bool>,
    source_file: String,
    source_line: usize,
    source_top: usize,
    sources: SourceTabs,
    logs: VecDeque<String>,
    console: VecDeque<String>,
    console_view: console::ConsoleView,
    input: String,
    editing: bool,
    watch_input: String,
    watch_editing: bool,
    watch_input_rect: Rect,
    pending_watch: Option<(u64, String)>,
    completion: completion::Completion,
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
    view_rects: [Rect; 11],
    view_tops: [usize; 11],
    log_follow: bool,
    pane_hits: Vec<(Rect, usize)>,
    action_hits: Vec<(Rect, &'static str)>,
    palette_hits: Vec<(Rect, usize)>,
    help_tab_hits: Vec<(Rect, bool)>,
    scrollbars: [Rect; 11],
    scroll_drag: Option<(usize, u16)>,
    view_stamps: [Option<String>; 11],
    view_errors: [Option<String>; 11],
    pending_view: Option<(u64, usize)>,
    pending_commands: HashSet<u64>,
    pending_task: Option<u64>,
    help_scroll: u16,
    pointer: Option<ratatui::layout::Position>,
    fx: effects::Effects,
    formats: formats::Formats,
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
            peripherals: peripherals::Peripherals::load(&project.program.svd),
            project,
            snapshot: Snapshot::default(),
            pane: 0,
            main_pane: 0,
            side_pane: 3,
            variable_pane: 1,
            selections: [0; 11],
            selection: 0,
            source: vec![],
            source_comments: vec![],
            source_file: String::new(),
            source_line: 0,
            source_top: 0,
            sources: SourceTabs::default(),
            logs: VecDeque::new(),
            console: VecDeque::new(),
            input: String::new(),
            editing: false,
            watch_input: String::new(),
            watch_editing: false,
            watch_input_rect: Rect::default(),
            pending_watch: None,
            completion: completion::Completion::default(),
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
            console_view: console::ConsoleView::default(),
            view_rects: [Rect::default(); 11],
            view_tops: [0; 11],
            log_follow: true,
            pane_hits: vec![],
            action_hits: vec![],
            palette_hits: vec![],
            help_tab_hits: vec![],
            scrollbars: [Rect::default(); 11],
            scroll_drag: None,
            view_stamps: Default::default(),
            view_errors: Default::default(),
            pending_view: None,
            pending_commands: HashSet::new(),
            pending_task: None,
            help_scroll: 0,
            pointer: None,
            fx: effects::Effects::default(),
            formats: formats::Formats::default(),
        };
        a.fx.mode = a.project.ui.animations;
        if demo {
            a.source = DEMO_SOURCE.lines().map(str::to_owned).collect();
            a.source_comments = highlight::comment_starts(&a.source);
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
        self.log_at(text, &crate::logging::Stamp::now().wall);
    }
    fn log_at(&mut self, text: String, timestamp: &str) {
        let text: String = text.chars().take(4000).collect();
        let console = !text.starts_with("[server]") && !text.starts_with("[diagnostic]");
        let clock = timestamp.get(11..23).unwrap_or(timestamp);
        let text = format!("[{clock}] {text}");
        if console {
            let evicted = self.console.len() >= console::HISTORY_LIMIT;
            if evicted {
                self.console.pop_front();
            }
            self.console_view.appended(evicted);
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
                self.fx.snapshot(&self.snapshot, &snapshot);
                if matches!(
                    snapshot.state.as_str(),
                    "DISCONNECTED" | "STARTING GDB" | "FAULT"
                ) {
                    self.view_stamps.fill(None);
                    self.peripherals.invalidate();
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
            Event::Log {
                channel,
                text,
                timestamp,
                ..
            } => {
                if channel == "progress"
                    && let Ok(v) = serde_json::from_str::<Value>(&text)
                    && let Some(task) = &mut self.fx.task
                {
                    task.progress = v
                        .get("percent")
                        .and_then(Value::as_u64)
                        .filter(|n| *n <= 100);
                }
                if channel != "mi>" && channel != "mi<" {
                    for line in text.lines() {
                        self.log_at(format!("[{channel}] {line}"), &timestamp);
                    }
                }
            }
            Event::Response {
                id,
                ok,
                result,
                error,
            } => {
                if self.formats.pending_save.remove(&id) {
                    if !ok {
                        self.notice = format!(
                            "Error: display settings were not saved: {}",
                            error.unwrap_or_default()
                        );
                    } else {
                        if result.get("saved").and_then(Value::as_bool) == Some(false) {
                            self.notice.push_str(" · session only (no saved project)");
                        }
                        if self.setup.is_none()
                            && let Ok(document) = Document::open(&self.document.path)
                        {
                            self.document = document;
                        }
                    }
                    return false;
                }
                self.fx.response(id, ok);
                if self.completion_response(id, &result, error.as_deref()) {
                    return false;
                }
                if self
                    .pending_watch
                    .as_ref()
                    .is_some_and(|(pending, _)| *pending == id)
                {
                    let (_, expression) = self.pending_watch.take().unwrap();
                    if ok && self.watch_input.trim() == expression {
                        self.watch_input.clear();
                    }
                    if ok {
                        let bottom = self
                            .view_len(1)
                            .saturating_sub(self.view_rects[1].height as usize);
                        self.set_view_top(1, bottom);
                    }
                }
                let background_view = self.pending_view.is_some_and(|(pending, _)| pending == id);
                self.peripherals.response(id, &result, error.as_deref());
                self.pending_commands.remove(&id);
                if self.pending_task == Some(id) {
                    self.pending_task = None;
                }
                if let Some((pending, pane)) = self.pending_view
                    && pending == id
                {
                    self.view_errors[pane] = if ok || pane == peripherals::PANE {
                        None
                    } else {
                        error.clone()
                    };
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
        if self.pending_task.is_some() && method != "quit" {
            self.notice = "Build / Download is in progress. Ctrl+Q cancels and exits.".into();
            return;
        }
        if self.demo {
            if method == "quit" {
                self.quitting = true;
                return;
            }
            self.notice = format!("DEMO: {method} (no hardware action)");
            self.log(self.notice.clone());
            return;
        }
        if (method == "download" && !self.project.has_download())
            || (method == "build" && !self.project.has_build())
            || (method == "restart" && self.project.actions.restart.is_empty())
        {
            self.notice = format!("{method} is not configured. Open F2 Setup to configure it.");
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
        self.completion.invalidate();
        self.fx.request(request.id, &request.method);
        if matches!(request.method.as_str(), "build" | "download") {
            self.pending_task = Some(request.id);
        }
        if request.method == "quit" {
            self.quitting = true;
        }
        self.notice = format!("{}…", request.method);
        if let Some(engine) = engine {
            let id = request.id;
            self.pending_commands.insert(id);
            if let Err(e) = engine.send(request) {
                self.pending_commands.remove(&id);
                if self.pending_task == Some(id) {
                    self.pending_task = None;
                }
                self.fx.response(id, false);
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
            "appearance" => self.open_appearance(),
            "format" => self.open_format(None),
            "animations" => {
                self.project.ui.animations = match arg {
                    "off" => crate::config::Motion::Off,
                    "subtle" => crate::config::Motion::Subtle,
                    "full" => crate::config::Motion::Full,
                    _ => {
                        self.notice = "Use :animations off|subtle|full".into();
                        return;
                    }
                };
                self.fx.mode = self.project.ui.animations;
                self.notice = format!("Animations: {arg}");
                self.save_ui(engine);
            }
            "setup" => self.open_setup(),
            "connect" | "reconnect" | "run" | "disconnect" | "continue" | "pause" | "step"
            | "next" | "stepi" | "finish" | "restart" | "download" | "refresh" | "build"
            | "quit" => {
                if name == "refresh" {
                    self.view_stamps.fill(None);
                    self.peripherals.invalidate();
                }
                self.submit(engine, name, json!({}))
            }
            "peripheral-refresh" => self.refresh_peripheral(engine),
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
            "help" => self.open_help(true),
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
        self.formats.selected = None;
        self.fx.trigger(format!("scroll:{}", self.pane), 650);
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
        } else if matches!(self.pane, 1 | 2 | 3 | 4 | 6 | 7 | 9 | 10) {
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
        if key.kind == KeyEventKind::Release || self.quitting {
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
        if self.format_key(key, engine) {
            return false;
        }
        if self.help {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?') => self.help = false,
                KeyCode::Tab | KeyCode::BackTab => self.open_help(false),
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
                KeyCode::Tab | KeyCode::BackTab => self.open_help(true),
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
        if self.console_key(key) {
            return false;
        }
        if !self.input_active() && key.modifiers.contains(KeyModifiers::CONTROL) {
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
        if self.input_active() && !workspace_shortcut {
            self.input_key(key, engine);
            return false;
        }
        match key.code {
            KeyCode::Char('f') if key.modifiers.is_empty() => self.open_format(None),
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
                self.palette_index = 0;
                self.open_help(false);
            }
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.focus_input(false);
                self.input = ":find ".into();
            }
            KeyCode::Char(':') => {
                self.focus_input(false);
                self.input = ":".into();
            }
            KeyCode::Char('/') => {
                self.focus_input(false);
                self.input.clear();
            }
            KeyCode::Char('?') => self.open_help(true),
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
            KeyCode::Char('r') if self.pane == peripherals::PANE => self.refresh_peripheral(engine),
            KeyCode::Left if self.pane == peripherals::PANE => self.toggle_peripheral(Some(false)),
            KeyCode::Right if self.pane == peripherals::PANE => self.toggle_peripheral(Some(true)),
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
                if self.pane == peripherals::PANE {
                    self.toggle_peripheral(None);
                } else if self.pane == 7 {
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
                    self.focus_input(self.pane == 1);
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
        self.console_view.focused = false;
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
        let panes = [0, 5, 7, 8, 3, 10, 2, 4, 6, 1, 9];
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
            || self.completion.busy()
            || !self.pending_commands.is_empty()
            || self.snapshot.state != "STOPPED"
        {
            return false;
        }
        if self.ensure_peripherals(engine) {
            return true;
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
    fn open_help(&mut self, shortcuts: bool) {
        self.help = shortcuts;
        self.palette = !shortcuts;
    }
    fn activate_palette(&mut self, engine: Option<&EngineHandle>) {
        let command = COMMANDS[self.palette_index];
        self.palette = false;
        if command.contains(' ') {
            self.input = format!(":{} ", command.split_whitespace().next().unwrap());
            self.focus_input(false);
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
        if self.pending_task.is_some() && !matches!(command, "commandlist" | "quit") {
            return false;
        }
        match command {
            "commandlist" | "quit" => true,
            "build" => {
                self.project.has_build()
                    && matches!(
                        self.snapshot.state.as_str(),
                        "STOPPED" | "READY" | "DISCONNECTED" | "FAULT"
                    )
            }
            "download" => {
                if !self.project.tasks.download.trim().is_empty() {
                    matches!(
                        self.snapshot.state.as_str(),
                        "STOPPED" | "READY" | "DISCONNECTED" | "FAULT"
                    )
                } else {
                    self.project.has_download() && self.snapshot.state == "STOPPED"
                }
            }
            "reconnect" => {
                !self.snapshot.state.starts_with("STARTING") && self.snapshot.state != "CONNECTING"
            }
            "pause" => self.snapshot.state == "RUNNING",
            "peripheral-refresh" => {
                self.side_pane == peripherals::PANE
                    && self.snapshot.state == "STOPPED"
                    && self.pending_commands.is_empty()
            }
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
            4 => self.memory_bytes().len().div_ceil(self.memory_columns()),
            5 => self.snapshot.assembly.len(),
            6 => self.snapshot.breakpoints.len(),
            7 => self.snapshot.files.len(),
            8 => self.logs.len(),
            10 => self.peripherals.len(),
            _ => self.snapshot.locals.len() * 2,
        }
    }
    fn set_view_top(&mut self, pane: usize, top: usize) {
        self.fx.trigger(format!("scroll:{pane}"), 650);
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
            if matches!(pane, 1 | 2 | 3 | 4 | 6 | 7 | 9 | 10) {
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
        self.pointer = Some((mouse.column, mouse.row).into());
        if self.setup.is_some() || self.confirm.is_some() || self.quitting {
            return;
        }
        let point = (mouse.column, mouse.row).into();
        if self.formats.popup.is_some() || self.formats.appearance {
            self.format_mouse(mouse, engine);
            return;
        }
        if self.help || self.palette {
            if mouse.kind == MouseEventKind::Down(event::MouseButton::Left)
                && let Some((_, shortcuts)) =
                    self.help_tab_hits.iter().find(|(r, _)| r.contains(point))
            {
                self.open_help(*shortcuts);
                return;
            }
            if self.help {
                match mouse.kind {
                    MouseEventKind::ScrollDown => {
                        self.help_scroll = self.help_scroll.saturating_add(3).min(30)
                    }
                    MouseEventKind::ScrollUp => {
                        self.help_scroll = self.help_scroll.saturating_sub(3)
                    }
                    _ => {}
                }
                return;
            }
        }
        if !self.help && !self.palette && self.completion.area.contains(point) {
            match mouse.kind {
                MouseEventKind::Down(event::MouseButton::Left) => {
                    if let Some((_, index)) = self
                        .completion
                        .hits
                        .iter()
                        .find(|(rect, _)| rect.contains(point))
                    {
                        self.completion.selected = *index;
                        self.accept_completion();
                    }
                }
                MouseEventKind::ScrollDown => {
                    self.completion.selected = (self.completion.selected + 1)
                        .min(self.completion.items.len().saturating_sub(1));
                }
                MouseEventKind::ScrollUp => {
                    self.completion.selected = self.completion.selected.saturating_sub(1)
                }
                _ => {}
            }
            return;
        }
        if !self.help && !self.palette && !self.sources.list_open {
            if self.format_mouse(mouse, engine) {
                return;
            }
            let hit = self
                .action_hits
                .iter()
                .map(|(r, _)| *r)
                .chain(self.pane_hits.iter().map(|(r, _)| *r))
                .find(|r| r.contains(point));
            if mouse.kind == MouseEventKind::Moved {
                if hit != self.fx.hover.map(|(r, _)| r) {
                    self.fx.hover = hit.map(|r| (r, Instant::now()));
                    self.fx.trigger("hover", 250);
                }
            } else if mouse.kind == MouseEventKind::Down(event::MouseButton::Left) {
                self.fx.pressed = hit;
                self.fx.trigger("press", 180);
            }
        }
        if !self.help && !self.palette && !self.sources.list_open && self.console_mouse(mouse) {
            return;
        }
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
                if self.watch_input_rect.contains(point) {
                    self.focus_input(true);
                    return;
                }
                if self.console_input_rect.contains(point) {
                    self.focus_input(false);
                    return;
                }
                self.editing = false;
                self.watch_editing = false;
                self.completion.invalidate();
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
                            self.palette_index = 0;
                            self.open_help(false);
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
                    } else if matches!(pane, 1 | 2 | 3 | 4 | 6 | 7 | 9 | 10) {
                        let clicked =
                            self.view_tops[pane] + mouse.row.saturating_sub(rect.y) as usize;
                        if clicked >= self.view_len(pane) {
                            return;
                        }
                        self.selection = clicked;
                        if matches!(pane, 2 | 7 | 10) {
                            self.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), engine);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    fn open_setup(&mut self) {
        if let Ok(document) = Document::open(&self.document.path) {
            self.document = document;
        }
        let mut setup = Setup::new(self.document.clone());
        if !matches!(self.snapshot.state.as_str(), "DISCONNECTED" | "FAULT") {
            setup.message =
                "Current session stays active until F5. Starting ends it and switches projects."
                    .into();
        }
        self.setup = Some(setup);
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
            DisableFocusChange,
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
        EnableFocusChange,
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
    let readiness = project.clone().prepare_workspace();
    let ready = readiness.is_ok();
    let connect_ready = readiness.unwrap_or(false);
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
        if connect_ready {
            app.submit(engine.as_ref(), "connect", json!({}));
        } else {
            app.notice = "ELF not built. Use Build, then Reconnect to start debugging.".into();
        }
    }
    const SWITCH_QUIT: u64 = u64::MAX - 1;
    let mut pending_launch: Option<Launch> = None;
    let mut switch_error = None;
    let mut dirty = true;
    let mut last_draw = Instant::now() - Duration::from_secs(1);
    let mut last_task_clock = Instant::now();
    loop {
        if app.demo && app.quitting {
            return Ok(());
        }
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
                project.prepare_workspace()?;
                if !launch.save {
                    project.path = None;
                }
                Ok(project)
            })();
            match prepared {
                Ok(project) => {
                    app.project = project.clone();
                    app.fx = effects::Effects::default();
                    app.fx.mode = project.ui.animations;
                    app.formats = formats::Formats::default();
                    app.peripherals = peripherals::Peripherals::load(&project.program.svd);
                    app.document = launch.document;
                    app.setup = None;
                    app.snapshot = Snapshot::default();
                    app.source.clear();
                    app.source_comments.clear();
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
                    app.pending_task = None;
                    app.editing = false;
                    app.watch_editing = false;
                    app.watch_input.clear();
                    app.pending_watch = None;
                    app.completion = completion::Completion::default();
                    app.confirm = None;
                    let connect_ready = project.clone().prepare_workspace().unwrap_or(false);
                    engine = Some(session::spawn(project));
                    if connect_ready {
                        app.submit(engine.as_ref(), "connect", json!({}));
                    } else {
                        app.notice =
                            "ELF not built. Use Build, then Reconnect to start debugging.".into();
                    }
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
        if app.ensure_completion(engine.as_ref()) {
            dirty = true;
        }
        if app.ensure_visible_data(engine.as_ref()) {
            dirty = true;
        }
        let active = app.setup.is_none()
            && (!app.pending_commands.is_empty()
                || app.pending_view.is_some()
                || app.snapshot.state == "RUNNING");
        if app.fx.tick(app.project.ui.animations, active) {
            dirty = true;
        }
        // Elapsed task time is real information, also updated with decoration disabled.
        if app.fx.focused
            && app.pending_task.is_some()
            && last_task_clock.elapsed() >= Duration::from_secs(1)
        {
            last_task_clock = Instant::now();
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
                Input::FocusLost => {
                    app.fx.focus(false);
                    dirty = true;
                }
                Input::FocusGained => {
                    app.fx.focus(true);
                    dirty = true;
                }
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
                    } else if app.watch_editing {
                        app.watch_input
                            .extend(text.chars().filter(|c| !c.is_control()));
                        dirty = true;
                    } else if app.editing {
                        app.input.extend(text.chars().filter(|c| !c.is_control()));
                        dirty = true;
                    }
                }
                Input::Mouse(mouse) => {
                    app.mouse(mouse, engine.as_ref());
                    dirty = true;
                }
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
    fn exit_button_cancels_worker_once_in_each_state_and_exits_demo() {
        for (w, h) in [(45, 12), (120, 36)] {
            for state in [
                "DISCONNECTED",
                "STARTING GDB",
                "CONNECTING",
                "READY",
                "STOPPED",
                "RUNNING",
                "FAULT",
            ] {
                let (engine, commands) = session::test_channel();
                let mut a = App::new(Project::default(), false);
                a.snapshot.state = state.into();
                render(&mut a, w, h);
                let hit = a
                    .action_hits
                    .iter()
                    .find(|(_, id)| *id == "quit")
                    .unwrap()
                    .0;
                mouse_at(
                    &mut a,
                    MouseEventKind::Down(event::MouseButton::Left),
                    hit.x,
                    hit.y,
                    Some(&engine),
                );
                assert_eq!(commands.try_recv().unwrap().method, "quit", "{state}");
                assert!(
                    engine
                        .cancellation
                        .load(std::sync::atomic::Ordering::Relaxed)
                );
                assert!(a.quitting);
                mouse_at(
                    &mut a,
                    MouseEventKind::Down(event::MouseButton::Left),
                    hit.x,
                    hit.y,
                    Some(&engine),
                );
                a.key(
                    KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL),
                    Some(&engine),
                );
                assert!(commands.try_recv().is_err());
                assert!(!a.action_enabled("quit"));
            }
        }
        let mut a = App::new(Project::default(), true);
        render(&mut a, 80, 24);
        let hit = a
            .action_hits
            .iter()
            .find(|(_, id)| *id == "quit")
            .unwrap()
            .0;
        mouse_at(
            &mut a,
            MouseEventKind::Down(event::MouseButton::Left),
            hit.x,
            hit.y,
            None,
        );
        assert!(a.quitting);
    }
    #[test]
    fn help_tabs_switch_without_dispatching_debug_commands() {
        let (engine, commands) = session::test_channel();
        let mut a = App::new(Project::default(), false);
        for (w, h) in [(45, 12), (120, 36)] {
            a.open_help(false);
            a.palette_index = COMMANDS.iter().position(|c| *c == "step").unwrap();
            let text = render(&mut a, w, h);
            assert!(text.contains("Commands") && text.contains("Shortcuts"));
            let hit = a
                .help_tab_hits
                .iter()
                .find(|(_, shortcuts)| *shortcuts)
                .unwrap()
                .0;
            mouse_at(
                &mut a,
                MouseEventKind::Down(event::MouseButton::Left),
                hit.x,
                hit.y,
                Some(&engine),
            );
            assert!(a.help && !a.palette);
            a.key(
                KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
                Some(&engine),
            );
            assert!(a.palette && !a.help);
            render(&mut a, w, h);
            let selected = a
                .palette_hits
                .iter()
                .find(|(_, i)| *i == a.palette_index)
                .unwrap()
                .0;
            assert!(selected.bottom() < h);
            a.key(
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                Some(&engine),
            );
            assert_eq!(commands.try_recv().unwrap().method, "step");
            assert!(commands.try_recv().is_err());
            a.command(Some(&engine), ":help");
            assert!(a.help && !a.palette);
            render(&mut a, w, h);
            let hit = a
                .help_tab_hits
                .iter()
                .find(|(_, shortcuts)| !shortcuts)
                .unwrap()
                .0;
            mouse_at(
                &mut a,
                MouseEventKind::Down(event::MouseButton::Left),
                hit.x,
                hit.y,
                Some(&engine),
            );
            assert!(a.palette && !a.help);
            a.key(
                KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                Some(&engine),
            );
            assert!(!a.palette && !a.help);
            assert!(commands.try_recv().is_err());
        }
    }
    #[test]
    fn project_actions_are_separate_and_gate_tasks_and_download_confirmation() {
        let (engine, commands) = session::test_channel();
        let mut a = App::new(Project::default(), false);
        assert!(!a.action_enabled("build"));
        assert!(!a.action_enabled("download"));
        a.project.tasks.build = "build.bat".into();
        a.project.tasks.download = "flash.bat".into();
        for (w, h) in [(45, 12), (80, 24), (160, 42)] {
            render(&mut a, w, h);
            let run_row = a
                .action_hits
                .iter()
                .find(|(_, id)| *id == "continue")
                .unwrap()
                .0
                .y;
            for cmd in ["build", "download"] {
                let hit = a.action_hits.iter().find(|(_, id)| *id == cmd).unwrap().0;
                assert!(hit.y < run_row && hit.right() <= w);
                assert!(a.action_enabled(cmd));
            }
        }
        let build = a
            .action_hits
            .iter()
            .find(|(_, id)| *id == "build")
            .unwrap()
            .0;
        mouse_at(
            &mut a,
            MouseEventKind::Down(event::MouseButton::Left),
            build.x,
            build.y,
            Some(&engine),
        );
        let request = commands.try_recv().unwrap();
        assert_eq!(request.method, "build");
        assert!(!a.action_enabled("download"));
        assert!(a.action_enabled("quit"));
        a.submit(Some(&engine), "build", json!({}));
        assert!(commands.try_recv().is_err());
        a.update(Event::Response {
            id: request.id,
            ok: false,
            result: Value::Null,
            error: Some("Build failed".into()),
        });
        assert!(a.action_enabled("build"));
        let download = a
            .action_hits
            .iter()
            .find(|(_, id)| *id == "download")
            .unwrap()
            .0;
        mouse_at(
            &mut a,
            MouseEventKind::Down(event::MouseButton::Left),
            download.x,
            download.y,
            Some(&engine),
        );
        assert!(commands.try_recv().is_err());
        assert!(a.confirm.is_some());
        a.key(
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
            Some(&engine),
        );
        assert!(a.confirm.is_none());
        assert!(commands.try_recv().is_err());
        a.submit(Some(&engine), "download", json!({}));
        a.key(
            KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
            Some(&engine),
        );
        assert_eq!(commands.try_recv().unwrap().method, "download");
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
