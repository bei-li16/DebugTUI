use super::*;

pub(super) struct CoreEditor {
    number: String,
    location: String,
    selected: Vec<bool>,
    owner: usize,
    field: usize,
    pub(super) error: String,
    hits: Vec<(Rect, usize)>,
}
impl App {
    pub(crate) fn open_break_cores(&mut self) {
        if !self.break_action_enabled("break-cores") {
            self.notice =
                "Select a persistent code breakpoint in a paused multicore session.".into();
            return;
        }
        let Some(b) = self.selected_break() else {
            return;
        };
        let owner = self.snapshot.core.as_ref().map(|c| c.index).unwrap_or(0);
        self.breaks.core_editor = Some(CoreEditor {
            number: b.id.clone(),
            location: b.location.clone(),
            owner,
            selected: (0..self.snapshot.cores.len())
                .map(|i| i == owner || b.cores.contains(&i))
                .collect(),
            field: owner,
            error: String::new(),
            hits: vec![],
        });
        self.breaks.editor = None;
        self.editing = false;
        self.watch_editing = false;
        self.console_view.focused = false;
        self.completion.invalidate();
        self.select_pane(6);
    }
    fn apply_break_cores(&mut self, engine: Option<&EngineHandle>) {
        let Some(e) = &self.breaks.core_editor else {
            return;
        };
        let cores: Vec<_> = e
            .selected
            .iter()
            .enumerate()
            .filter_map(|(i, checked)| checked.then_some(i))
            .collect();
        self.break_send(
            engine,
            "break_cores",
            json!({"number":e.number,"cores":cores}),
        );
    }
    fn activate_break_core(&mut self, engine: Option<&EngineHandle>) {
        let Some(e) = &mut self.breaks.core_editor else {
            return;
        };
        let count = e.selected.len();
        if e.field < count {
            if e.field == e.owner {
                e.error = "Keep the current core; switch cores to change ownership.".into();
            } else {
                e.selected[e.field] = !e.selected[e.field];
                e.error.clear();
            }
        } else if e.field == count {
            self.apply_break_cores(engine);
        } else {
            self.breaks.core_editor = None;
        }
    }
    pub(super) fn break_cores_key(&mut self, key: KeyEvent, engine: Option<&EngineHandle>) -> bool {
        if self.breaks.pending.is_some() {
            return true;
        }
        let Some(e) = &mut self.breaks.core_editor else {
            return false;
        };
        if key.kind != KeyEventKind::Press {
            return true;
        }
        if key.code == KeyCode::Esc {
            self.breaks.core_editor = None;
            return true;
        }
        if key.code == KeyCode::Enter && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.apply_break_cores(engine);
            return true;
        }
        let count = e.selected.len() + 2;
        match key.code {
            KeyCode::Tab | KeyCode::Down => e.field = (e.field + 1) % count,
            KeyCode::BackTab | KeyCode::Up => e.field = (e.field + count - 1) % count,
            KeyCode::Char('a') => e.selected.fill(true),
            KeyCode::Char('s') => {
                e.selected.fill(false);
                e.selected[e.owner] = true;
            }
            KeyCode::Enter | KeyCode::Char(' ') => self.activate_break_core(engine),
            _ => {}
        }
        true
    }
    pub(super) fn break_cores_mouse(
        &mut self,
        mouse: MouseEvent,
        engine: Option<&EngineHandle>,
    ) -> bool {
        if self.breaks.pending.is_some() {
            return true;
        }
        let Some(e) = &mut self.breaks.core_editor else {
            return false;
        };
        if mouse.kind == MouseEventKind::Down(event::MouseButton::Left)
            && let Some((_, index)) = e
                .hits
                .iter()
                .find(|(r, _)| r.contains((mouse.column, mouse.row).into()))
        {
            e.field = *index;
            self.activate_break_core(engine);
        }
        true
    }
}
pub(super) fn draw(f: &mut UiFrame, a: &mut App) {
    let Some(e) = &mut a.breaks.core_editor else {
        return;
    };
    e.hits.clear();
    let rect = center(f.area(), 90, (e.selected.len() as u16 + 12).min(28));
    theme::overlay(f, rect);
    let block = theme::card(" Breakpoint cores ", true);
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let parts = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .split(inner);
    f.render_widget(
        Paragraph::new(e.location.clone()).wrap(Wrap { trim: false }),
        parts[0],
    );
    let count = e.selected.len();
    let visible = parts[1].height as usize;
    let start = e
        .field
        .saturating_sub(visible.saturating_sub(1))
        .min((count + 2).saturating_sub(visible));
    for i in (start..count + 2).take(visible) {
        let label = if i < count {
            format!(
                " [{}] {}{}",
                if e.selected[i] { "x" } else { " " },
                a.snapshot.cores[i].name,
                if i == e.owner { " (current)" } else { "" }
            )
        } else if i == count {
            if a.breaks.pending.is_some() {
                " Applying..."
            } else {
                " Apply  Ctrl+Enter"
            }
            .into()
        } else {
            " Cancel  Esc".into()
        };
        let row = Rect::new(
            parts[1].x,
            parts[1].y + (i - start) as u16,
            parts[1].width,
            1,
        );
        f.render_widget(
            Paragraph::new(label).style(theme::selected(i == e.field)),
            row,
        );
        e.hits.push((row, i));
    }
    let hint = if e.error.is_empty() {
        "Hardware slots are allocated per core. Edits and deletion apply to all linked cores. Pause affected cores before Apply."
    } else {
        &e.error
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
        Paragraph::new("Space select | a All | s Core | Ctrl+Enter")
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
        a.snapshot.cores = (0..3)
            .map(|i| session::CoreStatus {
                index: i,
                name: format!("core{i}"),
                endpoint: String::new(),
                state: "STOPPED".into(),
            })
            .collect();
        a.snapshot.core = Some(a.snapshot.cores[0].clone());
        a.snapshot.breakpoints.push(session::Breakpoint {
            id: "7".into(),
            location: "tick".into(),
            enabled: true,
            ..Default::default()
        });
        a.select_pane(6);
        a
    }
    #[test]
    fn select_cores_by_mouse_apply_by_keyboard_and_keep_failure_visible() {
        let (engine, requests) = session::test_channel();
        let mut a = fixture();
        a.key(
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE),
            Some(&engine),
        );
        assert!(a.breaks.core_editor.is_some());
        assert_eq!(
            a.breaks.core_editor.as_ref().unwrap().selected,
            [true, false, false]
        );
        for (w, h) in [(45, 12), (80, 24), (160, 45)] {
            let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
            t.draw(|f| crate::ui::draw(f, &mut a)).unwrap();
            assert!(
                a.breaks
                    .core_editor
                    .as_ref()
                    .unwrap()
                    .hits
                    .iter()
                    .all(|(r, _)| r.right() <= w && r.bottom() <= h)
            );
        }
        let hit = a
            .breaks
            .core_editor
            .as_ref()
            .unwrap()
            .hits
            .iter()
            .find(|(_, i)| *i == 1)
            .unwrap()
            .0;
        a.mouse(
            MouseEvent {
                kind: MouseEventKind::Down(event::MouseButton::Left),
                column: hit.x,
                row: hit.y,
                modifiers: KeyModifiers::NONE,
            },
            Some(&engine),
        );
        if let Ok(root) = std::env::var("DEBUGTUI_RENDER_DIR") {
            std::fs::create_dir_all(&root).unwrap();
            for (name, w, h) in [
                ("breakpoint-cores", 140, 40),
                ("breakpoint-cores-compact", 45, 12),
            ] {
                crate::ui::visual_tests::capture(Path::new(&root), name, &mut a, w, h);
            }
        }
        a.key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            Some(&engine),
        );
        let req = requests.try_recv().unwrap();
        assert_eq!(req.method, "break_cores");
        assert_eq!(req.params, json!({"number":"7","cores":[0,1]}));
        a.break_response(req.id, false, Some("core1 unavailable; rolled back"));
        assert!(
            a.breaks
                .core_editor
                .as_ref()
                .unwrap()
                .error
                .contains("rolled back")
        );
        a.key(
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
            Some(&engine),
        );
        a.key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            Some(&engine),
        );
        let req = requests.try_recv().unwrap();
        assert_eq!(req.params["cores"], json!([0]));
        a.break_response(req.id, true, None);
        assert!(!a.breaks.modal());
    }
    #[test]
    fn source_click_request_remains_single_core_and_group_rows_are_marked() {
        let (engine, requests) = session::test_channel();
        let mut a = fixture();
        a.source_file = "main.c".into();
        a.source_line = 12;
        a.toggle_break(Some(&engine));
        let req = requests.try_recv().unwrap();
        assert_eq!(req.method, "break");
        assert!(req.params.get("cores").is_none());
        a.snapshot.breakpoints[0].cores = vec![0, 1];
        let line = super::super::rows(&a, 0, 1).remove(0).to_string();
        assert!(line.contains("[2 cores]"));
    }
}
