//! One explicit preview/apply/cancel editor, using the same API as headless clients.
use super::*;
use crate::{
    registers::Context,
    writes::{Input, InputKind, Selection},
};

pub(super) struct Candidate {
    pub target: Value,
    pub selection: Selection,
    pub title: String,
    pub bits: u16,
    pub value: String,
    pub reason: Option<String>,
}
struct Popup {
    candidate: Candidate,
    context: Context,
    input: Input,
    field: usize,
    preview: Option<Value>,
    detail: String,
    expired: bool,
    address: String,
    count: String,
}
impl Popup {
    fn memory(&self) -> bool {
        self.candidate.target["kind"] == "memory"
    }
    fn physical(&self) -> bool {
        self.candidate.target["kind"] == "register"
    }
    fn fields(&self) -> usize {
        if self.memory() { 7 } else { 5 }
    }
    fn text(&mut self) -> Option<&mut String> {
        match self.field {
            0 => Some(&mut self.input.text),
            5 if self.memory() => Some(&mut self.address),
            6 if self.memory() => Some(&mut self.count),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Preview,
    Apply,
}
struct Pending {
    id: u64,
    stage: Stage,
    context: Context,
    draft: Option<String>,
}
#[derive(Default)]
pub(super) struct Editor {
    popup: Option<Popup>,
    pending: Option<Pending>,
    cancels: VecDeque<String>,
    hits: Vec<(Rect, usize)>,
    pub(super) entry_hits: Vec<(Rect, usize)>,
}
impl Editor {
    pub(super) fn modal(&self) -> bool {
        self.popup.is_some()
    }
}
impl App {
    pub(super) fn open_edit_value(&mut self) {
        if !matches!(
            self.pane,
            pane::WATCH | pane::REGS | pane::MEMORY | pane::LOCALS | pane::PERIPHERALS
        ) {
            self.notice =
                "Select a Watch, Locals, System Regs, Memory or Peripherals object before editing."
                    .into();
            return;
        }
        if self.write_editor.pending.is_some() {
            self.notice = "Waiting for the outstanding edit response.".into();
            return;
        }
        let candidate = match match self.pane {
            pane::WATCH | pane::LOCALS => self.variable_edit_candidate(self.pane),
            pane::MEMORY => Ok(self.memory_edit_candidate()),
            pane::PERIPHERALS => self.peripheral_edit_candidate(),
            _ => self
                .register_view
                .edit_candidate(self.selected(pane::REGS), &self.register_context()),
        } {
            Ok(c) => c,
            Err(error) => {
                self.notice = error;
                return;
            }
        };
        let reason = candidate
            .reason
            .clone()
            .unwrap_or_else(|| "Enter a value, then Preview. Preview never sends a write.".into());
        let input = Input {
            kind: if candidate.target["kind"] == "variable"
                && candidate.target["type_hint"]
                    .as_str()
                    .is_some_and(|s| s == "float" || s == "double")
            {
                InputKind::Float
            } else if candidate.target["kind"] == "memory" {
                InputKind::Bytes
            } else {
                InputKind::Unsigned
            },
            text: candidate.value.clone(),
            little_endian: None,
        };
        let address = candidate.target["address"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        self.write_editor.popup = Some(Popup {
            candidate,
            context: self.register_context(),
            input,
            field: 0,
            preview: None,
            detail: reason,
            expired: false,
            address,
            count: "1".into(),
        });
    }
    pub(super) fn write_snapshot(&mut self, next: &Snapshot) {
        let context = Context {
            session: next.register_session,
            generation: next.generation,
            core: next
                .core
                .as_ref()
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "default".into()),
            frame: next.frame.level,
        };
        if let Some(popup) = &mut self.write_editor.popup
            && (popup.context != context || next.state != state::STOPPED)
        {
            if let Some(preview) = popup.preview.take()
                && let Some(token) = preview["draft"].as_str()
            {
                self.write_editor.cancels.push_back(token.into());
            }
            popup.expired = true;
            popup.detail = "Target context changed. Close the draft and open a new edit.".into();
        }
    }
    fn invalidate_write_preview(&mut self) {
        if let Some(popup) = &mut self.write_editor.popup
            && let Some(preview) = popup.preview.take()
            && let Some(token) = preview["draft"].as_str()
        {
            self.write_editor.cancels.push_back(token.into());
        }
    }
    pub(super) fn flush_write_cancels(&mut self, engine: Option<&EngineHandle>) -> bool {
        if engine.is_none() || self.write_editor.pending.is_some() {
            return false;
        }
        if let Some(token) = self.write_editor.cancels.pop_front() {
            self.submit(engine, "write_cancel", json!({"draft":token}));
            return true;
        }
        false
    }
    fn write_action(&mut self, field: usize, engine: Option<&EngineHandle>) {
        if field == 4 {
            self.invalidate_write_preview();
            self.write_editor.popup = None;
            self.notice = if self
                .write_editor
                .pending
                .as_ref()
                .is_some_and(|p| p.stage == Stage::Apply)
            {
                "Write already submitted; waiting for its result. Closing does not withdraw it."
            } else {
                "Edit cancelled; no Apply request sent."
            }
            .into();
            return;
        }
        if self.write_editor.pending.is_some() {
            return;
        }
        let Some(popup) = &mut self.write_editor.popup else {
            return;
        };
        if field == 1 {
            self.invalidate_write_preview();
            let popup = self.write_editor.popup.as_mut().unwrap();
            popup.input.kind = match popup.input.kind {
                InputKind::Unsigned => InputKind::Signed,
                InputKind::Signed
                    if matches!(popup.candidate.bits, 32 | 64)
                        || popup.candidate.target["kind"] == "variable" =>
                {
                    InputKind::Float
                }
                InputKind::Signed | InputKind::Float => InputKind::Bytes,
                InputKind::Bytes => InputKind::Enumeration,
                InputKind::Enumeration => InputKind::Unsigned,
            };
            // Byte input cannot inherit a guessed order. The chooser explicitly selects LE/BE.
            if popup.input.kind == InputKind::Bytes {
                popup.input.little_endian = Some(true);
            }
            return;
        }
        if !matches!(field, 2 | 3) {
            return;
        }
        if popup.candidate.reason.is_some()
            || popup.expired
            || self.demo
            || self.snapshot.state != state::STOPPED
            || (popup.physical() && popup.context.frame != 0)
        {
            popup.detail = popup
                .candidate
                .reason
                .clone()
                .unwrap_or_else(|| "A current stopped context is required; core registers also require physical frame 0.".into());
            return;
        }
        if engine.is_none() {
            popup.detail = "No debug connection.".into();
            return;
        }
        let id = self.next_id;
        let (stage, draft, method, params) = if field == 2 {
            if popup.memory() {
                let Some(count) = popup
                    .count
                    .parse::<u16>()
                    .ok()
                    .filter(|n| (1..=4096).contains(n))
                else {
                    popup.detail = "Memory byte count must be 1..4096.".into();
                    return;
                };
                popup.candidate.bits = count * 8;
                popup.candidate.target["address"] = json!(popup.address);
                popup.candidate.target["bits"] = json!(popup.candidate.bits);
            }
            let params = json!({"target":popup.candidate.target,"selection":popup.candidate.selection,"input":popup.input,"context":popup.context});
            self.invalidate_write_preview();
            (Stage::Preview, None, "write_preview", params)
        } else {
            let Some(token) = popup
                .preview
                .as_ref()
                .and_then(|v| v["draft"].as_str())
                .map(str::to_owned)
            else {
                popup.detail = "Preview the current value before Apply.".into();
                return;
            };
            popup.preview = None;
            (
                Stage::Apply,
                Some(token.clone()),
                "write_apply",
                json!({"draft":token}),
            )
        };
        let context = self.write_editor.popup.as_ref().unwrap().context.clone();
        self.write_editor.pending = Some(Pending {
            id,
            stage,
            context,
            draft,
        });
        self.write_editor.popup.as_mut().unwrap().detail = if stage == Stage::Preview {
            "Checking write capability; no write sent."
        } else {
            "Apply submitted; waiting for backend and verification."
        }
        .into();
        self.submit(engine, method, params);
        if !self.pending_commands.contains(&id) {
            self.write_editor.pending = None;
            if let Some(popup) = &mut self.write_editor.popup {
                popup.detail = format!("Not sent: {}", self.notice);
            }
        }
    }
    pub(super) fn write_response(&mut self, id: u64, result: &Value, error: Option<&str>) -> bool {
        if !self
            .write_editor
            .pending
            .as_ref()
            .is_some_and(|p| p.id == id)
        {
            return false;
        }
        let pending = self.write_editor.pending.take().unwrap();
        self.pending_commands.remove(&id);
        self.fx.response(id, error.is_none());
        if pending.stage == Stage::Preview {
            let current =
                pending.context == self.register_context() && self.snapshot.state == state::STOPPED;
            let matches = error.is_some()
                || serde_json::from_value::<Context>(result["context"].clone()).ok()
                    == Some(pending.context.clone());
            if !current || !matches || self.write_editor.popup.is_none() {
                if let Some(token) = result["draft"].as_str() {
                    self.write_editor.cancels.push_back(token.into());
                }
                return true;
            }
            let popup = self.write_editor.popup.as_mut().unwrap();
            popup.detail = if let Some(error) = error {
                error.into()
            } else {
                format!(
                    "{}\nOwner: {} · {} @ {}\nMask/range: {} · fresh read: {}\nApply sends this draft once.",
                    result["warning"].as_str().unwrap_or(""),
                    result["owner"].as_str().unwrap_or("?"),
                    result["channel"].as_str().unwrap_or("?"),
                    result["endpoint"].as_str().unwrap_or("?"),
                    result["plan"]["selected_mask"]["hex"]
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!(
                            "{} bytes at {}",
                            result["plan"]["byte_count"], result["address"]
                        )),
                    result["plan"]["needs_fresh_read"]
                )
            };
            if error.is_none() && result["draft"].is_string() {
                if let Some(bits) = result["metadata"]["scalar"]["bits"].as_u64() {
                    popup.candidate.bits = bits as u16;
                }
                popup.preview = Some(result.clone());
                popup.field = 3;
            }
        } else {
            // A completed write still belongs to its original owner if the stop/core changed.
            let outcome = result["outcome"].as_str().unwrap_or("unknown");
            let detail = format!(
                "Write {outcome} · owner {} · {}\n{}",
                result["owner"].as_str().unwrap_or(&pending.context.core),
                pending.draft.as_deref().unwrap_or("?"),
                error.unwrap_or_else(|| result["error"].as_str().unwrap_or(""))
            );
            self.notice = detail.replace('\n', " · ");
            if let Some(popup) = &mut self.write_editor.popup {
                popup.detail = detail;
                popup.field = 4;
            }
        }
        true
    }
    pub(super) fn write_key(&mut self, key: KeyEvent, engine: Option<&EngineHandle>) -> bool {
        let Some(popup) = &mut self.write_editor.popup else {
            return false;
        };
        match key.code {
            KeyCode::Esc => self.write_action(4, engine),
            KeyCode::Tab | KeyCode::Down => popup.field = (popup.field + 1) % popup.fields(),
            KeyCode::BackTab | KeyCode::Up => {
                popup.field = (popup.field + popup.fields() - 1) % popup.fields()
            }
            KeyCode::Enter => {
                let field = popup.field;
                self.write_action(if field == 0 { 2 } else { field }, engine);
            }
            KeyCode::Left | KeyCode::Right
                if popup.field == 1 && popup.input.kind == InputKind::Bytes =>
            {
                popup.input.little_endian = Some(!popup.input.little_endian.unwrap_or(true));
                self.invalidate_write_preview();
            }
            KeyCode::Char('u')
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(popup.field, 0 | 5 | 6)
                    && self.write_editor.pending.is_none() =>
            {
                if let Some(text) = popup.text() {
                    text.clear();
                }
                self.invalidate_write_preview();
            }
            KeyCode::Backspace
                if matches!(popup.field, 0 | 5 | 6) && self.write_editor.pending.is_none() =>
            {
                if let Some(text) = popup.text() {
                    text.pop();
                }
                self.invalidate_write_preview();
            }
            KeyCode::Char(c)
                if matches!(popup.field, 0 | 5 | 6)
                    && key.modifiers.is_empty()
                    && !c.is_control()
                    && self.write_editor.pending.is_none() =>
            {
                let limit = if popup.field == 5 {
                    256
                } else if popup.field == 6 {
                    4
                } else if popup.memory() {
                    16384
                } else {
                    1024
                };
                if let Some(text) = popup.text()
                    && text.len() + c.len_utf8() <= limit
                {
                    text.push(c);
                }
                self.invalidate_write_preview();
            }
            _ => {}
        }
        true
    }
    pub(super) fn write_paste(&mut self, text: &str) -> bool {
        let Some(popup) = &mut self.write_editor.popup else {
            return false;
        };
        if matches!(popup.field, 0 | 5 | 6) && self.write_editor.pending.is_none() {
            let limit = if popup.field == 5 {
                256
            } else if popup.field == 6 {
                4
            } else if popup.memory() {
                16384
            } else {
                1024
            };
            for c in text.chars().filter(|c| !c.is_control()) {
                if let Some(text) = popup.text()
                    && text.len() + c.len_utf8() <= limit
                {
                    text.push(c);
                }
            }
            self.invalidate_write_preview();
        }
        true
    }
    pub(super) fn write_mouse(&mut self, mouse: MouseEvent, engine: Option<&EngineHandle>) -> bool {
        if !self.write_editor.modal() {
            return false;
        }
        if mouse.kind == MouseEventKind::Down(event::MouseButton::Left)
            && let Some((_, field)) = self
                .write_editor
                .hits
                .iter()
                .find(|(r, _)| r.contains((mouse.column, mouse.row).into()))
                .copied()
        {
            self.write_editor.popup.as_mut().unwrap().field = field;
            if matches!(field, 1..=4) {
                self.write_action(field, engine);
            }
        }
        true
    }
}
pub(super) fn draw(f: &mut UiFrame, app: &mut App) {
    app.write_editor.hits.clear();
    let Some(popup) = &app.write_editor.popup else {
        return;
    };
    let rect = center(f.area(), 94, 17);
    theme::overlay(f, rect);
    let card = theme::card(" Edit value · Tab select · Esc cancel ", true);
    let inner = card.inner(rect);
    f.render_widget(card, rect);
    let input = if popup.input.text.len() > usize::from(inner.width.saturating_sub(8)) {
        popup
            .input
            .text
            .chars()
            .rev()
            .take(usize::from(inner.width.saturating_sub(8)))
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>()
    } else {
        popup.input.text.clone()
    };
    let mut labels = vec![
        format!("Value: {input}"),
        format!(
            "Input: {:?}{}",
            popup.input.kind,
            if popup.input.kind == InputKind::Bytes && popup.memory() {
                " · address order"
            } else if popup.input.kind == InputKind::Bytes {
                if popup.input.little_endian == Some(true) {
                    " LE · ←/→ change order"
                } else {
                    " BE · ←/→ change order"
                }
            } else {
                " · Enter changes format"
            }
        ),
        "Preview".into(),
        "Apply".into(),
        "Cancel".into(),
    ];
    if popup.memory() {
        labels.push(format!("Address: {}", popup.address));
        labels.push(format!("Byte count: {}", popup.count));
    }
    let rows = usize::from(inner.height.min(labels.len() as u16));
    let first = popup.field.saturating_sub(rows.saturating_sub(1));
    for (field, label) in labels.iter().enumerate().skip(first).take(rows) {
        let hit = Rect::new(inner.x, inner.y + (field - first) as u16, inner.width, 1);
        let available = popup.candidate.reason.is_none()
            && !popup.expired
            && !app.demo
            && app.snapshot.state == state::STOPPED
            && (!popup.physical() || popup.context.frame == 0)
            && app.write_editor.pending.is_none();
        let disabled =
            field == 2 && !available || field == 3 && (!available || popup.preview.is_none());
        f.render_widget(
            Paragraph::new(label.clone()).style(if disabled {
                Style::default().fg(theme::MUTED)
            } else {
                theme::selected(field == popup.field)
            }),
            hit,
        );
        app.write_editor.hits.push((hit, field));
    }
    if inner.height > rows as u16 {
        f.render_widget(Paragraph::new(format!("{} · {} bits · selected core {}\n{}\nCtrl+U clears the input. Editing invalidates the preview.", popup.candidate.title, popup.candidate.bits, popup.context.core, popup.detail)).wrap(Wrap { trim: false }).style(Style::default().fg(theme::MUTED)), Rect::new(inner.x, inner.y + rows as u16, inner.width, inner.height - rows as u16));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> App {
        let mut project = Project::default();
        project.registers.cpu = "cortex-r52".into();
        let mut app = App::new(project, false);
        app.snapshot.state = state::STOPPED.into();
        app.snapshot.register_session = 73;
        app.snapshot.generation = 9;
        app.select_pane(pane::REGS);
        app.selection = (0..app.register_view.rows.len())
            .find(|i| {
                app.register_view
                    .edit_candidate(*i, &app.register_context())
                    .is_ok_and(|c| c.target["id"] == "r0")
            })
            .unwrap();
        app
    }
    fn response(app: &App, token: &str) -> Value {
        json!({"draft":token,"context":app.register_context(),"target":{"kind":"register","id":"r0"},"owner":"default","channel":"gdb","endpoint":"localhost:3333","plan":{"selected_mask":{"bits":32,"hex":"0xffffffff"},"needs_fresh_read":false},"outcome":"not_sent"})
    }
    #[test]
    fn panel_edit_buttons_choose_rendered_locals_context_when_source_has_focus() {
        let (engine, requests) = session::test_channel();
        for (width, height) in [(45, 12), (80, 24), (160, 42)] {
            let mut app = app();
            app.snapshot.frame.level = 1;
            app.snapshot.locals = vec![Variable {
                name: "local_counter".into(),
                value: "7".into(),
                tree: Some(crate::session::WatchTree {
                    type_name: "unsigned int".into(),
                    ..Default::default()
                }),
                ..Default::default()
            }];
            app.select_pane(pane::LOCALS);
            // Compact layouts show the focused pane; the wide layout shows
            // Locals alongside Source and must route its button independently.
            if width >= 160 {
                app.select_pane(pane::SOURCE);
            }
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| super::super::draw(f, &mut app)).unwrap();
            let context = app
                .write_editor
                .entry_hits
                .iter()
                .find(|(_, pane)| *pane == pane::LOCALS)
                .unwrap()
                .0;
            let hit = app
                .action_hits
                .iter()
                .find(|(r, action)| *action == "edit-value" && context.contains((r.x, r.y).into()))
                .unwrap()
                .0;
            app.mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(event::MouseButton::Left),
                    column: hit.x,
                    row: hit.y,
                    modifiers: KeyModifiers::NONE,
                },
                Some(&engine),
            );
            assert_eq!(app.pane, pane::LOCALS);
            assert_eq!(
                app.write_editor.popup.as_ref().unwrap().candidate.target["pane"],
                "locals"
            );
            app.write_paste("77");
            app.write_action(2, Some(&engine));
            let request = requests.try_recv().unwrap();
            assert_eq!(request.method, "write_preview");
            assert_eq!(request.params["target"]["expression"], "local_counter");
            assert_eq!(request.params["context"]["frame"], 1);
            assert!(requests.try_recv().is_err());
        }
    }
    #[test]
    fn edit_value_controls_fit_narrow_windows_and_cancel_sends_no_apply() {
        let (engine, requests) = session::test_channel();
        let mut app = app();
        app.command(Some(&engine), ":edit-value");
        assert!(app.write_editor.modal());
        assert!(COMMANDS.contains(&"edit-value"));
        for (width, height) in [(45, 12), (80, 24), (120, 36)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            for field in 0..5 {
                app.write_editor.popup.as_mut().unwrap().field = field;
                terminal.draw(|f| super::super::draw(f, &mut app)).unwrap();
                assert!(app.write_editor.hits.iter().any(|(_, n)| *n == field));
                assert!(
                    app.write_editor
                        .hits
                        .iter()
                        .all(|(r, _)| r.right() <= width && r.bottom() <= height)
                );
            }
        }
        app.write_key(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            Some(&engine),
        );
        assert!(!app.write_editor.modal());
        assert!(requests.try_recv().is_err());
    }
    #[test]
    fn preview_is_required_and_editing_cancels_the_old_server_draft() {
        let (engine, requests) = session::test_channel();
        let mut app = app();
        app.open_edit_value();
        app.write_action(3, Some(&engine));
        assert!(requests.try_recv().is_err());
        assert!(app.write_paste("4294967295"));
        app.write_action(2, Some(&engine));
        let request = requests.try_recv().unwrap();
        assert_eq!(request.method, "write_preview");
        assert_eq!(request.params["input"]["text"], "4294967295");
        assert!(app.write_response(request.id, &response(&app, "a"), None));
        assert_eq!(app.write_editor.popup.as_ref().unwrap().field, 3);
        app.write_editor.popup.as_mut().unwrap().field = 0;
        app.write_key(
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
            Some(&engine),
        );
        assert!(app.write_editor.popup.as_ref().unwrap().preview.is_none());
        app.write_action(3, Some(&engine));
        assert!(requests.try_recv().is_err());
        assert!(app.flush_write_cancels(Some(&engine)));
        let cancelled = requests.try_recv().unwrap();
        assert_eq!(cancelled.method, "write_cancel");
        assert_eq!(cancelled.params["draft"], "a");
    }
    #[test]
    fn cancelled_or_expired_preview_is_discarded_and_errors_keep_the_input() {
        let (engine, requests) = session::test_channel();
        let mut app = app();
        app.open_edit_value();
        app.write_paste("42");
        app.write_action(2, Some(&engine));
        let request = requests.try_recv().unwrap();
        app.write_action(4, Some(&engine));
        assert!(app.write_response(request.id, &response(&app, "late"), None));
        assert!(!app.write_editor.modal());
        assert_eq!(
            app.write_editor.cancels.pop_front().as_deref(),
            Some("late")
        );
        app.open_edit_value();
        app.write_paste("42");
        app.write_action(2, Some(&engine));
        let request = requests.try_recv().unwrap();
        assert!(app.write_response(request.id, &Value::Null, Some("writer permission denied")));
        let popup = app.write_editor.popup.as_ref().unwrap();
        assert_eq!(popup.input.text, "42");
        assert!(popup.detail.contains("permission denied"));
        app.write_action(2, Some(&engine));
        let request = requests.try_recv().unwrap();
        let stale = response(&app, "stale");
        let mut next = app.snapshot.clone();
        next.generation += 1;
        app.update(Event::Snapshot {
            snapshot: Box::new(next),
        });
        assert!(app.write_response(request.id, &stale, None));
        assert!(app.write_editor.popup.as_ref().unwrap().preview.is_none());
        app.write_action(3, Some(&engine));
        assert!(requests.try_recv().is_err());
    }
    #[test]
    fn apply_uses_the_headless_draft_once_and_closing_does_not_claim_withdrawal() {
        let (engine, requests) = session::test_channel();
        let mut app = app();
        app.open_edit_value();
        app.write_paste("42");
        app.write_action(2, Some(&engine));
        let preview = requests.try_recv().unwrap();
        app.write_response(preview.id, &response(&app, "send-once"), None);
        app.write_action(3, Some(&engine));
        let apply = requests.try_recv().unwrap();
        assert_eq!(apply.method, "write_apply");
        assert_eq!(apply.params, json!({"draft":"send-once"}));
        app.write_action(3, Some(&engine));
        assert!(requests.try_recv().is_err());
        app.write_action(4, Some(&engine));
        assert!(app.notice.contains("does not withdraw"));
        let mut result = response(&app, "send-once");
        result["outcome"] = json!("unknown");
        result["error"] = json!("reply lost");
        app.write_response(apply.id, &result, None);
        assert!(app.notice.contains("unknown") && app.notice.contains("reply lost"));
        assert!(app.write_editor.pending.is_none());
    }

    #[test]
    fn keyboard_and_mouse_use_the_current_row_and_unsupported_objects_show_a_reason() {
        let (engine, requests) = session::test_channel();
        let mut app = app();
        assert_ne!(
            app.selection,
            app.selections[pane::REGS],
            "fixture keeps the inactive saved row different"
        );
        app.key(
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
            Some(&engine),
        );
        assert_eq!(
            app.write_editor.popup.as_ref().unwrap().candidate.target["id"],
            "r0"
        );
        app.write_paste("7");
        let mut terminal = Terminal::new(TestBackend::new(45, 12)).unwrap();
        terminal.draw(|f| super::super::draw(f, &mut app)).unwrap();
        let (hit, _) = app
            .write_editor
            .hits
            .iter()
            .find(|(_, n)| *n == 2)
            .copied()
            .unwrap();
        app.write_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(event::MouseButton::Left),
                column: hit.x,
                row: hit.y,
                modifiers: KeyModifiers::NONE,
            },
            Some(&engine),
        );
        let request = requests.try_recv().unwrap();
        assert_eq!(request.method, "write_preview");
        assert_eq!(request.params["target"]["id"], "r0");
        app.write_response(request.id, &Value::Null, Some("not writable"));
        app.write_action(4, Some(&engine));
        app.selection = (0..app.register_view.rows.len())
            .find(|i| {
                app.register_view
                    .edit_candidate(*i, &app.register_context())
                    .is_ok_and(|c| c.target["id"] == "cpsr")
            })
            .unwrap();
        app.open_edit_value();
        assert!(
            app.write_editor
                .popup
                .as_ref()
                .unwrap()
                .detail
                .contains("No independent writer")
        );
        app.write_action(2, Some(&engine));
        app.write_action(3, Some(&engine));
        assert!(requests.try_recv().is_err());
    }
    #[test]
    fn failed_send_leaves_an_editable_draft_instead_of_a_permanent_pending_request() {
        let (engine, requests) = session::test_channel();
        drop(requests);
        let mut app = app();
        app.open_edit_value();
        app.write_paste("42");
        app.write_action(2, Some(&engine));
        assert!(app.write_editor.pending.is_none());
        assert_eq!(app.write_editor.popup.as_ref().unwrap().input.text, "42");
        app.write_editor.popup.as_mut().unwrap().field = 0;
        app.write_paste("1");
        assert_eq!(app.write_editor.popup.as_ref().unwrap().input.text, "421");
    }
    #[test]
    fn memory_editor_edits_literal_address_count_and_address_order_bytes_on_nonzero_frame() {
        let mut app = app();
        app.select_pane(pane::MEMORY);
        app.snapshot.frame.level = 2;
        let (engine, requests) = session::test_channel();
        app.open_edit_value();
        app.write_editor.popup.as_mut().unwrap().field = 5;
        app.write_key(
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
            Some(&engine),
        );
        app.write_paste("0x100000001");
        app.write_editor.popup.as_mut().unwrap().field = 6;
        app.write_key(
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
            Some(&engine),
        );
        app.write_paste("4");
        app.write_editor.popup.as_mut().unwrap().field = 0;
        app.write_paste("12 34 56 78");
        app.write_action(2, Some(&engine));
        let preview = requests.try_recv().unwrap();
        assert_eq!(preview.method, "write_preview");
        assert_eq!(preview.params["target"]["address"], "0x100000001");
        assert_eq!(preview.params["target"]["bits"], 32);
        assert_eq!(preview.params["input"]["kind"], "bytes");
        assert_eq!(preview.params["context"]["frame"], 2);
        let context = app.register_context();
        app.write_response(preview.id,&json!({"draft":"ram-draft","context":context,"owner":"chip:fixture","plan":{"byte_count":4}}),None);
        app.write_editor.popup.as_mut().unwrap().field = 5;
        app.write_paste("0");
        assert!(app.write_editor.popup.as_ref().unwrap().preview.is_none());
        assert!(app.flush_write_cancels(Some(&engine)));
        assert_eq!(requests.try_recv().unwrap().method, "write_cancel");
        let mut terminal = Terminal::new(TestBackend::new(45, 12)).unwrap();
        app.write_editor.popup.as_mut().unwrap().field = 6;
        terminal.draw(|f| super::super::draw(f, &mut app)).unwrap();
        assert!(app.write_editor.hits.iter().any(|(_, i)| *i == 6));
    }
}
