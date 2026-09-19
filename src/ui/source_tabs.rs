use super::*;

pub(super) struct SourceDocument {
    pub file: String,
    key: String,
    line: usize,
    top: usize,
}

#[derive(Default)]
pub(super) struct SourceTabs {
    pub documents: Vec<SourceDocument>,
    pub active: Option<usize>,
    pub frame_key: String,
    first: usize,
    end: usize,
    reveal: bool,
    width: u16,
    strip: Rect,
    pub(super) tabs: Vec<(Rect, usize)>,
    closes: Vec<(Rect, usize)>,
    previous: Rect,
    next: Rect,
    list_button: Rect,
    pub list_open: bool,
    pub query: String,
    pub list_index: usize,
    list_hits: Vec<(Rect, usize)>,
}

impl App {
    pub(super) fn source_key(&self, file: &str) -> String {
        let resolved = self
            .project
            .source_path(file)
            .and_then(|p| fs::canonicalize(p).ok());
        let path = resolved
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| file.into());
        let path = path.replace('\\', "/");
        if cfg!(windows) {
            path.to_lowercase()
        } else {
            path
        }
    }

    fn save_source_position(&mut self) {
        if let Some(index) = self.sources.active {
            let document = &mut self.sources.documents[index];
            document.line = self.source_line;
            document.top = self.source_top;
        }
    }

    pub(super) fn remember_demo_source(&mut self) {
        let key = self.source_key(&self.source_file);
        self.sources.frame_key = key.clone();
        self.sources.documents.push(SourceDocument {
            file: self.source_file.clone(),
            key,
            line: self.source_line,
            top: self.source_top,
        });
        self.sources.active = Some(0);
    }

    pub(super) fn hide_source(&mut self) {
        self.save_source_position();
        self.sources.active = None;
        self.source.clear();
        self.source_comments.clear();
        self.source_file.clear();
        self.source_line = 0;
        self.source_top = 0;
    }

    pub(super) fn load_source(&mut self, file: &str) {
        self.save_source_position();
        let key = self.source_key(file);
        let index =
            if let Some(index) = self.sources.documents.iter().position(|doc| doc.key == key) {
                // Keep the debugger's latest spelling for source breakpoints/source maps.
                self.sources.documents[index].file = file.into();
                index
            } else {
                self.sources.documents.push(SourceDocument {
                    file: file.into(),
                    key,
                    line: 0,
                    top: 0,
                });
                self.sources.documents.len() - 1
            };
        self.activate_source(index);
    }

    fn read_source(&self, file: &str) -> Vec<String> {
        if self.demo && file == "demo/sample.c" {
            return DEMO_SOURCE.lines().map(str::to_owned).collect();
        }
        if let Some(path) = self.project.source_path(file) {
            match fs::metadata(&path) {
                Ok(meta) if meta.len() > 2 * 1024 * 1024 => {
                    vec!["Source file exceeds 2 MiB; use an external editor.".into()]
                }
                Ok(_) => fs::read(&path)
                    .map(|bytes| {
                        String::from_utf8_lossy(&bytes)
                            .lines()
                            .take(30000)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_else(|e| vec![format!("Cannot read source: {e}")]),
                Err(e) => vec![format!("Cannot read source: {e}")],
            }
        } else {
            vec![
                format!("Source not found: {file}"),
                "Add [[source_map]] to debug.toml or use :open PATH.".into(),
                "Use the Assembly panel when source is unavailable.".into(),
            ]
        }
    }

    pub(super) fn activate_source(&mut self, index: usize) {
        if index >= self.sources.documents.len() {
            return;
        }
        self.save_source_position();
        if self.sources.active != Some(index) {
            self.fx.tab_file = self.sources.documents[index].file.clone();
            self.fx.trigger("tab", 650);
        }
        let document = &self.sources.documents[index];
        if self.sources.active != Some(index) || self.source_file != document.file {
            // Cache metadata only for inactive tabs; source text is bounded to one file.
            self.source = self.read_source(&document.file);
            self.source_comments = highlight::comment_starts(&self.source);
        }
        self.source_file = document.file.clone();
        self.source_line = document.line.min(self.source.len().saturating_sub(1));
        self.source_top = document.top;
        self.sources.active = Some(index);
        self.sources.reveal = true;
        self.scroll_drag = None;
    }

    pub(super) fn close_source(&mut self, index: usize) {
        if index >= self.sources.documents.len() {
            return;
        }
        self.save_source_position();
        let active = self.sources.active;
        self.sources.documents.remove(index);
        self.sources.active = active.and_then(|a| {
            if a == index {
                None
            } else {
                Some(a - usize::from(a > index))
            }
        });
        if active == Some(index) {
            if self.sources.documents.is_empty() {
                self.hide_source();
            } else {
                self.activate_source(index.min(self.sources.documents.len() - 1));
            }
        }
        self.sources.first = self
            .sources
            .first
            .min(self.sources.documents.len().saturating_sub(1));
        self.sources.reveal = true;
        self.sources.list_index = self
            .sources
            .list_index
            .min(self.filtered_sources().len().saturating_sub(1));
        self.scroll_drag = None;
    }

    pub(super) fn cycle_source(&mut self, delta: isize) {
        let count = self.sources.documents.len();
        if count == 0 {
            return;
        }
        let index = self.sources.active.map_or(0, |i| {
            (i as isize + delta).rem_euclid(count as isize) as usize
        });
        self.activate_source(index);
        self.select_pane(0);
    }

    pub(super) fn source_is_frame(&self) -> bool {
        self.sources.active.is_some_and(|index| {
            let document = &self.sources.documents[index];
            document.file == self.source_file && document.key == self.sources.frame_key
        })
    }

    pub(super) fn open_source_list(&mut self) {
        self.sources.list_open = true;
        self.sources.query.clear();
        self.sources.list_index = self.sources.active.unwrap_or(0);
        self.editing = false;
        self.watch_editing = false;
        self.completion.invalidate();
        self.scroll_drag = None;
    }

    fn filtered_sources(&self) -> Vec<usize> {
        let query = self.sources.query.replace('\\', "/").to_lowercase();
        self.sources
            .documents
            .iter()
            .enumerate()
            .filter_map(|(i, doc)| {
                doc.file
                    .replace('\\', "/")
                    .to_lowercase()
                    .contains(&query)
                    .then_some(i)
            })
            .collect()
    }

    pub(super) fn source_list_key(&mut self, key: KeyEvent) {
        let filtered = self.filtered_sources();
        let max = filtered.len().saturating_sub(1);
        match key.code {
            KeyCode::Esc => self.sources.list_open = false,
            KeyCode::Down => self.sources.list_index = (self.sources.list_index + 1).min(max),
            KeyCode::Up => self.sources.list_index = self.sources.list_index.saturating_sub(1),
            KeyCode::PageDown => self.sources.list_index = (self.sources.list_index + 10).min(max),
            KeyCode::PageUp => self.sources.list_index = self.sources.list_index.saturating_sub(10),
            KeyCode::Home => self.sources.list_index = 0,
            KeyCode::End => self.sources.list_index = max,
            KeyCode::Enter => {
                if let Some(&index) = filtered.get(self.sources.list_index) {
                    self.activate_source(index);
                    self.select_pane(0);
                    self.sources.list_open = false;
                }
            }
            KeyCode::Delete => {
                if let Some(&index) = filtered.get(self.sources.list_index) {
                    self.close_source(index);
                }
            }
            KeyCode::Backspace => {
                self.sources.query.pop();
                self.sources.list_index = 0;
            }
            KeyCode::Char(ch)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.sources.query.push(ch);
                self.sources.list_index = 0;
            }
            _ => {}
        }
    }

    pub(super) fn source_tabs_mouse(&mut self, mouse: MouseEvent) -> bool {
        if self.palette {
            return false;
        }
        let point = (mouse.column, mouse.row).into();
        if self.sources.list_open {
            match mouse.kind {
                MouseEventKind::ScrollDown => {
                    self.source_list_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
                }
                MouseEventKind::ScrollUp => {
                    self.source_list_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
                }
                MouseEventKind::Down(event::MouseButton::Left) => {
                    if let Some(&(_, index)) = self
                        .sources
                        .list_hits
                        .iter()
                        .find(|(rect, _)| rect.contains(point))
                    {
                        self.activate_source(index);
                        self.select_pane(0);
                        self.sources.list_open = false;
                    }
                }
                _ => {}
            }
            return true;
        }
        if !self.sources.strip.contains(point) {
            return false;
        }
        match mouse.kind {
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                let delta = if mouse.kind == MouseEventKind::ScrollDown {
                    1
                } else {
                    -1
                };
                self.sources.first = self
                    .sources
                    .first
                    .saturating_add_signed(delta)
                    .min(self.sources.documents.len().saturating_sub(1));
                self.sources.reveal = false;
            }
            MouseEventKind::Down(event::MouseButton::Left) => {
                self.editing = false;
                self.watch_editing = false;
                self.completion.invalidate();
                self.scroll_drag = None;
                if let Some(&(_, index)) = self
                    .sources
                    .closes
                    .iter()
                    .find(|(rect, _)| rect.contains(point))
                {
                    self.close_source(index);
                } else if let Some(&(_, index)) = self
                    .sources
                    .tabs
                    .iter()
                    .find(|(rect, _)| rect.contains(point))
                {
                    self.activate_source(index);
                    self.select_pane(0);
                } else if self.sources.list_button.contains(point) {
                    self.open_source_list();
                } else if self.sources.previous.contains(point) {
                    self.sources.first = self
                        .sources
                        .first
                        .saturating_sub((self.sources.end - self.sources.first).max(1));
                    self.sources.reveal = false;
                } else if self.sources.next.contains(point)
                    && self.sources.end < self.sources.documents.len()
                {
                    self.sources.first = self.sources.end;
                    self.sources.reveal = false;
                }
            }
            _ => {}
        }
        true
    }
}

