//! Breakpoint manager: row checkboxes, explicit removal, and a keyboard/mouse editor.
use super::*;
mod cores;
use crate::config::{BreakpointKind as Kind, BreakpointOptions as Options};

pub(super) const ACTIONS: &[(&str, &str)] = &[
    ("+ Code", "break-new"),
    ("+ Data", "break-data"),
    ("Edit", "break-edit"),
    ("Cores", "break-cores"),
    ("Toggle", "break-toggle"),
    ("Delete", "break-remove"),
    ("Enable all", "break-enable-all"),
    ("Disable all", "break-disable-all"),
];
const KINDS: [Kind; 5] = [
    Kind::Code,
    Kind::Hardware,
    Kind::Write,
    Kind::Read,
    Kind::Access,
];
struct Editor {
    number: Option<String>,
    options: Options,
    ignore: String,
    field: usize,
    error: String,
}
#[derive(Default)]
pub(super) struct Breaks {
    editor: Option<Editor>,
    core_editor: Option<cores::CoreEditor>,
    pending: Option<u64>,
    hits: Vec<(Rect, usize)>,
    kinds: Vec<(Rect, Kind)>,
}
impl Breaks {
    pub fn modal(&self) -> bool {
        self.editor.is_some() || self.core_editor.is_some()
    }
}

