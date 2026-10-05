//! Counts of expanded register rows, using the same context rules as their values.
use super::*;
use crate::registers::Reason;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Category {
    NotRead,
    Valid,
    NotImplemented,
    Unavailable,
    ReaderUnsupported,
    Error,
    WriteOnly,
    Stale,
}
const CATEGORIES: [Category; 8] = [
    Category::NotRead,
    Category::Valid,
    Category::NotImplemented,
    Category::Unavailable,
    Category::ReaderUnsupported,
    Category::Error,
    Category::WriteOnly,
    Category::Stale,
];
impl Category {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::NotRead => "Not read",
            Self::Valid => "Valid",
            Self::NotImplemented => "Not implemented",
            Self::Unavailable => "Unavailable",
            Self::ReaderUnsupported => "Reader unsupported",
            Self::Error => "Error",
            Self::WriteOnly => "Write only",
            Self::Stale => "Stale",
        }
    }
}
pub(super) struct Counts {
    pub total: usize,
    pub shown: usize,
    pub readable: usize,
    categories: [usize; 8],
}
impl Counts {
    pub(super) fn get(&self, category: Category) -> usize {
        self.categories[category as usize]
    }
    pub(super) fn compact(&self) -> String {
        format!(
            "Total {}  Shown {}  Valid {}",
            self.total,
            self.shown,
            self.get(Category::Valid)
        )
    }
}
impl RegisterView {
    pub(super) fn category(
        &self,
        project: &Project,
        context: &Context,
        index: usize,
        stopped: bool,
    ) -> Category {
        let register = &self.catalogue.as_ref().unwrap().registers[index];
        if register.implementation(&self.facts).0 == Implementation::No {
            return Category::NotImplemented;
        }
        if !register.access.readable() {
            return Category::WriteOnly;
        }
        let Some(sample) = self.sample(project, context, index) else {
            return Category::NotRead;
        };
        if !stopped || !sample.applies(context, self.owner(project, context, index).as_deref()) {
            return Category::Stale;
        }
        if sample.implementation == Implementation::No
            || sample.reason == Reason::HardwareNotImplemented
        {
            return Category::NotImplemented;
        }
        if sample.reason == Reason::ReaderUnsupported {
            return Category::ReaderUnsupported;
        }
        match sample.state {
            State::Valid if sample.value.is_some() => Category::Valid,
            State::Valid | State::Error => Category::Error,
            State::NotRead => Category::NotRead,
            State::Unavailable | State::Unsupported => Category::Unavailable,
            State::Stale => Category::Stale,
        }
    }
    pub(super) fn counts(&self, project: &Project, context: &Context, stopped: bool) -> Counts {
        let mut counts = Counts {
            total: self.catalogue.as_ref().map_or(0, |c| c.registers.len()),
            shown: 0,
            readable: 0,
            categories: [0; 8],
        };
        for row in &self.rows {
            let Row::Register(index, _) = row else {
                continue;
            };
            let category = self.category(project, context, *index, stopped);
            counts.shown += 1;
            counts.categories[category as usize] += 1;
            if self.catalogue.as_ref().unwrap().registers[*index]
                .access
                .readable()
                && category != Category::NotImplemented
            {
                counts.readable += 1;
            }
        }
        counts
    }
}
pub(super) struct Popup {
    scroll: usize,
    max_scroll: usize,
    close: Rect,
}
impl App {
    pub(in crate::ui) fn open_register_status(&mut self) {
        if self.register_view.enabled() {
            self.register_view.status_popup = Some(Popup {
                scroll: 0,
                max_scroll: 0,
                close: Rect::default(),
            });
        }
    }
    pub(in crate::ui) fn register_status_key(&mut self, key: KeyEvent) -> bool {
        let Some(popup) = self.register_view.status_popup.as_mut() else {
            return false;
        };
        match key.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('t') => {
                self.register_view.status_popup = None
            }
            KeyCode::Up => popup.scroll = popup.scroll.saturating_sub(1),
            KeyCode::Down => popup.scroll = (popup.scroll + 1).min(popup.max_scroll),
            KeyCode::PageUp => popup.scroll = popup.scroll.saturating_sub(6),
            KeyCode::PageDown => popup.scroll = (popup.scroll + 6).min(popup.max_scroll),
            KeyCode::Home => popup.scroll = 0,
            KeyCode::End => popup.scroll = popup.max_scroll,
            _ => {}
        }
        true
    }
    pub(in crate::ui) fn register_status_mouse(&mut self, mouse: MouseEvent) -> bool {
        let Some(popup) = self.register_view.status_popup.as_mut() else {
            return false;
        };
        match mouse.kind {
            MouseEventKind::ScrollDown => popup.scroll = (popup.scroll + 3).min(popup.max_scroll),
            MouseEventKind::ScrollUp => popup.scroll = popup.scroll.saturating_sub(3),
            MouseEventKind::Down(event::MouseButton::Left)
                if popup.close.contains((mouse.column, mouse.row).into()) =>
            {
                self.register_view.status_popup = None
            }
            _ => {}
        }
        true
    }
}
pub(in crate::ui) fn draw(f: &mut UiFrame, app: &mut App) {
    if app.register_view.status_popup.is_none() {
        return;
    }
    let context = app.register_context();
    let counts = app
        .register_view
        .counts(&app.project, &context, app.snapshot.state == "STOPPED");
    let mut text = vec![
        format!("Catalogue total: {}", counts.total),
        format!("Shown registers: {}", counts.shown),
        format!("Readable definitions: {}", counts.readable),
    ];
    text.extend(
        CATEGORIES
            .iter()
            .map(|c| format!("{}: {}", c.label(), counts.get(*c))),
    );
    text.push("Shown excludes collapsed groups and fields. Valid requires the current owner and stop context.".into());
    text.push(format!(
        "Core {} / frame {} / stop {} / session {}",
        context.core, context.frame, context.generation, context.session
    ));
    if let Some(index) = app.register_view.register_index(app.selected(3)) {
        let register = &app.register_view.catalogue.as_ref().unwrap().registers[index];
        let category = app.register_view.category(
            &app.project,
            &context,
            index,
            app.snapshot.state == "STOPPED",
        );
        text.push(format!(
            "Selected: {} · {} bits {} · {}",
            register.name,
            register.bits,
            register.access.label(),
            category.label()
        ));
        text.push(format!(
            "Owner: {} · {:?}",
            app.register_view
                .owner(&app.project, &context, index)
                .unwrap_or_else(|| "unknown".into()),
            register.scope
        ));
        if let Some(sample) = app.register_view.sample(&app.project, &context, index) {
            text.push(match sample.view {
                crate::registers::SampleView::SelectedFrame => format!(
                    "Sampling view: selected stack frame {}",
                    sample.context.frame
                ),
                crate::registers::SampleView::PhysicalCore => {
                    "Sampling view: physical core state".into()
                }
            });
            text.push("Raw (current / last-known):".into());
            text.push(
                sample
                    .value
                    .as_ref()
                    .map(|v| v.hex.clone())
                    .unwrap_or_else(|| "none".into()),
            );
            text.push(format!(
                "sample at {} ms · {:?}",
                sample.timestamp_ms, sample.state
            ));
            if let Some(previous) = app
                .register_view
                .previous
                .get(&(sample.owner.clone().unwrap_or_default(), sample.id.clone()))
            {
                text.push(format!("Last valid sample at {} ms", previous.timestamp_ms));
            }
            text.push(format!("Reader source: {}", sample.source));
            text.push(format!("Reason: {:?} · {}", sample.reason, sample.detail));
        }
        text.push(format!("Access condition: {}", register.access_condition));
        if let Some(Row::Field(_, field, _)) = app.register_view.rows.get(app.selected(3)) {
            let field = &register.fields[*field];
            text.push(register.description.clone());
            text.push(format!("Field {}: {}", field.name, field.description));
            let bits: u16 = field.segments.iter().map(|segment| segment.width).sum();
            text.push(format!(
                "Field width: {bits} bits · {}",
                field.access.unwrap_or(register.access).label()
            ));
            text.push(format!(
                "Segments (logical low bits first): {}",
                field
                    .segments
                    .iter()
                    .map(|segment| format!(
                        "[{}:{}]",
                        segment.offset + segment.width - 1,
                        segment.offset
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            if let Some(value) = app
                .register_view
                .sample(&app.project, &context, index)
                .and_then(|sample| sample.value.as_ref())
                .and_then(|raw| field.extract(raw).ok())
            {
                text.push(format!(
                    "Field raw (current / last-known): {} · {}",
                    value.hex,
                    field.enum_name(&value).unwrap_or("no matching enum")
                ));
            }
            for entry in &field.enums {
                text.push(format!("Enum {} = {}", entry.value, entry.name));
            }
        } else {
            text.push(register.description.clone());
        }
    }
    text.push(format!("Catalogue: {}", app.register_view.source));
    let area = f.area();
    let width = area.width.min(72);
    let height = area.height.min(22);
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    theme::overlay(f, rect);
    let block = theme::card(" Register status ", true);
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if inner.height < 2 || inner.width == 0 {
        return;
    }
    let lines = super::mpu::wrap(text, usize::from(inner.width));
    let body = Rect::new(inner.x, inner.y, inner.width, inner.height - 1);
    let popup = app.register_view.status_popup.as_mut().unwrap();
    popup.max_scroll = lines.len().saturating_sub(usize::from(body.height));
    popup.scroll = popup.scroll.min(popup.max_scroll);
    f.render_widget(
        Paragraph::new(lines)
            .scroll((popup.scroll as u16, 0))
            .style(Style::default().fg(theme::TEXT)),
        body,
    );
    popup.close = Rect::new(inner.x, inner.bottom() - 1, inner.width, 1);
    f.render_widget(
        Paragraph::new("↑↓ scroll · Esc / Close").style(Style::default().fg(theme::ACCENT)),
        popup.close,
    );
}

#[cfg(test)]
mod tests;