fn basename(file: &str) -> &str {
    file.rsplit(['/', '\\']).next().unwrap_or(file)
}

fn label(documents: &[SourceDocument], index: usize) -> String {
    let document = &documents[index];
    let name = basename(&document.file);
    if documents
        .iter()
        .enumerate()
        .any(|(i, d)| i != index && basename(&d.file) == name)
    {
        // Show the shortest path suffix that distinguishes identical basenames.
        let normalized = document.file.replace('\\', "/");
        let mut suffix = name.to_owned();
        for part in normalized.rsplit('/').skip(1) {
            suffix = format!("{part}/{suffix}");
            if !documents
                .iter()
                .enumerate()
                .any(|(i, d)| i != index && d.file.replace('\\', "/").ends_with(&suffix))
            {
                break;
            }
        }
        suffix
    } else {
        name.into()
    }
}

fn fit(text: &str, width: usize) -> String {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
    if text.width() <= width {
        return text.into();
    }
    if width == 0 {
        return String::new();
    }
    // Keep the filename end (and its extension) visible when shortening a path.
    let mut size = 1;
    let mut tail = Vec::new();
    for ch in text.chars().rev() {
        size += ch.width().unwrap_or(0);
        if size > width {
            break;
        }
        tail.push(ch);
    }
    format!("…{}", tail.into_iter().rev().collect::<String>())
}

