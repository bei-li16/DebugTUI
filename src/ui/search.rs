//! Files filtering and asynchronous ELF symbol navigation, independent of execution.
use super::*;
use crate::session::Symbol;

#[derive(Default)]
pub(super) struct FileSearch {
    pub query: String,
    pub editing: bool,
    pub area: Rect,
}

#[derive(Clone, PartialEq, Eq)]
struct SearchKey {
    query: String,
    core: Option<usize>,
    epoch: u64,
}

pub(super) struct SymbolSearch {
    pub open: bool,
    pub query: String,
    pub bar: Rect,
    items: Vec<Symbol>,
    selected: usize,
    hits: Vec<(Rect, usize)>,
    changed: Instant,
    epoch: u64,
    requested: Option<SearchKey>,
    pending: Option<(u64, SearchKey)>,
    hint: String,
}

impl Default for SymbolSearch {
    fn default() -> Self {
        Self {
            open: false,
            query: String::new(),
            bar: Rect::default(),
            items: vec![],
            selected: 0,
            hits: vec![],
            changed: Instant::now(),
            epoch: 0,
            requested: None,
            pending: None,
            hint: String::new(),
        }
    }
}

impl SymbolSearch {
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn invalidate(&mut self) {
        self.epoch += 1;
        self.items.clear();
        self.hits.clear();
        self.selected = 0;
        self.requested = None;
        self.hint.clear();
        self.changed = Instant::now();
        // Retain the pending ID until its stale response arrives: never flood
        // the worker with a new request for every keystroke or core change.
    }
}

impl App {
    pub(super) fn filtered_files(&self) -> Vec<usize> {
        if self.file_search.query.trim().is_empty() {
            return (0..self.snapshot.files.len()).collect();
        }
        let mut found: Vec<_> = self
            .snapshot
            .files
            .iter()
            .enumerate()
            .filter_map(|(i, path)| {
                let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
                crate::search::score(name, &self.file_search.query)
                    .or_else(|| {
                        crate::search::score(path, &self.file_search.query).map(|s| s + 100_000)
                    })
                    .map(|score| (score, i))
            })
            .collect();
        found.sort();
        found.into_iter().map(|(_, i)| i).collect()
    }

    fn filter_changed(&mut self) {
        self.selection = 0;
        self.selections[7] = 0;
        self.view_tops[7] = 0;
        self.scroll_drag = None;
    }

    pub(super) fn file_search_key(&mut self, key: KeyEvent) -> bool {
        if self.pane != 7 || self.editing || self.watch_editing || self.console_view.focused {
            return false;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if (ctrl && key.code == KeyCode::Char('f'))
            || (!self.file_search.editing && key.code == KeyCode::Char('/'))
        {
            self.file_search.editing = true;
            self.completion.invalidate();
            return true;
        }
        if !self.file_search.editing {
            return false;
        }
        match key.code {
            KeyCode::Esc => {
                self.file_search.editing = false;
                return true;
            }
            KeyCode::Enter | KeyCode::Tab | KeyCode::BackTab => {
                self.file_search.editing = false;
                return false;
            }
            KeyCode::Char('u') if ctrl => self.file_search.query.clear(),
            KeyCode::Backspace => {
                self.file_search.query.pop();
            }
            KeyCode::Char(ch)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                if self.file_search.query.chars().count() < 128 {
                    self.file_search.query.push(ch);
                }
            }
            // Keep list navigation and debugger function keys working while filtering.
            _ => return false,
        }
        self.filter_changed();
        true
    }

    pub(super) fn open_symbol_search(&mut self) {
        self.symbol_search.open = true;
        self.editing = false;
        self.watch_editing = false;
        self.file_search.editing = false;
        self.console_view.focused = false;
        self.sources.list_open = false;
        self.source_text.reset(self.source_line);
        if self.symbol_search.items.is_empty() {
            self.symbol_search.requested = None;
        }
        self.completion.invalidate();
        self.scroll_drag = None;
    }

