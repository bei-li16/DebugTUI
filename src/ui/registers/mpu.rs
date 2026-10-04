//! Explicit MPU overview. Opening, scrolling and switching banks only display cached evidence.
use super::*;
use crate::registers::mpu::{Bank, View};

pub(super) struct Popup {
    bank: Bank,
    count: Option<(Context, u8)>,
    scroll: usize,
    max_scroll: usize,
    button: usize,
    hits: Vec<(Rect, usize)>,
    pending: Option<u64>,
    error: Option<String>,
}
impl App {
    pub(in crate::ui) fn open_mpu_view(&mut self, arg: &str) {
        if self
            .register_view
            .catalogue
            .as_ref()
            .is_none_or(|c| c.architecture != "armv8-r-aarch32")
        {
            self.notice = "MPU overview requires an R52 AArch32 catalogue.".into();
            return;
        }
        let bank = match arg.trim() {
            "el1" => Bank::El1,
            "el2" => Bank::El2,
            "" => {
                if self
                    .register_view
                    .register_index(self.selected(3))
                    .is_some_and(|i| {
                        self.register_view.catalogue.as_ref().unwrap().registers[i].group
                            == "mpu_el2"
                    })
                {
                    Bank::El2
                } else {
                    Bank::El1
                }
            }
            _ => {
                self.notice = "Use :mpu, :mpu el1 or :mpu el2".into();
                return;
            }
        };
        self.register_view.mpu_popup = Some(Popup {
            bank,
            count: None,
            scroll: 0,
            max_scroll: 0,
            button: 0,
            hits: vec![],
            pending: None,
            error: None,
        });
    }
    fn request_mpu_regions(&mut self, engine: Option<&EngineHandle>) {
        if self.demo
            || engine.is_none()
            || self.snapshot.state != "STOPPED"
            || self.register_view.pending.is_some()
            || self.register_view.probe_pending.is_some()
        {
            return;
        }
        let Some(popup) = self.register_view.mpu_popup.as_ref() else {
            return;
        };
        let bank = popup.bank;
        let context = self.register_context();
        if context.frame != 0
            || self.snapshot.register_probe.as_ref().is_none_or(|p| {
                p.context != context
                    || p.identity
                        .as_ref()
                        .is_none_or(|i| i.model.as_deref() != Some("Cortex-R52"))
                    || !p.facts.contains_key(bank.count_fact())
            })
        {
            self.register_view.mpu_popup.as_mut().unwrap().error =
                Some("Probe this physical R52 core at frame 0 before reading regions".into());
            return;
        }
        let id = self.next_id;
        self.register_view.pending = Some((id, context.clone()));
        let popup = self.register_view.mpu_popup.as_mut().unwrap();
        popup.pending = Some(id);
        popup.error = None;
        self.submit(
            engine,
            "registers_mpu",
            json!({"context":context,"bank":bank,"read":true}),
        );
        if !self.pending_commands.contains(&id) {
            self.register_view.pending = None;
            self.register_view.mpu_popup.as_mut().unwrap().pending = None;
        }
    }
    pub(super) fn mpu_read_response(&mut self, id: u64, error: Option<&str>) {
        if let Some(popup) = self
            .register_view
            .mpu_popup
            .as_mut()
            .filter(|p| p.pending == Some(id))
        {
            popup.pending = None;
            popup.error = error.map(String::from);
        }
    }
    fn activate_mpu_button(&mut self, engine: Option<&EngineHandle>) {
        let Some(popup) = self.register_view.mpu_popup.as_mut() else {
            return;
        };
        match popup.button {
            0 => {
                popup.bank = if popup.bank == Bank::El1 {
                    Bank::El2
                } else {
                    Bank::El1
                };
                popup.count = None;
                popup.scroll = 0;
                popup.error = None;
                popup.pending = None;
            }
            1 => self.probe_registers(engine),
            2 => self.request_mpu_regions(engine),
            _ => self.register_view.mpu_popup = None,
        }
    }
    pub(in crate::ui) fn mpu_key(&mut self, key: KeyEvent, engine: Option<&EngineHandle>) -> bool {
        let Some(popup) = self.register_view.mpu_popup.as_mut() else {
            return false;
        };
        match key.code {
            KeyCode::Esc => self.register_view.mpu_popup = None,
            KeyCode::Tab | KeyCode::Right => popup.button = (popup.button + 1) % 4,
            KeyCode::BackTab | KeyCode::Left => popup.button = (popup.button + 3) % 4,
            KeyCode::Up => popup.scroll = popup.scroll.saturating_sub(1),
            KeyCode::Down => popup.scroll = (popup.scroll + 1).min(popup.max_scroll),
            KeyCode::PageUp => popup.scroll = popup.scroll.saturating_sub(8),
            KeyCode::PageDown => popup.scroll = (popup.scroll + 8).min(popup.max_scroll),
            KeyCode::Home => popup.scroll = 0,
            KeyCode::End => popup.scroll = popup.max_scroll,
            KeyCode::Enter => self.activate_mpu_button(engine),
            KeyCode::Char('r') => self.request_mpu_regions(engine),
            KeyCode::Char('p') => self.probe_registers(engine),
            KeyCode::Char('b') => {
                popup.button = 0;
                self.activate_mpu_button(engine);
            }
            _ => {}
        }
        true
    }
    pub(in crate::ui) fn mpu_mouse(
        &mut self,
        mouse: MouseEvent,
        engine: Option<&EngineHandle>,
    ) -> bool {
        let Some(popup) = self.register_view.mpu_popup.as_mut() else {
            return false;
        };
        match mouse.kind {
            MouseEventKind::ScrollDown => popup.scroll = (popup.scroll + 3).min(popup.max_scroll),
            MouseEventKind::ScrollUp => popup.scroll = popup.scroll.saturating_sub(3),
            MouseEventKind::Down(event::MouseButton::Left) => {
                if let Some((_, button)) = popup
                    .hits
                    .iter()
                    .find(|(r, _)| r.contains((mouse.column, mouse.row).into()))
                {
                    popup.button = *button;
                    self.activate_mpu_button(engine);
                }
            }
            _ => {}
        }
        true
    }
}
fn evidence(sample: Option<&Sample>, context: &Context) -> String {
    match sample {
        None => "Not read".into(),
        Some(s) => format!(
            "{} · {:?} · {} {}",
            s.value.as_ref().map(|v| v.hex.as_str()).unwrap_or("—"),
            if crate::registers::mpu::current(Some(s), context).is_some() {
                State::Valid
            } else if s.state == State::Valid {
                State::Stale
            } else {
                s.state
            },
            s.source,
            s.detail
        ),
    }
}
fn content(view: &View) -> Vec<String> {
    let flag = |v: Option<bool>| v.map(|v| if v { "on" } else { "off" }).unwrap_or("unknown");
    let mut lines = vec![
        format!(
            "{} {:?} · {} regions · M={} BR={}",
            view.owner,
            view.bank,
            view.count,
            flag(view.global_enabled),
            flag(view.background_enabled)
        ),
        format!(
            "Control: {}",
            evidence(view.control.as_ref(), &view.context)
        ),
    ];
    for (index, sample) in view.mair.iter().enumerate() {
        lines.push(format!(
            "MAIR{index}: {}",
            evidence(sample.as_ref(), &view.context)
        ));
    }
    if view.bank == Bank::El2 {
        lines.push(format!(
            "HCR: {}",
            evidence(view.hcr.as_ref(), &view.context)
        ));
        lines.push(format!(
            "HPRENR: {}",
            evidence(view.hprenr.as_ref(), &view.context)
        ));
    }
    for region in &view.regions {
        if let Some(r) = &region.decoded {
            lines.push(format!(
                "#{:02} {}–{} · {} · {} · Attr{}",
                region.index,
                r.base,
                r.limit_inclusive,
                if r.enabled { "enabled" } else { "disabled" },
                if r.execute_never { "XN" } else { "executable" },
                r.attribute_index
            ));
            lines.push(format!("  {} · SH (Normal): {}", r.access, r.shareability));
            lines.push(format!(
                "  {}",
                region
                    .attribute
                    .as_ref()
                    .map(|a| format!("{} {}", a.raw, a.label()))
                    .unwrap_or("Memory type unknown".into())
            ));
        } else {
            lines.push(format!("#{:02} pair unavailable/stale", region.index));
        }
        lines.push(format!(
            "  Base: {}",
            evidence(region.base.as_ref(), &view.context)
        ));
        lines.push(format!(
            "  Limit: {}",
            evidence(region.limit.as_ref(), &view.context)
        ));
        for issue in &region.issues {
            lines.push(format!("  ! {issue}"));
        }
    }
    lines.extend(view.notes.iter().cloned());
    lines
}
fn wrap(lines: Vec<String>, width: usize) -> Vec<Line<'static>> {
    use unicode_width::UnicodeWidthChar;
    let mut result = vec![];
    for line in lines {
        let mut text = String::new();
        let mut used = 0;
        for c in line.chars() {
            let n = c.width().unwrap_or(0);
            if used + n > width && !text.is_empty() {
                result.push(Line::from(std::mem::take(&mut text)));
                used = 0;
            }
            text.push(c);
            used += n;
        }
        result.push(Line::from(text));
    }
    result
}
pub(in crate::ui) fn draw(f: &mut UiFrame, app: &mut App) {
    let context = app.register_context();
    let Some(popup) = app.register_view.mpu_popup.as_ref() else {
        return;
    };
    let bank = popup.bank;
    let fresh = app
        .snapshot
        .register_probe
        .as_ref()
        .filter(|p| {
            p.context == context
                && app.snapshot.state == "STOPPED"
                && p.identity
                    .as_ref()
                    .is_some_and(|i| i.model.as_deref() == Some("Cortex-R52"))
        })
        .and_then(|p| p.facts.get(bank.count_fact()))
        .and_then(|f| bank.validate_count(f.value).ok());
    if let Some(n) = fresh {
        app.register_view.mpu_popup.as_mut().unwrap().count = Some((context.clone(), n));
    }
    let popup = app.register_view.mpu_popup.as_ref().unwrap();
    let mut lines = vec![];
    if let Some((observed, count)) = &popup.count {
        if *observed != context || fresh.is_none() {
            lines.push("Region count is last-known; probe current stop before reading".into());
        }
        let samples: Vec<_> = app
            .register_view
            .values
            .values()
            .cloned()
            .map(|mut s| {
                if app.snapshot.state != "STOPPED" {
                    s.stale();
                }
                s
            })
            .collect();
        match View::from_samples(bank, u64::from(*count), &context, &samples) {
            Ok(view) => lines.extend(content(&view)),
            Err(e) => lines.push(e),
        }
    } else {
        lines.push(format!(
            "{} {:?}: implementation count unknown",
            context.core, bank
        ));
        lines.push(
            "Probe caps explicitly at physical frame 0; opening this view reads nothing".into(),
        );
    }
    if popup.pending.is_some() {
        lines.insert(0, "Reading current-core direct MPU registers…".into());
    }
    if let Some(error) = &popup.error {
        lines.insert(0, format!("Error: {error}"));
    }
    let area = f.area();
    let w = area.width.min(112);
    let h = area.height.min(30);
    let rect = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    );
    theme::overlay(f, rect);
    let card = theme::card(" MPU regions · ↑↓ scroll · Tab actions · Esc close ", true);
    let inner = card.inner(rect);
    f.render_widget(card, rect);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let lines = wrap(lines, usize::from(inner.width));
    let visible = inner.height.saturating_sub(1);
    let popup = app.register_view.mpu_popup.as_mut().unwrap();
    popup.max_scroll = lines.len().saturating_sub(usize::from(visible));
    popup.scroll = popup.scroll.min(popup.max_scroll);
    popup.hits.clear();
    f.render_widget(
        Paragraph::new(
            lines
                .into_iter()
                .skip(popup.scroll)
                .take(usize::from(visible))
                .collect::<Vec<_>>(),
        ),
        Rect::new(inner.x, inner.y, inner.width, visible),
    );
    let labels = [
        if bank == Bank::El1 {
            "EL1 / EL2"
        } else {
            "EL2 / EL1"
        },
        "Probe",
        "Read",
        "Close",
    ];
    for (button, label) in labels.iter().enumerate() {
        let start = inner.width * button as u16 / 4;
        let end = inner.width * (button as u16 + 1) / 4;
        let r = Rect::new(inner.x + start, inner.bottom() - 1, end - start, 1);
        f.render_widget(
            Paragraph::new(*label).style(theme::selected(button == popup.button)),
            r,
        );
        popup.hits.push((r, button));
    }
}

#[cfg(test)]
mod tests;