fn tab_width(documents: &[SourceDocument], index: usize, available: u16) -> u16 {
    let width = unicode_width::UnicodeWidthStr::width(label(documents, index).as_str());
    (width.min(30) as u16 + 5).min(available)
}

fn visible_end(documents: &[SourceDocument], first: usize, width: u16) -> usize {
    if width < 5 {
        return first;
    }
    let mut used = 0;
    let mut end = first;
    while end < documents.len() {
        let next = tab_width(documents, end, width);
        if used + next > width {
            break;
        }
        used += next;
        end += 1;
    }
    end
}

pub(super) fn reset_hits(a: &mut App) {
    a.sources.strip = Rect::default();
    a.sources.tabs.clear();
    a.sources.closes.clear();
    a.sources.previous = Rect::default();
    a.sources.next = Rect::default();
    a.sources.list_button = Rect::default();
    a.sources.list_hits.clear();
}

pub(super) fn draw_tabs(f: &mut UiFrame, a: &mut App, rect: Rect) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let count = a.sources.documents.len();
    let list = format!(" Files {count} ");
    let controls = list.len() as u16 + 8;
    let width = rect.width.saturating_sub(controls);
    if a.sources.width != width {
        a.sources.reveal = true;
        a.sources.width = width;
    }
    a.sources.first = a.sources.first.min(count.saturating_sub(1));
    if a.sources.reveal
        && let Some(active) = a.sources.active
        && (active < a.sources.first
            || visible_end(&a.sources.documents, a.sources.first, width) <= active)
    {
        a.sources.first = active;
        let mut used = tab_width(&a.sources.documents, active, width);
        while a.sources.first > 0 {
            let previous = tab_width(&a.sources.documents, a.sources.first - 1, width);
            if used + previous > width {
                break;
            }
            used += previous;
            a.sources.first -= 1;
        }
    }
    a.sources.reveal = false;
    a.sources.end = visible_end(&a.sources.documents, a.sources.first, width);
    a.sources.strip = rect;
    theme::surface(f, rect, theme::CANVAS);
    let mut x = rect.x;
    for index in a.sources.first..a.sources.end {
        let tab_width = tab_width(&a.sources.documents, index, width);
        let name = fit(
            &label(&a.sources.documents, index),
            tab_width.saturating_sub(5) as usize,
        );
        let current = a.sources.documents[index].key == a.sources.frame_key;
        let text = format!("{}{name}", if current { "▶" } else { " " });
        let hit = Rect::new(x, rect.y, tab_width, 1);
        let selected = a.sources.active == Some(index);
        let hover = a.pointer.is_some_and(|p| hit.contains(p));
        let style = theme::chip(selected, hover);
        f.render_widget(Paragraph::new(text).style(style), hit);
        let close = Rect::new(hit.right().saturating_sub(3), rect.y, 1, 1);
        f.render_widget(
            Paragraph::new("×").style(style.fg(if a.pointer.is_some_and(|p| close.contains(p)) {
                theme::RED
            } else {
                theme::MUTED
            })),
            close,
        );
        a.sources.tabs.push((hit, index));
        a.sources.closes.push((close, index));
        x += tab_width;
    }
    if count == 0 && width >= 10 {
        f.render_widget(
            Paragraph::new(" No open files ").style(Style::default().fg(theme::DIM)),
            Rect::new(rect.x, rect.y, width, 1),
        );
    }
    if rect.width >= controls {
        let start = rect.right() - controls;
        a.sources.previous = Rect::new(start, rect.y, 3, 1);
        a.sources.next = Rect::new(start + 4, rect.y, 3, 1);
        a.sources.list_button = Rect::new(start + 8, rect.y, list.len() as u16, 1);
        for (hit, text, enabled) in [
            (a.sources.previous, " ‹ ", a.sources.first > 0),
            (a.sources.next, " › ", a.sources.end < count),
            (a.sources.list_button, list.as_str(), true),
        ] {
            f.render_widget(
                Paragraph::new(text).style(
                    theme::chip(false, a.pointer.is_some_and(|p| hit.contains(p))).fg(if enabled {
                        theme::ACCENT
                    } else {
                        theme::DIM
                    }),
                ),
                hit,
            );
        }
    }
}

