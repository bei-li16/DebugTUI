//! Shared, asynchronous completion for the console and watch entry.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Key {
    watch: bool,
    text: String,
    context: String,
}

pub(super) struct Completion {
    key: Option<Key>,
    changed: Instant,
    requested: bool,
    pending: Option<(u64, Key)>,
    pub items: Vec<String>,
    pub selected: usize,
    pub hits: Vec<(Rect, usize)>,
    pub area: Rect,
    pub hint: String,
    epoch: u64,
}

impl Default for Completion {
    fn default() -> Self {
        Self {
            key: None,
            changed: Instant::now(),
            requested: false,
            pending: None,
            items: vec![],
            selected: 0,
            hits: vec![],
            area: Rect::default(),
            hint: String::new(),
            epoch: 0,
        }
    }
}

impl Completion {
    pub fn invalidate(&mut self) {
        self.key = None;
        self.items.clear();
        self.hits.clear();
        self.area = Rect::default();
        self.hint.clear();
        self.epoch += 1;
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
}

// These remain useful before a debugger is connected. Connected GDB supplies
// its complete command vocabulary, arguments, and expression members.
const GDB_COMMANDS: &[&str] = &[
    "backtrace",
    "break",
    "clear",
    "continue",
    "delete",
    "disable",
    "disassemble",
    "display",
    "enable",
    "finish",
    "frame",
    "help",
    "info",
    "interrupt",
    "list",
    "next",
    "nexti",
    "print",
    "printf",
    "quit",
    "run",
    "set",
    "show",
    "step",
    "stepi",
    "watch",
    "whatis",
    "x",
];

impl App {
    pub(super) fn input_active(&self) -> bool {
        self.editing || self.watch_editing
    }

    fn completion_key(&self) -> Option<Key> {
        if !self.input_active()
            || self.setup.is_some()
            || self.help
            || self.palette
            || self.confirm.is_some()
            || self.sources.list_open
            || self.quitting
            || self.monitor.modal()
            || self.breaks.modal()
        {
            return None;
        }
        let text = if self.watch_editing {
            &self.watch_input
        } else {
            &self.input
        };
        if text.is_empty() || text.len() > 512 {
            return None;
        }
        Some(Key {
            watch: self.watch_editing,
            text: text.clone(),
            context: format!(
                "{}:{}:{}",
                self.completion.epoch,
                self.snapshot.state,
                self.view_stamp()
            ),
        })
    }

    pub(super) fn sync_completion(&mut self) -> bool {
        let key = self.completion_key();
        if key == self.completion.key {
            return false;
        }
        self.completion.key = key.clone();
        self.completion.changed = Instant::now();
        self.completion.requested = false;
        self.completion.items.clear();
        self.completion.hits.clear();
        self.completion.hint.clear();
        self.completion.area = Rect::default();
        self.completion.selected = 0;
        if let Some(key) = key {
            if !key.watch && !key.text.contains(char::is_whitespace) {
                if let Some(prefix) = key.text.strip_prefix(':') {
                    self.completion.items = COMMANDS
                        .iter()
                        .filter_map(|s| {
                            let name = s.split_whitespace().next()?;
                            name.starts_with(prefix).then(|| format!(":{name}"))
                        })
                        .collect();
                } else {
                    self.completion.items = GDB_COMMANDS
                        .iter()
                        .filter(|s| s.starts_with(&key.text))
                        .map(|s| (*s).to_owned())
                        .collect();
                }
            }
            if !matches!(self.snapshot.state.as_str(), "STOPPED" | "READY") {
                self.completion.hint = "Symbol completion available when stopped".into();
            }
            if !self.completion.items.is_empty() {
                self.fx.trigger("completion", 120);
            }
        }
        true
    }

    pub(super) fn ensure_completion(&mut self, engine: Option<&EngineHandle>) -> bool {
        let changed = self.sync_completion();
        if self.demo
            || self.completion.pending.is_some()
            || self.completion.requested
            || self.completion.changed.elapsed() < Duration::from_millis(150)
            || !self.pending_commands.is_empty()
            || self.pending_view.is_some()
            || self.pending_task.is_some()
            || !matches!(self.snapshot.state.as_str(), "READY" | "STOPPED")
        {
            return changed;
        }
        let (Some(key), Some(engine)) = (self.completion.key.clone(), engine) else {
            return changed;
        };
        let (text, expression) = if key.watch {
            (key.text.as_str(), true)
        } else if let Some(text) = key.text.strip_prefix(":watch ") {
            (text, true)
        } else if let Some(text) = key.text.strip_prefix(":unwatch ") {
            (text, true)
        } else if key.text.starts_with(':') {
            return changed;
        } else {
            (key.text.as_str(), false)
        };
        self.completion.requested = true;
        if text.trim().is_empty() {
            return changed;
        }
        let id = self.next_id;
        self.next_id += 1;
        let request = Request::new(id, "complete", json!({"text":text,"expression":expression}));
        if engine.send(request).is_ok() {
            self.completion.pending = Some((id, key));
        }
        changed
    }