    fn symbol_key(&self) -> SearchKey {
        SearchKey {
            query: self.symbol_search.query.trim().into(),
            core: self.snapshot.core.as_ref().map(|c| c.index),
            epoch: self.symbol_search.epoch,
        }
    }

    pub(super) fn ensure_symbol_search(&mut self, engine: Option<&EngineHandle>) -> bool {
        if !self.symbol_search.open || self.setup.is_some() || self.quitting {
            return false;
        }
        let key = self.symbol_key();
        if key.query.is_empty() {
            return false;
        }
        if self.symbol_search.requested.as_ref() == Some(&key) {
            return false;
        }
        if !self.demo && !matches!(self.snapshot.state.as_str(), "READY" | "STOPPED") {
            let hint = "Pause/connect to search ELF symbols; existing results remain usable.";
            let changed = self.symbol_search.hint != hint;
            self.symbol_search.hint = hint.into();
            return changed;
        }
        if self.symbol_search.busy()
            || self.symbol_search.changed.elapsed() < Duration::from_millis(200)
            || !self.pending_commands.is_empty()
            || self.pending_view.is_some()
            || self.pending_task.is_some()
            || self.completion.busy()
            || self.monitor.busy()
        {
            return false;
        }
        if self.demo {
            self.symbol_search.items = [
                ("counter", "variable", 5),
                ("process_items", "function", 8),
                ("update_value", "function", 17),
            ]
            .into_iter()
            .filter(|(name, _, _)| crate::search::score(name, &key.query).is_some())
            .map(|(name, kind, line)| Symbol {
                name: name.into(),
                kind: kind.into(),
                line,
                file: "demo/sample.c".into(),
                description: String::new(),
            })
            .collect();
            self.symbol_search.requested = Some(key);
            self.symbol_search.hint = "Demo symbols".into();
            return true;
        }
        let Some(engine) = engine else {
            return false;
        };
        let id = self.next_id;
        self.next_id += 1;
        self.symbol_search.requested = Some(key.clone());
        match engine.send(Request::new(id, "symbols", json!({"query":key.query}))) {
            Ok(()) => {
                self.symbol_search.pending = Some((id, key));
                self.symbol_search.hint = "Searching ELF debug symbols...".into();
            }
            Err(error) => self.symbol_search.hint = error,
        }
        true
    }