impl App {
    pub(super) fn selected_break(&self) -> Option<&crate::session::Breakpoint> {
        self.snapshot.breakpoints.get(self.selected(6))
    }
    pub(super) fn break_action_enabled(&self, action: &str) -> bool {
        if self.breaks.pending.is_some()
            || self.pending_task.is_some()
            || !matches!(self.snapshot.state.as_str(), "READY" | "STOPPED")
        {
            return false;
        }
        match action {
            "break-data" => self.snapshot.state == "STOPPED",
            "break-new" => true,
            "break-cores" => {
                self.snapshot.cores.len() > 1
                    && self.selected_break().is_some_and(|b| {
                        !b.temporary && !b.options().kind.is_data() && !b.id.contains('.')
                    })
            }
            "break-enable-all" | "break-disable-all" => !self.snapshot.breakpoints.is_empty(),
            _ => self.selected_break().is_some(),
        }
    }
    pub(super) fn break_send(
        &mut self,
        engine: Option<&EngineHandle>,
        method: &str,
        params: Value,
    ) {
        if !self.break_action_enabled("break-new") {
            self.notice =
                "Pause the target and wait for the current breakpoint edit to finish.".into();
            return;
        }
        let id = self.next_id;
        self.submit(engine, method, params);
        if self.next_id != id && self.pending_commands.contains(&id) {
            self.breaks.pending = Some(id);
        }
    }
    pub(super) fn open_break_editor(&mut self, data: bool, edit: bool) {
        if !self.break_action_enabled(if data {
            "break-data"
        } else if edit {
            "break-edit"
        } else {
            "break-new"
        }) {
            self.notice =
                "Select a breakpoint to edit; creating data breakpoints requires a stopped target."
                    .into();
            return;
        }
        let (number, options) = if edit {
            let Some(b) = self.selected_break() else {
                return;
            };
            (Some(b.id.clone()), b.options())
        } else {
            (
                None,
                Options {
                    kind: if data { Kind::Write } else { Kind::Code },
                    location: if data || self.source_file.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "{}:{}",
                            self.source_file.replace('\\', "/"),
                            self.source_line + 1
                        )
                    },
                    ..Default::default()
                },
            )
        };
        self.editing = false;
        self.watch_editing = false;
        self.console_view.focused = false;
        self.completion.invalidate();
        self.select_pane(6);
        self.breaks.editor = Some(Editor {
            number,
            ignore: options.ignore_count.to_string(),
            options,
            field: if edit { 2 } else { 1 },
            error: String::new(),
        });
    }
    pub(super) fn toggle_selected_break(&mut self, engine: Option<&EngineHandle>) {
        if let Some(b) = self.selected_break() {
            self.break_send(
                engine,
                "enable_break",
                json!({"number":b.id,"enabled":!b.enabled || !b.restore_error.is_empty()}),
            );
        }
    }
    pub(super) fn remove_selected_break(&mut self, engine: Option<&EngineHandle>) {
        if let Some(b) = self.selected_break() {
            self.break_send(engine, "delete_break", json!({"number":b.id}));
        }
    }
    pub(super) fn reconcile_break_selection(&mut self, snapshot: &Snapshot, core_changed: bool) {
        let selected = self.selected_break().map(|b| b.id.as_str());
        let index = if core_changed {
            self.breaks.editor = None;
            self.breaks.core_editor = None;
            0
        } else {
            selected
                .and_then(|id| snapshot.breakpoints.iter().position(|b| b.id == id))
                .unwrap_or(self.selected(6))
                .min(snapshot.breakpoints.len().saturating_sub(1))
        };
        self.selections[6] = index;
        if self.pane == 6 {
            self.selection = index;
        }
    }
    pub(super) fn break_response(&mut self, id: u64, ok: bool, error: Option<&str>) {
        if self.breaks.pending != Some(id) {
            return;
        }
        self.breaks.pending = None;
        if ok {
            self.breaks.editor = None;
            self.breaks.core_editor = None;
        } else if let Some(editor) = &mut self.breaks.core_editor {
            editor.error = error.unwrap_or("Breakpoint core edit failed").into();
        } else if let Some(editor) = &mut self.breaks.editor {
            editor.error = error.unwrap_or("Breakpoint edit failed").into();
        }
    }
    pub(super) fn apply_break_editor(&mut self, engine: Option<&EngineHandle>) {
        let Some(editor) = &mut self.breaks.editor else {
            return;
        };
        if editor.options.location.trim().is_empty() {
            editor.error = "Enter a location or a data expression.".into();
            editor.field = 1;
            return;
        }
        let Ok(count) = editor.ignore.parse::<i32>() else {
            editor.error = "Ignore count must be 0..2147483647.".into();
            editor.field = 4;
            return;
        };
        if count < 0 {
            editor.error = "Ignore count cannot be negative.".into();
            return;
        }
        let o = &editor.options;
        let method = if editor.number.is_some() {
            "update_break"
        } else if o.kind.is_data() {
            "data_break"
        } else {
            "break"
        };
        let p = json!({"number":editor.number,"location":o.location,"expression":o.location,"hardware":o.kind == Kind::Hardware,
            "access":match o.kind { Kind::Read => "read", Kind::Access => "access", _ => "write" },
            "enabled":o.enabled,"condition":o.condition,"ignore_count":count,"temporary":o.temporary});
        self.break_send(engine, method, p);
    }
    pub(super) fn break_panel_key(&mut self, key: KeyEvent, engine: Option<&EngineHandle>) -> bool {
        if key.kind != KeyEventKind::Press || !key.modifiers.is_empty() {
            return false;
        }
        match key.code {
            KeyCode::Char(' ') | KeyCode::Enter => self.toggle_selected_break(engine),
            KeyCode::Insert | KeyCode::Char('n') => self.open_break_editor(false, false),
            KeyCode::Char('d') => self.open_break_editor(true, false),
            KeyCode::Char('e') => self.open_break_editor(false, true),
            KeyCode::Char('c') => self.open_break_cores(),
            KeyCode::Delete => self.remove_selected_break(engine),
            _ => return false,
        }
        true
    }
    pub(super) fn break_panel_mouse(
        &mut self,
        mouse: MouseEvent,
        engine: Option<&EngineHandle>,
    ) -> bool {
        let rect = self.view_rects[6];
        if !rect.contains((mouse.column, mouse.row).into()) {
            return false;
        }
        let index = self.view_tops[6] + mouse.row.saturating_sub(rect.y) as usize;
        if index >= self.snapshot.breakpoints.len() {
            return false;
        }
        if mouse.kind == MouseEventKind::Down(event::MouseButton::Right)
            || (mouse.kind == MouseEventKind::Down(event::MouseButton::Left)
                && mouse.column < rect.x + 5)
        {
            self.editing = false;
            self.watch_editing = false;
            self.completion.invalidate();
            self.select_pane(6);
            self.selection = index;
            if mouse.kind == MouseEventKind::Down(event::MouseButton::Right) {
                self.open_break_editor(false, true);
            } else {
                self.toggle_selected_break(engine);
            }
            return true;
        }
        false
    }
    pub(super) fn break_paste(&mut self, text: &str) {
        if self.breaks.pending.is_some() {
            return;
        }
        if let Some(editor) = &mut self.breaks.editor {
            let field = match editor.field {
                1 if editor.number.is_none() => &mut editor.options.location,
                3 => &mut editor.options.condition,
                4 => &mut editor.ignore,
                _ => return,
            };
            field.extend(text.chars().filter(|c| !c.is_control()));
        }
    }
    pub(super) fn break_dialog_key(
        &mut self,
        key: KeyEvent,
        engine: Option<&EngineHandle>,
    ) -> bool {
        if self.breaks.core_editor.is_some() {
            return self.break_cores_key(key, engine);
        }
        let Some(editor) = &mut self.breaks.editor else {
            return false;
        };
        if self.breaks.pending.is_some() {
            return true;
        }
        if key.code == KeyCode::Esc {
            self.breaks.editor = None;
            return true;
        }
        if key.code == KeyCode::Enter && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.apply_break_editor(engine);
            return true;
        }
        let mut activate = false;
        match key.code {
            KeyCode::Tab | KeyCode::Down => editor.field = (editor.field + 1) % 8,
            KeyCode::BackTab | KeyCode::Up => editor.field = (editor.field + 7) % 8,
            KeyCode::Enter => activate = true,
            KeyCode::Char(' ') if matches!(editor.field, 0 | 2 | 5 | 6 | 7) => activate = true,
            KeyCode::Left | KeyCode::Right if editor.field == 0 => {
                if editor.number.is_none() {
                    let index = KINDS
                        .iter()
                        .position(|k| *k == editor.options.kind)
                        .unwrap_or(0);
                    editor.options.kind =
                        KINDS[(index + if key.code == KeyCode::Left { 4 } else { 1 }) % 5];
                    if editor.options.kind.is_data() {
                        editor.options.temporary = false;
                    }
                }
            }
            KeyCode::Backspace | KeyCode::Delete | KeyCode::Char(_) => {
                let field = match editor.field {
                    1 if editor.number.is_none() => Some(&mut editor.options.location),
                    3 => Some(&mut editor.options.condition),
                    4 => Some(&mut editor.ignore),
                    _ => None,
                };
                if let Some(field) = field {
                    match key.code {
                        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            field.clear()
                        }
                        KeyCode::Char(c)
                            if !key
                                .modifiers
                                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                        {
                            field.push(c)
                        }
                        KeyCode::Backspace | KeyCode::Delete => {
                            field.pop();
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        if activate {
            self.activate_break_field(engine);
        }
        true
    }
    pub(super) fn activate_break_field(&mut self, engine: Option<&EngineHandle>) {
        let Some(e) = &mut self.breaks.editor else {
            return;
        };
        match e.field {
            0 if e.number.is_none() => {
                let index = KINDS.iter().position(|k| *k == e.options.kind).unwrap_or(0);
                e.options.kind = KINDS[(index + 1) % 5];
                if e.options.kind.is_data() {
                    e.options.temporary = false;
                }
            }
            2 => e.options.enabled = !e.options.enabled,
            5 if e.number.is_none() && !e.options.kind.is_data() => {
                e.options.temporary = !e.options.temporary
            }
            6 => self.apply_break_editor(engine),
            7 => self.breaks.editor = None,
            _ => e.field = (e.field + 1) % 8,
        }
    }
    pub(super) fn break_dialog_mouse(
        &mut self,
        mouse: MouseEvent,
        engine: Option<&EngineHandle>,
    ) -> bool {
        if self.breaks.core_editor.is_some() {
            return self.break_cores_mouse(mouse, engine);
        }
        if self.breaks.editor.is_none() {
            return false;
        }
        if self.breaks.pending.is_some() {
            return true;
        }
        if mouse.kind == MouseEventKind::Down(event::MouseButton::Left) {
            let point = (mouse.column, mouse.row).into();
            if let Some((_, kind)) = self.breaks.kinds.iter().find(|(r, _)| r.contains(point)) {
                let e = self.breaks.editor.as_mut().unwrap();
                if e.number.is_none() {
                    e.options.kind = *kind;
                    if kind.is_data() {
                        e.options.temporary = false;
                    }
                }
                e.field = 0;
            } else if let Some((_, field)) =
                self.breaks.hits.iter().find(|(r, _)| r.contains(point))
            {
                let field = *field;
                self.breaks.editor.as_mut().unwrap().field = field;
                if matches!(field, 2 | 5 | 6 | 7) {
                    self.activate_break_field(engine);
                }
            }
        }
        true
    }
}

pub(super) fn rows(a: &App, start: usize, height: usize) -> Vec<Line<'static>> {
    a.snapshot
        .breakpoints
        .iter()
        .enumerate()
        .skip(start)
        .take(height)
        .map(|(i, b)| {
            let marker = if !b.restore_error.is_empty() {
                "[!]"
            } else if b.enabled {
                "[x]"
            } else {
                "[ ]"
            };
            let state = if b.enabled { theme::AMBER } else { theme::DIM };
            let info = format!(
                "{}{}{}{}",
                if b.pending { " pending" } else { "" },
                if b.temporary { " once" } else { "" },
                if !b.condition.is_empty() { " if" } else { "" },
                if b.cores.len() > 1 {
                    format!(" [{} cores]", b.cores.len())
                } else {
                    String::new()
                }
            );
            let display = if !b.options().kind.is_data() && !b.file.is_empty() && b.line > 0 {
                format!(
                    "{}:{}",
                    b.file
                        .replace('\\', "/")
                        .rsplit('/')
                        .next()
                        .unwrap_or(&b.file),
                    b.line
                )
            } else {
                b.location.clone()
            };
            Line::from(vec![
                Span::styled(format!(" {marker} "), Style::default().fg(state)),
                Span::styled(
                    format!("{} {:<10} ", b.id, b.options().kind.label()),
                    Style::default().fg(state),
                ),
                Span::styled(
                    display,
                    Style::default().fg(if b.enabled { theme::TEXT } else { theme::MUTED }),
                ),
                Span::styled(info, Style::default().fg(theme::AMBER)),
            ])
            .style(theme::selected(i == a.selected(6)))
        })
        .collect()
}
pub(super) fn detail(a: &App) -> String {
    a.selected_break()
        .map(|b| {
            if !b.restore_error.is_empty() {
                format!(
                    "Restore failed: {} · Edit or Enable all to retry",
                    b.restore_error
                )
            } else {
                format!(
                    "Hits {} · Ignore {} · {}{} · Cores: {}",
                    b.hit_count,
                    b.ignore_count,
                    if b.condition.is_empty() {
                        "Unconditional"
                    } else {
                        "If "
                    },
                    b.condition,
                    if b.cores.is_empty() {
                        "current".into()
                    } else {
                        b.cores
                            .iter()
                            .filter_map(|i| a.snapshot.cores.get(*i))
                            .map(|c| c.name.clone())
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                )
            }
        })
        .unwrap_or_else(|| "Click [x] / Space: enable or disable · Right-click: edit".into())
}
pub(super) fn popup(f: &mut UiFrame, a: &mut App) {
    if a.breaks.core_editor.is_some() {
        cores::draw(f, a);
        return;
    }
    a.breaks.hits.clear();
    a.breaks.kinds.clear();
    let Some(e) = &a.breaks.editor else {
        return;
    };
    let rect = center(f.area(), 90, 19);
    theme::overlay(f, rect);
    let block = theme::card(
        if e.number.is_some() {
            " Edit breakpoint "
        } else {
            " New breakpoint "
        },
        true,
    );
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let parts = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(2),
        Constraint::Length(1),
    ])
    .split(inner);
    f.render_widget(
        Paragraph::new(if e.number.is_some() {
            "Location / type retained. Edit enabled state, condition and ignore count."
        } else {
            "Code: file:line / function / *address. Data: variable / *(uint32_t *)address."
        })
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(theme::MUTED)),
        parts[0],
    );
    let values = [
        ("Type", e.options.kind.label().to_owned()),
        (
            if e.options.kind.is_data() {
                "Expression"
            } else {
                "Location"
            },
            e.options.location.clone(),
        ),
        (
            "Enabled",
            if e.options.enabled {
                "[x] Enabled"
            } else {
                "[ ] Disabled (keep record)"
            }
            .into(),
        ),
        (
            "Condition",
            if e.options.condition.is_empty() {
                "(always)".into()
            } else {
                e.options.condition.clone()
            },
        ),
        ("Ignore hits", e.ignore.clone()),
        (
            "Temporary",
            if e.options.kind.is_data() {
                "Not applicable to data"
            } else if e.options.temporary {
                "[x] Remove after first stop"
            } else {
                "[ ] Keep after hit"
            }
            .into(),
        ),
        (
            "",
            if a.breaks.pending.is_some() {
                "Applying…"
            } else {
                "Apply  ·  Ctrl+Enter"
            }
            .into(),
        ),
        ("", "Cancel  ·  Esc".into()),
    ];
    let visible = parts[1].height as usize;
    let start = e
        .field
        .saturating_sub(visible.saturating_sub(1))
        .min(8usize.saturating_sub(visible));
    for (i, (name, value)) in values.iter().enumerate().skip(start).take(visible) {
        let row = Rect::new(
            parts[1].x,
            parts[1].y + (i - start) as u16,
            parts[1].width,
            1,
        );
        f.render_widget(
            Paragraph::new(format!(
                " {name:<12} {}",
                if i == 0 && row.width >= 65 { "" } else { value }
            ))
            .style(theme::selected(e.field == i)),
            row,
        );
        a.breaks.hits.push((row, i));
        if i == 0 && row.width >= 65 {
            let mut x = row.x + 14;
            for kind in KINDS {
                let label = format!(" {} ", kind.label());
                let width = label.len() as u16;
                let hit = Rect::new(x, row.y, width, 1);
                f.render_widget(
                    Paragraph::new(label).style(theme::chip(kind == e.options.kind, false)),
                    hit,
                );
                a.breaks.kinds.push((hit, kind));
                x += width + 1;
            }
        }
    }
    let hint = if !e.error.is_empty() {
        e.error.as_str()
    } else {
        "Read/write requires target hardware support. Write stops on a value change. Ignore N skips the next N hits."
    };
    f.render_widget(
        Paragraph::new(hint)
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(if e.error.is_empty() {
                theme::MUTED
            } else {
                theme::RED
            })),
        parts[2],
    );
    f.render_widget(
        Paragraph::new("Tab / ↑ ↓ fields · Enter toggle · Ctrl+U clear · Ctrl+Enter apply")
            .style(Style::default().fg(theme::DIM)),
        parts[3],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> App {
        let mut a = App::new(Project::default(), false);
        a.snapshot.state = "STOPPED".into();
        a.snapshot.breakpoints = vec![crate::session::Breakpoint {
            id: "7".into(),
            location: "main".into(),
            enabled: true,
            ..Default::default()
        }];
        a.select_pane(6);
        a
    }
    fn render(a: &mut App, w: u16, h: u16) -> Terminal<TestBackend> {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| draw(f, a)).unwrap();
        t
    }
    fn key(a: &mut App, code: KeyCode, engine: &EngineHandle) {
        a.key(KeyEvent::new(code, KeyModifiers::NONE), Some(engine));
    }
    #[test]
    fn checkbox_disables_without_deleting_and_guards_repeat() {
        let (engine, rx) = session::test_channel();
        let mut a = fixture();
        render(&mut a, 140, 40);
        let r = a.view_rects[6];
        a.mouse(
            MouseEvent {
                kind: MouseEventKind::Down(event::MouseButton::Left),
                column: r.x + 2,
                row: r.y,
                modifiers: KeyModifiers::NONE,
            },
            Some(&engine),
        );
        let req = rx.try_recv().unwrap();
        assert_eq!(req.method, "enable_break");
        assert_eq!(req.params["enabled"], false);
        assert_eq!(req.params["number"], "7");
        key(&mut a, KeyCode::Char(' '), &engine);
        assert!(rx.try_recv().is_err());
        a.break_response(req.id, true, None);
        a.snapshot.breakpoints[0].enabled = false;
        key(&mut a, KeyCode::Enter, &engine);
        let req = rx.try_recv().unwrap();
        assert_eq!(req.params["enabled"], true);
        a.break_response(req.id, true, None);
        key(&mut a, KeyCode::Delete, &engine);
        assert_eq!(rx.try_recv().unwrap().method, "delete_break");
    }
    #[test]
    fn data_editor_paste_delete_and_access_modes_do_not_send_console_commands() {
        let (engine, rx) = session::test_channel();
        let mut a = fixture();
        a.open_break_editor(true, false);
        a.break_paste("*(unsigned int *)0x20000000");
        key(&mut a, KeyCode::Delete, &engine);
        a.break_paste("0");
        assert!(rx.try_recv().is_err());
        a.breaks.editor.as_mut().unwrap().options.kind = Kind::Access;
        a.apply_break_editor(Some(&engine));
        let req = rx.try_recv().unwrap();
        assert_eq!(req.method, "data_break");
        assert_eq!(req.params["access"], "access");
        assert_eq!(req.params["expression"], "*(unsigned int *)0x20000000");
        a.break_response(req.id, false, Some("No watchpoint resources"));
        assert!(a.breaks.modal());
        assert!(
            a.breaks
                .editor
                .as_ref()
                .unwrap()
                .error
                .contains("resources")
        );
        a.breaks.editor.as_mut().unwrap().options.kind = Kind::Read;
        a.apply_break_editor(Some(&engine));
        let req = rx.try_recv().unwrap();
        assert_eq!(req.params["access"], "read");
        a.break_response(req.id, true, None);
        assert!(!a.breaks.modal());
    }
    #[test]
    fn selection_follows_id_and_core_switch_dismisses_editor() {
        let mut a = fixture();
        a.open_break_editor(false, true);
        let mut s = a.snapshot.clone();
        s.breakpoints.insert(
            0,
            crate::session::Breakpoint {
                id: "3".into(),
                ..Default::default()
            },
        );
        a.reconcile_break_selection(&s, false);
        assert_eq!(a.selection, 1);
        a.reconcile_break_selection(&s, true);
        assert_eq!(a.selection, 0);
        assert!(!a.breaks.modal());
    }
    #[test]
    fn breakpoint_panel_and_editor_fit_small_terminals() {
        let mut a = fixture();
        for (w, h) in [(45, 12), (80, 24), (140, 40)] {
            render(&mut a, w, h);
            assert!(a.view_rects[6].height > 0);
            if h >= 24 {
                assert!(
                    a.action_hits
                        .iter()
                        .any(|(_, action)| *action == "break-data")
                );
            }
            a.open_break_editor(true, false);
            for field in 0..8 {
                a.breaks.editor.as_mut().unwrap().field = field;
                render(&mut a, w, h);
                assert!(a.breaks.hits.iter().any(|(_, i)| *i == field));
                assert!(
                    a.breaks
                        .hits
                        .iter()
                        .all(|(r, _)| r.right() <= w && r.bottom() <= h)
                );
            }
            a.breaks.editor = None;
        }
    }
    #[test]
    fn export_breakpoint_previews() {
        let Ok(root) = std::env::var("DEBUGTUI_RENDER_DIR") else {
            return;
        };
        let root = Path::new(&root);
        fs::create_dir_all(root).unwrap();
        let mut a = fixture();
        a.snapshot.breakpoints.push(crate::session::Breakpoint {
            id: "8".into(),
            location: "xTickCount".into(),
            kind: "read watchpoint".into(),
            enabled: false,
            condition: "xTickCount > 100".into(),
            hit_count: 4,
            ..Default::default()
        });
        a.snapshot.breakpoints.push(crate::session::Breakpoint {
            id: "9".into(),
            location: "*(uint32_t *)0x20000000".into(),
            kind: "acc watchpoint".into(),
            enabled: true,
            ..Default::default()
        });
        a.selection = 1;
        super::super::visual_tests::capture(root, "breakpoints", &mut a, 160, 42);
        super::super::visual_tests::capture(root, "breakpoints-narrow", &mut a, 80, 24);
        a.open_break_editor(true, false);
        a.break_paste("*(uint32_t *)0x20000000");
        a.breaks.editor.as_mut().unwrap().options.kind = Kind::Access;
        super::super::visual_tests::capture(root, "breakpoint-editor", &mut a, 120, 36);
    }
}