pub(super) fn draw_list(f: &mut UiFrame, a: &mut App) {
    if !a.sources.list_open {
        return;
    }
    let rect = center(f.area(), 110, 24);
    theme::overlay(f, rect);
    let block = theme::card(
        "  ≡  Open files · Enter: switch · Delete: close · Esc  ",
        true,
    );
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inner);
    f.render_widget(
        Paragraph::new(format!(" / Filter: {}", a.sources.query))
            .style(Style::default().fg(theme::ACCENT).bg(theme::RAISED)),
        rows[0],
    );
    let filtered = a.filtered_sources();
    a.sources.list_index = a.sources.list_index.min(filtered.len().saturating_sub(1));
    let first = a
        .sources
        .list_index
        .saturating_sub(rows[1].height.saturating_sub(1) as usize);
    if filtered.is_empty() {
        f.render_widget(Paragraph::new("No matching open files."), rows[1]);
    }
    for (row, (position, &index)) in filtered
        .iter()
        .enumerate()
        .skip(first)
        .take(rows[1].height as usize)
        .enumerate()
    {
        let hit = Rect::new(rows[1].x, rows[1].y + row as u16, rows[1].width, 1);
        let active = a.sources.active == Some(index);
        let text = format!(
            "{}{} {}",
            if position == a.sources.list_index {
                "›"
            } else {
                " "
            },
            if active { "*" } else { " " },
            fit(
                &a.sources.documents[index].file,
                hit.width.saturating_sub(3) as usize
            )
        );
        f.render_widget(
            Paragraph::new(text).style(theme::selected(position == a.sources.list_index)),
            hit,
        );
        a.sources.list_hits.push((hit, index));
    }
    f.render_widget(
        Paragraph::new(format!(
            "{} / {} files · type to filter · ↑ ↓ PgUp PgDn",
            filtered.len(),
            a.sources.documents.len()
        ))
        .style(Style::default().fg(theme::DIM)),
        rows[2],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(a: &mut App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| draw(f, a)).unwrap();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                    .collect::<String>()
                    + "\n"
            })
            .collect()
    }

    fn click(a: &mut App, rect: Rect, engine: Option<&EngineHandle>) {
        a.mouse(
            MouseEvent {
                kind: MouseEventKind::Down(event::MouseButton::Left),
                column: rect.x,
                row: rect.y,
                modifiers: KeyModifiers::NONE,
            },
            engine,
        );
    }

    fn fixture() -> (App, String, String) {
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("artifacts")
            .join(format!("source-tabs-{}-{id}", std::process::id()));
        fs::create_dir_all(root.join("one")).unwrap();
        fs::create_dir_all(root.join("two")).unwrap();
        for folder in ["one", "two"] {
            fs::write(
                root.join(folder).join("main.c"),
                (0..200)
                    .map(|i| format!("// {folder} row {i}\n"))
                    .collect::<String>(),
            )
            .unwrap();
        }
        (
            App::new(Project::default(), false),
            root.join("one/main.c").to_string_lossy().into(),
            root.join("two/main.c").to_string_lossy().into(),
        )
    }

    fn stop(a: &mut App, file: &str, line: u32) {
        let mut snapshot = a.snapshot.clone();
        snapshot.state = "STOPPED".into();
        snapshot.generation += 1;
        snapshot.frame = Frame {
            file: file.into(),
            line,
            address: "0x08000100".into(),
            function: "main".into(),
            ..Default::default()
        };
        a.update(Event::Snapshot {
            snapshot: Box::new(snapshot),
        });
    }

    #[test]
    fn stop_files_and_open_share_tabs_restore_positions_and_deduplicate_paths() {
        let (mut a, first, second) = fixture();
        stop(&mut a, &first, 37);
        a.source_line = 80;
        a.source_top = 72;
        stop(&mut a, &second, 10);
        assert_eq!(a.sources.documents.len(), 2);
        assert_eq!(a.source_line, 9);
        assert_eq!(a.sources.active, Some(1));
        a.command(None, &format!(":open \"{first}\""));
        assert_eq!((a.source_line, a.source_top), (80, 72));
        let text = render(&mut a, 160, 45);
        assert!(text.contains("one/main.c"));
        assert!(text.contains("two/main.c"));
        assert!(!a.source_is_frame());
        a.snapshot.files = vec![second.clone()];
        a.select_pane(7);
        a.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), None);
        assert_eq!(a.source_file, second);
        assert_eq!((a.source_line, a.source_top), (9, 1));
        let alias = Path::new(&first)
            .parent()
            .unwrap()
            .join("./main.c")
            .to_string_lossy()
            .into_owned();
        a.load_source(&alias);
        assert_eq!(a.sources.documents.len(), 2);
        assert_eq!((a.source_line, a.source_top), (80, 72));
        let mut frame = a.snapshot.clone();
        frame.frame.file = first.clone();
        frame.frame.level = 1;
        frame.frame.line = 45;
        a.update(Event::Snapshot {
            snapshot: Box::new(frame),
        });
        assert_eq!(a.sources.active, Some(0));
        assert_eq!(a.source_line, 44);
        assert!(a.source_is_frame());
        a.activate_source(1);
        assert!(!a.source_is_frame());
        assert_eq!(a.snapshot.frame.file, first); // browsing never selects a GDB frame
        let (engine, requests) = session::test_channel();
        a.load_source(&alias);
        a.source_line = 44;
        a.snapshot.breakpoints.push(session::Breakpoint {
            id: "17".into(),
            location: format!("{first}:45"),
            kind: "breakpoint".into(),
            enabled: true,
            temporary: false,
            file: first,
            line: 45,
            ..Default::default()
        });
        a.toggle_break(Some(&engine));
        let request = requests.try_recv().unwrap();
        assert_eq!(request.method, "delete_break");
        assert_eq!(request.params["number"], "17");
    }

    #[test]
    fn closing_tabs_is_ui_only_and_unknown_stops_keep_open_documents() {
        let (engine, requests) = session::test_channel();
        let (mut a, first, second) = fixture();
        stop(&mut a, &first, 12);
        stop(&mut a, &second, 17);
        let mut snapshot = a.snapshot.clone();
        snapshot.generation += 1;
        snapshot.frame = Frame {
            address: "0x1234".into(),
            ..Default::default()
        };
        a.update(Event::Snapshot {
            snapshot: Box::new(snapshot),
        });
        let text = render(&mut a, 160, 45);
        assert!(text.contains("No source location for PC 0x1234"));
        assert_eq!(a.sources.documents.len(), 2);
        assert_eq!(a.sources.active, None);
        let tab = a.sources.tabs[0].0;
        click(&mut a, tab, Some(&engine));
        assert_eq!(a.source_file, first);
        assert_eq!(a.source_line, 11);
        render(&mut a, 160, 45);
        let close = a.sources.closes.iter().find(|(_, i)| *i == 0).unwrap().0;
        click(&mut a, close, Some(&engine));
        assert_eq!(a.source_file, second);
        assert_eq!(a.source_line, 16);
        a.key(
            KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
            Some(&engine),
        );
        assert!(a.sources.documents.is_empty());
        assert!(a.source_file.is_empty());
        assert!(requests.try_recv().is_err());
        stop(&mut a, &first, 25);
        assert_eq!(a.sources.documents.len(), 1);
        a.close_source(0);
        a.update(Event::Snapshot {
            snapshot: Box::new(a.snapshot.clone()),
        });
        assert!(a.sources.documents.is_empty()); // ordinary refresh must not reopen a closed tab
        assert!(render(&mut a, 120, 36).contains("No source file selected"));
        stop(&mut a, &first, 26);
        assert_eq!(a.source_line, 25); // a new stop deliberately opens its file again
    }

    #[test]
    fn many_tabs_resize_page_filter_switch_and_close_without_hiding_active_file() {
        let mut a = App::new(Project::default(), false);
        for index in 0..120 {
            a.load_source(&format!("virtual/path-{index:03}/main.c"));
        }
        for (w, h) in [(160, 45), (80, 24), (45, 12), (120, 36)] {
            render(&mut a, w, h);
            assert!(a.sources.tabs.iter().any(|(_, index)| *index == 119));
            assert!(
                a.sources
                    .tabs
                    .iter()
                    .all(|(rect, _)| rect.right() <= a.sources.previous.x)
            );
            assert!(
                a.sources
                    .closes
                    .iter()
                    .all(|(rect, _)| rect.width == 1 && rect.right() <= a.sources.strip.right())
            );
        }
        let prior = a.sources.first;
        let previous = a.sources.previous;
        click(&mut a, previous, None);
        render(&mut a, 120, 36);
        assert!(a.sources.first < prior);
        assert_eq!(a.sources.active, Some(119)); // browsing the strip doesn't switch files
        let next = a.sources.next;
        click(&mut a, next, None);
        render(&mut a, 120, 36);
        assert!(a.sources.tabs.iter().any(|(_, index)| *index == 119));
        let list = a.sources.list_button;
        click(&mut a, list, None);
        for ch in "path-003".chars() {
            a.key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE), None);
        }
        let text = render(&mut a, 80, 24);
        assert!(text.contains("1 / 120 files"));
        let hit = a.sources.list_hits[0].0;
        click(&mut a, hit, None);
        assert_eq!(a.sources.active, Some(3));
        assert!(!a.sources.list_open);
        render(&mut a, 80, 24);
        assert!(a.sources.tabs.iter().any(|(_, i)| *i == 3));
        a.key(
            KeyEvent::new(KeyCode::PageDown, KeyModifiers::CONTROL),
            None,
        );
        assert_eq!(a.sources.active, Some(4));
        a.key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::CONTROL), None);
        assert_eq!(a.sources.active, Some(3));
        a.open_source_list();
        a.sources.query = "path-003".into();
        a.sources.list_index = 0;
        a.source_list_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE));
        assert_eq!(a.sources.documents.len(), 119);
        assert!(a.filtered_sources().is_empty());
        assert_eq!(a.source_file, "virtual/path-004/main.c");
    }

    #[test]
    fn unicode_tab_close_hit_and_sidebar_divider_render_at_their_actual_cells() {
        let mut a = App::new(Project::default(), false);
        a.load_source("工程/很长的源码文件名字_初始化.c");
        let text = render(&mut a, 120, 36);
        assert!(text.contains("×"));
        let close = a.sources.closes[0].0;
        let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
        terminal.draw(|f| draw(f, &mut a)).unwrap();
        assert_eq!(
            terminal.backend().buffer()[(close.x, close.y)].symbol(),
            "×"
        );
        let watch = a.pane_hits.iter().find(|(_, pane)| *pane == 1).unwrap().0;
        for x in watch.x..120 {
            assert_eq!(terminal.backend().buffer()[(x, watch.y - 1)].symbol(), "─");
        }
        click(&mut a, close, None);
        assert!(a.sources.documents.is_empty());
    }

    #[test]
    fn write_open_files_previews_when_requested() {
        let Ok(root) = std::env::var("DEBUGTUI_RENDER_DIR") else {
            return;
        };
        fs::create_dir_all(&root).unwrap();
        let mut a = App::new(Project::default(), true);
        for file in [
            "Core/Src/main.c",
            "BSP/USER_TASK/Src/user_task.c",
            "FreeRTOS/tasks.c",
            "one/main.c",
            "two/main.c",
        ] {
            a.load_source(file);
        }
        a.activate_source(0);
        fs::write(
            Path::new(&root).join("source-tabs.txt"),
            render(&mut a, 160, 45),
        )
        .unwrap();
        fs::write(
            Path::new(&root).join("source-tabs-narrow.txt"),
            render(&mut a, 80, 24),
        )
        .unwrap();
        a.open_source_list();
        fs::write(
            Path::new(&root).join("open-files.txt"),
            render(&mut a, 120, 36),
        )
        .unwrap();
    }
}
