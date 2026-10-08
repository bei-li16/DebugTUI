//! Setup help is an overlay: it never takes space from the field list.
use super::*;
use ratatui::layout::Position;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub(super) struct Hover {
    pub field: usize,
    pub at: Position,
    pub popup: Popup,
}

pub(super) struct Popup {
    title: String,
    text: String,
    wrapped: Vec<String>,
    wrap_width: u16,
    scroll: usize,
    max_scroll: usize,
    pub rect: Rect,
    close: Rect,
}

impl Popup {
    pub fn new(title: impl Into<String>, text: String) -> Self {
        Self {
            title: title.into(),
            text,
            wrapped: vec![],
            wrap_width: 0,
            scroll: 0,
            max_scroll: 0,
            rect: Rect::default(),
            close: Rect::default(),
        }
    }

    fn wrap(&mut self, width: u16) {
        if self.wrap_width != width {
            self.wrapped = wrap(&self.text, usize::from(width));
            self.wrap_width = width;
        }
    }

    pub fn draw_hover(&mut self, f: &mut Frame, at: Position) {
        let bounds = f.area();
        if bounds.width < 4 || bounds.height < 4 {
            return;
        }
        let natural = self
            .text
            .lines()
            .map(UnicodeWidthStr::width)
            .max()
            .unwrap_or(0);
        let width = (natural.max(self.title.width()) + 2)
            .clamp(32, 80)
            .min(usize::from(bounds.width)) as u16;
        self.wrap(width - 2);
        let height = (self.wrapped.len() + 2).min(usize::from(bounds.height)) as u16;
        self.rect = place(bounds, at, width, height);
        let block = theme::card(format!(" {} ", self.title), true);
        let inner = block.inner(self.rect);
        theme::surface(f, self.rect, theme::RAISED);
        f.render_widget(
            block.style(Style::default().fg(theme::TEXT).bg(theme::RAISED)),
            self.rect,
        );
        let clipped = self.wrapped.len() > usize::from(inner.height);
        let body_height = inner.height.saturating_sub(u16::from(clipped));
        f.render_widget(
            Paragraph::new(
                self.wrapped
                    .iter()
                    .take(usize::from(body_height))
                    .cloned()
                    .map(Line::raw)
                    .collect::<Vec<_>>(),
            ),
            Rect::new(inner.x, inner.y, inner.width, body_height),
        );
        if clipped {
            f.render_widget(
                Paragraph::new("… F1: full help").style(Style::default().fg(theme::ACCENT)),
                Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
            );
        }
    }

    pub fn draw_details(&mut self, f: &mut Frame) {
        let screen = f.area();
        let width = screen.width.min(100);
        let height = screen.height.min(30);
        self.rect = Rect::new(
            screen.x + (screen.width - width) / 2,
            screen.y + (screen.height - height) / 2,
            width,
            height,
        );
        theme::overlay(f, self.rect);
        let block = theme::card(format!(" {} / help ", self.title), true);
        let inner = block.inner(self.rect);
        f.render_widget(block, self.rect);
        if inner.height < 2 || inner.width == 0 {
            return;
        }
        self.wrap(inner.width);
        let body = Rect::new(inner.x, inner.y, inner.width, inner.height - 1);
        self.max_scroll = self.wrapped.len().saturating_sub(usize::from(body.height));
        self.scroll = self.scroll.min(self.max_scroll);
        f.render_widget(
            Paragraph::new(
                self.wrapped
                    .iter()
                    .skip(self.scroll)
                    .take(usize::from(body.height))
                    .cloned()
                    .map(Line::raw)
                    .collect::<Vec<_>>(),
            ),
            body,
        );
        self.close = Rect::new(inner.x, inner.bottom() - 1, inner.width, 1);
        f.render_widget(
            Paragraph::new("↑↓ / PgUp/PgDn: scroll · Esc / Close")
                .style(Style::default().fg(theme::ACCENT)),
            self.close,
        );
    }

