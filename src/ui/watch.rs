use super::*;

#[derive(Default)]
pub(super) struct WatchView {
    pub add_rect: Rect,
    pub remove_rect: Rect,
    pub remove_hits: Vec<(Rect, String)>,
    pub pending_remove: Option<u64>,
}

impl App {
    pub(super) fn add_watch_input(&mut self, engine: Option<&EngineHandle>) {
        let expression = self.watch_input.trim().to_owned();
        if expression.is_empty() || self.pending_watch.is_some() {
            return;
        }
        self.fx.trigger("submit", 220);
        let id = self.next_id;
        self.submit(engine, "watch", json!({"expression":expression}));
        if self.pending_commands.contains(&id) {
            self.pending_watch = Some((id, expression));
        }
        self.completion.invalidate();
    }

    pub(super) fn reconcile_watch_selection(&mut self, next: &Snapshot) {
        if self
            .snapshot
            .watches
            .iter()
            .map(|v| &v.name)
            .eq(next.watches.iter().map(|v| &v.name))
        {
            return;
        }
        let row = self.selected(1);
        let previous = self.snapshot.watches.get(row / 2).map(|v| &v.name);
        let index = previous
            .and_then(|name| next.watches.iter().position(|v| &v.name == name))
            .unwrap_or_else(|| (row / 2).min(next.watches.len().saturating_sub(1)));
        let row = if next.watches.is_empty() {
            0
        } else {
            index * 2 + row % 2
        };
        self.selections[1] = row;
        if self.pane == 1 {
            self.selection = row;
        }
        let visible = self.view_rects[1].height.max(1) as usize;
        let max = (next.watches.len() * 2).saturating_sub(visible);
        self.view_tops[1] = self.view_tops[1].min(max);
        if self.pane == 1 {
            if row < self.view_tops[1] {
                self.view_tops[1] = row;
            } else if row >= self.view_tops[1] + visible {
                self.view_tops[1] = (row + 1 - visible).min(max);
            }
        }
        // A deleted expression must not leave a second, stale selected highlight.
        if self
            .formats
            .selected
            .as_ref()
            .is_some_and(|key| key.starts_with("watch:"))
        {
            self.formats.selected = None;
        }
    }
    pub(super) fn remove_selected_watch(&mut self, engine: Option<&EngineHandle>) {
        if let Some(value) = self.snapshot.watches.get(self.selected(1) / 2) {
            let name = value.name.clone();
            self.submit(engine, "unwatch", json!({"expression":name}));
        } else {
            self.notice = "Select a Watch expression first.".into();
        }
    }
    pub(super) fn watch_mouse(&mut self, mouse: MouseEvent, engine: Option<&EngineHandle>) -> bool {
        if self.variable_pane != 1 || mouse.kind != MouseEventKind::Down(event::MouseButton::Left) {
            return false;
        }
        let point = (mouse.column, mouse.row).into();
        if self.watch.add_rect.contains(point) {
            self.focus_input(true);
            self.add_watch_input(engine);
            return true;
        }
        // Bind the close button to the expression that was actually rendered,
        // not an index which a queued snapshot might have shifted underneath it.
        if let Some((_, name)) = self
            .watch
            .remove_hits
            .iter()
            .find(|(r, _)| r.contains(point))
        {
            let Some(index) = self.snapshot.watches.iter().position(|v| &v.name == name) else {
                return true;
            };
            self.select_pane(1);
            self.selection = index * 2;
        } else if !self.watch.remove_rect.contains(point) {
            return false;
        }
        self.editing = false;
        self.watch_editing = false;
        self.completion.invalidate();
        self.select_pane(1);
        self.remove_selected_watch(engine);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(names: &[&str]) -> App {
        let mut a = App::new(Project::default(), false);
        a.snapshot.state = "STOPPED".into();
        a.snapshot.watches = names
            .iter()
            .map(|name| Variable {
                name: (*name).into(),
                value: "1".into(),
                ..Default::default()
            })
            .collect();
        a.select_pane(1);
        a
    }
    fn key(a: &mut App, code: KeyCode, engine: &EngineHandle) {
        a.key(KeyEvent::new(code, KeyModifiers::NONE), Some(engine));
    }
    fn removed(a: &mut App, request: &Request) {
        let mut snapshot = a.snapshot.clone();
        snapshot
            .watches
            .retain(|v| v.name != request.params["expression"].as_str().unwrap());
        a.update(Event::Snapshot {
            snapshot: Box::new(snapshot),
        });
        a.update(Event::Response{id:request.id,ok:true,result:json!({"watches":a.snapshot.watches.iter().map(|v|v.name.clone()).collect::<Vec<_>>()}),error:None});
    }
    #[test]
    fn watch_delete_tail_keeps_a_valid_selection_until_empty() {
        let (engine, requests) = session::test_channel();
        let mut a = app(&["first", "middle", "last"]);
        a.selection = 5;
        for name in ["last", "middle", "first"] {
            key(&mut a, KeyCode::Delete, &engine);
            let request = requests
                .try_recv()
                .expect("Delete must keep working after deleting the tail");
            assert_eq!(request.method, "unwatch");
            assert_eq!(request.params["expression"], name);
            removed(&mut a, &request);
        }
        key(&mut a, KeyCode::Delete, &engine);
        assert!(requests.try_recv().is_err());
        assert_eq!(a.selection, 0);
        assert_eq!(a.view_tops[1], 0);
    }
    #[test]
    fn watch_selection_follows_expression_when_an_earlier_item_is_removed() {
        let (engine, _) = session::test_channel();
        let mut a = app(&["first", "middle", "last"]);
        a.selection = 3;
        removed(
            &mut a,
            &Request::new(15, "unwatch", json!({"expression":"first"})),
        );
        assert_eq!(a.snapshot.watches[a.selected(1) / 2].name, "middle");
        a.select_pane(0);
        removed(
            &mut a,
            &Request::new(16, "unwatch", json!({"expression":"last"})),
        );
        a.select_pane(1);
        assert_eq!(a.selected(1) / 2, 0);
        key(&mut a, KeyCode::Delete, &engine);
    }
    #[test]
    fn console_history_delete_never_removes_a_watch() {
        let (engine, requests) = session::test_channel();
        let mut a = app(&["first", "last"]);
        a.console_view.focused = true;
        key(&mut a, KeyCode::Delete, &engine);
        assert!(
            requests.try_recv().is_err(),
            "Console focus must not inherit the Watch Delete action"
        );
    }

    fn render(a: &mut App) -> Terminal<TestBackend> {
        let mut t = Terminal::new(TestBackend::new(160, 42)).unwrap();
        t.draw(|f| draw(f, a)).unwrap();
        t
    }
    fn click(a: &mut App, rect: Rect, engine: &EngineHandle) {
        a.mouse(
            MouseEvent {
                kind: MouseEventKind::Down(event::MouseButton::Left),
                column: rect.x,
                row: rect.y,
                modifiers: KeyModifiers::NONE,
            },
            Some(engine),
        );
    }
    #[test]
    fn watch_delete_handles_scrolled_name_and_value_rows_and_mouse_button() {
        let names = (0..40).map(|i| format!("counter_{i}")).collect::<Vec<_>>();
        let refs = names.iter().map(String::as_str).collect::<Vec<_>>();
        for value_row in [false, true] {
            let (engine, requests) = session::test_channel();
            let mut a = app(&refs);
            a.fx.mode = crate::config::Motion::Off;
            render(&mut a);
            a.set_view_top(1, 60);
            render(&mut a);
            let row = 60 + usize::from(value_row);
            let hit = a
                .formats
                .hits
                .iter()
                .find(|item| item.pane == 1 && item.row == row)
                .unwrap()
                .rect;
            click(&mut a, hit, &engine);
            let t = render(&mut a);
            for offset in 0..2 {
                assert_eq!(
                    t.backend().buffer()[(a.view_rects[1].x, a.view_rects[1].y + offset)].bg,
                    theme::SELECTED
                );
            }
            if value_row {
                let button = a.watch.remove_rect;
                click(&mut a, button, &engine);
            } else {
                key(&mut a, KeyCode::Delete, &engine);
            }
            let request = requests.try_recv().unwrap();
            assert_eq!(request.params["expression"], "counter_30");
            removed(&mut a, &request);
            render(&mut a);
            key(&mut a, KeyCode::Delete, &engine);
            assert_eq!(
                requests.try_recv().unwrap().params["expression"],
                "counter_31"
            );
        }
    }
    #[test]
    fn watch_delete_serializes_requests_and_recovers_from_failure() {
        let (engine, requests) = session::test_channel();
        let mut a = app(&["first", "last"]);
        a.selection = 2;
        key(&mut a, KeyCode::Delete, &engine);
        let first = requests.try_recv().unwrap();
        key(&mut a, KeyCode::Delete, &engine);
        assert!(requests.try_recv().is_err());
        assert_eq!(a.snapshot.watches.len(), 2);
        a.update(Event::Response {
            id: first.id,
            ok: false,
            result: Value::Null,
            error: Some("test failure".into()),
        });
        assert_eq!(a.snapshot.watches[1].name, "last");
        assert!(a.watch.pending_remove.is_none());
        key(&mut a, KeyCode::Delete, &engine);
        let retry = requests.try_recv().unwrap();
        removed(&mut a, &retry);
        a.key(
            KeyEvent::new_with_kind(KeyCode::Delete, KeyModifiers::NONE, KeyEventKind::Repeat),
            Some(&engine),
        );
        assert!(
            requests.try_recv().is_err(),
            "A held key must not consume the remaining list"
        );
        key(&mut a, KeyCode::Delete, &engine);
        assert_eq!(requests.try_recv().unwrap().params["expression"], "first");
    }
    #[test]
    fn watch_remove_button_works_while_running_and_input_delete_keeps_draft() {
        let (engine, requests) = session::test_channel();
        let mut a = app(&["first", "last"]);
        a.selection = 3;
        a.snapshot.state = "RUNNING".into();
        a.focus_input(true);
        a.watch_input = "unfinished_expression".into();
        render(&mut a);
        key(&mut a, KeyCode::Delete, &engine);
        assert!(requests.try_recv().is_err());
        assert_eq!(a.watch_input, "unfinished_expression");
        let button = a.watch.remove_rect;
        click(&mut a, button, &engine);
        let request = requests.try_recv().unwrap();
        assert_eq!(request.params["expression"], "last");
        assert!(!a.input_active());
        assert_eq!(a.watch_input, "unfinished_expression");
        removed(&mut a, &request);
        a.palette = true;
        key(&mut a, KeyCode::Delete, &engine);
        click(&mut a, button, &engine);
        assert!(requests.try_recv().is_err());
        a.palette = false;
        assert_eq!(a.snapshot.watches[0].name, "first");
        a.select_pane(9);
        // Ignore the old button hit rectangle before the next frame redraws Locals.
        click(&mut a, button, &engine);
        assert!(requests.try_recv().is_err());
    }

    #[test]
    fn watch_add_button_focuses_empty_input_and_submits_expression_directly() {
        let (engine, requests) = session::test_channel();
        let mut a = app(&[]);
        a.input = "p separate_console_draft".into();
        render(&mut a);
        let add = a.watch.add_rect;
        click(&mut a, add, &engine);
        assert!(a.watch_editing && !a.editing);
        assert!(requests.try_recv().is_err());
        for c in "counter_pair.value".chars() {
            key(&mut a, KeyCode::Char(c), &engine);
        }
        click(&mut a, add, &engine);
        let request = requests.try_recv().unwrap();
        assert_eq!(request.method, "watch");
        assert_eq!(request.params, json!({"expression":"counter_pair.value"}));
        click(&mut a, add, &engine);
        assert!(requests.try_recv().is_err());
        a.update(Event::Response {
            id: request.id,
            ok: false,
            result: Value::Null,
            error: Some("test failure".into()),
        });
        assert_eq!(a.watch_input, "counter_pair.value");
        click(&mut a, add, &engine);
        let retry = requests.try_recv().unwrap();
        a.update(Event::Response {
            id: retry.id,
            ok: true,
            result: Value::Null,
            error: None,
        });
        assert!(a.watch_input.is_empty());
        assert!(a.watch_editing);
        assert_eq!(a.input, "p separate_console_draft");
        // A late successful add must not clear a newer draft.
        a.watch_input = "counter".into();
        click(&mut a, add, &engine);
        let request = requests.try_recv().unwrap();
        a.watch_input = "counter_total".into();
        a.update(Event::Response {
            id: request.id,
            ok: true,
            result: Value::Null,
            error: None,
        });
        assert_eq!(a.watch_input, "counter_total");
    }

    #[test]
    fn watch_row_close_removes_clicked_expression_not_current_selection() {
        let (engine, requests) = session::test_channel();
        let mut a = app(&["first", "middle", "last"]);
        a.selection = 0;
        render(&mut a);
        let hit = a
            .watch
            .remove_hits
            .iter()
            .find(|(_, n)| n == "last")
            .unwrap()
            .0;
        a.focus_input(true);
        a.watch_input = "keep_this_draft".into();
        click(&mut a, hit, &engine);
        let request = requests.try_recv().unwrap();
        assert_eq!(request.params["expression"], "last");
        assert_eq!(a.watch_input, "keep_this_draft");
        removed(&mut a, &request);
        // A second click before redraw must not delete the neighbouring item.
        click(&mut a, hit, &engine);
        assert!(requests.try_recv().is_err());
        render(&mut a);
        let hit = a
            .watch
            .remove_hits
            .iter()
            .find(|(_, n)| n == "middle")
            .unwrap()
            .0;
        // A queued removal shifts the index after drawing, but not the identity.
        removed(
            &mut a,
            &Request::new(999, "unwatch", json!({"expression":"first"})),
        );
        click(&mut a, hit, &engine);
        assert_eq!(requests.try_recv().unwrap().params["expression"], "middle");
    }

    #[test]
    fn watch_mouse_actions_respect_modals_hidden_panel_and_running_target() {
        let (engine, requests) = session::test_channel();
        let mut a = app(&["first"]);
        render(&mut a);
        let add = a.watch.add_rect;
        let close = a.watch.remove_hits[0].0;
        a.watch_input = "second".into();
        a.help = true;
        click(&mut a, add, &engine);
        click(&mut a, close, &engine);
        a.help = false;
        a.select_pane(9);
        click(&mut a, add, &engine);
        click(&mut a, close, &engine);
        assert!(requests.try_recv().is_err());
        a.select_pane(1);
        a.snapshot.state = "RUNNING".into();
        click(&mut a, add, &engine);
        assert_eq!(requests.try_recv().unwrap().method, "watch");
        click(&mut a, close, &engine);
        let request = requests.try_recv().unwrap();
        assert_eq!(request.method, "unwatch");
        assert_eq!(request.params["expression"], "first");
        assert_eq!(a.snapshot.state, "RUNNING");
    }

    #[test]
    fn watch_direct_controls_fit_compact_layout_and_scrolled_values() {
        let (engine, requests) = session::test_channel();
        let names = (0..40).map(|i| format!("counter_{i}")).collect::<Vec<_>>();
        let refs = names.iter().map(String::as_str).collect::<Vec<_>>();
        let mut a = app(&refs);
        for (w, h) in [(45, 12), (80, 24), (100, 30), (160, 42)] {
            let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
            t.draw(|f| draw(f, &mut a)).unwrap();
            assert!(a.watch.add_rect.width > 0 && a.watch.add_rect.height > 0);
            assert_eq!(a.watch_input_rect.right(), a.watch.add_rect.x);
            assert!(a.watch_input_rect.width >= 10);
            a.set_view_top(1, 61);
            t.draw(|f| draw(f, &mut a)).unwrap();
            let (close, name) = a.watch.remove_hits.first().unwrap().clone();
            assert_eq!(name, "counter_30");
            assert_eq!(close.y, a.view_rects[1].y);
            assert!(close.right() <= a.scrollbars[1].x);
            assert!(
                a.formats
                    .hits
                    .iter()
                    .filter(|i| i.pane == 1)
                    .all(|i| i.rect.right() <= close.x)
            );
            click(&mut a, close, &engine);
            let request = requests.try_recv().unwrap();
            assert_eq!(request.params["expression"], name);
            a.update(Event::Response {
                id: request.id,
                ok: false,
                result: Value::Null,
                error: Some("keep fixture".into()),
            });
        }
    }

    #[test]
    fn export_watch_actions_previews_when_requested() {
        let Ok(root) = std::env::var("DEBUGTUI_RENDER_DIR") else {
            return;
        };
        let root = Path::new(&root);
        fs::create_dir_all(root).unwrap();
        let mut a = app(&["counter", "counter_total", "counter_pair.value"]);
        a.fx.mode = crate::config::Motion::Off;
        a.watch_input = "counter_static".into();
        a.focus_input(true);
        super::super::visual_tests::capture(root, "watch-actions", &mut a, 160, 42);
        super::super::visual_tests::capture(root, "watch-actions-compact", &mut a, 45, 12);
    }
}
