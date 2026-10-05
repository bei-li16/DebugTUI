//! On-demand register access. No discovery sweep and no implicit core control.
use super::*;
use crate::registers::provenance::{Access, Derivation, Phase, Provenance, Route};
use crate::registers::{
    Catalogue, Context, Implementation, RawValue, Reader, Reason, Register, Sample, SampleView,
    State,
};
use std::collections::BTreeMap;
use std::sync::atomic::AtomicU64;

#[derive(Default)]
pub(super) struct ReadCache {
    values: BTreeMap<String, RawValue>,
    pub(super) provenance: BTreeMap<String, Provenance>,
    manual: bool,
}
impl std::ops::Deref for ReadCache {
    type Target = BTreeMap<String, RawValue>;
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}
impl std::ops::DerefMut for ReadCache {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.values
    }
}

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);
pub(super) fn new_session() -> u64 {
    NEXT_SESSION.fetch_add(1, Ordering::Relaxed)
}

impl Engine {
    // Check only between complete operations. The MI/Tcl transport keeps using
    // the session's exit flag so cancellation cannot skip selector restoration.
    pub(super) fn check_register_read_cancelled(&self) -> Result<(), String> {
        if self.read_cancel.load(Ordering::Relaxed) || self.cancellation.load(Ordering::Relaxed) {
            if let Some(fault) = &self.register_access_fault {
                return Err(format!("Register read outcome unknown; reconnect: {fault}"));
            }
            return Err(
                "Register read cancelled; current transaction completed, new results discarded"
                    .into(),
            );
        }
        Ok(())
    }
    pub(super) fn invalidate_register_samples(&mut self) {
        let context = self.register_context();
        if self
            .snapshot
            .register_probe
            .as_ref()
            .is_some_and(|p| p.context != context || self.snapshot.state != "STOPPED")
        {
            self.snapshot.register_probe = None;
        }
        for sample in &mut self.snapshot.register_samples {
            if self.snapshot.state != "STOPPED"
                || !sample.applies(&context, sample.owner.as_deref())
            {
                sample.stale();
            }
        }
        if matches!(self.register_catalogue, Ok(Some(_))) {
            for variable in &mut self.snapshot.registers {
                variable.error = !self.snapshot.register_samples.iter().any(|sample| {
                    sample.state == State::Valid
                        && sample.source == format!("gdb:{}", variable.name)
                });
            }
        }
    }

    pub(super) fn invalidate_register_boundary(&mut self) {
        self.write_drafts.clear();
        self.reg_names.clear();
        self.snapshot.generation += 1;
        self.snapshot.register_probe = None;
        self.invalidate_register_samples();
        self.snapshot.assembly.clear();
        self.snapshot.memory.clear();
        self.publish();
    }

    pub(super) fn register_sample_origin(
        &self,
        register: &Register,
        catalogue: &Catalogue,
    ) -> (SampleView, String, Option<String>) {
        let mut root = register;
        let mut aliases = Vec::new();
        for _ in 0..catalogue.registers.len() {
            let Reader::Alias { source, offset } = &root.reader else {
                break;
            };
            aliases.push(format!("alias:{source}@{offset}"));
            let Some(parent) = catalogue.register(source) else {
                return (SampleView::SelectedFrame, route_name(register), None);
            };
            root = parent;
        }
        let (view, route, gdb_name) = match &root.reader {
            reader
                if !self.project.registers.timer_command.is_empty()
                    && crate::registers::timer::route(reader).is_some() =>
            {
                (
                    SampleView::PhysicalCore,
                    format!("openocd:{}", self.project.registers.timer_command),
                    None,
                )
            }
            Reader::Gdb { name } => (
                SampleView::SelectedFrame,
                format!("gdb:{name}"),
                Some(name.clone()),
            ),
            Reader::Cp15_64 { .. } if self.project.registers.cp15_64_command.is_empty() => (
                SampleView::SelectedFrame,
                format!("gdb:{}", root.id),
                Some(root.id.clone()),
            ),
            Reader::Cp15_64 { .. } => (
                SampleView::PhysicalCore,
                format!("openocd:{}", self.project.registers.cp15_64_command),
                None,
            ),
            Reader::Alias { .. } => (SampleView::SelectedFrame, route_name(root), None),
            _ => (SampleView::PhysicalCore, route_name(root), None),
        };
        aliases.push(route);
        (view, aliases.join(" <- "), gdb_name)
    }

    fn selected_register_frame(&mut self) -> Result<(String, u32, String), String> {
        let thread = self.write_thread()?;
        let record = self.mi("-stack-info-frame")?;
        let frame = record
            .data
            .field("frame")
            .ok_or("GDB omitted the selected stack frame")?;
        let level = frame
            .string("level")
            .parse()
            .map_err(|_| "GDB omitted a valid frame level")?;
        let address = frame.string("addr");
        self.snapshot.frame = Frame::from_mi(frame);
        Ok((thread, level, address))
    }

