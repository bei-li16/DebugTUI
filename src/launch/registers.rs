//! Read-only catalogue previews. Configured associations and current evidence stay separate.
use super::*;
use crate::registers::{Catalogue, Context, capabilities::Probe};
use std::collections::BTreeSet;

/// Catalogue spelling and the display spelling used by an adapted identity.
pub(crate) fn mismatch(label: &str, expected: &str, catalogue: &str) -> Option<String> {
    let normalized = |cpu: &str| cpu.to_ascii_lowercase().replace(' ', "-");
    (!expected.is_empty() && normalized(expected) != normalized(catalogue))
        .then(|| format!("Warning: {label} {expected} differs from catalogue CPU {catalogue}."))
}

fn target_key(project: &Project) -> serde_json::Value {
    let mut access = project.registers.clone();
    // Switching the displayed catalogue keeps the same target. Switching an
    // actual access route must never lend the old target's identity to it.
    access.cpu.clear();
    access.catalogue.clear();
    serde_json::json!({
        "debug": project.debug,
        "cores": project.cores,
        "target": project.target,
        "service": project.service,
        "gdb": project.gdb,
        "register_access": access,
        "live_watch": project.live_watch,
    })
}

#[derive(Clone, PartialEq)]
pub(super) struct Observation {
    target: serde_json::Value,
    context: Context,
    model: Option<String>,
}

impl Observation {
    pub(super) fn current(
        project: &Project,
        probe: Option<&Probe>,
        context: &Context,
        stopped: bool,
    ) -> Option<Self> {
        let probe = probe.filter(|probe| stopped && &probe.context == context)?;
        Some(Self {
            target: target_key(project),
            context: context.clone(),
            model: probe
                .identity
                .as_ref()
                .and_then(|identity| identity.model.clone()),
        })
    }
}

impl Setup {
    pub(super) fn catalogue_preview_key(&self) -> PreviewKey {
        PreviewKey {
            document: self.document.raw.clone(),
            observation: self.register_observation.clone(),
        }
    }
    pub(crate) fn observe_register_target(
        &mut self,
        project: &Project,
        probe: Option<&Probe>,
        context: &Context,
        stopped: bool,
    ) {
        self.register_observation = Observation::current(project, probe, context, stopped);
    }
    pub(super) fn catalogue_preview(&self) -> Result<Vec<String>, String> {
        let mut document = self.document.clone();
        if let Some(Choice::Cpu { id, .. }) = self
            .picker
            .as_ref()
            .and_then(|picker| picker.choices.get(picker.selected))
        {
            document.select_register_cpu(id.as_deref());
        } else if self.selected == CATALOGUE {
            if let Some(path) = self
                .browser
                .as_ref()
                .and_then(|browser| browser.entries.get(browser.selected))
            {
                if !path.is_dir() {
                    document.set_path("registers", "catalogue", path);
                }
            } else if let Some(editor) = &self.editor {
                let value = editor.text.trim().trim_matches('"');
                if value.is_empty() {
                    document.set("registers", "catalogue", "".into());
                } else {
                    let path = absolute(document.base(), Path::new(value));
                    document.set_path("registers", "catalogue", &path);
                }
            }
        }
        let project = document.project()?;
        let loaded = project.registers.load()?;
        let association = if project.debug.chip.is_empty() {
            None
        } else {
            crate::devices::Catalogue::user()?.cpu_association(&project.debug.chip)?
        };
        Ok(preview(
            &project,
            loaded.as_ref(),
            association.as_ref(),
            self.register_observation.as_ref(),
        ))
    }
}

