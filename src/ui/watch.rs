use super::*;

#[derive(Default)]
pub(super) struct WatchView {
    pub add_rect: Rect,
    pub remove_rect: Rect,
    pub remove_hits: Vec<(Rect, String)>,
    pub pending_remove: Option<u64>,
    pub expand_hits: Vec<(Rect, String)>,
}

pub(super) struct WatchRow<'a> {
    pub root: &'a str,
    pub value: &'a Variable,
    pub path: &'a [usize],
    pub more: bool,
}
impl WatchRow<'_> {
    pub fn key(&self) -> String {
        if self.path.is_empty() && !self.more {
            format!("watch:{}", self.root)
        } else {
            format!("watch-child:{}", json!([self.root, self.path, self.more]))
        }
    }
    pub fn removable(&self) -> bool {
        self.path.is_empty() && !self.more
    }
}
pub(super) fn rows(watches: &[Variable]) -> Vec<WatchRow<'_>> {
    fn visit<'a>(out: &mut Vec<WatchRow<'a>>, root: &'a str, value: &'a Variable) {
        let path = value
            .tree
            .as_ref()
            .map(|t| t.path.as_slice())
            .unwrap_or(&[]);
        out.push(WatchRow {
            root,
            value,
            path,
            more: false,
        });
        if let Some(tree) = &value.tree
            && tree.expanded
        {
            for child in &tree.children {
                visit(out, root, child);
            }
            if tree.has_more {
                out.push(WatchRow {
                    root,
                    value,
                    path,
                    more: true,
                });
            }
        }
    }
    let mut out = vec![];
    for value in watches {
        visit(&mut out, &value.name, value);
    }
    out
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
        let old = rows(&self.snapshot.watches);
        let next = rows(&next.watches);
        if old
            .iter()
            .map(WatchRow::key)
            .eq(next.iter().map(WatchRow::key))
        {
            return;
        }
        let row = self.selected(1);
        let previous = old.get(row / 2);
        let index = previous
            .and_then(|previous| {
                next.iter()
                    .position(|v| v.key() == previous.key())
                    .or_else(|| {
                        next.iter().rposition(|v| {
                            v.root == previous.root && !v.more && previous.path.starts_with(v.path)
                        })
                    })
            })
            .unwrap_or_else(|| (row / 2).min(next.len().saturating_sub(1)));
        let row = if next.is_empty() {
            0
        } else {
            index * 2 + row % 2
        };
        self.selections[1] = row;
        if self.pane == 1 {
            self.selection = row;
        }
        let visible = self.view_rects[1].height.max(1) as usize;
        let max = (next.len() * 2).saturating_sub(visible);
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
            .is_some_and(|key| key.starts_with("watch:") || key.starts_with("watch-child:"))
        {
            self.formats.selected = None;
        }
    }
    pub(super) fn remove_selected_watch(&mut self, engine: Option<&EngineHandle>) {
        if let Some(value) = rows(&self.snapshot.watches)
            .get(self.selected(1) / 2)
            .filter(|v| v.removable())
        {
            let name = value.root.to_owned();
            self.submit(engine, "unwatch", json!({"expression":name}));
        } else {
            self.notice = "Select a top-level Watch expression to remove it.".into();
        }
    }
    pub(super) fn watch_removable(&self) -> bool {
        rows(&self.snapshot.watches)
            .get(self.selected(1) / 2)
            .is_some_and(WatchRow::removable)
    }
    /// Returns false for scalar rows so Enter can still focus the input.
    pub(super) fn toggle_watch(
        &mut self,
        expand: Option<bool>,
        engine: Option<&EngineHandle>,
    ) -> bool {
        let nodes = rows(&self.snapshot.watches);
        let Some(node) = nodes.get(self.selected(1) / 2) else {
            return false;
        };
        let Some(tree) = &node.value.tree else {
            return false;
        };
        if tree.child_count == 0 {
            return false;
        }
        let expanded = expand.unwrap_or(!tree.expanded || node.more);
        if expanded && self.snapshot.state != "STOPPED" {
            self.notice = "Pause the target before expanding Watch values.".into();
            return true;
        }
        let params = json!({"expression":node.root,"path":node.path,"expanded":expanded,"more":node.more && expanded});
        self.submit(engine, "watch_expand", params);
        true
    }
    pub(super) fn watch_item(&self, row: usize) -> Option<formats::Item> {
        let nodes = rows(&self.snapshot.watches);
        let node = nodes.get(row / 2).filter(|v| !v.more)?;
        Some(formats::Item {
            rect: Rect::default(),
            pane: 1,
            row,
            key: node.key(),
            name: node.value.name.clone(),
            raw: self
                .watch_sample(&node.key())
                .map(|s| s.0)
                .unwrap_or_else(|| node.value.value.clone()),
            default: crate::config::Radix::Decimal,
        })
    }
    pub(super) fn watch_numeric_view(&mut self, f: &mut UiFrame, rect: Rect) {
        let watches = self.snapshot.watches.clone();
        let nodes = rows(&watches);
        let start = self.view_tops[1];
        for row in start..start + rect.height as usize {
            let Some(node) = nodes.get(row / 2) else {
                break;
            };
            let mut displayed = node.value.clone();
            if let Some((text, changed, error)) = self.watch_sample(&node.key()) {
                displayed.value = text;
                displayed.changed = changed;
                displayed.error = error;
            }
            let value = &displayed;
            let close_width = if node.removable() && rect.width >= 8 {
                3
            } else {
                0
            };
            let hit = Rect::new(
                rect.x,
                rect.y + (row - start) as u16,
                rect.width - close_width,
                1,
            );
            let mut item = formats::Item {
                rect: hit,
                pane: 1,
                row,
                key: node.key(),
                name: value.name.clone(),
                raw: value.value.clone(),
                default: crate::config::Radix::Decimal,
            };
            let tree = value.tree.as_ref();
            let expandable = tree.is_some_and(|t| t.child_count > 0);
            let indent = "  ".repeat(node.path.len() + usize::from(node.more));
            let spans = if node.more {
                vec![Span::styled(
                    format!(
                        "{indent}  {}",
                        if row % 2 == 0 {
                            if tree.is_some_and(|t| t.limited) {
                                "Expansion limit reached"
                            } else {
                                "… Load more (Enter)"
                            }
                        } else {
                            ""
                        }
                    ),
                    Style::default().fg(theme::MUTED),
                )]
            } else if row % 2 == 0 {
                let arrow = match (
                    expandable,
                    tree.is_some_and(|t| t.expanded),
                    self.project.ui.unicode,
                ) {
                    (true, true, true) => "▾",
                    (true, false, true) => "▸",
                    (true, true, false) => "-",
                    (true, false, false) => "+",
                    _ => " ",
                };
                vec![
                    Span::styled(
                        format!("{indent}{arrow} {}", value.name),
                        Style::default().fg(theme::MUTED),
                    ),
                    Span::styled(
                        tree.map(|t| format!("  {}", t.type_name))
                            .unwrap_or_default(),
                        Style::default().fg(theme::DIM),
                    ),
                ]
            } else {
                let mut spans = vec![Span::raw(format!("{indent}    "))];
                spans.extend(self.numeric_spans(&item, value.changed, value.error));
                spans
            };
            let selected = self.pane == 1 && self.selected(1) / 2 == row / 2;
            let bg = if selected {
                theme::SELECTED
            } else {
                theme::PANEL
            };
            theme::lines(
                f,
                vec![Line::from(spans).style(Style::default().bg(bg))],
                hit,
            );
            if node.more || (expandable && row % 2 == 0) {
                let arrow = if node.more {
                    hit
                } else {
                    Rect::new(
                        hit.x + (indent.len() as u16).min(hit.width),
                        hit.y,
                        hit.width.saturating_sub(indent.len() as u16).min(2),
                        1,
                    )
                };
                self.watch.expand_hits.push((arrow, node.key()));
                // Value/name selection outside the disclosure arrow is unchanged.
                if !node.more && arrow.right() > hit.x {
                    item.rect.x = arrow.right();
                    item.rect.width = hit.right().saturating_sub(item.rect.x);
                }
            }
            if close_width > 0 {
                let close = Rect::new(hit.right(), hit.y, close_width, 1);
                let show = row % 2 == 0 || row == start;
                let enabled = self.watch.pending_remove.is_none() && self.pending_task.is_none();
                let hover = self.pointer.is_some_and(|p| close.contains(p));
                f.render_widget(
                    Paragraph::new(if !show {
                        "   "
                    } else if self.project.ui.unicode {
                        " × "
                    } else {
                        " x "
                    })
                    .style(
                        Style::default()
                            .fg(if !enabled {
                                theme::DIM
                            } else if hover {
                                theme::RED
                            } else {
                                theme::MUTED
                            })
                            .bg(if show && enabled && hover {
                                theme::HOVER
                            } else {
                                bg
                            }),
                    ),
                    close,
                );
                if show {
                    self.watch.remove_hits.push((close, node.root.into()));
                }
            }
            if !node.more {
                self.formats.hits.push(item);
            }
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
        if let Some((_, key)) = self
            .watch
            .expand_hits
            .iter()
            .find(|(r, _)| r.contains(point))
        {
            let index = rows(&self.snapshot.watches)
                .iter()
                .position(|v| v.key() == *key);
            if let Some(index) = index {
                self.select_pane(1);
                self.selection = index * 2;
                self.editing = false;
                self.watch_editing = false;
                self.completion.invalidate();
                self.toggle_watch(None, engine);
            }
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
            let Some(index) = rows(&self.snapshot.watches)
                .iter()
                .position(|v| v.removable() && v.root == name)
            else {
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
        let mut a = tree_app();
        a.fx.mode = crate::config::Motion::Off;
        super::super::visual_tests::capture(root, "watch-tree", &mut a, 160, 55);
        super::super::visual_tests::capture(root, "watch-tree-compact", &mut a, 80, 24);
    }

    fn tree_app() -> App {
        use crate::session::WatchTree;
        let mut a = app(&["outer", "counter"]);
        a.snapshot.watches[0].tree = Some(WatchTree {
            type_name: "struct Outer".into(),
            child_count: 2,
            expanded: true,
            children: vec![
                Variable {
                    name: "pair".into(),
                    value: "{...}".into(),
                    tree: Some(WatchTree {
                        path: vec![0],
                        type_name: "struct Pair".into(),
                        child_count: 1,
                        expanded: true,
                        children: vec![Variable {
                            name: "value".into(),
                            value: "53".into(),
                            tree: Some(WatchTree {
                                path: vec![0, 0],
                                type_name: "uint32_t".into(),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                Variable {
                    name: "items".into(),
                    value: "[70]".into(),
                    tree: Some(WatchTree {
                        path: vec![1],
                        type_name: "uint32_t [70]".into(),
                        child_count: 70,
                        expanded: true,
                        has_more: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            ],
            ..Default::default()
        });
        a
    }
    #[test]
    fn watch_tree_disclosure_mouse_and_keyboard_dispatch_paths() {
        let (engine, requests) = session::test_channel();
        let mut a = tree_app();
        render(&mut a);
        let arrow = a
            .watch
            .expand_hits
            .iter()
            .find(|(_, key)| key == "watch:outer")
            .unwrap()
            .0;
        click(&mut a, arrow, &engine);
        let request = requests.try_recv().unwrap();
        assert_eq!(request.method, "watch_expand");
        assert_eq!(
            request.params,
            json!({"expression":"outer","path":[],"expanded":false,"more":false})
        );
        a.selection = 2; // Nested pair, not the second root.
        key(&mut a, KeyCode::Right, &engine);
        assert_eq!(
            requests.try_recv().unwrap().params,
            json!({"expression":"outer","path":[0],"expanded":true,"more":false})
        );
        key(&mut a, KeyCode::Enter, &engine);
        assert_eq!(requests.try_recv().unwrap().params["expanded"], false);
        assert!(!a.watch_editing);
        a.selection = 10; // Scalar root still opens Watch input on Enter.
        key(&mut a, KeyCode::Enter, &engine);
        assert!(a.watch_editing);
        assert!(requests.try_recv().is_err());
    }
    #[test]
    fn watch_tree_delete_cannot_remove_a_sibling_root_from_a_child_row() {
        let (engine, requests) = session::test_channel();
        let mut a = tree_app();
        a.selection = 4;
        key(&mut a, KeyCode::Delete, &engine);
        assert!(requests.try_recv().is_err());
        assert!(!a.watch_removable());
        a.selection = 10;
        key(&mut a, KeyCode::Delete, &engine);
        assert_eq!(requests.try_recv().unwrap().params["expression"], "counter");
    }
    #[test]
    fn watch_tree_selection_follows_collapsed_ancestor_and_shifted_roots() {
        let mut a = tree_app();
        a.selection = 5;
        let mut snapshot = a.snapshot.clone();
        snapshot.watches[0].tree.as_mut().unwrap().expanded = false;
        a.update(Event::Snapshot {
            snapshot: Box::new(snapshot),
        });
        assert_eq!(a.selection, 1); // Same name/value parity, closest visible ancestor.
        assert_eq!(a.view_len(1), 4);
        a.selection = 3;
        let expanded = tree_app().snapshot;
        a.update(Event::Snapshot {
            snapshot: Box::new(expanded),
        });
        assert_eq!(a.selection, 11); // Root counter stayed selected after new rows appeared.
    }
    #[test]
    fn watch_tree_children_have_independent_radix_and_more_is_not_a_value() {
        let mut a = tree_app();
        a.selection = 4;
        a.open_format(None);
        let item = a.formats.popup.take().unwrap();
        assert_eq!(item.raw, "53");
        assert_eq!(
            item.key,
            format!("watch-child:{}", json!(["outer", [0, 0], false]))
        );
        a.project
            .ui
            .formats
            .insert(item.key.clone(), crate::config::Radix::Hex);
        assert_eq!(
            a.base_for(&item.key, crate::config::Radix::Decimal),
            crate::config::Radix::Hex
        );
        assert_eq!(
            a.base_for("watch:outer", crate::config::Radix::Decimal),
            crate::config::Radix::Decimal
        );
        a.formats.selected = Some(item.key);
        a.selection = 8; // Load more.
        a.open_format(None);
        assert!(a.formats.popup.is_none());
    }
    #[test]
    fn watch_tree_paging_and_running_expansion_guards() {
        let (engine, requests) = session::test_channel();
        let mut a = tree_app();
        a.selection = 8;
        key(&mut a, KeyCode::Enter, &engine);
        assert_eq!(
            requests.try_recv().unwrap().params,
            json!({"expression":"outer","path":[1],"expanded":true,"more":true})
        );
        a.snapshot.state = "RUNNING".into();
        key(&mut a, KeyCode::Right, &engine);
        assert!(requests.try_recv().is_err());
        a.selection = 0;
        key(&mut a, KeyCode::Left, &engine);
        assert_eq!(requests.try_recv().unwrap().params["expanded"], false);
    }
    #[test]
    fn watch_tree_render_clips_hits_and_closes_the_named_root_after_expansion() {
        let (engine, requests) = session::test_channel();
        let mut a = tree_app();
        for (width, height) in [(45, 12), (80, 24), (160, 55)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| draw(f, &mut a)).unwrap();
            a.set_view_top(1, 10);
            terminal.draw(|f| draw(f, &mut a)).unwrap();
            assert!(
                a.watch
                    .expand_hits
                    .iter()
                    .all(|(r, _)| r.right() <= a.view_rects[1].right())
            );
            assert!(
                a.watch
                    .remove_hits
                    .iter()
                    .all(|(_, name)| name == "outer" || name == "counter")
            );
            let hit = a
                .watch
                .remove_hits
                .iter()
                .find(|(_, name)| name == "counter")
                .unwrap()
                .0;
            click(&mut a, hit, &engine);
            let request = requests.try_recv().unwrap();
            assert_eq!(request.params["expression"], "counter");
            a.update(Event::Response {
                id: request.id,
                ok: false,
                result: Value::Null,
                error: Some("fixture".into()),
            });
        }
    }
}