    pub(super) fn register_context(&self) -> Context {
        Context {
            session: self.register_session,
            generation: self.snapshot.generation,
            core: self
                .project
                .preference_core
                .clone()
                .unwrap_or_else(|| "default".into()),
            frame: self.snapshot.frame.level,
        }
    }
    pub(super) fn registers_list(&self) -> Result<Json, String> {
        let configured = self.register_catalogue.as_ref().map_err(Clone::clone)?;
        let mut topology = self.project.registers.topology.clone();
        if topology.chip.is_empty() {
            topology.chip = self.project.debug.chip.clone();
        }
        Ok(match configured {
            Some((catalogue, source)) => {
                json!({"catalogue":catalogue,"source":source,"context":self.register_context(),"topology":topology,"topology_source":"configuration","facts":self.effective_register_facts(),"probe":self.snapshot.register_probe,"fact_source":if self.snapshot.register_probe.is_some(){"configuration_and_current_target_observation"}else{"configuration"}})
            }
            None => json!({"catalogue":null,"source":"gdb","context":self.register_context()}),
        })
    }
    pub(super) fn read_registers(&mut self, params: &Json) -> Result<Json, String> {
        self.stopped()?;
        let context = self.register_context();
        if let Some(expected) = params.get("context") {
            let expected: Context = serde_json::from_value(expected.clone())
                .map_err(|e| format!("Register context: {e}"))?;
            if expected != context {
                return Err(
                    "Register request belongs to an expired core, frame or stop context".into(),
                );
            }
        }
        let ids = params
            .get("ids")
            .and_then(Json::as_array)
            .ok_or("Register IDs are required")?;
        if ids.is_empty() || ids.len() > 128 {
            return Err("Read 1..128 register IDs per request".into());
        }
        let ids: Vec<&str> = ids
            .iter()
            .map(|id| id.as_str().ok_or("Register ID must be text"))
            .collect::<Result<_, _>>()?;
        let (catalogue, catalogue_source) = self
            .register_catalogue
            .as_ref()
            .map_err(Clone::clone)?
            .as_ref()
            .ok_or("Select a register catalogue first")?
            .clone();
        // Validate the complete request before touching the target.
        for id in &ids {
            if catalogue.register(id).is_none() {
                return Err(format!("Unknown register ID {id}"));
            }
        }
        let manual = params
            .get("manual")
            .and_then(Json::as_bool)
            .unwrap_or(false);
        let mut values = ReadCache {
            manual,
            ..Default::default()
        };
        let mut samples = Vec::new();
        let mut frame_proof = None;
        for id in ids {
            self.check_register_read_cancelled()?;
            let register = catalogue.register(id).unwrap();
            let evidence = catalogue
                .eligibility(
                    register,
                    &self.project.registers.facts,
                    self.snapshot.register_probe.as_ref(),
                    &context,
                )
                .with_catalogue_source(&catalogue_source);
            let implementation = evidence.implementation;
            let (readable, side_effect) = catalogue.read_policy(register);
            let mut topology = self.project.registers.topology.clone();
            if topology.chip.is_empty() {
                topology.chip = self.project.debug.chip.clone();
            }
            let owner = topology.owner(register.scope, &context.core);
            let (view, source, gdb_name) = self.register_sample_origin(register, &catalogue);
            let mut sample = Sample {
                id: id.into(),
                state: State::NotRead,
                implementation,
                reason: Reason::Unknown,
                detail: evidence.detail.clone(),
                value: None,
                owner,
                context: context.clone(),
                timestamp_ms: Stamp::now().elapsed_ms(self.session_started),
                source,
                view,
                owner_generation: None,
                provenance: Some(Provenance::declared(&register.reader)),
                last_value_provenance: None,
                eligibility: Some(evidence),
                last_value_eligibility: None,
            };
            if implementation == Implementation::No {
                sample.state = State::Unsupported;
                sample.reason = Reason::HardwareNotImplemented;
            } else if !readable {
                sample.reason = Reason::WriteOnly;
                sample.detail = "Register or alias dependency is write-only; no read sent".into();
            } else if sample.owner.is_none() {
                sample.state = State::Unavailable;
                sample.detail = "Register owner is unknown; configure chip/cluster topology".into();
            } else if side_effect && !manual {
                sample.detail = "Reading has side effects; explicit manual read required".into();
            } else if implementation == Implementation::Unknown
                && sample
                    .eligibility
                    .as_ref()
                    .is_some_and(|e| !e.conditions.is_empty())
                && !manual
            {
                sample.detail =
                    "Capability is unknown; verify it or request an explicit manual read".into();
            } else {
                // Missing target-description entries remain isolated Reader unsupported results.
                // Only an actual GDB value read requires a selected-frame proof.
                let available = gdb_name
                    .as_deref()
                    .map(|name| self.gdb_register_index(name).map(|_| ()))
                    .unwrap_or(Ok(()));
                let attempted = available.is_ok();
                if attempted && sample.view == SampleView::SelectedFrame && frame_proof.is_none() {
                    match self.selected_register_frame() {
                        Ok(proof) if proof.1 == context.frame => frame_proof = Some(proof),
                        result => {
                            self.invalidate_register_boundary();
                            self.refresh_pending = true;
                            return Err(format!(
                                "Selected register frame could not be verified; samples discarded: {result:?}"
                            ));
                        }
                    }
                }
                let result = available
                    .and_then(|()| self.read_register_value(register, &catalogue, &mut values));
                if let Some(provenance) = values.provenance.get(id) {
                    sample.provenance = Some(provenance.clone());
                }
                if attempted
                    && sample.view == SampleView::SelectedFrame
                    && self.snapshot.state == "STOPPED"
                {
                    let final_proof = self.selected_register_frame();
                    if final_proof.as_ref().ok() != frame_proof.as_ref() {
                        self.invalidate_register_boundary();
                        self.refresh_pending = true;
                        return Err(format!(
                            "GDB thread/frame changed during register read; samples discarded: {final_proof:?}"
                        ));
                    }
                }
                match result {
                    Ok(value)
                        if self.register_context() == context
                            && self.snapshot.state == "STOPPED" =>
                    {
                        sample.value = Some(value);
                        sample.state = State::Valid;
                        sample.detail.clear();
                    }
                    Ok(_) => {
                        sample.state = State::Stale;
                        sample.detail =
                            "Target context or running state changed during the read".into();
                    }
                    Err((reason, error)) => {
                        sample.reason = reason;
                        sample.detail = error;
                        sample.state = match reason {
                            Reason::HardwareNotImplemented => {
                                sample.implementation = Implementation::No;
                                State::Unsupported
                            }
                            Reason::ReaderUnsupported => State::Unsupported,
                            Reason::AccessRestricted
                            | Reason::FeatureDisabled
                            | Reason::Unknown => State::Unavailable,
                            _ => State::Error,
                        };
                    }
                }
            }
            samples.push(sample);
            if self.register_context() != context || self.snapshot.state != "STOPPED" {
                break;
            }
        }
        self.check_register_read_cancelled()?;
        self.store_register_samples(&samples, &catalogue);
        self.publish();
        Ok(json!({"context":context,"samples":samples}))
    }
    pub(super) fn store_register_samples(&mut self, samples: &[Sample], catalogue: &Catalogue) {
        for sample in samples {
            let previous = self
                .snapshot
                .register_samples
                .iter()
                .position(|old| old.id == sample.id);
            let mut stored = sample.clone();
            if stored.value.is_none()
                && let Some(index) = previous
            {
                stored.value = self.snapshot.register_samples[index].value.clone();
                stored.inherit_value_origin(&self.snapshot.register_samples[index]);
            }
            if let Some(index) = previous {
                self.snapshot.register_samples[index] = stored.clone();
            } else {
                self.snapshot.register_samples.push(stored.clone());
            }
            // Compatibility projection: core GDB values retain the old snapshot
            // shape, while precise values, state and provenance live in samples.
            if let Some(Register {
                reader: Reader::Gdb { name },
                ..
            }) = catalogue.register(&sample.id)
                && let Some(value) = &stored.value
            {
                let previous = self
                    .snapshot
                    .registers
                    .iter()
                    .position(|old| &old.name == name);
                let changed =
                    previous.is_some_and(|index| self.snapshot.registers[index].value != value.hex);
                let variable = Variable {
                    name: name.clone(),
                    value: value.hex.clone(),
                    changed,
                    error: stored.state != State::Valid,
                    ..Default::default()
                };
                if let Some(index) = previous {
                    self.snapshot.registers[index] = variable;
                } else {
                    self.snapshot.registers.push(variable);
                }
            }
        }
    }
    pub(super) fn read_register_value(
        &mut self,
        register: &Register,
        catalogue: &Catalogue,
        values: &mut ReadCache,
    ) -> Result<RawValue, (Reason, String)> {
        self.check_register_read_cancelled()
            .map_err(|error| (Reason::Unknown, error))?;
        if let Some(value) = values.get(&register.id) {
            self.register_value_access = values
                .provenance
                .get(&register.id)
                .and_then(|p| p.access.clone());
            return Ok(value.clone());
        }
        self.register_value_access = None;
        let result = (|| {
            if !self.project.registers.timer_command.is_empty()
                && let Some((name, bits)) = crate::registers::timer::route(&register.reader)
            {
                if register.bits != bits {
                    return Err((
                        Reason::ReaderUnsupported,
                        "Timer route requires its native register width".into(),
                    ));
                }
                return self.read_timer_register(name, bits);
            }
            Ok(match &register.reader {
                Reader::Gdb { name } => self.gdb_register_value(name, register.bits)?,
                Reader::Banked { name } => self.read_banked_register(name)?,
                Reader::Vfp { name } => self.read_vfp_register(name, values)?,
                Reader::Alias { source, offset } => {
                    let parent = catalogue.register(source).ok_or_else(|| {
                        (
                            Reason::ReaderUnsupported,
                            format!("Unknown alias source {source}"),
                        )
                    })?;
                    // Alias requests cannot bypass a parent's access or implementation restrictions.
                    let (implementation, evidence) =
                        catalogue.implementation(parent, &self.effective_register_facts());
                    let (readable, side_effect) = catalogue.read_policy(parent);
                    if implementation == Implementation::No {
                        return Err((Reason::HardwareNotImplemented, evidence));
                    }
                    if !readable
                        || (side_effect && !values.manual)
                        || (!values.manual
                            && !catalogue.automatic_read(parent, &self.effective_register_facts()))
                    {
                        return Err((
                            Reason::AccessRestricted,
                            "Alias source is not available for an automatic read".into(),
                        ));
                    }
                    self.read_register_value(parent, catalogue, values)?
                        .slice(*offset, register.bits)
                        .map_err(|error| (Reason::TransportError, error))?
                }
                Reader::Cp15 {
                    cp,
                    op1,
                    crn,
                    crm,
                    op2,
                } => {
                    if self.project.registers.cp15_command.is_empty() {
                        return Err((
                            Reason::ReaderUnsupported,
                            "Configure a verified registers.cp15_command for this backend".into(),
                        ));
                    }
                    let command = format!(
                        "{} {cp} {op1} {crn} {crm} {op2}",
                        self.project.registers.cp15_command
                    );
                    let text = self.register_tcl_value(&command, &command)?;
                    RawValue::parse(&text, register.bits)
                        .map_err(|error| (Reason::TransportError, error))?
                }
                Reader::Cp15_64 { cp, op1, crm } => {
                    // A named GDB register may already provide a genuine MRRC-backed value.
                    // Never synthesize a 64-bit system register from unrelated 32-bit MRC reads.
                    if self.project.registers.cp15_64_command.is_empty() {
                        self.gdb_register_value(&register.id, register.bits)?
                    } else {
                        let command = format!(
                            "if {{[catch {{aarch64 debugtui_adapter}} __dt_adapter] || $__dt_adapter ne \"{}\"}} {{error \"MRRC adapter protocol unsupported\"}}; {} {cp} {op1} {crm}",
                            crate::registers::OPENOCD_ADAPTER_PROTOCOL,
                            self.project.registers.cp15_64_command
                        );
                        let label = format!(
                            "{} {cp} {op1} {crm}",
                            self.project.registers.cp15_64_command
                        );
                        let text = self.register_tcl_value(&command, &label).map_err(
                            |(reason, error)| {
                                if error.contains("adapter protocol unsupported") {
                                    (Reason::ReaderUnsupported, error)
                                } else {
                                    (reason, error)
                                }
                            },
                        )?;
                        let text = text.trim();
                        if text.len() != 18
                            || !text.starts_with("0x")
                            || !text[2..].bytes().all(|b| b.is_ascii_hexdigit())
                        {
                            return Err((
                                Reason::ReaderUnsupported,
                                "MRRC adapter must return exactly 16 hexadecimal digits".into(),
                            ));
                        }
                        RawValue::parse(text, register.bits)
                            .map_err(|error| (Reason::TransportError, error))?
                    }
                }
                Reader::Backend { name } => {
                    let text = self.register_tcl_value(
                        &format!(
                            "dict get [get_reg -force [list {}]] {}",
                            crate::live_watch::word(name),
                            crate::live_watch::word(name)
                        ),
                        &format!("get_reg {name}"),
                    )?;
                    RawValue::parse(&text, register.bits)
                        .map_err(|error| (Reason::TransportError, error))?
                }
                Reader::Mmio { component, offset } => {
                    let binding = self
                        .project
                        .registers
                        .components
                        .get(component)
                        .cloned()
                        .ok_or_else(|| {
                            (
                                Reason::ReaderUnsupported,
                                format!("Component {component} has no board mapping"),
                            )
                        })?;
                    let address = binding.base.checked_add(*offset).ok_or_else(|| {
                        (Reason::TransportError, "Component address overflows".into())
                    })?;
                    let result = self.read_memory_channel(&json!({"channel":binding.channel,"address":address,"bits":register.bits,"little_endian":binding.little_endian})).map_err(|error|(Reason::TransportError,error))?;
                    let raw = result
                        .get("value")
                        .and_then(Json::as_u64)
                        .map(u128::from)
                        .ok_or_else(|| {
                            (
                                Reason::TransportError,
                                "Memory response lacks an exact integer value".into(),
                            )
                        })?;
                    RawValue::from_integer(raw, register.bits)
                        .map_err(|error| (Reason::TransportError, error))?
                }
            })
        })();
        let mut provenance = Provenance::declared(&register.reader);
        provenance.access = self.register_value_access.clone();
        if let Reader::Alias { source, offset } = &register.reader {
            provenance.aliases = values
                .provenance
                .get(source)
                .map(|p| p.aliases.clone())
                .unwrap_or_default();
            provenance.aliases.push(Derivation {
                source: source.clone(),
                offset: *offset,
                bits: register.bits,
            });
        }
        values.provenance.insert(register.id.clone(), provenance);
        if let Ok(value) = &result {
            values.insert(register.id.clone(), value.clone());
        }
        result
    }
    pub(super) fn gdb_register_value(
        &mut self,
        name: &str,
        bits: u16,
    ) -> Result<RawValue, (Reason, String)> {
        let index = self.gdb_register_index(name)?;
        self.check_register_read_cancelled()
            .map_err(|error| (Reason::Unknown, error))?;
        let command = format!("-data-list-register-values r {index}");
        self.plan_register_value_access(
            Route::GdbRegister {
                endpoint: self.connected_gdb_endpoint.clone(),
                configured_endpoint: self.project.target.endpoint.clone(),
                name: name.into(),
                index,
            },
            command.clone(),
        );
        let response = self
            .mi(&command)
            .map_err(|error| (Reason::Unknown, error))?;
        let value = response
            .data
            .field("register-values")
            .and_then(|values| {
                values
                    .items()
                    .iter()
                    .find(|value| value.string("number") == index.to_string())
            })
            .ok_or_else(|| (Reason::Unknown, format!("GDB did not return {name}")))?;
        RawValue::parse(&value.string("value"), bits).map_err(|error| (Reason::Unknown, error))
    }
    fn gdb_register_index(&mut self, name: &str) -> Result<usize, (Reason, String)> {
        if self.reg_names.is_empty() {
            let response = self
                .mi("-data-list-register-names")
                .map_err(|error| (Reason::TransportError, error))?;
            self.reg_names = response
                .data
                .field("register-names")
                .map(|names| {
                    names
                        .items()
                        .iter()
                        .map(|name| name.text().to_owned())
                        .collect()
                })
                .unwrap_or_default();
        }
        self.reg_names
            .iter()
            .position(|entry| entry == name)
            .ok_or_else(|| {
                (
                    Reason::ReaderUnsupported,
                    format!("GDB target description does not expose {name}"),
                )
            })
    }
    pub(super) fn register_tcl_value(
        &mut self,
        operation: &str,
        label: &str,
    ) -> Result<String, (Reason, String)> {
        self.register_tcl_value_tracked(operation, label, &mut false)
    }
    /// True once entering transport: errors afterwards may follow a hardware write.
    pub(super) fn register_tcl_tracked(
        &mut self,
        operation: &str,
        submitted: &mut bool,
    ) -> Result<String, (Reason, String)> {
        self.register_tcl_value_tracked(operation, "OpenOCD register transaction", submitted)
    }
    fn register_tcl_value_tracked(
        &mut self,
        operation: &str,
        label: &str,
        submitted: &mut bool,
    ) -> Result<String, (Reason, String)> {
        *submitted = false;
        self.check_register_read_cancelled()
            .map_err(|error| (Reason::Unknown, error))?;
        if let Some(error) = &self.register_access_fault {
            return Err((
                Reason::TransportError,
                format!(
                    "Register channel stopped after an uncertain access; reconnect to recover: {error}"
                ),
            ));
        }
        let endpoint = self.project.registers.tcl_endpoint.clone();
        let core = self.register_context().core;
        let target = self.project.registers.targets.get(&core).ok_or_else(|| {
            (
                Reason::ReaderUnsupported,
                format!("No system-register target for {core}"),
            )
        })?;
        if endpoint.is_empty() {
            return Err((
                Reason::ReaderUnsupported,
                "No register TCL endpoint configured".into(),
            ));
        }
        // One server-side Tcl evaluation encloses selection, access and restoration.
        // OpenOCD cannot dispatch another client's command between these statements.
        let target_name = target.clone();
        let target = crate::live_watch::word(target);
        let script = format!(
            "set __dt_old [target current]; set __dt_rc [catch {{targets {target}; if {{[{target} curstate] ne \"halted\"}} {{error \"Physical core is not halted\"}}; {operation}}} __dt_result]; set __dt_restore [catch {{targets $__dt_old; if {{[target current] ne $__dt_old}} {{error \"Target restore readback mismatch\"}}}} __dt_restore_error]; if {{$__dt_restore}} {{error \"Target restoration failed: $__dt_restore_error\"}}; if {{$__dt_rc}} {{error $__dt_result}}; set __dt_result"
        );
        self.plan_register_value_access(
            Route::TclRegister {
                endpoint: endpoint.clone(),
                target: target_name,
                operation: label.into(),
            },
            script.clone(),
        );
        let mut stream = crate::live_watch::connect(&endpoint)
            .map_err(|error| (Reason::TransportError, error))?;
        let mut progress = crate::live_watch::TransactionProgress::default();
        let result = crate::live_watch::transact_tracked(&mut stream, &script, &mut progress);
        *submitted = progress.started.is_some();
        self.register_value_progress(&progress);
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                if error.contains("Target restoration failed")
                    || error.contains("Selector restoration failed")
                    || error.contains("Core state restoration failed")
                    || !error.starts_with("TCL command failed")
                {
                    self.register_access_fault = Some(error.clone());
                    Err((Reason::TransportError, error))
                } else {
                    Err((Reason::Unknown, error))
                }
            }
        }
    }
    pub(super) fn plan_register_value_access(&mut self, route: Route, command: String) {
        self.register_value_access = Some(Access {
            timer: None,
            route,
            command,
            phase: Phase::Planned,
            context: self.register_context(),
            timestamp_ms: Stamp::now().elapsed_ms(self.session_started),
        });
    }
    pub(super) fn register_value_progress(
        &mut self,
        progress: &crate::live_watch::TransactionProgress,
    ) {
        if let Some(access) = &mut self.register_value_access
            && let Some(started) = progress.started
        {
            access.phase = if progress.responded {
                Phase::Responded
            } else {
                Phase::Started
            };
            access.timestamp_ms = started
                .saturating_duration_since(self.session_started)
                .as_millis() as u64;
        }
    }
}