pub(super) fn preview(
    project: &Project,
    loaded: Option<&(Catalogue, String)>,
    association: Option<&(String, &'static str)>,
    observation: Option<&Observation>,
) -> Vec<String> {
    let mut lines = vec![];
    let cpu = loaded.map(|(catalogue, _)| catalogue.cpu.as_str());
    if let Some((catalogue, source)) = loaded {
        lines.push(format!("Source: {source}; CPU: {}", catalogue.cpu));
        lines.push(format!("Architecture: {}", catalogue.architecture));
        lines.push(format!(
            "Configured CPU: {}",
            if project.registers.cpu.is_empty() {
                "unspecified"
            } else {
                &project.registers.cpu
            }
        ));
        if let Some(warning) = mismatch("configured CPU", &project.registers.cpu, &catalogue.cpu) {
            lines.push(warning);
            lines.push(
                if project.registers.catalogue.as_os_str().is_empty() {
                    "A user preset name can differ from its declared CPU; review the definition."
                } else {
                    "The catalogue file takes precedence over the CPU preset."
                }
                .into(),
            );
        }
    } else {
        lines.push("Source: GDB target description".into());
    }
    if let Some((chip_cpu, source)) = association {
        lines.push(format!(
            "Chip association: {} / {chip_cpu} ({source}); configuration only",
            project.debug.chip
        ));
        if let Some(warning) = cpu.and_then(|cpu| mismatch("chip association", chip_cpu, cpu)) {
            lines.push(warning);
        }
    } else {
        lines.push("Chip CPU association: unspecified; hardware identity remains unknown.".into());
    }
    let observation = observation.filter(|observation| observation.target == target_key(project));
    let cores: Vec<_> = if project.cores.is_empty() {
        vec!["default"]
    } else {
        project
            .cores
            .iter()
            .map(|core| core.name.as_str())
            .collect()
    };
    for core in cores {
        let evidence = observation.filter(|observation| observation.context.core == core);
        let model = evidence.and_then(|observation| observation.model.as_deref());
        lines.push(format!(
            "Observed CPU [{core}]: {}",
            model.unwrap_or("Unknown; no current adapted identity evidence")
        ));
        if let Some(warning) = cpu.and_then(|cpu| {
            model.and_then(|model| mismatch(&format!("observed CPU [{core}]"), model, cpu))
        }) {
            lines.push(warning);
        }
        if let Some(evidence) = evidence {
            lines.push(format!(
                "Identity context: session {} / stop {} / frame {}",
                evidence.context.session, evidence.context.generation, evidence.context.frame
            ));
        }
    }
    lines.push("Selection does not prove hardware, revision, optional extensions or reader support. No hardware access is performed.".into());
    if let Some((catalogue, _)) = loaded {
        lines.push(format!(
            "Catalogue: {} groups / {} register definitions",
            catalogue.groups.len(),
            catalogue.registers.len()
        ));
        lines.push(catalogue.description.clone());
        lines.push("Support conditions (definitions, not observed capabilities):".into());
        let conditions: BTreeSet<_> = catalogue
            .registers
            .iter()
            .filter_map(|register| {
                (!register.access_condition.is_empty())
                    .then_some(register.access_condition.as_str())
            })
            .collect();
        lines.extend(conditions.into_iter().map(str::to_owned));
        let facts: BTreeSet<_> = catalogue
            .registers
            .iter()
            .flat_map(|register| &register.conditions)
            .map(|condition| {
                format!(
                    "Fact {}: >= {}{}",
                    condition.fact,
                    condition.min,
                    condition
                        .max
                        .map(|max| format!(" and <= {max}"))
                        .unwrap_or_default()
                )
            })
            .collect();
        lines.extend(facts);
        if catalogue
            .registers
            .iter()
            .any(|register| !register.conditions.is_empty())
        {
            lines.push("Optional definitions also require their per-register capability facts; Unknown is not Not implemented.".into());
        }
    }
    lines.push(
        "Esc / Close returns to the draft. Save config or Start persists the selection.".into(),
    );
    lines
}

#[derive(PartialEq)]
pub(super) struct PreviewKey {
    document: toml::Value,
    observation: Option<Observation>,
}

#[derive(Default)]
pub(super) struct Details {
    pub scroll: usize,
    pub max_scroll: usize,
    close: Rect,
    lines: Vec<String>,
    wrapped: Vec<String>,
    wrap_width: u16,
    key: Option<PreviewKey>,
}

impl Details {
    pub(super) fn new(preview: Result<Vec<String>, String>, key: PreviewKey) -> Self {
        let mut lines = preview.unwrap_or_else(|error| {
            vec![
                format!("Catalogue error: {error}"),
                "Esc / Close returns to the draft. No file was changed.".into(),
            ]
        });
        lines.push("Preview loaded when opened. Close and reopen to reload the file.".into());
        Self {
            lines,
            key: Some(key),
            ..Default::default()
        }
    }
    pub(super) fn matches(&self, key: &PreviewKey) -> bool {
        self.key.as_ref() == Some(key)
    }
    pub(super) fn key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::F(1) => return true,
            KeyCode::Down => self.scroll = (self.scroll + 1).min(self.max_scroll),
            KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::PageDown => self.scroll = (self.scroll + 6).min(self.max_scroll),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(6),
            KeyCode::Home => self.scroll = 0,
            KeyCode::End => self.scroll = self.max_scroll,
            _ => {}
        }
        false
    }
    pub(super) fn mouse(&mut self, mouse: MouseEvent) -> bool {
        match mouse.kind {
            MouseEventKind::ScrollDown => self.scroll = (self.scroll + 3).min(self.max_scroll),
            MouseEventKind::ScrollUp => self.scroll = self.scroll.saturating_sub(3),
            MouseEventKind::Down(MouseButton::Left) => {
                return self.close.contains((mouse.column, mouse.row).into());
            }
            _ => {}
        }
        false
    }
    pub(super) fn draw(&mut self, f: &mut Frame) {
        let screen = f.area();
        let width = screen.width.min(100);
        let height = screen.height.min(30);
        let rect = Rect::new(
            screen.x + (screen.width - width) / 2,
            screen.y + (screen.height - height) / 2,
            width,
            height,
        );
        theme::overlay(f, rect);
        let block = theme::card(" Register catalogue / preview ", true);
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        if inner.height < 2 || inner.width == 0 {
            return;
        }
        if self.wrap_width != inner.width {
            self.wrapped.clear();
            self.wrap_width = inner.width;
            for line in &self.lines {
                let mut row = String::new();
                let mut used = 0;
                for ch in line.chars() {
                    let cells = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                    if ch == '\n' || used + cells > usize::from(inner.width) {
                        self.wrapped.push(std::mem::take(&mut row));
                        used = 0;
                    }
                    if ch != '\n' {
                        row.push(ch);
                        used += cells;
                    }
                }
                self.wrapped.push(row);
            }
        }
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
            )
            .style(Style::default().fg(theme::TEXT)),
            body,
        );
        self.close = Rect::new(inner.x, inner.bottom() - 1, inner.width, 1);
        f.render_widget(
            Paragraph::new("Up/Down scroll · Esc / Close")
                .style(Style::default().fg(theme::ACCENT)),
            self.close,
        );
    }
}

#[cfg(test)]
mod tests;