    pub(super) fn completion_response(
        &mut self,
        id: u64,
        result: &Value,
        error: Option<&str>,
    ) -> bool {
        if self
            .completion
            .pending
            .as_ref()
            .is_none_or(|(pending, _)| *pending != id)
        {
            return false;
        }
        let (_, key) = self.completion.pending.take().unwrap();
        // Focus, input, frame, session and foreground commands can all change
        // while GDB answers. Never replace newer input with an old reply.
        if self.completion_key().as_ref() != Some(&key) {
            return true;
        }
        self.completion.hint = if error.is_some() {
            "GDB completion unavailable; input still works".into()
        } else {
            String::new()
        };
        if error.is_none() {
            let prefix = if !key.watch && key.text.starts_with(":watch ") {
                ":watch "
            } else if !key.watch && key.text.starts_with(":unwatch ") {
                ":unwatch "
            } else {
                ""
            };
            self.completion.items = result
                .get("matches")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .take(64)
                .map(|s| format!("{prefix}{s}"))
                .collect();
            self.completion.selected = 0;
            self.fx.trigger("completion", 120);
            if self.completion.items.is_empty() {
                self.completion.hint = "No matches · Enter submits your input".into();
            }
        }
        true
    }

    pub(super) fn focus_input(&mut self, watch: bool) {
        self.console_view.focused = false;
        self.fx.trigger("input-focus", 180);
        self.editing = !watch;
        self.watch_editing = watch;
        if watch {
            self.select_pane(1);
        } else {
            self.history_index = self.history.len();
        }
        self.sync_completion();
    }

    pub(super) fn accept_completion(&mut self) {
        if let Some(text) = self.completion.items.get(self.completion.selected).cloned() {
            if self.watch_editing {
                self.watch_input = text;
            } else {
                self.input = text;
            }
            self.completion.invalidate();
            self.sync_completion();
            // An accepted candidate stays editable. Enter is always an explicit
            // submit, never an implicit acceptance that might run a wrong command.
            self.completion.items.clear();
            self.completion.requested = true;
            self.completion.hint.clear();
        }
    }

    pub(super) fn input_key(&mut self, key: KeyEvent, engine: Option<&EngineHandle>) {
        self.sync_completion();
        let has_items = !self.completion.items.is_empty();
        match key.code {
            KeyCode::Tab => self.accept_completion(),
            KeyCode::BackTab | KeyCode::Up if has_items => {
                self.completion.selected = (self.completion.selected + self.completion.items.len()
                    - 1)
                    % self.completion.items.len();
            }
            KeyCode::Down if has_items => {
                self.completion.selected =
                    (self.completion.selected + 1) % self.completion.items.len();
            }
            KeyCode::Esc => {
                if self.editing {
                    self.input.clear();
                }
                self.editing = false;
                self.watch_editing = false;
                self.completion.invalidate();
            }
            KeyCode::Enter => {
                self.fx.trigger("submit", 220);
                if self.watch_editing {
                    self.add_watch_input(engine);
                } else {
                    let input = std::mem::take(&mut self.input);
                    if !input.trim().is_empty() {
                        self.history.push(input.clone());
                        self.history_index = self.history.len();
                        self.command(engine, &input);
                    }
                }
                self.completion.invalidate();
            }
            KeyCode::Backspace => {
                if self.watch_editing {
                    self.watch_input.pop();
                } else {
                    self.input.pop();
                }
            }
            KeyCode::Up if !self.watch_editing && self.history_index > 0 => {
                self.history_index -= 1;
                self.input = self.history[self.history_index].clone();
            }
            KeyCode::Down if !self.watch_editing => {
                self.history_index = (self.history_index + 1).min(self.history.len());
                self.input = self
                    .history
                    .get(self.history_index)
                    .cloned()
                    .unwrap_or_default();
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.watch_editing {
                    self.watch_input.clear();
                } else {
                    self.input.clear();
                }
                self.editing = false;
                self.watch_editing = false;
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                if self.watch_editing {
                    self.watch_input.push(c);
                } else {
                    self.input.push(c);
                }
            }
            _ => {}
        }
        self.sync_completion();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut a = App::new(Project::default(), false);
        a.snapshot.state = "STOPPED".into();
        a
    }
    fn type_text(a: &mut App, text: &str, engine: &EngineHandle) {
        for c in text.chars() {
            a.key(
                KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE),
                Some(engine),
            );
        }
    }
    fn key(a: &mut App, code: KeyCode, engine: &EngineHandle) {
        a.key(KeyEvent::new(code, KeyModifiers::NONE), Some(engine));
    }
    fn ready(a: &mut App, engine: &EngineHandle) {
        a.sync_completion();
        a.completion.changed -= Duration::from_secs(1);
        a.ensure_completion(Some(engine));
    }
    fn answer(a: &mut App, id: u64, values: &[&str]) {
        a.update(Event::Response {
            id,
            ok: true,
            result: json!({"matches":values}),
            error: None,
        });
    }

