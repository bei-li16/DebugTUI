//! Presentation-only integer conversion: no target writes, global GDB radix, or MI queries.
use super::*;
use crate::config::{Motion, Radix};

pub(super) const BASES: [(Radix, &str); 4] = [
    (Radix::Binary, "2   Binary"),
    (Radix::Octal, "8   Octal"),
    (Radix::Decimal, "10  Decimal"),
    (Radix::Hex, "16  Hexadecimal"),
];
pub(super) fn number(raw: &str, radix: Radix) -> Option<String> {
    let raw = raw.trim();
    let end = raw.find(char::is_whitespace).unwrap_or(raw.len());
    let (token, suffix) = raw.split_at(end);
    // GDB scalar annotations are retained, but floats, enums and aggregate values stay natural.
    if !suffix.is_empty() && !matches!(suffix.trim_start().chars().next(), Some('\'' | '"' | '<')) {
        return None;
    }
    let negative = token.starts_with('-');
    let digits = token.strip_prefix(['-', '+']).unwrap_or(token);
    let (base, digits) = if let Some(s) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        (16, s)
    } else if let Some(s) = digits.strip_prefix("0b") {
        (2, s)
    } else if let Some(s) = digits.strip_prefix("0o") {
        (8, s)
    } else if digits.len() > 1 && digits.starts_with('0') {
        // GDB uses a leading zero for octal output.
        (8, digits)
    } else {
        (10, digits)
    };
    let n = u128::from_str_radix(digits, base).ok()?;
    let value = match radix {
        Radix::Binary => format!("0b{n:b}"),
        Radix::Octal => format!("0o{n:o}"),
        Radix::Decimal => n.to_string(),
        Radix::Hex => format!("0x{n:x}"),
    };
    Some(format!(
        "{}{value}{suffix}",
        if negative && n != 0 { "-" } else { "" }
    ))
}
pub(super) fn tag(base: Radix) -> &'static str {
    match base {
        Radix::Binary => "bin",
        Radix::Octal => "oct",
        Radix::Decimal => "dec",
        Radix::Hex => "hex",
    }
}
#[derive(Clone)]
pub(super) struct Item {
    pub rect: Rect,
    pub pane: usize,
    pub row: usize,
    pub key: String,
    pub name: String,
    pub raw: String,
    pub default: Radix,
}
#[derive(Default)]
pub(super) struct Formats {
    pub hits: Vec<Item>,
    pub selected: Option<String>,
    pub popup: Option<Item>,
    pub index: usize,
    pub menu_hits: Vec<(Rect, usize)>,
    pub appearance: bool,
    pub appearance_hits: Vec<(Rect, usize)>,
    pub pending_save: HashSet<u64>,
}
impl App {
    pub(super) fn base_for(&self, key: &str, default: Radix) -> Radix {
        self.project.ui.formats.get(key).copied().unwrap_or(default)
    }
    pub(super) fn numeric_key(&self, pane: usize, name: &str) -> String {
        match pane {
            1 => format!("watch:{name}"),
            9 => {
                let mut file = crate::config::portable_path(Path::new(&self.snapshot.frame.file));
                let mut root = crate::config::portable_path(&self.project.program.source_root);
                if cfg!(windows) {
                    file = file.to_lowercase();
                    root = root.to_lowercase();
                }
                let prefix = format!("{}/", root.trim_end_matches('/'));
                let file = file.strip_prefix(&prefix).unwrap_or(&file);
                format!("local:{file}:{}:{name}", self.snapshot.frame.function)
            }
            _ => format!("register:{name}"),
        }
    }
    pub(super) fn save_ui(&mut self, engine: Option<&EngineHandle>) {
        if let Some(engine) = engine {
            let id = self.next_id;
            self.next_id += 1;
            match engine.send(Request::new(id, "ui_preferences", json!(self.project.ui))) {
                Ok(()) => {
                    self.formats.pending_save.insert(id);
                }
                Err(e) => self.notice = format!("Error: cannot save display settings: {e}"),
            }
        }
    }
    pub(super) fn open_format(&mut self, item: Option<Item>) {
        let item = item.or_else(|| self.selected_numeric_item()).or_else(|| {
            self.formats.selected.as_ref().and_then(|key| {
                self.formats
                    .hits
                    .iter()
                    .find(|i| i.pane == self.pane && &i.key == key)
                    .cloned()
            })
        });
        let item = item.or_else(|| {
            self.formats
                .hits
                .iter()
                .find(|i| i.pane == self.pane && i.row == self.selected(self.pane))
                .cloned()
        });
        if let Some(item) = item {
            self.formats.index = BASES
                .iter()
                .position(|(b, _)| *b == self.base_for(&item.key, item.default))
                .unwrap_or(2);
            self.formats.popup = Some(item);
            self.editing = false;
            self.watch_editing = false;
            self.completion.invalidate();
        } else {
            self.notice = "Select a data value, then press f or right-click it.".into();
        }
    }
    fn selected_numeric_item(&self) -> Option<Item> {
        let pane = self.pane;
        let row = self.selected(pane);
        if pane == 10 {
            return self.peripheral_format_item(row);
        }
        if pane == 4 {
            let (address, raw) = self
                .memory_bytes()
                .into_iter()
                .nth(row * self.memory_columns())?;
            return Some(Item {
                rect: Rect::default(),
                pane,
                row,
                key: format!("memory:{address:x}"),
                name: format!("Byte at 0x{address:x}"),
                raw,
                default: Radix::Decimal,
            });
        }
        let vars = match pane {
            1 => &self.snapshot.watches,
            9 => &self.snapshot.locals,
            3 => &self.snapshot.registers,
            _ => return None,
        };
        let v = vars.get(if pane == 3 { row } else { row / 2 })?;
        Some(Item {
            rect: Rect::default(),
            pane,
            row,
            key: self.numeric_key(pane, &v.name),
            name: v.name.clone(),
            raw: v.value.clone(),
            default: if pane == 3 {
                Radix::Hex
            } else {
                Radix::Decimal
            },
        })
    }
    pub(super) fn open_appearance(&mut self) {
        self.formats.appearance = true;
        self.editing = false;
        self.watch_editing = false;
        self.completion.invalidate();
    }
    pub(super) fn apply_format(&mut self, engine: Option<&EngineHandle>) {
        if let Some(item) = self.formats.popup.take() {
            let base = BASES[self.formats.index].0;
            self.project.ui.formats.insert(item.key, base);
            self.notice = format!("{} · {} display", item.name, tag(base));
            self.save_ui(engine);
        }
    }
    pub(super) fn format_key(&mut self, key: KeyEvent, engine: Option<&EngineHandle>) -> bool {
        if self.formats.appearance {
            match key.code {
                KeyCode::Esc => self.formats.appearance = false,
                KeyCode::Char('0' | '1' | '2') => {
                    self.project.ui.animations = match key.code {
                        KeyCode::Char('0') => Motion::Off,
                        KeyCode::Char('2') => Motion::Full,
                        _ => Motion::Subtle,
                    };
                    self.save_ui(engine);
                }
                KeyCode::Char('u') => {
                    self.project.ui.unicode = !self.project.ui.unicode;
                    self.save_ui(engine);
                }
                _ => {}
            }
            return true;
        }
        if self.formats.popup.is_none() {
            return false;
        }
        match key.code {
            KeyCode::Esc => self.formats.popup = None,
            KeyCode::Up | KeyCode::BackTab => self.formats.index = (self.formats.index + 3) % 4,
            KeyCode::Down | KeyCode::Tab => self.formats.index = (self.formats.index + 1) % 4,
            KeyCode::Enter => self.apply_format(engine),
            _ => {}
        }
        true
    }
    pub(super) fn format_mouse(
        &mut self,
        mouse: MouseEvent,
        engine: Option<&EngineHandle>,
    ) -> bool {
        let point = (mouse.column, mouse.row).into();
        if self.formats.appearance {
            if mouse.kind == MouseEventKind::Down(event::MouseButton::Left)
                && let Some((_, i)) = self
                    .formats
                    .appearance_hits
                    .iter()
                    .find(|(r, _)| r.contains(point))
            {
                let code = ['0', '1', '2', 'u'][*i];
                self.format_key(
                    KeyEvent::new(KeyCode::Char(code), KeyModifiers::NONE),
                    engine,
                );
            }
            return true;
        }
        if self.formats.popup.is_some() {
            if mouse.kind == MouseEventKind::Down(event::MouseButton::Left)
                && let Some((_, i)) = self
                    .formats
                    .menu_hits
                    .iter()
                    .find(|(r, _)| r.contains(point))
            {
                self.formats.index = *i;
                self.apply_format(engine);
            }
            return true;
        }
        if matches!(
            mouse.kind,
            MouseEventKind::Down(event::MouseButton::Left | event::MouseButton::Right)
        ) && let Some(item) = self
            .formats
            .hits
            .iter()
            .find(|i| i.rect.contains(point))
            .cloned()
        {
            self.select_pane(item.pane);
            self.selection = item.row;
            self.formats.selected = Some(item.key.clone());
            if mouse.kind == MouseEventKind::Down(event::MouseButton::Right) {
                self.open_format(Some(item));
                return true;
            }
            // Peripheral left-click keeps expansion; other numeric cells simply select.
            if item.pane != 10 {
                self.editing = false;
                self.watch_editing = false;
                self.completion.invalidate();
                return true;
            }
        }
        false
    }
    pub(super) fn numeric_spans(
        &mut self,
        item: &Item,
        changed: bool,
        error: bool,
    ) -> Vec<Span<'static>> {
        let base = self.base_for(&item.key, item.default);
        let value = number(&item.raw, base).unwrap_or_else(|| item.raw.clone());
        let old = self.fx.observe(&item.key, &item.raw);
        let old = old.and_then(|s| number(&s, base));
        let aligned = base == Radix::Hex && old.as_ref().is_some_and(|s| s.len() == value.len());
        let stale = self.snapshot.state == "RUNNING";
        let fg = if error {
            theme::RED
        } else if stale {
            theme::DIM
        } else {
            theme::TEXT
        };
        let mut spans = value
            .chars()
            .enumerate()
            .map(|(i, ch)| {
                let digit_changed = aligned
                    && i < value.find(char::is_whitespace).unwrap_or(value.len())
                    && ch.is_ascii_hexdigit()
                    && old.as_ref().and_then(|s| s.chars().nth(i)) != Some(ch);
                Span::styled(
                    ch.to_string(),
                    Style::default().fg(if !stale && changed && (!aligned || digit_changed) {
                        theme::AMBER
                    } else {
                        fg
                    }),
                )
            })
            .collect::<Vec<_>>();
        spans.push(Span::styled(
            format!("  {}{}", if changed { "• " } else { "" }, tag(base)),
            Style::default().fg(theme::DIM),
        ));
        spans
    }
    pub(super) fn numeric_view(&mut self, f: &mut UiFrame, pane: usize, rect: Rect) {
        let vars = match pane {
            1 => &self.snapshot.watches,
            9 => &self.snapshot.locals,
            _ => &self.snapshot.registers,
        }
        .clone();
        let stride = if pane == 3 { 1 } else { 2 };
        let start = self.view_tops[pane];
        for row in start..start + rect.height as usize {
            let Some(v) = vars.get(row / stride) else {
                break;
            };
            let hit = Rect::new(rect.x, rect.y + (row - start) as u16, rect.width, 1);
            let item = Item {
                rect: hit,
                pane,
                row,
                key: self.numeric_key(pane, &v.name),
                name: v.name.clone(),
                raw: v.value.clone(),
                default: if pane == 3 {
                    Radix::Hex
                } else {
                    Radix::Decimal
                },
            };
            let spans = if stride == 2 && row % 2 == 0 {
                vec![Span::styled(
                    format!("  {}", v.name),
                    Style::default().fg(theme::MUTED),
                )]
            } else {
                let mut spans = vec![Span::styled(
                    if pane == 3 {
                        format!("  {:8} ", v.name)
                    } else {
                        "    ".into()
                    },
                    Style::default().fg(theme::MUTED),
                )];
                spans.extend(self.numeric_spans(&item, v.changed, v.error));
                spans
            };
            let selected = self.formats.selected.as_ref() == Some(&item.key)
                || (self.pane == pane && self.selected(pane) == row);
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
            self.formats.hits.push(item);
        }
    }
    pub(super) fn memory_columns(&self) -> usize {
        (self.view_rects[4].width.saturating_sub(12) as usize / 12).clamp(1, 16)
    }
    pub(super) fn memory_bytes(&self) -> Vec<(u64, String)> {
        self.snapshot
            .memory
            .iter()
            .flat_map(|line| {
                let mut parts = line.split_whitespace();
                let base =
                    u64::from_str_radix(parts.next().unwrap_or("").trim_start_matches("0x"), 16)
                        .unwrap_or(0);
                parts
                    .enumerate()
                    .map(move |(i, b)| (base + i as u64, format!("0x{b}")))
            })
            .collect()
    }
    pub(super) fn memory_view(&mut self, f: &mut UiFrame, rect: Rect) {
        let cols = self.memory_columns();
        let bytes = self.memory_bytes();
        for (row, chunk) in bytes
            .chunks(cols)
            .enumerate()
            .skip(self.view_tops[4])
            .take(rect.height as usize)
        {
            let y = rect.y + (row - self.view_tops[4]) as u16;
            f.render_widget(
                Paragraph::new(format!("{:08x} ", chunk[0].0))
                    .style(Style::default().fg(theme::MUTED)),
                Rect::new(rect.x, y, 10.min(rect.width), 1),
            );
            let mut x = rect.x + 10;
            for (address, raw) in chunk {
                if x >= rect.right() {
                    break;
                }
                let item = Item {
                    rect: Rect::new(x, y, 12.min(rect.right() - x), 1),
                    pane: 4,
                    row,
                    key: format!("memory:{address:x}"),
                    name: format!("Byte at 0x{address:x}"),
                    raw: raw.clone(),
                    default: Radix::Decimal,
                };
                let value = number(raw, self.base_for(&item.key, item.default))
                    .unwrap_or_else(|| raw.clone());
                f.render_widget(
                    Paragraph::new(value).style(theme::selected(
                        self.formats.selected.as_ref() == Some(&item.key),
                    )),
                    item.rect,
                );
                self.formats.hits.push(item);
                x += 12;
            }
        }
    }
}
pub(super) fn popup(f: &mut UiFrame, a: &mut App) {
    a.formats.menu_hits.clear();
    a.formats.appearance_hits.clear();
    if a.formats.appearance {
        let r = center(f.area(), 68, 14);
        theme::overlay(f, r);
        let card = theme::card(" Appearance · Esc close ", true);
        let inner = card.inner(r);
        f.render_widget(card, r);
        let labels = [
            "0  Off — static feedback",
            "1  Subtle — brief, quiet feedback (default)",
            "2  Full — sweeps, traces and particle accents",
            "u  Toggle Unicode / ASCII effect glyphs",
        ];
        for (i, label) in labels.iter().enumerate() {
            let hit = Rect::new(
                inner.x + 1,
                inner.y + 1 + i as u16,
                inner.width.saturating_sub(2),
                1,
            );
            let active = i
                == match a.project.ui.animations {
                    Motion::Off => 0,
                    Motion::Subtle => 1,
                    Motion::Full => 2,
                };
            f.render_widget(Paragraph::new(*label).style(theme::selected(active)), hit);
            a.formats.appearance_hits.push((hit, i));
        }
        f.render_widget(Paragraph::new(format!("Glyphs: {}\nCopper / graphite · no target polling\nSettings saved to this project's [ui] section.",if a.project.ui.unicode {"Unicode"} else {"ASCII"})).style(Style::default().fg(theme::MUTED)).wrap(Wrap{trim:false}),Rect::new(inner.x+1,inner.y+6,inner.width.saturating_sub(2),inner.height.saturating_sub(6)));
    }
    if let Some(item) = a.formats.popup.clone() {
        let r = center(f.area(), 100, 17);
        theme::overlay(f, r);
        let card = theme::card(" Number format · Enter apply · Esc close ", true);
        let inner = card.inner(r);
        f.render_widget(card, r);
        f.render_widget(
            Paragraph::new(item.name.clone()).style(Style::default().fg(theme::ACCENT)),
            Rect::new(inner.x + 1, inner.y, inner.width.saturating_sub(2), 1),
        );
        for (i, (_, label)) in BASES.iter().enumerate() {
            let hit = Rect::new(
                inner.x + 1,
                inner.y + 2 + i as u16,
                inner.width.saturating_sub(2),
                1,
            );
            f.render_widget(
                Paragraph::new(*label).style(theme::selected(i == a.formats.index)),
                hit,
            );
            a.formats.menu_hits.push((hit, i));
        }
        let preview=number(&item.raw,BASES[a.formats.index].0).unwrap_or_else(||format!("{}\n\nNot a scalar integer. Natural display is retained; watch an individual member to format it.",item.raw));
        f.render_widget(
            Paragraph::new(preview).wrap(Wrap { trim: false }),
            Rect::new(
                inner.x + 1,
                inner.y + 7,
                inner.width.saturating_sub(2),
                inner.height.saturating_sub(7),
            ),
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn render(a: &mut App, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| draw(f, a)).unwrap();
        t.backend()
            .buffer()
            .content
            .chunks(w as usize)
            .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }
    #[test]
    fn exact_integers_and_annotations() {
        assert_eq!(
            number("18446744073709551615", Radix::Hex).unwrap(),
            "0xffffffffffffffff"
        );
        assert_eq!(
            number("-9223372036854775808", Radix::Hex).unwrap(),
            "-0x8000000000000000"
        );
        assert_eq!(number("0xff", Radix::Decimal).unwrap(), "255");
        assert_eq!(number("65 'A'", Radix::Binary).unwrap(), "0b1000001 'A'");
        assert_eq!(number("8", Radix::Octal).unwrap(), "0o10");
        assert_eq!(number("077", Radix::Decimal).unwrap(), "63");
        assert!(number("--5", Radix::Hex).is_none());
        assert_eq!(
            number(&u128::MAX.to_string(), Radix::Hex).unwrap(),
            "0xffffffffffffffffffffffffffffffff"
        );
        assert_eq!(number("0x10 <main>", Radix::Decimal).unwrap(), "16 <main>");
        for s in [
            "1.25",
            "nan",
            "{x = 3}",
            "READY",
            "1e10",
            "<optimized out>",
            "0xzz",
        ] {
            assert!(number(s, Radix::Hex).is_none(), "{s}");
        }
    }
    #[test]
    fn per_item_menu_defaults_scope_and_keyboard_do_not_send_debug_commands() {
        let (engine, requests) = session::test_channel();
        let mut a = App::new(Project::default(), true);
        a.select_pane(3);
        a.key(
            KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE),
            Some(&engine),
        );
        assert_eq!(a.formats.popup.as_ref().unwrap().name, "pc"); // No rendered hit regions yet.
        a.formats.popup = None;
        a.select_pane(0);
        let text = render(&mut a, 160, 42);
        assert!(text.contains("15679512")); // Watch checksum default decimal.
        assert!(text.contains("0x800068c")); // Register PC default hex.
        let original = a.snapshot.clone();
        let item = a
            .formats
            .hits
            .iter()
            .find(|i| i.key == "watch:counter")
            .unwrap()
            .clone();
        a.mouse(
            MouseEvent {
                kind: MouseEventKind::Down(event::MouseButton::Right),
                column: item.rect.x,
                row: item.rect.y,
                modifiers: KeyModifiers::NONE,
            },
            Some(&engine),
        );
        assert!(a.formats.popup.is_some());
        a.key(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            Some(&engine),
        );
        a.key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            Some(&engine),
        );
        assert_eq!(a.project.ui.formats.get("watch:counter"), Some(&Radix::Hex));
        let req = requests.try_recv().unwrap();
        assert_eq!(req.method, "ui_preferences");
        assert!(requests.try_recv().is_err());
        assert_eq!(a.snapshot.watches[0].value, original.watches[0].value);
        assert_eq!(a.base_for("watch:flag", Radix::Decimal), Radix::Decimal);
        assert!(render(&mut a, 160, 42).contains("0x3039"));
        a.select_pane(3);
        a.selection = 0;
        a.formats.selected = None;
        a.key(
            KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE),
            Some(&engine),
        );
        assert!(
            a.formats
                .popup
                .as_ref()
                .unwrap()
                .key
                .starts_with("register:")
        );
        for (w, h) in [(45, 12), (80, 24), (160, 42)] {
            render(&mut a, w, h);
            assert!(
                a.formats
                    .menu_hits
                    .iter()
                    .all(|(r, _)| r.right() <= w && r.bottom() <= h)
            );
        }
    }
    #[test]
    fn appearance_owns_focus_and_does_not_send_debug_commands() {
        let (engine, requests) = session::test_channel();
        let mut a = App::new(Project::default(), true);
        a.focus_input(false);
        a.input = ":appearance".into();
        a.key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            Some(&engine),
        );
        assert!(a.formats.appearance);
        assert!(!a.input_active());
        a.key(
            KeyEvent::new(KeyCode::Char('0'), KeyModifiers::NONE),
            Some(&engine),
        );
        assert_eq!(a.project.ui.animations, Motion::Off);
        assert_eq!(requests.try_recv().unwrap().method, "ui_preferences");
        assert!(requests.try_recv().is_err());
    }
    #[test]
    fn memory_byte_formats_and_long_values_have_accessible_preview() {
        let mut a = App::new(Project::default(), true);
        a.select_pane(4);
        a.snapshot.memory =
            vec!["20000000  ff 08 00 01 7f 80 09 aa bb cc dd ee 22 33 44 55".into()];
        render(&mut a, 160, 42);
        let item = a
            .formats
            .hits
            .iter()
            .find(|i| i.key == "memory:20000001")
            .unwrap()
            .clone();
        a.open_format(Some(item));
        a.formats.index = 0;
        a.apply_format(None);
        assert!(render(&mut a, 160, 42).contains("0b1000"));
        a.select_pane(3);
        a.snapshot.registers[0].value = u128::MAX.to_string();
        render(&mut a, 160, 42);
        let item = a.formats.hits.iter().find(|i| i.pane == 3).unwrap().clone();
        a.open_format(Some(item));
        a.formats.index = 0;
        let text = render(&mut a, 100, 24);
        assert!(text.contains("1111111111111111111111111"));
    }
}
