//! A separate read-only Guest inspector keeps physical writes/control unambiguous.
use super::*;

#[derive(Default)]
pub(super) struct GuestView {
    pub open: bool,
    image: String,
    pending: Option<u64>,
    stamp: String,
    value: Value,
    stack: Vec<Frame>,
    locals: Vec<Variable>,
    selected: usize,
    source: Vec<String>,
    source_top: usize,
    error: String,
    hits: Vec<(Rect, usize)>,
}

#[cfg(test)]
mod tests;

impl App {
    fn guest_stamp(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.snapshot.register_session,
            self.snapshot.generation,
            self.snapshot
                .core
                .as_ref()
                .map(|c| c.name.as_str())
                .unwrap_or("default"),
            self.snapshot.state
        )
    }
    pub(super) fn open_guest(&mut self, engine: Option<&EngineHandle>, image: &str) {
        if self.demo || self.snapshot.state != state::STOPPED || self.pending_task.is_some() {
            self.notice = "Saved Guest inspection requires a stopped physical target".into();
            return;
        }
        let core = self
            .snapshot
            .core
            .as_ref()
            .map(|c| c.name.as_str())
            .unwrap_or("default");
        let candidates: Vec<_> = self
            .project
            .images
            .iter()
            .filter(|i| {
                i.saved_context.is_some()
                    && (i.cores.is_empty() || i.cores.iter().any(|c| c == core))
            })
            .map(|i| i.id.clone())
            .collect();
        let image = if image.is_empty() && candidates.len() == 1 {
            candidates[0].clone()
        } else {
            image.to_string()
        };
        if !candidates.contains(&image) {
            self.notice = format!(
                "Use :guest IMAGE for a configured saved Guest: {}",
                candidates.join(", ")
            );
            return;
        }
        self.guest_view = GuestView {
            open: true,
            image,
            ..Default::default()
        };
        self.guest_request(engine, 0);
    }
    fn guest_request(&mut self, engine: Option<&EngineHandle>, level: usize) {
        if self.guest_view.pending.is_some() {
            return;
        }
        self.guest_view.stamp = self.guest_stamp();
        self.guest_view.pending = Some(self.next_id);
        self.guest_view.error.clear();
        self.submit(
            engine,
            "guest_context",
            json!({"image":self.guest_view.image,"level":level}),
        );
    }
    pub(super) fn guest_snapshot(&mut self, snapshot: &Snapshot) {
        if snapshot.generation != self.snapshot.generation
            || snapshot.register_session != self.snapshot.register_session
            || snapshot.state != state::STOPPED
            || snapshot.core.as_ref().map(|c| &c.name)
                != self.snapshot.core.as_ref().map(|c| &c.name)
        {
            self.guest_view.open = false;
        }
    }
    pub(super) fn guest_response(&mut self, id: u64, result: &Value, error: Option<&str>) -> bool {
        if self.guest_view.pending != Some(id) {
            return false;
        }
        self.guest_view.pending = None;
        self.pending_commands.remove(&id);
        self.fx.response(id, error.is_none());
        if !self.guest_view.open || self.guest_view.stamp != self.guest_stamp() {
            return true;
        }
        if let Some(error) = error {
            self.guest_view.error = error.into();
            return true;
        }
        self.guest_view.stack = serde_json::from_value(result["stack"].clone()).unwrap_or_default();
        self.guest_view.locals =
            serde_json::from_value(result["locals"].clone()).unwrap_or_default();
        self.guest_view.selected = result["selected_frame"]["level"].as_u64().unwrap_or(0) as usize;
        let frame = self
            .guest_view
            .stack
            .iter()
            .find(|f| f.level as usize == self.guest_view.selected);
        self.guest_view.source = frame
            .and_then(|f| {
                self.project
                    .image_source_path(&f.file, Some(&self.guest_view.image))
            })
            .filter(|p| fs::metadata(p).is_ok_and(|m| m.len() <= 2 * 1024 * 1024))
            .and_then(|p| fs::read_to_string(p).ok())
            .map(|t| t.lines().take(30000).map(str::to_owned).collect())
            .unwrap_or_else(|| {
                vec!["Guest source unavailable; check this image's source root/maps.".into()]
            });
        self.guest_view.source_top = frame
            .map(|f| f.line.saturating_sub(6) as usize)
            .unwrap_or(0);
        self.guest_view.value = result.clone();
        true
    }
    pub(super) fn guest_key(&mut self, key: KeyEvent, engine: Option<&EngineHandle>) -> bool {
        if !self.guest_view.open {
            return false;
        }
        match key.code {
            KeyCode::Esc => self.guest_view.open = false,
            KeyCode::Char('r') if self.guest_view.pending.is_none() => {
                self.guest_view.open = false;
                self.submit(
                    engine,
                    "return_guest",
                    json!({"image":self.guest_view.image}),
                );
            }
            KeyCode::Up => {
                let n = self.guest_view.selected.saturating_sub(1);
                self.guest_request(engine, n);
            }
            KeyCode::Down => {
                let n = (self.guest_view.selected + 1)
                    .min(self.guest_view.stack.len().saturating_sub(1));
                self.guest_request(engine, n);
            }
            KeyCode::PageUp => {
                self.guest_view.source_top = self.guest_view.source_top.saturating_sub(8)
            }
            KeyCode::PageDown => {
                self.guest_view.source_top = (self.guest_view.source_top + 8)
                    .min(self.guest_view.source.len().saturating_sub(1))
            }
            _ => (),
        }
        true
    }
    pub(super) fn guest_mouse(&mut self, event: MouseEvent, engine: Option<&EngineHandle>) -> bool {
        if !self.guest_view.open {
            return false;
        }
        if event.kind == MouseEventKind::Down(event::MouseButton::Left) {
            if let Some((_, n)) = self
                .guest_view
                .hits
                .iter()
                .find(|(r, _)| r.contains((event.column, event.row).into()))
                .copied()
            {
                self.guest_request(engine, n);
            }
        } else if event.kind == MouseEventKind::ScrollUp {
            self.guest_view.source_top = self.guest_view.source_top.saturating_sub(3);
        } else if event.kind == MouseEventKind::ScrollDown {
            self.guest_view.source_top = (self.guest_view.source_top + 3)
                .min(self.guest_view.source.len().saturating_sub(1));
        }
        true
    }
    pub(super) fn guest_draw(&mut self, f: &mut UiFrame) {
        if !self.guest_view.open {
            return;
        }
        let area = f.area();
        let rect = Rect::new(
            area.x + 1,
            area.y + 1,
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );
        theme::surface(f, rect, theme::PANEL);
        let block = Block::bordered()
            .title(format!(
                " Saved Guest {} · read only · physical CPU unchanged ",
                self.guest_view.image
            ))
            .style(theme::base());
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        let rows = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(2),
            Constraint::Length(1),
        ])
        .split_cached(inner);
        let saved = &self.guest_view.value["saved"];
        let status = if self.guest_view.pending.is_some() {
            "Reading saved context…".into()
        } else if !self.guest_view.error.is_empty() {
            self.guest_view.error.clone()
        } else {
            format!(
                "CPU {} / VM {} · saved PC 0x{:08x} SP 0x{:08x} CPSR 0x{:08x} · sequence {}",
                saved["cpu"],
                saved["vm"],
                saved["registers"][15].as_u64().unwrap_or(0),
                saved["registers"][13].as_u64().unwrap_or(0),
                saved["registers"][16].as_u64().unwrap_or(0),
                saved["sequence"]
            )
        };
        f.render_widget(
            Paragraph::new(status)
                .wrap(Wrap { trim: false })
                .style(theme::base()),
            rows[0],
        );
        let cols = Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split_cached(rows[1]);
        let frame = self
            .guest_view
            .stack
            .iter()
            .find(|f| f.level as usize == self.guest_view.selected);
        let lines: Vec<Line> = self
            .guest_view
            .source
            .iter()
            .enumerate()
            .skip(self.guest_view.source_top)
            .take(cols[0].height.saturating_sub(2) as usize)
            .map(|(n, text)| {
                Line::from(format!(
                    "{}{:5} {text}",
                    if frame.is_some_and(|f| f.line as usize == n + 1) {
                        '>'
                    } else {
                        ' '
                    },
                    n + 1
                ))
                .style(if frame.is_some_and(|f| f.line as usize == n + 1) {
                    Style::default().fg(theme::ACCENT)
                } else {
                    theme::base()
                })
            })
            .collect();
        f.render_widget(
            Paragraph::new(lines).block(Block::bordered().title(" Guest source ")),
            cols[0],
        );
        let side = Layout::vertical([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split_cached(cols[1]);
        self.guest_view.hits.clear();
        let stack_inner = Block::bordered().inner(side[0]);
        let stack: Vec<Line> = self
            .guest_view
            .stack
            .iter()
            .take(stack_inner.height as usize)
            .enumerate()
            .map(|(n, fr)| {
                self.guest_view.hits.push((
                    Rect::new(
                        stack_inner.x,
                        stack_inner.y + n as u16,
                        stack_inner.width,
                        1,
                    ),
                    fr.level as usize,
                ));
                Line::from(format!(
                    "{}#{} {}:{}",
                    if fr.level as usize == self.guest_view.selected {
                        '>'
                    } else {
                        ' '
                    },
                    fr.level,
                    fr.function,
                    fr.line
                ))
            })
            .collect();
        f.render_widget(
            Paragraph::new(stack).block(Block::bordered().title(" Saved Guest stack ")),
            side[0],
        );
        let locals: Vec<Line> = self
            .guest_view
            .locals
            .iter()
            .take(side[1].height.saturating_sub(2) as usize)
            .map(|v| Line::from(format!("{} = {}", v.name, v.value)))
            .collect();
        f.render_widget(
            Paragraph::new(locals).block(
                Block::bordered().title(" Guest locals · optimized values stay unavailable "),
            ),
            side[1],
        );
        f.render_widget(
            Paragraph::new(
                "↑↓ / click: frame  PgUp/PgDn: source  r: return Guest after HVC  Esc: physical",
            )
            .style(theme::base()),
            rows[2],
        );
    }
}
