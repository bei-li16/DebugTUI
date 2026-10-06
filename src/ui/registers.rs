//! ADS-style register groups and fields, backed by visible on-demand requests.
use super::*;
use crate::registers::{Catalogue, Context, Implementation, Sample, State};
use std::collections::{BTreeMap, BTreeSet};
#[cfg(test)]
mod framework_tests;
mod mpu;
mod provenance;
#[cfg(test)]
mod shared_tests;
mod status;
pub(super) use mpu::draw as draw_mpu;
pub(super) use status::draw as draw_status;

pub(super) const ACTIONS: &[(&str, &str)] = &[
    ("Edit value", "edit-value"),
    ("↻ Read", "register-refresh"),
    ("Cancel read", "register-cancel"),
    ("Status", "register-status"),
    ("Find", "register-search"),
    ("Group", "register-filter"),
    ("Target / All", "register-definitions"),
    ("Probe caps", "register-probe"),
    ("Read bank", "register-bank-read"),
    ("MPU regions", "mpu"),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Row {
    Group(usize, usize),
    Register(usize, usize),
    Field(usize, usize, usize),
}

// Measure terminal cells, so customer names and Chinese descriptions cannot displace values.
fn column(text: &str, width: usize) -> String {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
    let truncated = UnicodeWidthStr::width(text) > width;
    let limit = width.saturating_sub(usize::from(truncated));
    let mut result = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let cells = ch.width().unwrap_or(0);
        if used + cells > limit {
            break;
        }
        result.push(ch);
        used += cells;
    }
    if truncated && width > 0 {
        result.push('…');
        used += 1;
    }
    result.push_str(&" ".repeat(width.saturating_sub(used)));
    result
}

pub(super) struct RegisterView {
    configuration: (String, PathBuf),
    catalogue: Option<Catalogue>,
    source: String,
    error: Option<String>,
    pub(super) rows: Vec<Row>,
    open: BTreeSet<String>,
    fields: BTreeSet<String>,
    // A shared owner does not make distinct worker sessions/routes interchangeable.
    values: BTreeMap<(String, String, String), Sample>,
    previous: BTreeMap<(String, String, String), Sample>,
    owner_generations: BTreeMap<String, u64>,
    attempts: BTreeSet<(u64, u64, String, u32, String)>,
    pending: Option<(u64, Context)>,
    bank_pending: Option<[String; 2]>,
    probe_pending: Option<(u64, Context)>,
    pub(super) read_request: Option<Request>,
    status_popup: Option<status::Popup>,
    query: String,
    search_original: String,
    pub(super) searching: bool,
    filter: usize,
    all_definitions: bool,
    facts: BTreeMap<String, u64>,
    runtime_absent: BTreeSet<String>,
    pub(super) preference_scope: String,
    display_formats: BTreeMap<String, crate::registers::display::Format>,
    mpu_popup: Option<mpu::Popup>,
    vfp_write_targets: BTreeSet<String>,
}
impl RegisterView {
    pub(super) fn edit_candidate(
        &self,
        row: usize,
        context: &Context,
    ) -> Result<super::writes::Candidate, String> {
        let catalogue = self
            .catalogue
            .as_ref()
            .ok_or("Select a register catalogue first")?;
        let (index, field) = match self.rows.get(row) {
            Some(Row::Register(i, _)) => (*i, None),
            Some(Row::Field(i, f, _)) => (*i, Some(*f)),
            _ => return Err("Select a register or field to edit".into()),
        };
        let register = &catalogue.registers[index];
        let mut title = register.name.clone();
        let mut bits = register.bits;
        let mut value = String::new();
        // The actual topology is checked by the engine. A sample must still match this core/frame.
        let sample = self.values.values().find(|sample| {
            sample.id == register.id
                && sample.context.core == context.core
                && sample.state == State::Valid
                && sample.applies_at(context, sample.owner.as_deref(), &self.owner_generations)
        });
        let selection = if let Some(field) = field {
            let field = &register.fields[field];
            title.push('.');
            title.push_str(&field.name);
            bits = field.segments.iter().map(|s| s.width).sum();
            if let Some(sample) = sample.and_then(|s| s.value.as_ref()) {
                value = field.extract(sample).map(|v| v.hex).unwrap_or_default();
            }
            crate::writes::Selection::Field {
                name: field.name.clone(),
            }
        } else {
            if let Some(sample) = sample.and_then(|s| s.value.as_ref()) {
                value = sample.hex.clone();
            }
            crate::writes::Selection::Register
        };
        let reason = if register.writer.is_none() || register.write.is_none() {
            Some("No independent writer and write semantics are declared for this object.".into())
        } else if matches!(register.writer, Some(crate::registers::Writer::Vfp { .. }))
            && !self.vfp_write_targets.contains(&context.core)
        {
            Some("Configure the independent registers.vfp_write_command and this core's TCL target before editing VFP storage.".into())
        } else if !register.access.writable() {
            Some("Register is read-only.".into())
        } else if self
            .catalogue
            .as_ref()
            .unwrap()
            .implementation(register, &self.facts)
            .0
            == Implementation::No
        {
            Some("Capability conditions exclude this register; see Status for their source.".into())
        } else {
            None
        };
        Ok(super::writes::Candidate {
            target: json!({"kind":"register","id":register.id}),
            selection,
            title,
            bits,
            value,
            reason,
        })
    }
    pub(super) fn load(project: &Project) -> Self {
        let core = project
            .preference_core
            .as_deref()
            .or_else(|| project.cores.first().map(|core| core.name.as_str()))
            .unwrap_or("default");
        Self::load_config(&project.registers_for_core(core))
    }
    fn load_config(config: &crate::registers::Config) -> Self {
        let (catalogue, source, error) = match config.load() {
            Ok(Some((catalogue, source))) => (Some(catalogue), source, None),
            Ok(None) => (None, String::new(), None),
            Err(error) => (None, String::new(), Some(error)),
        };
        let mut view = Self {
            configuration: (config.cpu.clone(), config.catalogue.clone()),
            catalogue,
            source,
            error,
            rows: vec![],
            open: BTreeSet::from(["core".into()]),
            fields: BTreeSet::new(),
            values: BTreeMap::new(),
            owner_generations: BTreeMap::new(),
            previous: BTreeMap::new(),
            attempts: BTreeSet::new(),
            pending: None,
            bank_pending: None,
            probe_pending: None,
            read_request: None,
            status_popup: None,
            query: String::new(),
            search_original: String::new(),
            searching: false,
            filter: 0,
            all_definitions: false,
            facts: config.facts.clone(),
            runtime_absent: BTreeSet::new(),
            preference_scope: String::new(),
            display_formats: BTreeMap::new(),
            mpu_popup: None,
            vfp_write_targets: if config.vfp_write_command == "aarch64 vfp_write"
                && config.vfp_command == "aarch64 vfp"
                && !config.tcl_endpoint.is_empty()
            {
                config.targets.keys().cloned().collect()
            } else {
                BTreeSet::new()
            },
        };
        view.rebuild();
        view
    }
    pub(super) fn enabled(&self) -> bool {
        self.catalogue.is_some() || self.error.is_some()
    }
    fn matches(&self, index: usize) -> bool {
        let register = &self.catalogue.as_ref().unwrap().registers[index];
        (self.all_definitions
            || (self
                .catalogue
                .as_ref()
                .unwrap()
                .implementation(register, &self.facts)
                .0
                != Implementation::No
                && !self.runtime_absent.contains(&register.id)))
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
    fn toggle(&mut self, row: usize, expand: Option<bool>) -> bool {
        let Some(row) = self.rows.get(row).cloned() else {
            return false;
        };
        let catalogue = self.catalogue.as_ref().unwrap();
        let (set, id) = match row {
            Row::Group(index, _) => (&mut self.open, catalogue.groups[index].id.clone()),
            Row::Register(index, _) if !catalogue.registers[index].fields.is_empty() => {
                (&mut self.fields, catalogue.registers[index].id.clone())
            }
            _ => return false,
        };
        let changed = if expand.unwrap_or(!set.contains(&id)) {
            set.insert(id)
        } else {
            set.remove(&id)
        };
        if changed {
            self.rebuild();
        }
        changed
    }
    fn register_index(&self, row: usize) -> Option<usize> {
        match self.rows.get(row)? {
            Row::Register(index, _) | Row::Field(index, _, _) => Some(*index),
            _ => None,
        }
    }
    fn owner(&self, project: &Project, context: &Context, index: usize) -> Option<String> {
        let mut topology = project.registers_for_core(&context.core).topology;
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
            context.core.clone(),
        ))
    }
}

