//! Setup fields scroll independently of the selected field and its edit draft.
use super::*;

#[derive(Default)]
pub(super) struct FieldScroll {
    pub top: usize,
    pub rect: Rect,
    pub bar: Rect,
    len: usize,
    seen: Option<(usize, u16)>,
    reveal: bool,
    drag: Option<u16>,
}

impl FieldScroll {
    pub fn clear_hits(&mut self) {
        self.rect = Rect::default();
        self.bar = Rect::default();
    }
    pub fn keyboard(&mut self) {
        self.reveal = true;
        self.drag = None;
    }
    pub fn layout(&mut self, area: Rect, len: usize, selected: usize) {
        self.len = len;
        let overflow = len > area.height as usize && area.height > 0;
        self.rect = Rect {
            width: area.width.saturating_sub(u16::from(overflow)),
            ..area
        };
        self.bar = if overflow {
            Rect::new(
                area.right().saturating_sub(1),
                area.y,
                area.width.min(1),
                area.height,
            )
        } else {
            self.drag = None;
            Rect::default()
        };
        let height = area.height as usize;
        if selected < len
            && height > 0
            && (self.reveal || self.seen != Some((selected, area.height)))
        {
            if selected < self.top {
                self.top = selected;
            } else if selected >= self.top + height {
                self.top = selected + 1 - height;
            }
        }
        self.top = self.top.min(self.max());
        self.seen = Some((selected, area.height));
        self.reveal = false;
    }
    fn max(&self) -> usize {
        self.len.saturating_sub(self.rect.height as usize)
    }
    fn thumb(&self) -> (usize, usize) {
        let height = self.bar.height as usize;
        if height == 0 {
            return (0, 0);
        }
        let size = (height * self.rect.height as usize / self.len.max(1)).clamp(1, height);
        let top = self
            .top
            .saturating_mul(height - size)
            .checked_div(self.max())
            .unwrap_or(0);
        (top, size)
    }
    fn drag_to(&mut self, row: u16, offset: u16) {
        let (_, size) = self.thumb();
        let travel = (self.bar.height as usize).saturating_sub(size);
        let position = row.saturating_sub(self.bar.y).saturating_sub(offset) as usize;
        self.top = position
            .min(travel)
            .saturating_mul(self.max())
            .checked_div(travel)
            .unwrap_or(0);
    }
    pub fn mouse(&mut self, mouse: MouseEvent) -> bool {
        if let Some(offset) = self.drag {
            match mouse.kind {
                MouseEventKind::Drag(MouseButton::Left) => {
                    self.drag_to(mouse.row, offset);
                    return true;
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    self.drag = None;
                    return true;
                }
                _ => {}
            }
        }
        let point = (mouse.column, mouse.row).into();
        if !self.rect.contains(point) && !self.bar.contains(point) {
            return false;
        }
        match mouse.kind {
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                self.top = self
                    .top
                    .saturating_add_signed(if mouse.kind == MouseEventKind::ScrollDown {
                        3
                    } else {
                        -3
                    })
                    .min(self.max());
                true
            }
            MouseEventKind::Down(MouseButton::Left) if self.bar.contains(point) => {
                let (top, size) = self.thumb();
                let click = mouse.row.saturating_sub(self.bar.y) as usize;
                let offset = if (top..top + size).contains(&click) {
                    click - top
                } else {
                    size / 2
                } as u16;
                self.drag = Some(offset);
                self.drag_to(mouse.row, offset);
                true
            }
            _ => false,
        }
    }
    pub fn draw(&self, f: &mut Frame) {
        let (top, size) = self.thumb();
        let lines = (0..self.bar.height as usize)
            .map(|row| {
                let active = (top..top + size).contains(&row);
                Line::styled(
                    if active { "┃" } else { "│" },
                    Style::default().fg(if active { theme::ACCENT } else { theme::DIM }),
                )
            })
            .collect();
        theme::lines(f, lines, self.bar);
    }
}