    #[test]
    fn console_completion_fills_without_executing_and_keeps_raw_gdb_namespace() {
        let (engine, requests) = session::test_channel();
        let mut a = app();
        a.focus_input(false);
        type_text(&mut a, "pri", &engine);
        a.ensure_completion(Some(&engine));
        assert!(requests.try_recv().is_err()); // Debounced while typing.
        ready(&mut a, &engine);
        let request = requests.try_recv().unwrap();
        assert_eq!(request.method, "complete");
        assert_eq!(request.params, json!({"text":"pri","expression":false}));
        answer(&mut a, request.id, &["print", "printf"]);
        key(&mut a, KeyCode::Down, &engine);
        key(&mut a, KeyCode::Tab, &engine);
        assert_eq!(a.input, "printf");
        assert!(requests.try_recv().is_err());
        assert!(a.console.is_empty()); // No background completion JSON in console.
        key(&mut a, KeyCode::Enter, &engine);
        assert_eq!(requests.try_recv().unwrap().params["command"], "printf");
        a.input = ":wat".into();
        key(&mut a, KeyCode::Tab, &engine);
        assert_eq!(a.input, ":watch");
        assert!(requests.try_recv().is_err());
    }

    #[test]
    fn late_completion_never_overwrites_input_focus_or_new_session() {
        let (engine, requests) = session::test_channel();
        let mut a = app();
        a.focus_input(true);
        a.watch_input = "cou".into();
        ready(&mut a, &engine);
        let old = requests.try_recv().unwrap();
        type_text(&mut a, "nter", &engine);
        ready(&mut a, &engine);
        assert!(requests.try_recv().is_err()); // At most one outstanding query.
        answer(&mut a, old.id, &["country"]);
        assert!(a.completion.items.is_empty());
        assert_eq!(a.watch_input, "counter");
        ready(&mut a, &engine);
        let current = requests.try_recv().unwrap();
        a.focus_input(false);
        answer(&mut a, current.id, &["counter"]);
        assert!(a.completion.items.is_empty());
        a.input = "p cou".into();
        ready(&mut a, &engine);
        let old = requests.try_recv().unwrap();
        a.snapshot.generation += 1;
        answer(&mut a, old.id, &["p counter"]);
        assert!(a.completion.items.is_empty());
    }