impl App {
    fn active_register_config(&self) -> crate::registers::Config {
        let core = self
            .snapshot
            .core
            .as_ref()
            .map(|core| core.name.as_str())
            .or(self.project.preference_core.as_deref())
            .or_else(|| self.project.cores.first().map(|core| core.name.as_str()))
            .unwrap_or("default");
        self.project.registers_for_core(core)
    }
    pub(super) fn sync_register_configuration(&mut self) {
        let config = self.active_register_config();
        let key = (config.cpu.clone(), config.catalogue.clone());
        if self.register_view.configuration != key {
            if let Some(request) = &self.register_view.read_request {
                request.cancel_read();
            }
            self.register_view = RegisterView::load_config(&config);
            self.selections[3] = 0;
            if self.pane == 3 {
                self.selection = 0;
            }
        }
        self.register_view.vfp_write_targets = if config.vfp_write_command == "aarch64 vfp_write"
            && config.vfp_command == "aarch64 vfp"
            && !config.tcl_endpoint.is_empty()
        {
            config.targets.keys().cloned().collect()
        } else {
            BTreeSet::new()
        };
    }
    pub(super) fn sync_register_sample_validity(&mut self) {
        let context = self.register_context();
        let generations = &self.snapshot.register_owner_generations;
        let changed: BTreeSet<_> = generations
            .keys()
            .chain(self.register_view.owner_generations.keys())
            .filter(|owner| {
                generations.get(*owner) != self.register_view.owner_generations.get(*owner)
            })
            .cloned()
            .collect();
        if !changed.is_empty() {
            let mut topology = self.active_register_config().topology;
            if topology.chip.is_empty() {
                topology.chip = self.project.debug.chip.clone();
            }
            let catalogue = self.register_view.catalogue.as_ref();
            self.register_view.attempts.retain(|(_, _, core, _, id)| {
                !catalogue
                    .and_then(|catalogue| catalogue.register(id))
                    .and_then(|register| topology.owner(register.scope, core))
                    .is_some_and(|owner| changed.contains(&owner))
            });
        }
        self.register_view.owner_generations = generations.clone();
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
        for (key, sample) in &mut self.register_view.values {
            if sample.context.core != context.core || sample.state != State::Valid {
                continue;
            }
            let engine_invalidated = self.snapshot.register_samples.iter().any(|item| {
                item.id == sample.id
                    && item.owner == sample.owner
                    && item.context == sample.context
                    && item.state == State::Stale
            });
            if self.snapshot.state != "STOPPED"
                || engine_invalidated
                || !sample.applies_at(&context, sample.owner.as_deref(), generations)
            {
                self.register_view
                    .previous
                    .insert(key.clone(), sample.clone());
                sample.stale();
            }
        }
    }
    fn sync_register_absence(&mut self) {
        let context = self.register_context();
        let absent = self
            .register_view
            .catalogue
            .as_ref()
            .map(|c| {
                c.registers
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| {
                        self.register_view
                            .sample(&self.project, &context, *index)
                            .is_some_and(|s| {
                                self.snapshot.state == "STOPPED"
                                    && s.applies_at(
                                        &context,
                                        self.register_view
                                            .owner(&self.project, &context, *index)
                                            .as_deref(),
                                        &self.register_view.owner_generations,
                                    )
                                    && (s.implementation == Implementation::No
                                        || s.reason
                                            == crate::registers::Reason::HardwareNotImplemented)
                            })
                    })
                    .map(|(_, r)| r.id.clone())
                    .collect()
            })
            .unwrap_or_default();
        if self.register_view.runtime_absent != absent {
            self.register_view.runtime_absent = absent;
            self.register_view.rebuild();
            self.selections[3] = self
                .selected(3)
                .min(self.register_view.rows.len().saturating_sub(1));
            if self.pane == 3 {
                self.selection = self.selections[3];
            }
        }
    }
    pub(super) fn register_read_pending(&self) -> bool {
        self.register_view.pending.is_some() || self.register_view.probe_pending.is_some()
    }
    pub(super) fn cancel_register_read(&mut self) -> bool {
        if !self.register_read_pending() {
            return false;
        }
        if let Some(request) = &self.register_view.read_request {
            request.cancel_read();
            if let Some((_, context)) = self
                .register_view
                .pending
                .as_ref()
                .or(self.register_view.probe_pending.as_ref())
            {
                let ids: Vec<String> = if request.method == "registers_probe" {
                    crate::registers::capabilities::PROBE_IDS
                        .iter()
                        .map(|id| (*id).into())
                        .collect()
                } else if let Some(ids) = &self.register_view.bank_pending {
                    ids.to_vec()
                } else {
                    vec![]
                };
                for id in ids {
                    self.register_view.attempts.insert((
                        context.session,
                        context.generation,
                        context.core.clone(),
                        context.frame,
                        id,
                    ));
                }
            }
            self.notice =
                "Cancelling register read; waiting for the current transaction to finish.".into();
        }
        true
    }
    fn finish_register_read(&mut self, id: u64) -> bool {
        if self
            .register_view
            .read_request
            .as_ref()
            .is_some_and(|r| r.id == id)
        {
            return self
                .register_view
                .read_request
                .take()
                .unwrap()
                .read_cancel
                .load(std::sync::atomic::Ordering::Relaxed);
        }
        false
    }
    pub(super) fn sync_register_preferences(&mut self) {
        let Some(catalogue) = &self.register_view.catalogue else {
            return;
        };
        let mut context = self.register_context();
        if self.snapshot.core.is_none()
            && let Some(core) = self.project.cores.first()
        {
            context.core = core.name.clone();
        }
        let chip = if self.project.debug.chip.is_empty() {
            let endpoint = self
                .snapshot
                .core
                .as_ref()
                .map(|core| core.endpoint.as_str())
                .or_else(|| {
                    self.project
                        .cores
                        .first()
                        .map(|core| core.endpoint.as_str())
                })
                .unwrap_or(&self.project.target.endpoint);
            format!("unidentified:{}:{endpoint}", self.project.target.mode)
        } else {
            self.project.debug.chip.clone()
        };
        let scope = serde_json::to_string(&(
            &chip,
            &context.core,
            &catalogue.cpu,
            &catalogue.architecture,
            &self.register_view.source,
            catalogue.version,
        ))
        .unwrap();
        if scope == self.register_view.preference_scope {
            return;
        }
        let migrate = self.project.ui.register_views.is_empty();
        let mut preferences = self
            .project
            .ui
            .register_views
            .get(&scope)
            .cloned()
            .filter(|p| p.validate().is_ok())
            .unwrap_or_default();
        preferences
            .open
            .retain(|id| catalogue.groups.iter().any(|group| &group.id == id));
        preferences
            .fields
            .retain(|id| catalogue.register(id).is_some_and(|r| !r.fields.is_empty()));
        preferences.formats.retain(|object, format| {
            let Ok((id, field)) = serde_json::from_str::<(String, Option<String>)>(object) else {
                return false;
            };
            let Some(register) = catalogue.register(&id) else {
                return false;
            };
            let bits = if let Some(field) = &field {
                let Some(field) = register.fields.iter().find(|f| &f.name == field) else {
                    return false;
                };
                field.segments.iter().map(|segment| segment.width).sum()
            } else {
                register.bits
            };
            crate::registers::display::choices(bits, field.is_some())
                .iter()
                .any(|(choice, _)| choice == format)
        });
        if migrate {
            for (index, register) in catalogue.registers.iter().enumerate() {
                let Some(owner) = self.register_view.owner(&self.project, &context, index) else {
                    continue;
                };
                for field in std::iter::once(None).chain(register.fields.iter().map(Some)) {
                    let name = field
                        .map(|field| format!("{}.{}", register.name, field.name))
                        .unwrap_or_else(|| register.name.clone());
                    let key = format!(
                        "register:{}:{}:{}:{name}",
                        self.project.debug.chip, catalogue.cpu, owner
                    );
                    let legacy = format!("register:{}", register.id);
                    if let Some(radix) = self.project.ui.formats.get(&key).or_else(|| {
                        if field.is_none() {
                            self.project.ui.formats.get(&legacy)
                        } else {
                            None
                        }
                    }) {
                        let object =
                            serde_json::to_string(&(&register.id, field.map(|field| &field.name)))
                                .unwrap();
                        preferences.formats.entry(object).or_insert(
                            crate::registers::display::Format::Unsigned { radix: *radix },
                        );
                    }
                }
            }
        }
        self.project
            .ui
            .register_views
            .insert(scope.clone(), preferences.clone());
        self.register_view.preference_scope = scope;
        self.register_view.open = preferences.open;
        self.register_view.fields = preferences.fields;
        self.register_view.filter = usize::from(preferences.filter);
        self.register_view.all_definitions = preferences.all_definitions;
        self.register_view.query = preferences.query;
        self.register_view.display_formats = preferences.formats;
        self.register_view.searching = false;
        self.register_view.mpu_popup = None;
        if self
            .formats
            .popup
            .as_ref()
            .is_some_and(|item| item.register.is_some())
        {
            self.formats.popup = None;
        }
        self.register_view.rebuild();
        self.view_tops[3] = 0;
        self.selections[3] = 0;
        if self.pane == 3 {
            self.selection = 0;
        }
    }
    pub(super) fn save_register_preferences(&mut self, engine: Option<&EngineHandle>) {
        let preferences = crate::registers::display::Preferences {
            open: self.register_view.open.clone(),
            fields: self.register_view.fields.clone(),
            filter: self.register_view.filter as u8,
            all_definitions: self.register_view.all_definitions,
            query: self.register_view.query.clone(),
            formats: self.register_view.display_formats.clone(),
        };
        if let Err(error) = preferences.validate() {
            self.notice = error;
            return;
        }
        let scope = self.register_view.preference_scope.clone();
        if scope.is_empty() {
            return;
        }
        self.project
            .ui
            .register_views
            .insert(scope.clone(), preferences.clone());
        if let Some(engine) = engine {
            let id = self.next_id;
            self.next_id += 1;
            match engine.send(Request::new(
                id,
                "register_preferences",
                json!({"scope":scope,"preferences":preferences}),
            )) {
                Ok(()) => {
                    self.formats.pending_save.insert(id);
                }
                Err(error) => {
                    self.notice = format!("Cannot save register display preferences: {error}")
                }
            }
        }
    }
    pub(super) fn register_display_format(
        &self,
        object: &str,
    ) -> crate::registers::display::Format {
        self.register_view
            .display_formats
            .get(object)
            .copied()
            .unwrap_or_default()
    }
    pub(super) fn set_register_display_format(
        &mut self,
        scope: &str,
        object: String,
        format: crate::registers::display::Format,
        engine: Option<&EngineHandle>,
    ) {
        if scope != self.register_view.preference_scope {
            return;
        }
        self.register_view.display_formats.insert(object, format);
        self.save_register_preferences(engine);
    }
    pub(super) fn read_register_bank(&mut self, engine: Option<&EngineHandle>) {
        if self.demo
            || engine.is_none()
            || self.snapshot.state != "STOPPED"
            || self.register_view.pending.is_some()
            || self.register_view.probe_pending.is_some()
        {
            return;
        }
        let Some(index) = self.register_view.register_index(self.selected(3)) else {
            return;
        };
        let id = &self.register_view.catalogue.as_ref().unwrap().registers[index].id;
        let Some((kind, index)) = crate::registers::selector::register_selection(id) else {
            self.notice =
                "Select an indexed MPU region or PMU event counter to read its bank.".into();
            return;
        };
        let context = self.register_context();
        if context.frame != 0
            || self
                .snapshot
                .register_probe
                .as_ref()
                .is_none_or(|p| p.context != context)
        {
            self.notice =
                "Probe this physical core at frame 0 before reading a selector bank.".into();
            return;
        }
        if self.active_register_config().selector_command.is_empty() {
            self.notice =
                "A verified selector MCR command must be declared; direct Read remains available."
                    .into();
            return;
        }
        let request_id = self.next_id;
        self.register_view.pending = Some((request_id, context.clone()));
        self.register_view.bank_pending = Some(kind.ids(index));
        self.submit(
            engine,
            "registers_select",
            json!({"context":context,"kind":kind,"index":index}),
        );
        if !self.pending_commands.contains(&request_id) {
            self.register_view.pending = None;
            self.register_view.bank_pending = None;
        }
    }
    pub(super) fn sync_register_capabilities(&mut self) {
        let config = self.active_register_config();
        let mut topology = config.topology.clone();
        if topology.chip.is_empty() {
            topology.chip = self.project.debug.chip.clone();
        }
        let facts = if self.snapshot.state == "STOPPED"
            && let Some(catalogue) = &self.register_view.catalogue
        {
            catalogue.observation_facts_for_owners(
                &config.facts,
                self.snapshot.register_probe.as_ref(),
                &self.snapshot.register_samples,
                &self.register_context(),
                &topology,
            )
        } else {
            self.snapshot
                .register_probe
                .as_ref()
                .filter(|p| {
                    p.context == self.register_context() && self.snapshot.state == "STOPPED"
                })
                .map(|p| p.effective(&config.facts))
                .unwrap_or_else(|| config.facts.clone())
        };
        if facts != self.register_view.facts {
            self.register_view.facts = facts;
            for sample in self.register_view.values.values_mut() {
                sample.stale();
            }
            self.register_view.rebuild();
            self.selections[3] = self
                .selected(3)
                .min(self.register_view.rows.len().saturating_sub(1));
            if self.pane == 3 {
                self.selection = self.selections[3];
            }
        }
        let context = self.register_context();
        if let Some(probe) = &self.snapshot.register_probe
            && probe.context == context
            && self.snapshot.state == "STOPPED"
        {
            for sample in &probe.samples {
                if sample.context != context {
                    continue;
                }
                let Some(index) = self
                    .register_view
                    .catalogue
                    .as_ref()
                    .and_then(|c| c.registers.iter().position(|r| r.id == sample.id))
                else {
                    continue;
                };
                let owner = self.register_view.owner(&self.project, &context, index);
                if owner != sample.owner {
                    continue;
                }
                let Some(owner) = owner else {
                    continue;
                };
                let key = (owner, sample.id.clone(), context.core.clone());
                if self
                    .register_view
                    .values
                    .get(&key)
                    .is_none_or(|old| old.timestamp_ms <= sample.timestamp_ms)
                {
                    self.register_view.values.insert(key, sample.clone());
                }
                self.register_view.attempts.insert((
                    context.session,
                    context.generation,
                    context.core.clone(),
                    context.frame,
                    sample.id.clone(),
                ));
            }
        }
    }
    pub(super) fn probe_registers(&mut self, engine: Option<&EngineHandle>) {
        if self.demo
            || engine.is_none()
            || !self.register_view.enabled()
            || self.snapshot.state != "STOPPED"
            || self.register_view.pending.is_some()
            || self.register_view.probe_pending.is_some()
        {
            return;
        }
        let context = self.register_context();
        if context.frame != 0 {
            self.notice = "Select physical frame 0 before probing capabilities.".into();
            return;
        }
        let id = self.next_id;
        self.register_view.probe_pending = Some((id, context.clone()));
        self.submit(engine, "registers_probe", json!({"context":context}));
        if !self.pending_commands.contains(&id) {
            self.register_view.probe_pending = None;
        }
    }
    pub(super) fn register_probe_response(
        &mut self,
        id: u64,
        result: &Value,
        error: Option<&str>,
    ) -> bool {
        let Some((pending, context)) = self.register_view.probe_pending.clone() else {
            return false;
        };
        if pending != id {
            return false;
        }
        self.register_view.probe_pending = None;
        let cancelled = self.finish_register_read(id);
        self.pending_commands.remove(&id);
        self.fx.response(id, error.is_none() && !cancelled);
        if cancelled {
            self.notice = error
                .unwrap_or("Register read cancelled; late results discarded.")
                .into();
            return true;
        }
        if context != self.register_context() || self.snapshot.state != "STOPPED" {
            return true;
        }
        if let Some(error) = error {
            self.notice = format!("Capability probe: {error}");
            return true;
        }
        if let Ok(probe) =
            serde_json::from_value::<crate::registers::capabilities::Probe>(result["probe"].clone())
            && probe.context == context
        {
            self.notice = format!(
                "Capability probe: {} facts; {} · unknown/failed {}. Raw evidence in Log.",
                probe.facts.len(),
                probe
                    .identity
                    .as_ref()
                    .map(|i| format!(
                        "{} {}",
                        i.model.as_deref().unwrap_or("Unknown CPU"),
                        i.revision_name
                    ))
                    .unwrap_or_else(|| "Unknown CPU".into()),
                probe
                    .samples
                    .iter()
                    .filter(|s| s.state != State::Valid)
                    .count()
            );
            self.snapshot.register_probe = Some(probe);
            self.sync_register_capabilities();
        } else {
            self.notice = "Invalid or expired capability probe response".into();
        }
        true
    }
    pub(super) fn register_context(&self) -> Context {
        Context {
            session: self.snapshot.register_session,
            generation: self
                .snapshot
                .register_generation
                .unwrap_or(self.snapshot.generation),
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
    pub(super) fn filter_registers(&mut self, engine: Option<&EngineHandle>) {
        self.register_view.filter = (self.register_view.filter + 1) % 4;
        self.register_view.rebuild();
        self.selection = 0;
        self.view_tops[3] = 0;
        self.save_register_preferences(engine);
    }
    pub(super) fn toggle_register_definitions(&mut self, engine: Option<&EngineHandle>) {
        self.register_view.all_definitions = !self.register_view.all_definitions;
        self.register_view.rebuild();
        self.selection = 0;
        self.view_tops[3] = 0;
        self.save_register_preferences(engine);
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
                KeyCode::Enter => {
                    self.register_view.searching = false;
                    self.save_register_preferences(engine);
                }
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
        let mut changed = false;
        match key.code {
            KeyCode::Esc if self.cancel_register_read() => {}
            KeyCode::Enter | KeyCode::Char(' ') => {
                changed = self.register_view.toggle(self.selected(3), None);
            }
            KeyCode::Left => changed = self.register_view.toggle(self.selected(3), Some(false)),
            KeyCode::Right => changed = self.register_view.toggle(self.selected(3), Some(true)),
            KeyCode::Char('r') => {
                self.refresh_register(engine);
            }
            KeyCode::Char('s') => self.start_register_search(),
            KeyCode::Char('v') => self.filter_registers(engine),
            KeyCode::Char('a') => self.toggle_register_definitions(engine),
            KeyCode::Char('t') => self.open_register_status(),
            _ => return false,
        }
        self.selection = self
            .selection
            .min(self.register_view.rows.len().saturating_sub(1));
        if changed {
            self.save_register_preferences(engine);
        }
        true
    }
    pub(super) fn register_mouse(
        &mut self,
        mouse: MouseEvent,
        engine: Option<&EngineHandle>,
    ) -> bool {
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
        if mouse.column <= rect.x.saturating_add((depth * 2 + 2) as u16)
            && self.register_view.toggle(row, None)
        {
            self.save_register_preferences(engine);
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
            || self.register_view.probe_pending.is_some()
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
        self.sync_register_sample_validity();
        self.sync_register_absence();
        if self.side_pane != 3
            || self.register_view.mpu_popup.is_some()
            || self.register_view.status_popup.is_some()
            || !self.register_view.enabled()
            || self.register_view.pending.is_some()
            || self.register_view.probe_pending.is_some()
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
            if catalogue.automatic_read(register, &self.register_view.facts)
                && catalogue
                    .access_denial(
                        register,
                        &self.register_view.facts,
                        self.snapshot.state == "STOPPED",
                        None,
                    )
                    .is_none()
                && self.register_view.category(
                    &self.project,
                    &context,
                    index,
                    self.snapshot.state == "STOPPED",
                ) != status::Category::Valid
                && !self.register_view.runtime_absent.contains(&register.id)
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
        let cancelled = self.finish_register_read(id);
        let bank = self.register_view.bank_pending.take();
        self.mpu_read_response(
            id,
            if cancelled {
                error.or(Some("Register read cancelled; new results discarded."))
            } else {
                error
            },
        );
        self.pending_commands.remove(&id);
        self.fx.response(id, error.is_none() && !cancelled);
        if cancelled {
            self.notice = error
                .unwrap_or("Register read cancelled; late results discarded.")
                .into();
            return true;
        }
        if context != self.register_context() {
            return true;
        }
        if let Some(error) = error {
            if let Some(ids) = bank {
                for id in ids {
                    let key = (
                        format!("core:{}", context.core),
                        id.clone(),
                        context.core.clone(),
                    );
                    let previous = self.register_view.values.get(&key).cloned();
                    let mut sample = previous.clone().unwrap_or_else(|| Sample {
                        id: id.clone(),
                        state: State::Unavailable,
                        implementation: Implementation::Unknown,
                        reason: crate::registers::Reason::Unknown,
                        detail: String::new(),
                        value: None,
                        owner: Some(key.0.clone()),
                        context: context.clone(),
                        view: crate::registers::SampleView::PhysicalCore,
                        owner_generation: None,
                        provenance: None,
                        last_value_provenance: None,
                        eligibility: None,
                        last_value_eligibility: None,
                        timestamp_ms: 0,
                        source: "selector".into(),
                    });
                    sample.state = State::Unavailable;
                    sample.context = context.clone();
                    // A whole-request error has no new value-route evidence.
                    // Keep the previous raw value's origin separately.
                    sample.provenance = None;
                    sample.eligibility = None;
                    if let Some(old) = &previous {
                        sample.inherit_value_origin(old);
                    }
                    sample.reason = if error.contains("synchronization unsupported") {
                        crate::registers::Reason::ReaderUnsupported
                    } else {
                        crate::registers::Reason::Unknown
                    };
                    sample.detail = match previous {
                        Some(old) if old.value.is_some() => {
                            format!("{error}; last sample at {} ms", old.timestamp_ms)
                        }
                        _ => error.into(),
                    };
                    self.register_view.values.insert(key, sample);
                    self.register_view.attempts.insert((
                        context.session,
                        context.generation,
                        context.core.clone(),
                        context.frame,
                        id,
                    ));
                }
            }
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
                if sample.state == State::Valid
                    && !sample.applies_at(
                        &context,
                        expected_owner.as_deref(),
                        &self.register_view.owner_generations,
                    )
                {
                    continue;
                }
                let owner = sample
                    .owner
                    .clone()
                    .unwrap_or_else(|| format!("unknown:{}", context.core));
                let key = (owner, sample.id.clone(), context.core.clone());
                if let Some(old) = self.register_view.values.get(&key)
                    && old.state == State::Valid
                {
                    self.register_view.previous.insert(key.clone(), old.clone());
                }
                if sample.value.is_none()
                    && let Some(previous) = self.register_view.previous.get(&key)
                {
                    sample.value = previous.value.clone();
                    sample.inherit_value_origin(previous);
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
        let (name, raw, field_name) = if let Row::Field(_, field, _) = self.register_view.rows[row]
        {
            let field = &register.fields[field];
            (
                format!("{}.{}", register.name, field.name),
                field.extract(raw).ok()?,
                Some(field.name.as_str()),
            )
        } else {
            (register.name.clone(), raw.clone(), None)
        };
        Some(formats::Item {
            register: Some(formats::RegisterBinding {
                scope: self.register_view.preference_scope.clone(),
                object: serde_json::to_string(&(&register.id, field_name)).ok()?,
                bits: raw.bits,
                field: field_name.is_some(),
            }),
            rect: Rect::default(),
            pane: 3,
            row,
            key: format!(
                "register:{}:{}:{}:{}",
                self.project.debug.chip, catalogue.cpu, owner, name
            ),
            name,
            raw: raw.hex,
            default: crate::config::Radix::Hex,
        })
    }
    pub(super) fn draw_registers(&mut self, f: &mut UiFrame, rect: Rect) {
        self.sync_register_sample_validity();
        self.sync_register_preferences();
        self.sync_register_absence();
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
        let name_width = (usize::from(rect.width).saturating_sub(16) / 3).clamp(12, 32);
        let value_width = usize::from(rect.width).saturating_sub(name_width + 16);
        let filter = ["All", "Core", "SIMD", "System"][self.register_view.filter];
        let header = if self.register_view.searching {
            format!("Find: {}▏", self.register_view.query)
        } else if wide {
            format!(
                "{}   {} {} {}",
                column(&format!("Name · {filter}"), name_width),
                column("Value", value_width),
                column("Size", 5),
                column("Access", 6)
            )
        } else {
            format!("Name / Value · {filter}")
        };
        f.render_widget(
            Paragraph::new(header).style(Style::default().fg(theme::ACCENT)),
            Rect::new(rect.x, rect.y, rect.width, 1),
        );
        let details_height = if rect.height >= 7 { 2 } else { 0 };
        let summary_height = u16::from(rect.height >= 4);
        if summary_height > 0 {
            let counts = self.register_view.counts(
                &self.project,
                &context,
                self.snapshot.state == "STOPPED",
            );
            f.render_widget(
                Paragraph::new(counts.compact()).style(Style::default().fg(theme::MUTED)),
                Rect::new(rect.x, rect.y + 1, rect.width, 1),
            );
        }
        let rows_y = rect.y + 1 + summary_height;
        let height = rect
            .height
            .saturating_sub(1 + summary_height + details_height);
        self.view_rects[3] = Rect::new(rect.x, rows_y, rect.width, height);
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
                    let current = self.register_view.category(
                        &self.project,
                        &context,
                        *index,
                        self.snapshot.state == "STOPPED",
                    ) == status::Category::Valid;
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
                        let object =
                            serde_json::to_string(&(&register.id, field.map(|field| &field.name)))
                                .unwrap();
                        let value = self
                            .register_display_format(&object)
                            .render(raw)
                            .unwrap_or_else(|_| raw.hex.clone());
                        field
                            .and_then(|field| field.enum_name(raw))
                            .map(|name| format!("{value} ({name})"))
                            .unwrap_or(value)
                    });
                    let state = self
                        .register_view
                        .category(
                            &self.project,
                            &context,
                            *index,
                            self.snapshot.state == "STOPPED",
                        )
                        .label()
                        .to_string();
                    let name = field
                        .map(|field| field.name.as_str())
                        .unwrap_or(&register.name);
                    let value = value.unwrap_or(state.clone());
                    let marker = if field.is_none() && !register.fields.is_empty() {
                        if self.register_view.fields.contains(&register.id) {
                            "▾"
                        } else {
                            "▸"
                        }
                    } else {
                        "·"
                    };
                    let confidence = if field.is_none()
                        && matches!(
                            register.confidence,
                            crate::registers::metadata::Confidence::Low
                                | crate::registers::metadata::Confidence::Medium
                        ) {
                        format!(" [{}]", register.confidence.label())
                    } else {
                        String::new()
                    };
                    let name = format!("{}{marker} {name}{confidence}", "  ".repeat(*depth));
                    let shown_name_width = if wide {
                        name_width
                    } else {
                        unicode_width::UnicodeWidthStr::width(name.as_str())
                            .min(usize::from(rect.width).saturating_sub(13))
                    };
                    let shown_value_width = if wide {
                        value_width
                    } else {
                        usize::from(rect.width).saturating_sub(shown_name_width + 3)
                    };
                    if let Some(mut item) = self.register_format_item(row_index) {
                        let prefix_width = shown_name_width + 3;
                        item.rect = Rect::new(
                            rect.x.saturating_add(prefix_width as u16),
                            rows_y + (row_index - self.view_tops[3]) as u16,
                            shown_value_width as u16,
                            1,
                        );
                        if item.rect.width > 0 {
                            self.formats.hits.push(item);
                        }
                    }
                    let value = if current {
                        value
                    } else {
                        format!("{value} [{state}]")
                    };
                    let bits = field
                        .map(|field| field.segments.iter().map(|s| s.width).sum())
                        .unwrap_or(register.bits);
                    let access = field
                        .and_then(|field| field.access)
                        .unwrap_or(register.access);
                    let text = format!(
                        "{} = {}{}",
                        column(&name, shown_name_width),
                        column(&value, shown_value_width),
                        if wide {
                            format!(
                                " {} {}",
                                column(&bits.to_string(), 5),
                                column(access.label(), 6)
                            )
                        } else {
                            String::new()
                        }
                    );
                    let previous = self
                        .register_view
                        .owner(&self.project, &context, *index)
                        .and_then(|owner| {
                            self.register_view.previous.get(&(
                                owner,
                                register.id.clone(),
                                context.core.clone(),
                            ))
                        })
                        .filter(|old| {
                            old.state == State::Valid
                                && old.context.session == context.session
                                && old.context.core == context.core
                                && old.context.frame == context.frame
                        })
                        .and_then(|old| old.value.as_ref())
                        .and_then(|old| {
                            field.map_or_else(|| Some(old.clone()), |f| f.extract(old).ok())
                        });
                    let changed = current && previous.is_some() && previous != raw;
                    (
                        text,
                        if !current {
                            theme::MUTED
                        } else if changed {
                            theme::AMBER
                        } else {
                            theme::TEXT
                        },
                    )
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
            Rect::new(rect.x, rows_y, rect.width, height),
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
                let (bits, access) = if let Some(Row::Field(_, field, _)) =
                    self.register_view.rows.get(self.selected(3))
                {
                    let field = &register.fields[*field];
                    (
                        field.segments.iter().map(|s| s.width).sum(),
                        field.access.unwrap_or(register.access),
                    )
                } else {
                    (register.bits, register.access)
                };
                format!(
                    "{} · {} bits {} · {:?} / {}\n{} {}",
                    catalogue.cpu,
                    bits,
                    access.label(),
                    register.scope,
                    owner,
                    sample
                        .map(|sample| format!(
                            "{} · {} · {}",
                            self.register_view
                                .category(
                                    &self.project,
                                    &context,
                                    index,
                                    self.snapshot.state == "STOPPED"
                                )
                                .label(),
                            sample.source,
                            sample.detail
                        ))
                        .unwrap_or_else(|| register.access_condition.clone()),
                    description
                )
            } else {
                format!(
                    "{} · {} · {} entries\nEnter/Space expand · r read · Esc cancel · t status · s find · v group · a target/all",
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
    pub(super) fn app() -> App {
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
    pub(super) fn engine() -> (EngineHandle, mpsc::Receiver<Request>) {
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
    pub(super) fn sample(app: &App, id: &str, value: &str) -> Sample {
        Sample {
            id: id.into(),
            state: State::Valid,
            implementation: Implementation::Unknown,
            reason: Reason::Unknown,
            detail: String::new(),
            value: Some(RawValue::parse(value, 32).unwrap()),
            owner: Some("core:default".into()),
            context: app.register_context(),
            view: crate::registers::SampleView::SelectedFrame,
            owner_generation: None,
            provenance: None,
            last_value_provenance: None,
            eligibility: None,
            last_value_eligibility: None,
            timestamp_ms: 23,
            source: format!("gdb:{id}"),
        }
    }
    #[test]
    fn vfp_editor_uses_native_width_and_requires_this_cores_independent_writer() {
        for (name, bits, raw) in [
            ("s31", 32, "0x7fa12345"),
            ("d31", 64, "0xfff0123456789abc"),
            ("q15", 128, "0x8123456789abcdef7ff0123456789abc"),
        ] {
            for enabled in [false, true] {
                for (width, height) in [(45, 12), (80, 24)] {
                    let mut project = app().project;
                    project.registers.vfp_command = "aarch64 vfp".into();
                    project.registers.tcl_endpoint = "127.0.0.1:6666".into();
                    project
                        .registers
                        .targets
                        .insert("default".into(), "cpu0".into());
                    if enabled {
                        project.registers.vfp_write_command = "aarch64 vfp_write".into();
                    }
                    let mut app = App::new(project, false);
                    app.snapshot.state = "STOPPED".into();
                    app.select_pane(3);
                    app.register_view.query = name.into();
                    app.register_view.rebuild();
                    app.selection = (0..app.register_view.rows.len())
                        .find(|i| {
                            app.register_view
                                .edit_candidate(*i, &app.register_context())
                                .is_ok_and(|c| c.target["id"] == name)
                        })
                        .unwrap();
                    let candidate = app
                        .register_view
                        .edit_candidate(app.selection, &app.register_context())
                        .unwrap();
                    assert_eq!(candidate.bits, bits);
                    assert_eq!(candidate.reason.is_none(), enabled);
                    if enabled {
                        let peer = Context {
                            core: "core1".into(),
                            ..app.register_context()
                        };
                        assert!(
                            app.register_view
                                .edit_candidate(app.selection, &peer)
                                .unwrap()
                                .reason
                                .unwrap()
                                .contains("this core's TCL target")
                        );
                    }
                    let (engine, requests) = engine();
                    app.key(
                        KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
                        Some(&engine),
                    );
                    app.write_paste(raw);
                    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                    terminal.draw(|f| super::super::draw(f, &mut app)).unwrap();
                    if width == 45 {
                        app.write_key(
                            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                            Some(&engine),
                        );
                    } else {
                        let buffer = terminal.backend().buffer();
                        let (x, y) = (0..height)
                            .find_map(|y| {
                                let row: String =
                                    (0..width).map(|x| buffer[(x, y)].symbol()).collect();
                                row.find("Preview").map(|x| (x as u16, y))
                            })
                            .unwrap();
                        app.write_mouse(
                            MouseEvent {
                                kind: MouseEventKind::Down(event::MouseButton::Left),
                                column: x,
                                row: y,
                                modifiers: KeyModifiers::NONE,
                            },
                            Some(&engine),
                        );
                    }
                    if enabled {
                        let request = requests.try_recv().unwrap();
                        assert_eq!(request.method, "write_preview");
                        assert_eq!(request.params["target"]["id"], name);
                        assert_eq!(request.params["input"]["text"], raw);
                        assert_eq!(request.params["context"], json!(app.register_context()));
                    } else {
                        assert!(requests.try_recv().is_err());
                    }
                }
            }
        }
    }
    #[test]
    fn register_view_preferences_migrate_once_and_restore_by_chip_core_and_catalogue() {
        let mut project = app().project;
        project.ui.register_views.clear();
        project.debug.chip = "chip-A".into();
        project
            .ui
            .formats
            .insert("register:r0".into(), crate::config::Radix::Decimal);
        let mut app = App::new(project, false);
        let object = serde_json::to_string(&("r0", Option::<String>::None)).unwrap();
        assert_eq!(
            app.register_display_format(&object),
            crate::registers::display::Format::Unsigned {
                radix: crate::config::Radix::Decimal
            }
        );
        app.register_view.open.insert("simd".into());
        app.register_view.fields.insert("cpsr".into());
        app.register_view.filter = 3;
        app.register_view.all_definitions = true;
        app.register_view.query = "cpsr".into();
        app.save_register_preferences(None);
        let core0_scope = app.register_view.preference_scope.clone();
        app.snapshot.core = Some(crate::session::CoreStatus {
            index: 1,
            name: "core1".into(),
            endpoint: "localhost:3334".into(),
            state: "STOPPED".into(),
        });
        app.sync_register_preferences();
        assert_ne!(app.register_view.preference_scope, core0_scope);
        assert_eq!(
            app.register_display_format(&object),
            crate::registers::display::Format::default()
        );
        assert_eq!(app.register_view.filter, 0);
        app.snapshot.core = None;
        app.sync_register_preferences();
        assert!(app.register_view.open.contains("simd"));
        assert!(app.register_view.fields.contains("cpsr"));
        assert_eq!(app.register_view.query, "cpsr");
        assert_eq!(app.register_view.filter, 3);
        assert!(app.register_view.all_definitions);
        let source = app.register_view.source.clone();
        app.register_view.source = "user:other-register-catalogue.toml".into();
        app.sync_register_preferences();
        assert_eq!(
            app.register_display_format(&object),
            crate::registers::display::Format::default()
        );
        app.register_view.source = source;
        app.project.debug.chip = "chip-B".into();
        app.sync_register_preferences();
        assert_eq!(app.register_view.filter, 0);
        assert_eq!(
            app.register_display_format(&object),
            crate::registers::display::Format::default()
        );
        app.project.debug.chip = "chip-A".into();
        app.sync_register_preferences();
        assert_eq!(app.register_view.preference_scope, core0_scope);
        let serialized = toml::to_string(&app.project).unwrap();
        let reloaded: Project = toml::from_str(&serialized).unwrap();
        let app = App::new(reloaded, false);
        assert_eq!(app.register_view.preference_scope, core0_scope);
        assert_eq!(app.register_view.filter, 3);
        assert_eq!(app.register_view.query, "cpsr");
    }
    #[test]
    fn register_float_vector_menu_uses_keyboard_mouse_and_never_reads_or_changes_samples() {
        let mut app = app();
        let (engine, requests) = engine();
        app.register_view.open = app
            .register_view
            .catalogue
            .as_ref()
            .unwrap()
            .groups
            .iter()
            .map(|g| g.id.clone())
            .collect();
        app.register_view.rebuild();
        let mut value = sample(&app, "d0", "0x0");
        value.value = Some(RawValue::parse("0x800000003f800000", 64).unwrap());
        let before = value.value.clone();
        app.register_view.values.insert(
            ("core:default".into(), "d0".into(), "default".into()),
            value,
        );
        app.selection = app.register_view.rows.iter().position(|row|matches!(row, Row::Register(i,_) if app.register_view.catalogue.as_ref().unwrap().registers[*i].id=="d0")).unwrap();
        app.open_format(None);
        let format = crate::registers::display::Format::Vector {
            lane_bits: 32,
            interpretation: crate::registers::display::Lane::Float,
        };
        let selected = crate::registers::display::choices(64, false)
            .iter()
            .position(|(f, _)| *f == format)
            .unwrap();
        for _ in 0..selected {
            app.format_key(
                KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
                Some(&engine),
            );
        }
        for (width, height) in [(100, 28), (44, 12)] {
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| formats::popup(f, &mut app)).unwrap();
            let text = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(text.contains("32-bit float"), "{width}x{height}: {text}");
            assert!(text.contains("-0.0"), "{width}x{height}: {text}");
        }
        assert!(requests.try_recv().is_err());
        let hit = app
            .formats
            .menu_hits
            .iter()
            .find(|(_, i)| *i == selected)
            .unwrap()
            .0;
        app.mouse(
            MouseEvent {
                kind: MouseEventKind::Down(event::MouseButton::Left),
                column: hit.x,
                row: hit.y,
                modifiers: KeyModifiers::NONE,
            },
            Some(&engine),
        );
        let request = requests.try_recv().unwrap();
        assert_eq!(request.method, "register_preferences");
        let object = serde_json::to_string(&("d0", Option::<String>::None)).unwrap();
        assert_eq!(app.register_display_format(&object), format);
        assert_eq!(
            app.register_view.values[&("core:default".into(), "d0".into(), "default".into())].value,
            before
        );
        assert!(requests.try_recv().is_err());
        app.open_format(None);
        app.format_key(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            Some(&engine),
        );
        assert!(requests.try_recv().is_err());
        app.open_format(None);
        app.project.debug.chip = "another-chip".into();
        app.sync_register_preferences();
        assert!(app.formats.popup.is_none());
        assert!(requests.try_recv().is_err());
    }
    #[test]
    fn expanded_groups_and_committed_search_save_only_view_preferences() {
        let mut app = app();
        let (engine, requests) = engine();
        app.selection = 0;
        app.register_key(
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
            Some(&engine),
        );
        assert_eq!(requests.try_recv().unwrap().method, "register_preferences");
        app.register_key(
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
            Some(&engine),
        );
        assert!(requests.try_recv().is_err());
        app.start_register_search();
        app.register_key(
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
            Some(&engine),
        );
        app.register_key(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            Some(&engine),
        );
        assert!(app.register_view.query.is_empty());
        assert!(requests.try_recv().is_err());
        app.start_register_search();
        app.register_key(
            KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE),
            Some(&engine),
        );
        app.register_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            Some(&engine),
        );
        let request = requests.try_recv().unwrap();
        assert_eq!(request.method, "register_preferences");
        assert_eq!(request.params["preferences"]["query"], "p");
        assert!(requests.try_recv().is_err());
    }
    #[test]
    fn capability_probe_is_explicit_single_flight_and_uses_physical_stop_generation() {
        let mut app = app();
        app.snapshot.generation = 900;
        app.snapshot.register_generation = Some(3);
        let (engine, requests) = engine();
        assert!(requests.try_recv().is_err());
        app.probe_registers(Some(&engine));
        let request = requests.try_recv().unwrap();
        assert_eq!(request.method, "registers_probe");
        assert_eq!(request.params["context"]["generation"], 3);
        app.probe_registers(Some(&engine));
        assert!(!app.ensure_registers(Some(&engine)));
        assert!(requests.try_recv().is_err());
        app.register_probe_response(request.id, &json!({}), Some("unavailable"));
        assert!(app.register_view.probe_pending.is_none());
        assert!(app.notice.contains("unavailable"));
        assert!(requests.try_recv().is_err(), "Failures must not retry");
        app.snapshot.frame.level = 1;
        app.probe_registers(Some(&engine));
        assert!(requests.try_recv().is_err());
        assert!(app.notice.contains("frame 0"));
    }
    #[test]
    fn selector_action_is_single_flight_and_failure_marks_only_its_pair_unavailable() {
        let mut app = app();
        let (engine, requests) = engine();
        app.register_view.open.insert("system".into());
        app.register_view.open.insert("mpu_el1".into());
        app.register_view.rebuild();
        app.selection = app.register_view.rows.iter().position(|row| matches!(row,
            Row::Register(i, _) if app.register_view.catalogue.as_ref().unwrap().registers[*i].id == "prbar23")).unwrap();
        app.command(Some(&engine), ":register-bank-read");
        assert!(requests.try_recv().is_err());
        assert!(app.notice.contains("Probe"));
        app.snapshot.register_probe = Some(crate::registers::capabilities::Probe {
            context: app.register_context(),
            thread: "1".into(),
            identity: None,
            facts: BTreeMap::new(),
            samples: vec![],
            gdb_names: vec![],
            notes: vec![],
        });
        app.command(Some(&engine), ":register-bank-read");
        assert!(requests.try_recv().is_err());
        assert!(app.notice.contains("MCR"));
        app.project.registers.selector_command = "arm mcr".into();
        for id in ["r0", "prbar23", "prlar23"] {
            let mut value = sample(&app, id, "0x1234");
            if id == "prlar23" {
                let mut provenance = crate::registers::provenance::Provenance::declared(
                    &crate::registers::Reader::Cp15 {
                        cp: 15,
                        op1: 0,
                        crn: 6,
                        crm: 3,
                        op2: 1,
                    },
                );
                provenance.access = Some(crate::registers::provenance::Access {
                    completed_ms: None,
                    timer: None,
                    pmu: None,
                    gic: None,
                    banked: None,
                    vfp: None,
                    vfp_pair: None,
                    route: crate::registers::provenance::Route::TclRegister {
                        endpoint: "127.0.0.1:6666".into(),
                        target: "cpu0".into(),
                        operation: "arm mrc".into(),
                    },
                    phase: crate::registers::provenance::Phase::Responded,
                    command: "last accepted request".into(),
                    context: value.context.clone(),
                    timestamp_ms: 23,
                });
                value.provenance = Some(provenance);
            }
            app.register_view
                .values
                .insert(("core:default".into(), id.into(), "default".into()), value);
        }
        app.command(Some(&engine), ":register-bank-read");
        let request = requests.try_recv().unwrap();
        assert_eq!(request.method, "registers_select");
        assert_eq!(request.params["kind"], "mpu_el1");
        assert_eq!(request.params["index"], 23);
        app.command(Some(&engine), ":register-bank-read");
        assert!(requests.try_recv().is_err());
        app.register_response(
            request.id,
            &json!({}),
            Some("data failed; selector restored"),
        );
        for id in ["prbar23", "prlar23"] {
            let value =
                &app.register_view.values[&("core:default".into(), id.into(), "default".into())];
            assert_eq!(value.state, State::Unavailable);
            assert_eq!(value.value.as_ref().unwrap().hex, "0x00001234");
            assert!(value.detail.contains("last sample at 23 ms"));
            assert!(
                value.provenance.is_none(),
                "whole-request errors cannot relabel old source as the latest attempt"
            );
            if id == "prlar23" {
                assert_eq!(
                    value
                        .value_provenance()
                        .unwrap()
                        .access
                        .as_ref()
                        .unwrap()
                        .command,
                    "last accepted request"
                );
            } else {
                assert!(matches!(
                    value.last_value_provenance,
                    Some(crate::registers::provenance::RetainedOrigin::Unknown)
                ));
            }
        }
        assert_eq!(
            app.register_view.values[&("core:default".into(), "r0".into(), "default".into())].state,
            State::Valid
        );
        assert!(app.register_view.bank_pending.is_none());
        assert!(app.pending_commands.is_empty());
        assert!(requests.try_recv().is_err());
    }
    #[test]
    fn a_late_selector_failure_cannot_damage_another_physical_stop() {
        let mut app = app();
        let original = sample(&app, "prbar1", "0x42");
        let key = ("core:default".into(), "prbar1".into(), "default".into());
        app.register_view
            .values
            .insert(key.clone(), original.clone());
        app.register_view.pending = Some((44, app.register_context()));
        app.register_view.bank_pending = Some(crate::registers::selector::Kind::MpuEl1.ids(1));
        app.pending_commands.insert(44);
        app.snapshot.generation += 1;
        app.register_response(44, &json!({}), Some("previous stop error"));
        let stored = &app.register_view.values[&key];
        assert_eq!(stored.state, original.state);
        assert_eq!(stored.context, original.context);
        assert!(app.register_view.bank_pending.is_none());
        assert!(app.pending_commands.is_empty());
    }
    #[test]
    fn late_capability_probe_response_cannot_change_another_stop_frame_or_session() {
        for change in 0..3 {
            let mut app = app();
            let (engine, requests) = engine();
            app.probe_registers(Some(&engine));
            let request = requests.try_recv().unwrap();
            match change {
                0 => app.snapshot.generation += 1,
                1 => app.snapshot.frame.level = 1,
                _ => app.snapshot.register_session += 1,
            }
            assert!(app.register_probe_response(request.id, &json!({"probe":{}}), None));
            assert!(app.snapshot.register_probe.is_none());
            assert!(app.register_view.probe_pending.is_none());
            assert!(app.pending_commands.is_empty());
            assert!(requests.try_recv().is_err());
        }
    }
    #[test]
    fn capability_facts_are_context_scoped_and_never_persist_as_customer_configuration() {
        let mut app = app();
        app.project
            .registers
            .facts
            .insert("icc.physical.prebits".into(), 7);
        app.register_view.facts = app.project.registers.facts.clone();
        let old = sample(&app, "r0", "0x42");
        app.register_view
            .values
            .insert(("core:default".into(), "r0".into(), "default".into()), old);
        let mut probe = crate::registers::capabilities::Probe {
            context: app.register_context(),
            thread: "1".into(),
            identity: None,
            facts: BTreeMap::new(),
            samples: vec![
                sample(&app, "midr", "0x411fd134"),
                sample(&app, "cpsr", "0x1a"),
                sample(&app, "icc_ctlr", "0x400"),
            ],
            gdb_names: vec![],
            notes: vec![],
        };
        let entry = probe
            .samples
            .iter_mut()
            .find(|s| s.id == "icc_ctlr")
            .unwrap();
        let wire = "view physical_icc midr 0x411fd134 dscr 0x01000200 dspsr 0xa2000410 dlr 0x81234568 id_pfr1 0x10111011 icc_hsre 0x0000000f icc_sre 0x00000007 icc_ctlr 0x00000400 ich_vtr 0x90180003 hcr 0x00000038 ich_hcr 0x00007c01 hstr 0x00001000 value 0x00000400";
        let mut provenance = crate::registers::provenance::Provenance::declared(
            &crate::registers::Catalogue::builtin("cortex-r52")
                .unwrap()
                .register("icc_ctlr")
                .unwrap()
                .reader,
        );
        provenance.access = Some(crate::registers::provenance::Access {
            banked: None,
            vfp: None,
            vfp_pair: None,
            gic: Some(
                crate::registers::gic::Response::parse(wire, "icc_ctlr", 32)
                    .unwrap()
                    .evidence,
            ),
            timer: None,
            pmu: None,
            route: crate::registers::provenance::Route::TclRegister {
                endpoint: "localhost:1".into(),
                target: "cpu0".into(),
                operation: "GIC read icc_ctlr".into(),
            },
            phase: crate::registers::provenance::Phase::Responded,
            command: "aarch64 gic icc_ctlr".into(),
            context: probe.context.clone(),
            timestamp_ms: 31,
            completed_ms: Some(32),
        });
        entry.source = "openocd:aarch64 gic".into();
        entry.view = crate::registers::SampleView::PhysicalCore;
        entry.provenance = Some(provenance);
        probe.decode();
        let (engine, requests) = engine();
        app.probe_registers(Some(&engine));
        let request = requests.try_recv().unwrap();
        app.register_probe_response(request.id, &json!({"probe":probe}), None);
        assert_eq!(app.register_view.facts["icc.physical.prebits"], 5);
        assert_eq!(app.project.registers.facts["icc.physical.prebits"], 7);
        assert_eq!(
            app.register_view.values[&("core:default".into(), "r0".into(), "default".into())].state,
            State::Stale
        );
        assert_eq!(
            app.register_view.values[&("core:default".into(), "cpsr".into(), "default".into())]
                .state,
            State::Valid
        );
        assert_eq!(
            app.register_view.values[&("core:default".into(), "icc_ctlr".into(), "default".into())]
                .value
                .as_ref()
                .unwrap()
                .hex,
            "0x00000400"
        );
        assert!(app.notice.contains("Raw evidence in Log"));
        assert!(requests.try_recv().is_err());
        app.snapshot.generation += 1;
        app.sync_register_capabilities();
        assert_eq!(app.register_view.facts["icc.physical.prebits"], 7);
        assert_eq!(app.project.registers.facts["icc.physical.prebits"], 7);
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
        for change in ["core", "session", "generation"] {
            let mut app = app();
            let (engine, requests) = engine();
            assert!(app.ensure_registers(Some(&engine)));
            let request = requests.try_recv().unwrap();
            let old = sample(&app, "r0", "0x12345678");
            match change {
                "session" => app.snapshot.register_session += 1,
                "generation" => app.snapshot.generation += 1,
                _ => {
                    app.snapshot.core = Some(crate::session::CoreStatus {
                        index: 1,
                        name: "core1".into(),
                        endpoint: "localhost:3334".into(),
                        state: "STOPPED".into(),
                    })
                }
            }
            app.register_response(request.id, &json!({"samples":[old]}), None);
            assert!(app.register_view.values.is_empty());
            assert!(app.register_view.pending.is_none());
            assert!(app.pending_commands.is_empty());
        }
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
        assert!(app.register_mouse(mouse, None));
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
        app.toggle_register_definitions(None);
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
        app.register_view.values.insert(
            ("core:default".into(), "cpsr".into(), "default".into()),
            value,
        );
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
                assert!(text.contains("Size") && text.contains("Access"));
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
