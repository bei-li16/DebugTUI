//! Terminal-native visual language. No assets, fonts or rendering dependencies.
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

pub const CANVAS: Color = Color::Rgb(24, 23, 21);
pub const PANEL: Color = Color::Rgb(31, 30, 27);
pub const RAISED: Color = Color::Rgb(43, 40, 35);
pub const HOVER: Color = Color::Rgb(65, 53, 42);
pub const SELECTED: Color = Color::Rgb(69, 52, 38);
pub const BORDER: Color = Color::Rgb(87, 79, 66);
pub const TEXT: Color = Color::Rgb(231, 222, 204);
pub const MUTED: Color = Color::Rgb(172, 161, 141);
pub const DIM: Color = Color::Rgb(129, 120, 104);
pub const ACCENT: Color = Color::Rgb(217, 160, 105);
pub const GREEN: Color = Color::Rgb(164, 190, 126);
pub const AMBER: Color = Color::Rgb(222, 184, 109);
pub const RED: Color = Color::Rgb(220, 121, 105);
pub const VIOLET: Color = Color::Rgb(194, 168, 148);
pub const PC: Color = Color::Rgb(48, 58, 40);
pub const CHANGE: Color = Color::Rgb(72, 53, 32);

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
    Style::default()
        .fg(if active { TEXT } else { MUTED })
        .bg(if hover {
            HOVER
        } else if active {
            SELECTED
        } else {
            PANEL
        })
        .add_modifier(if active {
            Modifier::BOLD
        } else {
            Modifier::empty()
        })
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