    #[test]
    fn watch_input_mouse_completion_submission_and_failed_add_keep_input_isolated() {
        let (engine, requests) = session::test_channel();
        let mut a = app();
        a.input = "p other".into();
        let mut t = Terminal::new(TestBackend::new(160, 42)).unwrap();
        t.draw(|f| draw(f, &mut a)).unwrap();
        let click = |a: &mut App, rect: Rect| {
            a.mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(event::MouseButton::Left),
                    column: rect.x,
                    row: rect.y,
                    modifiers: KeyModifiers::NONE,
                },
                Some(&engine),
            )
        };
        let rect = a.watch_input_rect;
        assert!(rect.height > 0);
        click(&mut a, rect);
        type_text(&mut a, "cou", &engine);
        ready(&mut a, &engine);
        let request = requests.try_recv().unwrap();
        assert_eq!(request.params, json!({"text":"cou","expression":true}));
        answer(&mut a, request.id, &["counter", "counter_total"]);
        t.draw(|f| draw(f, &mut a)).unwrap();
        let hit = a.completion.hits[1].0;
        click(&mut a, hit);
        assert_eq!(a.watch_input, "counter_total");
        assert!(requests.try_recv().is_err());
        key(&mut a, KeyCode::Enter, &engine);
        let watch = requests.try_recv().unwrap();
        assert_eq!(watch.method, "watch");
        assert_eq!(watch.params["expression"], "counter_total");
        key(&mut a, KeyCode::Enter, &engine);
        assert!(requests.try_recv().is_err()); // Double Enter must not enqueue duplicates.
        a.update(Event::Response {
            id: watch.id,
            ok: false,
            result: Value::Null,
            error: Some("Watch limit reached".into()),
        });
        assert_eq!(a.watch_input, "counter_total");
        key(&mut a, KeyCode::Enter, &engine);
        let watch = requests.try_recv().unwrap();
        answer(&mut a, watch.id, &[]);
        assert!(a.watch_input.is_empty());
        assert_eq!(a.input, "p other");
        assert!(a.watch_editing && !a.editing);
        key(&mut a, KeyCode::F(10), &engine);
        assert_eq!(requests.try_recv().unwrap().method, "next");
    }

    #[test]
    fn popup_clips_resizes_and_running_never_queries_symbols() {
        let (engine, requests) = session::test_channel();
        let mut a = app();
        a.focus_input(true);
        a.watch_input = "vari".into();
        a.sync_completion();
        a.completion.items = (0..64).map(|i| format!("variable_{i:03}")).collect();
        a.completion.selected = 63;
        for (width, height) in [(45, 12), (80, 24), (100, 30), (160, 42)] {
            let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
            t.draw(|f| draw(f, &mut a)).unwrap();
            assert!(a.watch_input_rect.height > 0);
            assert!(a.completion.hits.iter().any(|(_, i)| *i == 63));
            assert!(a.completion.area.right() <= width && a.completion.area.bottom() <= height);
        }
        a.snapshot.state = "RUNNING".into();
        ready(&mut a, &engine);
        assert!(requests.try_recv().is_err());
        assert!(a.completion.items.is_empty());
        a.snapshot.state = "STOPPED".into();
        ready(&mut a, &engine);
        assert_eq!(requests.try_recv().unwrap().method, "complete");
    }

    #[test]
    fn help_and_file_tabs_release_watch_focus_and_popup_is_opaque() {
        let (engine, _) = session::test_channel();
        let mut a = app();
        a.focus_input(true);
        a.watch_input = "counter".into();
        a.open_help(false);
        a.palette_index = COMMANDS
            .iter()
            .position(|s| s.starts_with("break "))
            .unwrap();
        a.activate_palette(Some(&engine));
        assert!(a.editing && !a.watch_editing);
        assert_eq!(a.input, ":break ");
        a.focus_input(true);
        a.open_source_list();
        assert!(!a.input_active());
        a.sources.list_open = false;
        a.focus_input(false);
        a.input = "pri".into();
        a.sync_completion();
        a.completion.items = vec!["print".into(), "printf".into()];
        a.console = (0..20)
            .map(|_| "XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX".into())
            .collect();
        let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
        t.draw(|f| draw(f, &mut a)).unwrap();
        let row = a.completion.hits[0].0;
        assert_eq!(t.backend().buffer()[(row.right() - 1, row.y)].symbol(), " ");
    }

    #[test]
    fn export_completion_previews_when_requested() {
        let Ok(root) = std::env::var("DEBUGTUI_RENDER_DIR") else {
            return;
        };
        let root = Path::new(&root);
        fs::create_dir_all(root).unwrap();
        let mut a = App::new(Project::default(), true);
        a.focus_input(true);
        a.watch_input = "ux".into();
        a.sync_completion();
        a.completion.items = vec![
            "uxCurrentNumberOfTasks".into(),
            "uxDeletedTasksWaitingCleanUp".into(),
            "uxTopReadyPriority".into(),
        ];
        super::super::visual_tests::capture(root, "watch-completion", &mut a, 160, 42);
        super::super::visual_tests::capture(root, "watch-compact", &mut a, 45, 12);
        a.focus_input(false);
        a.input = "info reg".into();
        a.sync_completion();
        a.completion.items = vec!["info registers".into(), "info reggroups".into()];
        super::super::visual_tests::capture(root, "gdb-completion", &mut a, 160, 42);
    }
}