    pub(super) fn symbol_response(&mut self, id: u64, result: &Value, error: Option<&str>) -> bool {
        if self
            .symbol_search
            .pending
            .as_ref()
            .is_none_or(|(pending, _)| *pending != id)
        {
            return false;
        }
        let (_, key) = self.symbol_search.pending.take().unwrap();
        if !self.symbol_search.open || self.symbol_key() != key {
            return true;
        }
        self.symbol_search.items = result
            .get("symbols")
            .cloned()
            .and_then(|v| serde_json::from_value::<Vec<Symbol>>(v).ok())
            .unwrap_or_default();
        self.symbol_search.selected = 0;
        self.symbol_search.hint = if let Some(error) = error {
            format!("{error} · F4 retries")
        } else if let Some(warnings) = result["warnings"].as_array().filter(|w| !w.is_empty()) {
            format!(
                "Partial results: {}",
                warnings
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        } else if result["truncated"] == true {
            "Result limit reached (200 per kind); refine the query.".into()
        } else if self.symbol_search.items.is_empty() {
            "No matching debug symbols. Check the ELF / debug information.".into()
        } else {
            "Functions, global/static variables and types · Enter/click opens source".into()
        };
        true
    }

    fn open_symbol_result(&mut self) {
        let Some(symbol) = self
            .symbol_search
            .items
            .get(self.symbol_search.selected)
            .cloned()
        else {
            return;
        };
        if symbol.file.is_empty() || symbol.line == 0 {
            self.symbol_search.hint = "This symbol has no source location in the ELF.".into();
            return;
        }
        self.load_source(&symbol.file);
        self.source_line = (symbol.line as usize - 1).min(self.source.len().saturating_sub(1));
        self.source_top = self.source_line.saturating_sub(8);
        self.source_text.reset(self.source_line);
        self.select_pane(0);
        self.symbol_search.open = false;
        self.notice = format!("{} · {}:{}", symbol.name, symbol.file, symbol.line);
    }

    pub(super) fn symbol_search_key(
        &mut self,
        key: KeyEvent,
        engine: Option<&EngineHandle>,
    ) -> bool {
        if !self.symbol_search.open {
            return false;
        }
        let max = self.symbol_search.items.len().saturating_sub(1);
        match key.code {
            KeyCode::Esc => {
                self.symbol_search.open = false;
                if self.symbol_search.busy() {
                    self.symbol_search.invalidate();
                }
            }
            KeyCode::Enter => self.open_symbol_result(),
            KeyCode::Down => {
                self.symbol_search.selected = (self.symbol_search.selected + 1).min(max)
            }
            KeyCode::Up => {
                self.symbol_search.selected = self.symbol_search.selected.saturating_sub(1)
            }
            KeyCode::PageDown => {
                self.symbol_search.selected = (self.symbol_search.selected + 10).min(max)
            }
            KeyCode::PageUp => {
                self.symbol_search.selected = self.symbol_search.selected.saturating_sub(10)
            }
            KeyCode::Home => self.symbol_search.selected = 0,
            KeyCode::End => self.symbol_search.selected = max,
            KeyCode::F(6) => self.submit(engine, "pause", json!({})),
            KeyCode::F(4) => self.symbol_search.invalidate(),
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.symbol_search.query.clear();
                self.symbol_search.invalidate();
            }
            KeyCode::Backspace => {
                self.symbol_search.query.pop();
                self.symbol_search.invalidate();
            }
            KeyCode::Char(ch)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                    && self.symbol_search.query.chars().count() < 128 =>
            {
                self.symbol_search.query.push(ch);
                self.symbol_search.invalidate();
            }
            _ => {}
        }
        true
    }

    pub(super) fn search_paste(&mut self, text: &str) -> bool {
        if self.help
            || self.palette
            || self.sources.list_open
            || self.confirm.is_some()
            || self.monitor.modal()
            || self.breaks.modal()
            || self.formats.popup.is_some()
            || self.formats.appearance
        {
            return false;
        }
        let symbol = self.symbol_search.open;
        if !symbol && !self.file_search.editing {
            return false;
        }
        let query = if symbol {
            &mut self.symbol_search.query
        } else {
            &mut self.file_search.query
        };
        let available = 128usize.saturating_sub(query.chars().count());
        query.extend(text.chars().filter(|c| !c.is_control()).take(available));
        if symbol {
            self.symbol_search.invalidate();
        } else {
            self.filter_changed();
        }
        true
    }

    pub(super) fn search_mouse(&mut self, mouse: MouseEvent) -> bool {
        let point = (mouse.column, mouse.row).into();
        if self.symbol_search.open {
            match mouse.kind {
                MouseEventKind::Down(event::MouseButton::Left) => {
                    if let Some((_, index)) = self
                        .symbol_search
                        .hits
                        .iter()
                        .find(|(r, _)| r.contains(point))
                    {
                        self.symbol_search.selected = *index;
                        self.open_symbol_result();
                    }
                }
                MouseEventKind::ScrollDown => {
                    self.symbol_search.selected = (self.symbol_search.selected + 3)
                        .min(self.symbol_search.items.len().saturating_sub(1))
                }
                MouseEventKind::ScrollUp => {
                    self.symbol_search.selected = self.symbol_search.selected.saturating_sub(3)
                }
                _ => {}
            }
            return true;
        }
        if mouse.kind == MouseEventKind::Down(event::MouseButton::Left) {
            if self.symbol_search.bar.contains(point) {
                self.open_symbol_search();
                return true;
            }
            self.file_search.editing = self.file_search.area.contains(point);
            if self.file_search.editing {
                self.select_pane(7);
                self.editing = false;
                self.watch_editing = false;
                self.console_view.focused = false;
                self.completion.invalidate();
                return true;
            }
        }
        false
    }
}