pub(super) fn route_name(register: &Register) -> String {
    match &register.reader {
        Reader::Gdb { name } => format!("gdb:{name}"),
        Reader::Alias { source, .. } => format!("alias:{source}"),
        Reader::Mmio { component, .. } => format!("mmio:{component}"),
        Reader::Backend { name } => format!("openocd:{name}"),
        Reader::Banked { name } => format!("openocd:aarch64 banked:{name}"),
        Reader::Vfp { name } => format!("openocd:aarch64 vfp:{name}"),
        Reader::Cp15 { .. } => "openocd:cp15".into(),
        Reader::Cp15_64 { .. } => "gdb:cp15_64".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    fn engine() -> Engine {
        let mut project = Project::default();
        project.registers.cpu = "cortex-r52".into();
        project.registers.cp15_command = "aarch64 mrc".into();
        project
            .registers
            .targets
            .insert("default".into(), "cpu0".into());
        let (events, _) = mpsc::sync_channel(512);
        let mut engine = Engine::new(project, events, Arc::new(AtomicBool::new(false)));
        engine.snapshot.state = "STOPPED".into();
        engine
    }
    fn server(response: &'static str) -> (String, thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap().to_string();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut packet = Vec::new();
            let mut byte = [0];
            while stream.read_exact(&mut byte).is_ok() && byte[0] != 0x1a {
                packet.push(byte[0]);
            }
            stream.write_all(response.as_bytes()).unwrap();
            String::from_utf8(packet).unwrap()
        });
        (endpoint, worker)
    }
    #[test]
    fn catalogues_and_requests_validate_before_any_target_access() {
        let mut engine = engine();
        let listing = engine.registers_list().unwrap();
        assert_eq!(listing["catalogue"]["cpu"], "cortex-r52");
        assert!(
            engine
                .read_registers(&json!({"ids":["sctlr","missing"]}))
                .is_err()
        );
        let old = Context {
            session: engine.register_session + 1,
            ..engine.register_context()
        };
        assert!(
            engine
                .read_registers(&json!({"ids":["sctlr"],"context":old}))
                .is_err()
        );
        engine.snapshot.state = "RUNNING".into();
        assert!(engine.read_registers(&json!({"ids":["sctlr"]})).is_err());
    }
    #[test]
    fn register_display_preferences_require_no_gdb_and_validate_before_persistence() {
        let mut engine = engine();
        engine.snapshot.state = "RUNNING".into();
        let context = engine.register_context();
        let preferences = crate::registers::display::Preferences::default();
        assert_eq!(
            engine
                .execute(
                    "register_preferences",
                    &json!({"scope":"chip/core/catalogue","preferences":preferences})
                )
                .unwrap()["saved"],
            false
        );
        assert_eq!(engine.snapshot.state, "RUNNING");
        assert_eq!(engine.register_context(), context);
        assert!(
            engine
                .execute(
                    "register_preferences",
                    &json!({"scope":"","preferences":preferences})
                )
                .is_err()
        );
        assert!(
            engine
                .execute(
                    "register_preferences",
                    &json!({"scope":"chip/core/catalogue","preferences":{"filter":200}})
                )
                .is_err()
        );
        assert!(engine.execute("register_preferences", &json!({"scope":"chip/core/catalogue","preferences":preferences,"command":"continue"})).is_err());
        assert_eq!(engine.project.ui.register_views.len(), 1);
    }
    #[test]
    fn cp15_routes_physical_core_and_restores_target_in_one_packet() {
        let mut engine = engine();
        let (endpoint, worker) = server("__DEBUGTUI_RPC__0:0x12345678\x1a");
        engine.project.registers.tcl_endpoint = endpoint;
        let result = engine
            .read_registers(&json!({"ids":["sctlr"],"context":engine.register_context()}))
            .unwrap();
        assert_eq!(result["samples"][0]["state"], "valid");
        assert_eq!(result["samples"][0]["value"]["hex"], "0x12345678");
        assert_eq!(result["samples"][0]["owner"], "core:default");
        let access = &result["samples"][0]["provenance"]["access"];
        assert_eq!(access["route"]["kind"], "tcl_register");
        assert_eq!(access["route"]["target"], "cpu0");
        assert_eq!(
            access["route"]["endpoint"],
            engine.project.registers.tcl_endpoint
        );
        assert_eq!(access["route"]["operation"], "aarch64 mrc 15 0 1 0 0");
        assert_eq!(access["phase"], "responded");
        let packet = worker.join().unwrap();
        assert!(packet.contains("aarch64 mrc 15 0 1 0 0"));
        assert!(packet.contains("target current"));
        assert!(packet.contains("curstate"));
        assert!(packet.contains("targets \\$__dt_old"));
    }
    #[test]
    fn generic_backend_reader_records_its_target_request_without_claiming_banked_access() {
        let mut engine = engine();
        let (endpoint, worker) = server("__DEBUGTUI_RPC__0:0x01020304\x1a");
        engine.project.registers.tcl_endpoint = endpoint;
        engine
            .register_catalogue
            .as_mut()
            .unwrap()
            .as_mut()
            .unwrap()
            .0
            .registers[0]
            .reader = Reader::Backend {
            name: "custom".into(),
        };
        let result = engine.read_registers(&json!({"ids":["r0"]})).unwrap();
        let sample = &result["samples"][0];
        assert_eq!(sample["state"], "valid");
        assert_eq!(sample["value"]["hex"], "0x01020304");
        assert_eq!(sample["provenance"]["catalogue_reader"]["kind"], "backend");
        let access = &sample["provenance"]["access"];
        assert_eq!(access["phase"], "responded");
        assert_eq!(access["route"]["target"], "cpu0");
        assert_eq!(access["route"]["operation"], "get_reg custom");
        let packet = worker.join().unwrap();
        assert!(packet.contains("get_reg -force") && packet.contains("custom"));
        assert!(!packet.contains("aarch64 banked"));
    }
    #[test]
    fn restoration_failure_quarantines_channel_until_reconnect() {
        let mut engine = engine();
        let (endpoint, worker) =
            server("__DEBUGTUI_RPC__1:Target restoration failed: unavailable\x1a");
        engine.project.registers.tcl_endpoint = endpoint;
        let first = engine.read_registers(&json!({"ids":["sctlr"]})).unwrap();
        worker.join().unwrap();
        assert_eq!(first["samples"][0]["reason"], "transport_error");
        assert!(engine.register_access_fault.is_some());
        let second = engine.read_registers(&json!({"ids":["sctlr"]})).unwrap();
        assert!(
            second["samples"][0]["detail"]
                .as_str()
                .unwrap()
                .contains("reconnect")
        );
        let mut other_core = self::engine();
        other_core.project.registers.tcl_endpoint = engine.project.registers.tcl_endpoint.clone();
        let shared = other_core
            .read_registers(&json!({"ids":["sctlr"]}))
            .unwrap();
        assert!(
            shared["samples"][0]["detail"]
                .as_str()
                .unwrap()
                .contains("reconnect")
        );
        assert!(
            other_core
                .mi("-exec-continue")
                .unwrap_err()
                .contains("reconnect")
        );
        other_core
            .project
            .memory_access
            .push(crate::config::MemoryAccess {
                id: "ap".into(),
                target: "bus0".into(),
                tcl_endpoint: engine.project.registers.tcl_endpoint.clone(),
                ..Default::default()
            });
        assert!(
            other_core
                .read_memory_channel(
                    &json!({"channel":"ap","address":0,"bits":32,"little_endian":true})
                )
                .unwrap_err()
                .contains("reconnect")
        );
        crate::debug_access::recover(&other_core.project).unwrap();
    }
    #[test]
    fn absent_unknown_and_side_effect_items_do_not_probe_backend() {
        let mut engine = engine();
        engine
            .project
            .registers
            .facts
            .insert("icc.physical.prebits".into(), 5);
        let result = engine
            .read_registers(&json!({"ids":["icc_ap0r1","d0","icc_iar0"]}))
            .unwrap();
        assert_eq!(result["samples"][0]["implementation"], "no");
        assert_eq!(result["samples"][0]["reason"], "hardware_not_implemented");
        assert_eq!(result["samples"][1]["state"], "not_read");
        assert_eq!(result["samples"][2]["state"], "not_read");
        assert!(engine.register_access_fault.is_none());
    }
    #[test]
    fn timer_unknown_presence_never_dispatches_automatic_mrc_or_mrrc_reads() {
        let mut engine = engine();
        let catalogue = engine
            .register_catalogue
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .0
            .clone();
        let ids: Vec<_> = catalogue
            .registers
            .iter()
            .filter(|r| r.group == "timer")
            .map(|r| r.id.clone())
            .collect();
        assert_eq!(ids.len(), 15);
        let result = engine.read_registers(&json!({"ids":ids})).unwrap();
        for sample in result["samples"].as_array().unwrap() {
            assert_eq!(sample["state"], "not_read", "{}", sample["id"]);
            assert_eq!(sample["implementation"], "unknown");
            assert!(sample["value"].is_null());
            assert!(sample["provenance"]["access"].is_null());
        }
        assert!(engine.register_access_fault.is_none());
    }
    #[test]
    fn unsupported_64bit_register_never_issues_two_mrc_commands() {
        let mut engine = engine();
        engine
            .project
            .registers
            .facts
            .insert("timer.present".into(), 1);
        engine.reg_names = vec!["r0".into(), String::new(), "pc".into()];
        let result = engine.read_registers(&json!({"ids":["cntpct"]})).unwrap();
        assert_eq!(result["samples"][0]["reason"], "reader_unsupported");
    }
    #[test]
    fn explicit_mrrc_requires_full_width_and_preserves_high_bits_with_exact_source() {
        for (reply, expected) in [
            (
                "__DEBUGTUI_RPC__0:0xfedcba9876543210\x1a",
                Some("0xfedcba9876543210"),
            ),
            (
                "__DEBUGTUI_RPC__0:0x0000000000000001\x1a",
                Some("0x0000000000000001"),
            ),
            ("__DEBUGTUI_RPC__0:0x76543210\x1a", None),
            ("__DEBUGTUI_RPC__0:0x00000000000000zz\x1a", None),
        ] {
            let mut engine = engine();
            engine.project.registers.cp15_64_command = "aarch64 mrrc".into();
            let (endpoint, worker) = server(reply);
            engine.project.registers.tcl_endpoint = endpoint;
            let result = engine
                .read_registers(&json!({"ids":["cntpct"],"manual":true}))
                .unwrap();
            let sample = &result["samples"][0];
            assert_eq!(sample["source"], "openocd:aarch64 mrrc");
            if let Some(expected) = expected {
                assert_eq!(sample["state"], "valid");
                assert_eq!(sample["value"]["hex"], expected);
                assert_eq!(sample["value"]["bits"], 64);
            } else {
                assert_eq!(sample["reason"], "reader_unsupported");
                assert!(sample["value"].is_null());
            }
            let packet = worker.join().unwrap();
            assert_eq!(packet.matches("aarch64 mrrc 15 0 14").count(), 1);
            assert!(packet.contains("aarch64 debugtui_adapter"));
            assert!(!packet.contains("aarch64 mrc "));
            assert!(engine.register_access_fault.is_none());
        }
    }
    #[test]
    fn mrrc_uncertain_core_state_fault_stops_the_channel_and_shared_gdb_access() {
        let mut engine = engine();
        engine.project.registers.cp15_64_command = "aarch64 mrrc".into();
        let (endpoint, worker) =
            server("__DEBUGTUI_RPC__1:Core state restoration failed: outcome unknown\x1a");
        engine.project.registers.tcl_endpoint = endpoint;
        let result = engine
            .read_registers(&json!({"ids":["cntpct"],"manual":true}))
            .unwrap();
        worker.join().unwrap();
        assert_eq!(result["samples"][0]["reason"], "transport_error");
        assert!(engine.register_access_fault.is_some());
        assert!(
            engine
                .mi("-exec-continue")
                .unwrap_err()
                .contains("reconnect")
        );
        let again = engine
            .read_registers(&json!({"ids":["cntpct"],"manual":true}))
            .unwrap();
        assert!(
            again["samples"][0]["detail"]
                .as_str()
                .unwrap()
                .contains("reconnect")
        );
    }
}
