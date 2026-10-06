//! Terminal-native visual language. No assets, fonts or rendering dependencies.
use ratatui::{
    Frame,
    layout::{Alignment, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

/// `Layout::split` with a per-thread cache. ratatui keeps its own cache behind
/// its default `layout-cache` feature, which this build leaves off, so every
/// uncached split re-runs the constraint solver: hundreds of allocations, and
/// the same few layouts are solved again every frame.
pub trait SplitCached {
    fn split_cached(self, area: Rect) -> Rc<[Rect]>;
}
type Splits = HashMap<(Rect, Layout), Rc<[Rect]>>;
impl SplitCached for Layout {
    fn split_cached(self, area: Rect) -> Rc<[Rect]> {
        thread_local! {
            static SPLITS: RefCell<Splits> = RefCell::new(HashMap::new());
        }
        SPLITS.with_borrow_mut(|splits| {
            let key = (area, self);
            if let Some(rects) = splits.get(&key) {
                return rects.clone();
            }
            // Resizing creates new keys; start over rather than grow forever.
            if splits.len() >= 256 {
                splits.clear();
            }
            let rects = key.1.split(area);
            splits.insert(key, rects.clone());
            rects
        })
    }
}

pub const CANVAS: Color = Color::Rgb(17, 21, 27);
pub const PANEL: Color = Color::Rgb(23, 29, 37);
pub const RAISED: Color = Color::Rgb(30, 38, 48);
pub const HOVER: Color = Color::Rgb(39, 53, 69);
pub const SELECTED: Color = Color::Rgb(34, 55, 76);
pub const BORDER: Color = Color::Rgb(54, 66, 81);
pub const TEXT: Color = Color::Rgb(221, 230, 240);
pub const MUTED: Color = Color::Rgb(150, 166, 185);
pub const DIM: Color = Color::Rgb(99, 115, 134);
pub const ACCENT: Color = Color::Rgb(110, 180, 245);
pub const GREEN: Color = Color::Rgb(143, 202, 158);
pub const AMBER: Color = Color::Rgb(229, 188, 122);
pub const RED: Color = Color::Rgb(237, 137, 137);
pub const VIOLET: Color = Color::Rgb(182, 161, 228);
pub const PC: Color = Color::Rgb(34, 54, 47);
// Controls remain distinct from both plain and core-tinted panel surfaces.
pub const BUTTON: Color = Color::Rgb(27, 35, 45);
pub const BUTTON_HOVER: Color = Color::Rgb(39, 53, 69);
pub const BUTTON_DISABLED: Color = Color::Rgb(26, 31, 39);
pub const BUTTON_EDGE: Color = Color::Rgb(69, 87, 107);

pub fn base() -> Style {
    Style::default().fg(TEXT).bg(CANVAS)
}
pub fn surface(f: &mut Frame, rect: Rect, color: Color) {
    f.render_widget(Clear, rect);
    f.render_widget(
        Block::default().style(Style::default().fg(TEXT).bg(color)),
        rect,
    );
}
pub fn lines(f: &mut Frame, lines: Vec<Line<'static>>, rect: Rect) {
    for (index, line) in lines.iter().take(rect.height as usize).enumerate() {
        f.buffer_mut().set_style(
            Rect::new(rect.x, rect.y + index as u16, rect.width, 1),
            line.style,
        );
    }
    f.render_widget(Paragraph::new(lines), rect);
}
pub fn section(title: impl Into<Line<'static>>) -> Block<'static> {
    Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(BORDER))
        .title(title)
        .title_style(Style::default().fg(MUTED))
}
pub fn card(title: impl Into<Line<'static>>, focused: bool) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .style(Style::default().fg(TEXT).bg(PANEL))
        .border_style(Style::default().fg(if focused { ACCENT } else { BORDER }))
        .title(title)
        .title_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
}
pub fn selected(active: bool) -> Style {
    Style::default()
        .fg(if active { ACCENT } else { TEXT })
        .bg(if active { SELECTED } else { PANEL })
}
pub fn chip(active: bool, hover: bool) -> Style {
    control(true, active, hover, TEXT)
}

pub fn control(enabled: bool, active: bool, hover: bool, tone: Color) -> Style {
    Style::default()
        .fg(if !enabled {
            DIM
        } else if active {
            ACCENT
        } else if hover {
            TEXT
        } else {
            tone
        })
        .bg(if !enabled {
            BUTTON_DISABLED
        } else if active {
            SELECTED
        } else if hover {
            BUTTON_HOVER
        } else {
            BUTTON
        })
        .add_modifier(if enabled && (active || hover) {
            Modifier::BOLD
        } else {
            Modifier::empty()
        })
}

/// Use the same rounded frame as Watch's Add control. Compact layouts keep
/// real side borders; the entire supplied rectangle remains clickable.
pub fn button(f: &mut Frame, rect: Rect, label: &str, style: Style) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let edge = if style.fg == Some(CANVAS) {
        style.bg.unwrap_or(ACCENT)
    } else if style.bg == Some(SELECTED) || style.bg == Some(BUTTON_HOVER) {
        ACCENT
    } else if style.bg == Some(BUTTON_DISABLED) {
        DIM
    } else {
        BUTTON_EDGE
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .borders(if rect.height >= 3 {
            Borders::ALL
        } else {
            Borders::LEFT | Borders::RIGHT
        })
        // The frame inherits its parent's background. Only the interior is
        // filled, so the terminal cells outside the rounded outline stay clear.
        .border_style(Style::default().fg(edge))
        .style(Style::default().fg(TEXT));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if inner.width > 0 && inner.height > 0 {
        f.buffer_mut().set_style(inner, style);
        f.render_widget(
            Paragraph::new(label)
                .alignment(Alignment::Center)
                .style(style),
            Rect::new(inner.x, inner.y + (inner.height - 1) / 2, inner.width, 1),
        );
    }
}

/// Dim the actual rendered cells; leave every popup opaque and legible.
pub fn overlay(f: &mut Frame, rect: Rect) {
    let area = f.area();
    for cell in &mut f.buffer_mut().content {
        cell.fg = DIM;
        cell.bg = CANVAS;
        cell.modifier = Modifier::empty();
    }
    let shadow = Rect::new(
        rect.x.saturating_add(1),
        rect.y.saturating_add(1),
        rect.width,
        rect.height,
    )
    .intersection(area);
    surface(f, shadow, Color::Rgb(12, 11, 10));
    surface(f, rect, PANEL);
}

pub fn empty(f: &mut Frame, rect: Rect, title: &str, detail: &str) {
    let y = rect.y + u16::from(rect.height > 4);
    f.render_widget(
        Paragraph::new(vec![
            Line::styled(format!("  ◇  {title}"), Style::default().fg(MUTED)),
            Line::raw(""),
            Line::styled(format!("  {detail}"), Style::default().fg(DIM)),
        ])
        .wrap(ratatui::widgets::Wrap { trim: false }),
        Rect::new(rect.x, y, rect.width, rect.bottom().saturating_sub(y)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn button_fill_is_confined_to_interior_in_every_state_and_size() {
        for height in [1, 3] {
            for style in [
                control(true, false, false, TEXT),
                control(true, true, false, TEXT),
                control(true, false, true, TEXT),
                control(false, false, false, TEXT),
                control(true, true, false, TEXT).fg(CANVAS).bg(GREEN),
            ] {
                let mut terminal = Terminal::new(TestBackend::new(24, 6)).unwrap();
                let rect = Rect::new(2, 1, 12, height);
                terminal
                    .draw(|frame| {
                        surface(frame, frame.area(), PANEL);
                        button(frame, rect, "Action", style);
                    })
                    .unwrap();
                let buffer = terminal.backend().buffer();
                for y in rect.y..rect.bottom() {
                    for x in rect.x..rect.right() {
                        let border = x == rect.x
                            || x == rect.right() - 1
                            || (height == 3 && (y == rect.y || y == rect.bottom() - 1));
                        assert_eq!(
                            buffer[(x, y)].bg,
                            if border { PANEL } else { style.bg.unwrap() }
                        );
                    }
                }
            }
        }
    }
}