pub(super) fn file_bar(f: &mut UiFrame, a: &mut App, rect: Rect) {
    a.file_search.area = rect;
    super::render::input_box(
        f,
        rect,
        "Find: files",
        &a.file_search.query,
        a.file_search.editing,
        "Ctrl+F / click: file name or path...",
        "Up/Down select · Enter open · Ctrl+U clear",
    );
}

pub(super) fn symbol_bar(f: &mut UiFrame, a: &mut App, rect: Rect) {
    a.symbol_search.bar = rect;
    super::render::input_box(
        f,
        rect,
        "Symbols",
        &a.symbol_search.query,
        false,
        "Ctrl+K / click: function, variable, type...",
        "Ctrl+K  Search",
    );
}

pub(super) fn draw_symbols(f: &mut UiFrame, a: &mut App) {
    a.symbol_search.hits.clear();
    if !a.symbol_search.open {
        return;
    }
    let rect = center(f.area(), 118, 26);
    theme::overlay(f, rect);
    let title = if rect.width < 65 {
        " Symbols · Enter: open · Esc: close "
    } else {
        " Symbols · Enter: source · F4: retry · Esc: close "
    };
    let block = theme::card(title, true);
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let rows = Layout::vertical([
        Constraint::Length(if inner.height >= 12 { 3 } else { 1 }),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .split(inner);
    super::render::input_box(
        f,
        rows[0],
        "Find: symbols",
        &a.symbol_search.query,
        true,
        "Name contains / fuzzy matches...",
        "Ctrl+U clear",
    );
    f.render_widget(
        Paragraph::new(format!(
            " {} matches · kind / name / file:line",
            a.symbol_search.items.len()
        ))
        .style(Style::default().fg(theme::DIM)),
        rows[1],
    );
    let height = (rows[2].height as usize / 2).max(1);
    let start = a.symbol_search.selected.saturating_sub(height - 1);
    if a.symbol_search.query.trim().is_empty() {
        f.render_widget(Paragraph::new("Type a name, e.g. spi or uart_cnt.\nSearches symbols from the loaded ELF, not source text.").wrap(Wrap { trim: false }), rows[2]);
    }
    for (row, (index, symbol)) in a
        .symbol_search
        .items
        .iter()
        .enumerate()
        .skip(start)
        .take(height)
        .enumerate()
    {
        let y = rows[2].y.saturating_add(row as u16 * 2);
        if y >= rows[2].bottom() {
            break;
        }
        let hit = Rect::new(rows[2].x, y, rows[2].width, 2.min(rows[2].bottom() - y));
        let location = if symbol.file.is_empty() || symbol.line == 0 {
            "[no source location]".into()
        } else {
            format!("{}:{}", symbol.file, symbol.line)
        };
        let text = format!(
            "{} {:8} {}\n  {}",
            if index == a.symbol_search.selected {
                "›"
            } else {
                " "
            },
            symbol.kind,
            symbol.name,
            source_tabs::fit(&location, hit.width.saturating_sub(2) as usize)
        );
        f.render_widget(
            Paragraph::new(text).style(theme::selected(index == a.symbol_search.selected)),
            hit,
        );
        a.symbol_search.hits.push((hit, index));
    }
    f.render_widget(
        Paragraph::new(a.symbol_search.hint.clone())
            .style(Style::default().fg(theme::DIM))
            .wrap(Wrap { trim: false }),
        rows[3],
    );
}

#[cfg(test)]
mod tests;
