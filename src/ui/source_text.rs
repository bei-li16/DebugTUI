//! Read-only, UTF-8 source selection. Positions address the original text, not its gutter.
use super::*;
use unicode_width::UnicodeWidthChar;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Position {
    pub row: usize,
    pub byte: usize,
}

#[derive(Default)]
pub(super) struct SourceText {
    pub caret: Position,
    pub anchor: Option<Position>,
    pub left: usize,
    pub area: Rect,
    pub buttons: Vec<(Rect, bool)>, // true: Watch, false: Copy
    dragging: bool,
    last_click: Option<(Position, Instant)>,
    preferred_column: Option<usize>,
    menu: Option<(u16, u16, usize)>,
    menu_hits: Vec<(Rect, bool)>,
}

pub(super) fn width(ch: char) -> usize {
    if ch == '\t' {
        4
    } else {
        ch.width().unwrap_or(0)
    }
}
pub(super) fn column(line: &str, byte: usize) -> usize {
    line[..byte.min(line.len())].chars().map(width).sum()
}
fn byte_at(line: &str, column: usize) -> usize {
    let mut col = 0;
    for (byte, ch) in line.char_indices() {
        let next = col + width(ch);
        if next > column {
            return byte;
        }
        col = next;
    }
    line.len()
}
pub(super) fn gutter(lines: usize) -> u16 {
    lines.to_string().len().max(4) as u16 + 5
}

impl SourceText {
    pub fn range(&self) -> Option<(Position, Position)> {
        self.anchor
            .filter(|a| *a != self.caret)
            .map(|a| (a.min(self.caret), a.max(self.caret)))
    }
    pub fn reset(&mut self, row: usize) {
        *self = Self {
            caret: Position { row, byte: 0 },
            ..Self::default()
        };
    }
    pub fn selected(&self, row: usize, byte: usize) -> bool {
        let p = Position { row, byte };
        self.range().is_some_and(|(a, b)| a <= p && p < b)
    }
    fn select_word(&mut self, lines: &[String], p: Position) {
        let Some(line) = lines.get(p.row) else {
            return;
        };
        let Some(ch) = line[p.byte..].chars().next() else {
            self.anchor = None;
            self.caret = p;
            return;
        };
        let word = |c: char| c.is_alphanumeric() || c == '_';
        let same = |c: char| (word(ch) && word(c)) || (ch.is_whitespace() && c.is_whitespace());
        let mut start = p.byte;
        let mut end = p.byte + ch.len_utf8();
        for (i, c) in line[..p.byte].char_indices().rev() {
            if !same(c) {
                break;
            }
            start = i;
        }
        for (i, c) in line[end..].char_indices() {
            if !same(c) {
                break;
            }
            end = p.byte + ch.len_utf8() + i + c.len_utf8();
        }
        self.anchor = Some(Position {
            row: p.row,
            byte: start,
        });
        self.caret = Position {
            row: p.row,
            byte: end,
        };
    }
    fn text(&self, lines: &[String]) -> Option<String> {
        let (a, b) = self.range()?;
        if b.row >= lines.len() {
            return None;
        }
        let mut result = String::new();
        for (row, line) in lines.iter().enumerate().take(b.row + 1).skip(a.row) {
            if row > a.row {
                result.push('\n');
            }
            let start = if row == a.row { a.byte } else { 0 };
            let end = if row == b.row { b.byte } else { line.len() };
            result.push_str(line.get(start..end)?);
        }
        Some(result)
    }
}