    pub fn key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::F(1) => return true,
            KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down => self.scroll = (self.scroll + 1).min(self.max_scroll),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(6),
            KeyCode::PageDown => self.scroll = (self.scroll + 6).min(self.max_scroll),
            KeyCode::Home => self.scroll = 0,
            KeyCode::End => self.scroll = self.max_scroll,
            _ => {}
        }
        false
    }

    pub fn mouse(&mut self, mouse: MouseEvent) -> bool {
        match mouse.kind {
            MouseEventKind::ScrollUp => self.scroll = self.scroll.saturating_sub(3),
            MouseEventKind::ScrollDown => self.scroll = (self.scroll + 3).min(self.max_scroll),
            MouseEventKind::Down(MouseButton::Left) => {
                return self.close.contains((mouse.column, mouse.row).into());
            }
            _ => {}
        }
        false
    }
}

/// Prefer below/right, then above/left, finally clamp to the terminal bounds.
fn place(bounds: Rect, at: Position, width: u16, height: u16) -> Rect {
    let width = width.min(bounds.width);
    let height = height.min(bounds.height);
    let x = if at.x.saturating_add(2).saturating_add(width) <= bounds.right() {
        at.x.saturating_add(2)
    } else {
        at.x.saturating_sub(width.saturating_add(2))
    }
    .clamp(bounds.x, bounds.right().saturating_sub(width));
    let y = if at.y.saturating_add(1).saturating_add(height) <= bounds.bottom() {
        at.y.saturating_add(1)
    } else {
        at.y.saturating_sub(height)
    }
    .clamp(bounds.y, bounds.bottom().saturating_sub(height));
    Rect::new(x, y, width, height)
}

/// Wrap words, splitting long paths only when needed; count terminal cells.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = vec![];
    for paragraph in text.split('\n') {
        let mut line = String::new();
        let mut used = 0;
        for word in paragraph.split_whitespace() {
            if used > 0 {
                if used + 1 + word.width() > width {
                    lines.push(std::mem::take(&mut line));
                    used = 0;
                } else {
                    line.push(' ');
                    used += 1;
                }
            }
            for ch in word.chars() {
                let cells = ch.width().unwrap_or(0);
                if used + cells > width && used > 0 {
                    lines.push(std::mem::take(&mut line));
                    used = 0;
                }
                line.push(ch);
                used += cells;
            }
        }
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn hover_flips_at_edges_and_stays_inside_offset_and_tiny_screens() {
        let bounds = Rect::new(3, 5, 100, 30);
        assert_eq!(
            place(bounds, Position::new(5, 7), 40, 6),
            Rect::new(7, 8, 40, 6)
        );
        assert_eq!(
            place(bounds, Position::new(101, 33), 40, 6),
            Rect::new(59, 27, 40, 6)
        );
        for bounds in [bounds, Rect::new(0, 0, 45, 12), Rect::new(7, 9, 1, 1)] {
            for at in [
                Position::new(bounds.x, bounds.y),
                Position::new(bounds.right() - 1, bounds.bottom() - 1),
            ] {
                let popup = place(bounds, at, 80, 20);
                assert_eq!(popup.intersection(bounds), popup);
            }
        }
    }

    #[test]
    fn long_unicode_help_wraps_and_keyboard_can_reach_the_last_line() {
        let text = format!(
            "Probe: 中文路径 D:/{}\nLast help line",
            "很长的源码目录/".repeat(40)
        );
        for line in wrap(&text, 20) {
            assert!(line.width() <= 20, "{line}");
        }
        let mut terminal = Terminal::new(TestBackend::new(45, 12)).unwrap();
        let mut popup = Popup::new("Probe", text);
        terminal
            .draw(|f| popup.draw_hover(f, Position::new(44, 11)))
            .unwrap();
        assert_eq!(
            popup.rect.intersection(terminal.backend().buffer().area),
            popup.rect
        );
        let shown: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(shown.contains("F1: full help"));
        terminal.draw(|f| popup.draw_details(f)).unwrap();
        popup.key(KeyCode::End);
        terminal.draw(|f| popup.draw_details(f)).unwrap();
        let shown: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(shown.contains("Last help line"));
        assert!(popup.key(KeyCode::Esc));
    }
}
