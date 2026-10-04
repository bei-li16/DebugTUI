//! ADS-style register groups and fields, backed by visible on-demand requests.
use super::*;
use crate::registers::{Catalogue, Context, Implementation, Sample, State};
use std::collections::{BTreeMap, BTreeSet};

pub(super) const ACTIONS: &[(&str, &str)] = &[
    ("↻ Read", "register-refresh"),
    ("Find", "register-search"),
    ("Group", "register-filter"),
    ("Target / All", "register-definitions"),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Row {
    Group(usize, usize),
    Register(usize, usize),
    Field(usize, usize, usize),
}

pub(super) struct RegisterView {
    catalogue: Option<Catalogue>,
    source: String,
    error: Option<String>,
    pub(super) rows: Vec<Row>,
    open: BTreeSet<String>,
    fields: BTreeSet<String>,
    values: BTreeMap<(String, String), Sample>,
    previous: BTreeMap<(String, String), Sample>,
    attempts: BTreeSet<(u64, u64, String, u32, String)>,
    pending: Option<(u64, Context)>,
    query: String,
    search_original: String,
    pub(super) searching: bool,
    filter: usize,
    all_definitions: bool,
    facts: BTreeMap<String, u64>,
}
impl RegisterView {
    pub(super) fn load(project: &Project) -> Self {
        let (catalogue, source, error) = match project.registers.load() {
            Ok(Some((catalogue, source))) => (Some(catalogue), source, None),
            Ok(None) => (None, String::new(), None),
            Err(error) => (None, String::new(), Some(error)),
        };
        let mut view = Self {
            catalogue,
            source,
            error,
            rows: vec![],
            open: BTreeSet::from(["core".into()]),
            fields: BTreeSet::new(),
            values: BTreeMap::new(),
            previous: BTreeMap::new(),
            attempts: BTreeSet::new(),
            pending: None,
            query: String::new(),
            search_original: String::new(),
            searching: false,
            filter: 0,
            all_definitions: false,
            facts: project.registers.facts.clone(),
        };
        view.rebuild();
        view
    }
    pub(super) fn enabled(&self) -> bool {
        self.catalogue.is_some() || self.error.is_some()
    }
    fn matches(&self, index: usize) -> bool {
        let register = &self.catalogue.as_ref().unwrap().registers[index];
        (self.all_definitions || register.implementation(&self.facts).0 != Implementation::No)
            && (self.query.is_empty()
                || format!("{} {} {}", register.name, register.description, register.id)
                    .to_lowercase()
                    .contains(&self.query.to_lowercase()))
    }
    fn group_matches(&self, index: usize) -> bool {
        let Some(catalogue) = &self.catalogue else {
            return false;
        };
        let group = &catalogue.groups[index];
        catalogue
            .registers
            .iter()
            .enumerate()
            .any(|(i, r)| r.group == group.id && self.matches(i))
            || catalogue
                .groups
                .iter()
                .enumerate()
                .any(|(i, g)| g.parent.as_deref() == Some(&group.id) && self.group_matches(i))
    }
    fn append_group(&mut self, index: usize, depth: usize) {
        if !self.group_matches(index) {
            return;
        }
        self.rows.push(Row::Group(index, depth));
        let catalogue = self.catalogue.as_ref().unwrap();
        let group = catalogue.groups[index].id.clone();
        if !self.open.contains(&group) && self.query.is_empty() {
            return;
        }
        let registers: Vec<_> = catalogue
            .registers
            .iter()
            .enumerate()
            .filter(|(i, r)| r.group == group && self.matches(*i))
            .map(|(i, _)| i)
            .collect();
        let children: Vec<_> = catalogue
            .groups
            .iter()
            .enumerate()
            .filter(|(_, g)| g.parent.as_deref() == Some(&group))
            .map(|(i, _)| i)
            .collect();
        for index in registers {
            self.rows.push(Row::Register(index, depth + 1));
            let register = &self.catalogue.as_ref().unwrap().registers[index];
            if self.fields.contains(&register.id) {
                for field in 0..register.fields.len() {
                    self.rows.push(Row::Field(index, field, depth + 2));
                }
            }
        }
        for child in children {
            self.append_group(child, depth + 1);
        }
    }
    fn rebuild(&mut self) {
        self.rows.clear();
        let Some(catalogue) = &self.catalogue else {
            return;
        };
        let filter = ["", "core", "simd", "system"][self.filter];
        let roots: Vec<_> = catalogue
            .groups
            .iter()
            .enumerate()
            .filter(|(_, g)| g.parent.is_none() && (filter.is_empty() || g.id == filter))
            .map(|(i, _)| i)
            .collect();
        for root in roots {
            self.append_group(root, 0);
        }
    }
    fn toggle(&mut self, row: usize, expand: Option<bool>) {
        let Some(row) = self.rows.get(row).cloned() else {
            return;
        };
        let catalogue = self.catalogue.as_ref().unwrap();
        let (set, id) = match row {
            Row::Group(index, _) => (&mut self.open, catalogue.groups[index].id.clone()),
            Row::Register(index, _) => (&mut self.fields, catalogue.registers[index].id.clone()),
            Row::Field(_, _, _) => return,
        };
        if expand.unwrap_or(!set.contains(&id)) {
            set.insert(id);
        } else {
            set.remove(&id);
        }
        self.rebuild();
    }
    fn register_index(&self, row: usize) -> Option<usize> {
        match self.rows.get(row)? {
            Row::Register(index, _) | Row::Field(index, _, _) => Some(*index),
            _ => None,
        }
    }
    fn owner(&self, project: &Project, context: &Context, index: usize) -> Option<String> {
        let mut topology = project.registers.topology.clone();
        if topology.chip.is_empty() {
            topology.chip = project.debug.chip.clone();
        }
        topology.owner(
            self.catalogue.as_ref()?.registers[index].scope,
            &context.core,
        )
    }
    fn sample(&self, project: &Project, context: &Context, index: usize) -> Option<&Sample> {
        let id = &self.catalogue.as_ref()?.registers[index].id;
        self.values.get(&(
            self.owner(project, context, index)
                .unwrap_or_else(|| format!("unknown:{}", context.core)),
            id.clone(),
        ))
    }
}

impl App {
    pub(super) fn register_context(&self) -> Context {
        Context {
            session: self.snapshot.register_session,
            generation: self.snapshot.generation,
            core: self
                .snapshot
                .core
                .as_ref()
                .map(|core| core.name.clone())
                .unwrap_or_else(|| "default".into()),
            frame: self.snapshot.frame.level,
        }
    }
    pub(super) fn register_action_labels(&self) -> Vec<&'static str> {
        ACTIONS.iter().map(|(label, _)| *label).collect()
    }
    pub(super) fn start_register_search(&mut self) {
        if !self.register_view.enabled() {
            return;
        }
        self.select_pane(3);
        self.register_view.search_original = self.register_view.query.clone();
        self.register_view.searching = true;
    }
    pub(super) fn filter_registers(&mut self) {
        self.register_view.filter = (self.register_view.filter + 1) % 4;
        self.register_view.rebuild();
        self.selection = 0;
        self.view_tops[3] = 0;
    }
    pub(super) fn toggle_register_definitions(&mut self) {
        self.register_view.all_definitions = !self.register_view.all_definitions;
        self.register_view.rebuild();
        self.selection = 0;
        self.view_tops[3] = 0;
    }
    pub(super) fn register_key(&mut self, key: KeyEvent, engine: Option<&EngineHandle>) -> bool {
        if !self.register_view.enabled() {
            return false;
        }
        if self.register_view.searching {
            match key.code {
                KeyCode::Esc => {
                    self.register_view.query = self.register_view.search_original.clone();
                    self.register_view.searching = false;
                }
                KeyCode::Enter => self.register_view.searching = false,
                KeyCode::Backspace => {
                    self.register_view.query.pop();
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.register_view.query.clear()
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.register_view.query.push(c)
                }
                _ => return false,
            }
            self.register_view.rebuild();
            self.selection = 0;
            self.view_tops[3] = 0;
            return true;
        }
        if !key.modifiers.is_empty() {
            return false;
        }
        match key.code {
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.register_view.toggle(self.selected(3), None)
            }
            KeyCode::Left => self.register_view.toggle(self.selected(3), Some(false)),
            KeyCode::Right => self.register_view.toggle(self.selected(3), Some(true)),
            KeyCode::Char('r') => {
                self.refresh_register(engine);
            }
            KeyCode::Char('s') => self.start_register_search(),
            KeyCode::Char('v') => self.filter_registers(),
            KeyCode::Char('a') => self.toggle_register_definitions(),
            _ => return false,
        }
        self.selection = self
            .selection
            .min(self.register_view.rows.len().saturating_sub(1));
        true
    }
    pub(super) fn register_mouse(&mut self, mouse: MouseEvent) -> bool {
        let rect = self.view_rects[3];
        if self.side_pane != 3
            || !self.register_view.enabled()
            || !rect.contains((mouse.column, mouse.row).into())
            || mouse.kind != MouseEventKind::Down(event::MouseButton::Left)
        {
            return false;
        }
        let row = self.view_tops[3] + usize::from(mouse.row - rect.y);
        if row >= self.register_view.rows.len() {
            return false;
        }
        self.select_pane(3);
        self.selection = row;
        self.selections[3] = row;
        let depth = match self.register_view.rows[row] {
            Row::Group(_, d) | Row::Register(_, d) | Row::Field(_, _, d) => d,
        };
        if mouse.column <= rect.x.saturating_add((depth * 2 + 2) as u16) {
            self.register_view.toggle(row, None);
        }
        true
    }
    fn request_registers(
        &mut self,
        engine: Option<&EngineHandle>,
        ids: Vec<String>,
        manual: bool,
    ) -> bool {
        if ids.is_empty()
            || engine.is_none()
            || self.demo
            || self.snapshot.state != "STOPPED"
            || self.register_view.pending.is_some()
        {
            return false;
        }
        let context = self.register_context();
        for id in &ids {
            self.register_view.attempts.insert((
                context.session,
                context.generation,
                context.core.clone(),
                context.frame,
                id.clone(),
            ));
        }
        self.register_view.pending = Some((self.next_id, context.clone()));
        let request_id = self.next_id;
        self.submit(
            engine,
            "registers_read",
            json!({"ids":ids,"context":context,"manual":manual}),
        );
        if !self.pending_commands.contains(&request_id) {
            self.register_view.pending = None;
            return false;
        }
        true
    }
    pub(super) fn refresh_register(&mut self, engine: Option<&EngineHandle>) -> bool {
        let Some(index) = self.register_view.register_index(self.selected(3)) else {
            self.notice = "Select a register or field to read.".into();
            return false;
        };
        let id = self.register_view.catalogue.as_ref().unwrap().registers[index]
            .id
            .clone();
        self.request_registers(engine, vec![id], true)
    }
    pub(super) fn ensure_registers(&mut self, engine: Option<&EngineHandle>) -> bool {
        if self.side_pane != 3
            || !self.register_view.enabled()
            || self.register_view.pending.is_some()
            || self.view_rects[3].height == 0
        {
            return false;
        }
        let Some(catalogue) = &self.register_view.catalogue else {
            return false;
        };
        let context = self.register_context();
        let mut ids = BTreeSet::new();
        for row in self.view_tops[3]..self.view_tops[3] + usize::from(self.view_rects[3].height) {
            let Some(index) = self.register_view.register_index(row) else {
                continue;
            };
            let register = &catalogue.registers[index];
            let implementation = register.implementation(&self.register_view.facts).0;
            if register.auto_read(implementation)
                && (register.conditions.is_empty() || implementation == Implementation::Yes)
                && !self.register_view.attempts.contains(&(
                    context.session,
                    context.generation,
                    context.core.clone(),
                    context.frame,
                    register.id.clone(),
                ))
            {
                ids.insert(register.id.clone());
            }
        }
        self.request_registers(engine, ids.into_iter().take(128).collect(), false)
    }
    pub(super) fn register_response(
        &mut self,
        id: u64,
        result: &Value,
        error: Option<&str>,
    ) -> bool {
        let Some((pending, context)) = self.register_view.pending.clone() else {
            return false;
        };
        if pending != id {
            return false;
        }
        self.register_view.pending = None;
        self.pending_commands.remove(&id);
        self.fx.response(id, error.is_none());
        if context != self.register_context() {
            return true;
        }
        if let Some(error) = error {
            self.notice = format!("Register read: {error}");
            return true;
        }
        let samples: Result<Vec<Sample>, _> =
            serde_json::from_value(result.get("samples").cloned().unwrap_or(json!([])));
        if let Ok(samples) = samples {
            for mut sample in samples {
                if sample.context != context {
                    continue;
                }
                let Some(index) = self.register_view.catalogue.as_ref().and_then(|catalogue| {
                    catalogue
                        .registers
                        .iter()
                        .position(|register| register.id == sample.id)
                }) else {
                    continue;
                };
                let expected_owner = self.register_view.owner(&self.project, &context, index);
                if sample.owner != expected_owner {
                    continue;
                }
                let owner = sample
                    .owner
                    .clone()
                    .unwrap_or_else(|| format!("unknown:{}", context.core));
                let key = (owner, sample.id.clone());
                if let Some(old) = self.register_view.values.get(&key)
                    && old.state == State::Valid
                {
                    self.register_view.previous.insert(key.clone(), old.clone());
                }
                if sample.value.is_none()
                    && let Some(previous) = self.register_view.previous.get(&key)
                {
                    sample.value = previous.value.clone();
                    sample.detail = format!(
                        "{}; last valid sample at {} ms",
                        sample.detail, previous.timestamp_ms
                    );
                }
                self.register_view.values.insert(key, sample);
            }
        } else {
            self.notice = "Invalid register response".into();
        }
        self.register_view
            .attempts
            .retain(|(session, generation, core, frame, _)| {
                (*session, *generation, core.as_str(), *frame)
                    == (
                        context.session,
                        context.generation,
                        context.core.as_str(),
                        context.frame,
                    )
            });
        true
    }
    pub(super) fn register_format_item(&self, row: usize) -> Option<formats::Item> {
        let index = self.register_view.register_index(row)?;
        let catalogue = self.register_view.catalogue.as_ref()?;
        let register = &catalogue.registers[index];
        let context = self.register_context();
        let owner = self.register_view.owner(&self.project, &context, index)?;
        let sample = self.register_view.sample(&self.project, &context, index)?;
        let raw = sample.value.as_ref()?;
        let (name, raw) = if let Row::Field(_, field, _) = self.register_view.rows[row] {
            let field = &register.fields[field];
            (
                format!("{}.{}", register.name, field.name),
                field.extract(raw).ok()?.hex,
            )
        } else {
            (register.name.clone(), raw.hex.clone())
        };
        Some(formats::Item {
            rect: Rect::default(),
            pane: 3,
            row,
            key: format!(
                "register:{}:{}:{}:{}",
                self.project.debug.chip, catalogue.cpu, owner, name
            ),
            name,
            raw,
            default: crate::config::Radix::Hex,
        })
    }
    pub(super) fn draw_registers(&mut self, f: &mut UiFrame, rect: Rect) {
        if let Some(error) = &self.register_view.error {
            theme::empty(f, rect, "Register catalogue error", error);
            return;
        }
        let Some(catalogue) = &self.register_view.catalogue else {
            return;
        };
        if rect.height == 0 {
            return;
        }
        let context = self.register_context();
        let wide = rect.width >= 50;
        let header = if self.register_view.searching {
            format!("Find: {}▏", self.register_view.query)
        } else {
            format!(
                "Name / Value{} · {}",
                if wide { " / Size / Access" } else { "" },
                ["All", "Core", "SIMD", "System"][self.register_view.filter]
            )
        };
        f.render_widget(
            Paragraph::new(header).style(Style::default().fg(theme::ACCENT)),
            Rect::new(rect.x, rect.y, rect.width, 1),
        );
        let details_height = if rect.height >= 7 { 2 } else { 0 };
        let height = rect.height.saturating_sub(1 + details_height);
        self.view_rects[3] = Rect::new(rect.x, rect.y + 1, rect.width, height);
        let mut lines = Vec::new();
        for (row_index, row) in self
            .register_view
            .rows
            .iter()
            .enumerate()
            .skip(self.view_tops[3])
            .take(height as usize)
        {
            let (text, color) = match row {
                Row::Group(index, depth) => {
                    let group = &catalogue.groups[*index];
                    (
                        format!(
                            "{}{} {}",
                            "  ".repeat(*depth),
                            if self.register_view.open.contains(&group.id)
                                || !self.register_view.query.is_empty()
                            {
                                "▾"
                            } else {
                                "▸"
                            },
                            group.name
                        ),
                        theme::ACCENT,
                    )
                }
                Row::Register(index, depth) | Row::Field(index, _, depth) => {
                    let register = &catalogue.registers[*index];
                    let sample = self.register_view.sample(&self.project, &context, *index);
                    let owner = self.register_view.owner(&self.project, &context, *index);
                    let current = sample.is_some_and(|sample| {
                        sample.state == State::Valid
                            && sample.applies(&context, owner.as_deref())
                            && self.snapshot.state == "STOPPED"
                    });
                    let field = if let Row::Field(_, field, _) = row {
                        Some(&register.fields[*field])
                    } else {
                        None
                    };
                    let raw = sample
                        .and_then(|sample| sample.value.as_ref())
                        .and_then(|raw| {
                            field.map_or_else(|| Some(raw.clone()), |field| field.extract(raw).ok())
                        });
                    let value = raw.as_ref().map(|raw| {
                        field
                            .and_then(|field| field.enum_name(raw))
                            .map(str::to_owned)
                            .unwrap_or_else(|| {
                                self.register_format_item(row_index)
                                    .and_then(|item| {
                                        formats::number(
                                            &raw.hex,
                                            self.base_for(&item.key, item.default),
                                        )
                                    })
                                    .unwrap_or_else(|| raw.hex.clone())
                            })
                    });
                    let state = if !register.access.readable() {
                        "Write only".into()
                    } else if register.implementation(&self.register_view.facts).0
                        == Implementation::No
                    {
                        "Not implemented".into()
                    } else {
                        sample
                            .map(|sample| {
                                format!(
                                    "{:?}",
                                    if sample.state == State::Valid && !current {
                                        State::Stale
                                    } else {
                                        sample.state
                                    }
                                )
                            })
                            .unwrap_or_else(|| "Not read".into())
                    };
                    let name = field
                        .map(|field| field.name.as_str())
                        .unwrap_or(&register.name);
                    let value = value.unwrap_or(state.clone());
                    if let Some(mut item) = self.register_format_item(row_index) {
                        let prefix_width =
                            *depth * 2 + 5 + unicode_width::UnicodeWidthStr::width(name);
                        item.rect = Rect::new(
                            rect.x.saturating_add(prefix_width as u16),
                            rect.y + 1 + (row_index - self.view_tops[3]) as u16,
                            rect.width.saturating_sub(prefix_width as u16),
                            1,
                        );
                        if item.rect.width > 0 {
                            self.formats.hits.push(item);
                        }
                    }
                    let text = format!(
                        "{}{} {} = {}{}{}",
                        "  ".repeat(*depth),
                        if field.is_none() && !register.fields.is_empty() {
                            if self.register_view.fields.contains(&register.id) {
                                "▾"
                            } else {
                                "▸"
                            }
                        } else {
                            "·"
                        },
                        name,
                        value,
                        if current {
                            String::new()
                        } else {
                            format!(" [{state}]")
                        },
                        if wide {
                            format!(
                                "  {} {}",
                                raw.as_ref().map(|raw| raw.bits).unwrap_or_else(|| field
                                    .map(|field| field
                                        .segments
                                        .iter()
                                        .map(|segment| segment.width)
                                        .sum())
                                    .unwrap_or(register.bits)),
                                field
                                    .and_then(|field| field.access)
                                    .unwrap_or(register.access)
                                    .label()
                            )
                        } else {
                            String::new()
                        }
                    );
                    (text, if current { theme::TEXT } else { theme::MUTED })
                }
            };
            let style = if row_index == self.selected(3) && self.pane == 3 {
                Style::default().fg(color).bg(theme::SELECTED)
            } else {
                Style::default().fg(color)
            };
            lines.push(Line::styled(text, style));
        }
        f.render_widget(
            Paragraph::new(lines),
            Rect::new(rect.x, rect.y + 1, rect.width, height),
        );
        if details_height > 0 {
            let detail = if let Some(index) = self.register_view.register_index(self.selected(3)) {
                let register = &catalogue.registers[index];
                let owner = self
                    .register_view
                    .owner(&self.project, &context, index)
                    .unwrap_or_else(|| "Unknown owner".into());
                let sample = self.register_view.sample(&self.project, &context, index);
                let description = if let Some(Row::Field(_, field, _)) =
                    self.register_view.rows.get(self.selected(3))
                {
                    register.fields[*field].description.as_str()
                } else {
                    register.description.as_str()
                };
                format!(
                    "{} · {} bits {} · {:?} / {}\n{} {}",
                    catalogue.cpu,
                    register.bits,
                    register.access.label(),
                    register.scope,
                    owner,
                    sample
                        .map(|sample| sample.detail.as_str())
                        .unwrap_or(&register.access_condition),
                    description
                )
            } else {
                format!(
                    "{} · {} · {} entries\nEnter/Space expand · r read · s find · v group · a target/all",
                    catalogue.cpu,
                    self.register_view.source,
                    catalogue.registers.len()
                )
            };
            f.render_widget(
                Paragraph::new(detail)
                    .wrap(Wrap { trim: false })
                    .style(Style::default().fg(theme::MUTED)),
                Rect::new(
                    rect.x,
                    rect.bottom() - details_height,
                    rect.width,
                    details_height,
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registers::{RawValue, Reason};
    use std::sync::{Arc, atomic::AtomicBool, mpsc};
    fn app() -> App {
        let mut project = Project::default();
        project.registers.catalogue =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("profiles/registers/cortex-r52.toml");
        let mut app = App::new(project, false);
        app.snapshot.state = "STOPPED".into();
        app.snapshot.register_session = 17;
        app.snapshot.generation = 3;
        app.select_pane(3);
        app.view_rects[3] = Rect::new(0, 0, 80, 5);
        app
    }
    fn engine() -> (EngineHandle, mpsc::Receiver<Request>) {
        let (commands, requests) = mpsc::channel();
        let (_, events) = mpsc::sync_channel(512);
        (
            EngineHandle {
                commands,
                events,
                cancellation: Arc::new(AtomicBool::new(false)),
            },
            requests,
        )
    }
    fn sample(app: &App, id: &str, value: &str) -> Sample {
        Sample {
            id: id.into(),
            state: State::Valid,
            implementation: Implementation::Unknown,
            reason: Reason::Unknown,
            detail: String::new(),
            value: Some(RawValue::parse(value, 32).unwrap()),
            owner: Some("core:default".into()),
            context: app.register_context(),
            timestamp_ms: 23,
            source: format!("gdb:{id}"),
        }
    }
    #[test]
    fn only_visible_expanded_registers_are_read_and_pending_does_not_accumulate() {
        let mut app = app();
        let (engine, requests) = engine();
        assert!(app.ensure_registers(Some(&engine)));
        let request = requests.try_recv().unwrap();
        assert_eq!(request.method, "registers_read");
        assert_eq!(request.params["ids"], json!(["r0", "r1", "r2", "r3"]));
        assert!(!app.ensure_registers(Some(&engine)));
        app.register_response(request.id, &json!({"samples":[]}), None);
        assert!(!app.ensure_registers(Some(&engine)));
        app.register_view.toggle(0, Some(false));
        app.snapshot.generation += 1;
        assert!(!app.ensure_registers(Some(&engine)));
        assert!(requests.try_recv().is_err());
    }
    #[test]
    fn late_response_after_core_or_session_change_is_discarded() {
        let mut app = app();
        let (engine, requests) = engine();
        assert!(app.ensure_registers(Some(&engine)));
        let request = requests.try_recv().unwrap();
        let old = sample(&app, "r0", "0x12345678");
        app.snapshot.register_session += 1;
        app.register_response(request.id, &json!({"samples":[old]}), None);
        assert!(app.register_view.values.is_empty());
        assert!(app.register_view.pending.is_none());
        assert!(app.pending_commands.is_empty());
    }
    #[test]
    fn keyboard_mouse_fields_and_search_cancel_do_not_issue_reads() {
        let mut app = app();
        let cpsr=app.register_view.rows.iter().position(|row|matches!(row,Row::Register(index,_) if app.register_view.catalogue.as_ref().unwrap().registers[*index].id=="cpsr")).unwrap();
        app.selection = cpsr;
        assert!(app.register_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE), None));
        assert!(
            app.register_view
                .rows
                .iter()
                .any(|row| matches!(row, Row::Field(_, _, _)))
        );
        app.start_register_search();
        app.register_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE), None);
        assert_eq!(app.register_view.query, "p");
        app.register_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), None);
        assert!(app.register_view.query.is_empty());
        let mouse = MouseEvent {
            kind: MouseEventKind::Down(event::MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        assert!(app.register_mouse(mouse));
        assert!(!app.register_view.open.contains("core"));
        assert!(app.pending_commands.is_empty());
    }
    #[test]
    fn target_filter_hides_unimplemented_ap_entries_but_architecture_view_explains_them() {
        let mut app = app();
        app.register_view
            .facts
            .insert("icc.physical.prebits".into(), 5);
        app.register_view.query = "icc_ap0r".into();
        app.register_view.rebuild();
        assert_eq!(
            app.register_view
                .rows
                .iter()
                .filter(|row| matches!(row, Row::Register(_, _)))
                .count(),
            1
        );
        app.toggle_register_definitions();
        assert_eq!(
            app.register_view
                .rows
                .iter()
                .filter(|row| matches!(row, Row::Register(_, _)))
                .count(),
            4
        );
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 15)).unwrap();
        terminal
            .draw(|frame| app.draw_registers(frame, frame.area()))
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("Not implemented"));
    }
    #[test]
    fn cpsr_enum_and_stale_values_render_in_wide_and_narrow_views() {
        let mut app = app();
        let cpsr=app.register_view.rows.iter().position(|row|matches!(row,Row::Register(index,_) if app.register_view.catalogue.as_ref().unwrap().registers[*index].id=="cpsr")).unwrap();
        app.register_view.toggle(cpsr, Some(true));
        app.view_tops[3] = cpsr;
        let value = sample(&app, "cpsr", "0x20000013");
        app.register_view
            .values
            .insert(("core:default".into(), "cpsr".into()), value);
        for (width, height) in [(100, 24), (35, 12)] {
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| app.draw_registers(frame, frame.area()))
                .unwrap();
            let text = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(text.contains("CPSR"));
            if width == 100 {
                assert!(text.contains("AArch32_SVC"));
                assert!(text.contains("Size / Access"));
            }
        }
        app.snapshot.state = "RUNNING".into();
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 24)).unwrap();
        terminal
            .draw(|frame| app.draw_registers(frame, frame.area()))
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("Stale"));
        assert!(text.contains("0x20000013"));
    }
}