impl App {
    fn source_copy(&mut self) {
        if let Some(text) = self.source_text.text(&self.source) {
            self.notice = match crate::clipboard::copy(&text) {
                Ok(()) => format!("Copied {} source characters", text.chars().count()),
                Err(e) => format!("Copy failed: {e}"),
            };
        } else {
            self.notice = "Select source text to copy.".into();
        }
    }
    fn source_watch(&mut self, engine: Option<&EngineHandle>) {
        let Some(text) = self.source_text.text(&self.source) else {
            self.notice = "Select a variable or expression first.".into();
            return;
        };
        let expression = text.trim();
        if expression.is_empty() || expression.contains(['\n', '\r', '\0']) {
            self.notice = "Add to Watch needs a single-line variable or expression.".into();
            return;
        }
        if self.pending_watch.is_some() || self.pending_task.is_some() {
            self.notice = "Wait for the current Watch / project operation.".into();
            return;
        }
        // Reuse the normal Watch path: selected core, current frame, persistence and errors.
        self.watch_input = expression.into();
        self.select_pane(1);
        self.add_watch_input(engine);
    }
    fn source_action(&mut self, watch: bool, engine: Option<&EngineHandle>) {
        self.source_text.menu = None;
        if watch {
            self.source_watch(engine);
        } else {
            self.source_copy();
        }
    }
    pub(super) fn source_text_key(&mut self, key: KeyEvent, engine: Option<&EngineHandle>) -> bool {
        if let Some((x, y, selected)) = self.source_text.menu {
            match key.code {
                KeyCode::Esc => self.source_text.menu = None,
                KeyCode::Up | KeyCode::Down | KeyCode::Tab => {
                    self.source_text.menu = Some((x, y, 1 - selected))
                }
                KeyCode::Enter => self.source_action(
                    selected == 1 || key.modifiers.contains(KeyModifiers::CONTROL),
                    engine,
                ),
                KeyCode::Char('c') => self.source_action(false, engine),
                KeyCode::Char('w') => self.source_action(true, engine),
                _ => {
                    self.source_text.menu = None;
                    return false;
                }
            }
            return true;
        }
        if self.pane != 0 || self.main_pane != 0 || self.input_active() || self.console_view.focused
        {
            return false;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        if ctrl && key.code == KeyCode::Char('c') && self.source_text.range().is_some() {
            self.source_copy();
            return true;
        }
        if ctrl && key.code == KeyCode::Enter {
            self.source_watch(engine);
            return true;
        }
        if key.code == KeyCode::Esc && self.source_text.anchor.is_some() {
            self.source_text.anchor = None;
            self.source_text.dragging = false;
            return true;
        }
        if self.source.is_empty() || key.modifiers.contains(KeyModifiers::ALT) {
            return false;
        }
        if ctrl && key.code == KeyCode::Char('a') {
            self.source_text.anchor = Some(Position::default());
            let row = self.source.len() - 1;
            self.source_text.caret = Position {
                row,
                byte: self.source[row].len(),
            };
            self.reveal_source_caret();
            return true;
        }
        if !matches!(
            key.code,
            KeyCode::Left
                | KeyCode::Right
                | KeyCode::Up
                | KeyCode::Down
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::PageUp
                | KeyCode::PageDown
        ) {
            return false;
        }
        if ctrl && !matches!(key.code, KeyCode::Home | KeyCode::End) {
            return false;
        }
        if !shift
            && matches!(key.code, KeyCode::Left | KeyCode::Right)
            && let Some((start, end)) = self.source_text.range()
        {
            self.source_text.caret = if key.code == KeyCode::Left {
                start
            } else {
                end
            };
            self.source_text.anchor = None;
            self.source_text.preferred_column = None;
            self.source_text.last_click = None;
            self.reveal_source_caret();
            return true;
        }
        let mut p = self.source_text.caret;
        if self.source_text.anchor.is_none() && p.row != self.source_line {
            p = Position {
                row: self.source_line,
                byte: 0,
            };
        }
        p.row = p.row.min(self.source.len() - 1);
        p.byte = p.byte.min(self.source[p.row].len());
        if shift {
            self.source_text.anchor.get_or_insert(p);
        } else {
            self.source_text.anchor = None;
        }
        let col = column(&self.source[p.row], p.byte);
        let desired = self.source_text.preferred_column.unwrap_or(col);
        let mut vertical = false;
        match key.code {
            KeyCode::Left => {
                if p.byte > 0 {
                    p.byte = self.source[p.row][..p.byte]
                        .char_indices()
                        .next_back()
                        .unwrap()
                        .0;
                } else if p.row > 0 {
                    p.row -= 1;
                    p.byte = self.source[p.row].len();
                }
            }
            KeyCode::Right => {
                if p.byte < self.source[p.row].len() {
                    p.byte += self.source[p.row][p.byte..]
                        .chars()
                        .next()
                        .unwrap()
                        .len_utf8();
                } else if p.row + 1 < self.source.len() {
                    p.row += 1;
                    p.byte = 0;
                }
            }
            KeyCode::Home => {
                if ctrl {
                    p.row = 0;
                }
                p.byte = 0;
            }
            KeyCode::End => {
                if ctrl {
                    p.row = self.source.len() - 1;
                }
                p.byte = self.source[p.row].len();
            }
            _ => {
                vertical = true;
                let step = if matches!(key.code, KeyCode::PageUp | KeyCode::PageDown) {
                    self.source_rect.height.max(1) as isize
                } else {
                    1
                };
                let delta = if matches!(key.code, KeyCode::Up | KeyCode::PageUp) {
                    -step
                } else {
                    step
                };
                p.row = p
                    .row
                    .saturating_add_signed(delta)
                    .min(self.source.len() - 1);
                p.byte = byte_at(&self.source[p.row], desired);
            }
        }
        self.source_text.preferred_column = vertical.then_some(desired);
        self.source_text.caret = p;
        self.source_text.last_click = None;
        self.reveal_source_caret();
        true
    }
    fn reveal_source_caret(&mut self) {
        let p = self.source_text.caret;
        self.source_line = p.row;
        let height = self.source_rect.height.max(1) as usize;
        if p.row < self.source_top {
            self.source_top = p.row;
        }
        if p.row >= self.source_top + height {
            self.source_top = p.row + 1 - height;
        }
        if let Some(line) = self.source.get(p.row) {
            let col = column(line, p.byte);
            let width = self.source_text.area.width.max(1) as usize;
            if col < self.source_text.left {
                self.source_text.left = col;
            }
            if col >= self.source_text.left + width {
                self.source_text.left = col + 1 - width;
            }
            self.source_text.left = self.source_text.left.min(u16::MAX as usize);
        }
    }
    fn source_point(&self, mouse: MouseEvent) -> Position {
        let rect = self.source_text.area;
        let row = (self.source_top
            + mouse
                .row
                .saturating_sub(rect.y)
                .min(rect.height.saturating_sub(1)) as usize)
            .min(self.source.len().saturating_sub(1));
        let col =
            mouse.column.saturating_sub(rect.x).min(rect.width) as usize + self.source_text.left;
        Position {
            row,
            byte: self.source.get(row).map_or(0, |s| byte_at(s, col)),
        }
    }
    pub(super) fn source_text_mouse(
        &mut self,
        mouse: MouseEvent,
        engine: Option<&EngineHandle>,
    ) -> bool {
        use event::MouseButton::{Left, Right};
        let point = (mouse.column, mouse.row).into();
        if self.source_text.menu.is_some() {
            if matches!(mouse.kind, MouseEventKind::Down(Left | Right)) {
                let action = self
                    .source_text
                    .menu_hits
                    .iter()
                    .find(|(r, _)| r.contains(point))
                    .map(|(_, w)| *w);
                self.source_text.menu = None;
                if let Some(watch) = action {
                    self.source_action(watch, engine);
                }
            }
            return true;
        }
        if mouse.kind == MouseEventKind::Down(Left)
            && let Some((_, watch)) = self
                .source_text
                .buttons
                .iter()
                .find(|(r, _)| r.contains(point))
        {
            self.source_action(*watch, engine);
            return true;
        }
        if self.source_text.dragging {
            match mouse.kind {
                MouseEventKind::Up(Left) => {
                    self.source_text.dragging = false;
                    return true;
                }
                MouseEventKind::Drag(Left) => {
                    let r = self.source_text.area;
                    if r.is_empty() {
                        self.source_text.dragging = false;
                        return true;
                    }
                    if mouse.row < r.y {
                        self.source_top = self.source_top.saturating_sub(1);
                    }
                    if mouse.row >= r.bottom() {
                        self.source_top = (self.source_top + 1)
                            .min(self.source.len().saturating_sub(r.height as usize));
                    }
                    if mouse.column < r.x {
                        self.source_text.left = self.source_text.left.saturating_sub(3);
                    }
                    if mouse.column >= r.right() {
                        self.source_text.left = self
                            .source_text
                            .left
                            .saturating_add(3)
                            .min(u16::MAX as usize);
                    }
                    self.source_text.caret = self.source_point(mouse);
                    self.source_line = self.source_text.caret.row;
                    self.source_text.last_click = None;
                    return true;
                }
                _ => {}
            }
        }
        if self.main_pane != 0 || self.source.is_empty() {
            return false;
        }
        if !self.source_text.area.contains(point) {
            if mouse.kind == MouseEventKind::Down(Left) && self.source_rect.contains(point) {
                self.source_text.anchor = None;
                self.source_text.last_click = None;
            }
            return false;
        }
        if !matches!(mouse.kind, MouseEventKind::Down(Left | Right)) {
            return false;
        }
        self.select_pane(0);
        self.editing = false;
        self.watch_editing = false;
        self.console_view.focused = false;
        self.completion.invalidate();
        let p = self.source_point(mouse);
        self.source_text.preferred_column = None;
        if mouse.kind == MouseEventKind::Down(Right) {
            if !self.source_text.selected(p.row, p.byte) {
                self.source_text.select_word(&self.source, p);
            }
            self.source_text.menu = Some((mouse.column, mouse.row, 0));
            self.source_line = self.source_text.caret.row;
            return true;
        }
        let double = self
            .source_text
            .last_click
            .is_some_and(|(last, at)| last == p && at.elapsed() < Duration::from_millis(500));
        if double && !mouse.modifiers.contains(KeyModifiers::SHIFT) {
            self.source_text.select_word(&self.source, p);
            self.source_text.dragging = false;
            self.source_text.last_click = None;
        } else {
            let anchor = if mouse.modifiers.contains(KeyModifiers::SHIFT) {
                self.source_text.anchor.unwrap_or(self.source_text.caret)
            } else {
                p
            };
            self.source_text.anchor = Some(anchor);
            self.source_text.caret = p;
            self.source_text.dragging = true;
            self.source_text.last_click = Some((p, Instant::now()));
        }
        self.source_line = p.row;
        true
    }
}

pub(super) fn buttons(f: &mut UiFrame, a: &mut App, rect: Rect) {
    if a.main_pane != 0 || rect.width < 62 {
        return;
    }
    let width = 27;
    let mut x = rect.right() - width;
    for (label, watch) in [(" Copy ", false), (" Add to Watch ", true)] {
        let hit = Rect::new(x, rect.y, label.len() as u16, 1);
        let active = a.source_text.range().is_some();
        f.render_widget(
            Paragraph::new(label).style(
                Style::default()
                    .fg(if active { theme::ACCENT } else { theme::DIM })
                    .bg(theme::PANEL),
            ),
            hit,
        );
        a.source_text.buttons.push((hit, watch));
        x += hit.width + 1;
    }
}

pub(super) fn popup(f: &mut UiFrame, a: &mut App) {
    let Some((x, y, selected)) = a.source_text.menu else {
        return;
    };
    let area = f.area();
    let width = 32.min(area.width);
    let height = 4.min(area.height);
    let rect = Rect::new(
        x.min(area.right().saturating_sub(width)),
        y.min(area.bottom().saturating_sub(height)),
        width,
        height,
    );
    theme::overlay(f, rect);
    let card = theme::card(" Source ", true);
    let inner = card.inner(rect);
    f.render_widget(card, rect);
    a.source_text.menu_hits.clear();
    for (i, label) in [" Copy           Ctrl+C", " Add to Watch   Ctrl+Enter"]
        .iter()
        .enumerate()
        .take(inner.height as usize)
    {
        let hit = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        f.render_widget(
            Paragraph::new(*label).style(theme::selected(i == selected)),
            hit,
        );
        a.source_text.menu_hits.push((hit, i == 1));
    }
}

#[cfg(test)]
mod hardware_tests;
#[cfg(test)]
mod tests;
