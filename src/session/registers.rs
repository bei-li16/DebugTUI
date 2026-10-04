//! On-demand register access. No discovery sweep and no implicit core control.
use super::*;
use crate::registers::{
    Catalogue, Context, Implementation, RawValue, Reader, Reason, Register, Sample, State,
};
use std::collections::BTreeMap;
use std::sync::atomic::AtomicU64;

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);
pub(super) fn new_session() -> u64 {
    NEXT_SESSION.fetch_add(1, Ordering::Relaxed)
}

impl Engine {
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
        Ok(match configured {
            Some((catalogue, source)) => {
                json!({"catalogue":catalogue,"source":source,"context":self.register_context(),"facts":self.effective_register_facts(),"probe":self.snapshot.register_probe,"fact_source":if self.snapshot.register_probe.is_some(){"configuration_and_current_target_observation"}else{"configuration"}})
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
        let (catalogue, _) = self
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
        let mut values = BTreeMap::new();
        let mut samples = Vec::new();
        for id in ids {
            if self.cancellation.load(Ordering::Relaxed) {
                return Err("Register read cancelled".into());
            }
            let register = catalogue.register(id).unwrap();
            let (implementation, evidence) =
                register.implementation(&self.effective_register_facts());
            let mut topology = self.project.registers.topology.clone();
            if topology.chip.is_empty() {
                topology.chip = self.project.debug.chip.clone();
            }
            let owner = topology.owner(register.scope, &context.core);
            let mut sample = Sample {
                id: id.into(),
                state: State::NotRead,
                implementation,
                reason: Reason::Unknown,
                detail: evidence,
                value: None,
                owner,
                context: context.clone(),
                timestamp_ms: Stamp::now().elapsed_ms(self.session_started),
                source: if matches!(register.reader, Reader::Cp15_64 { .. })
                    && !self.project.registers.cp15_64_command.is_empty()
                {
                    format!("openocd:{}", self.project.registers.cp15_64_command)
                } else {
                    route_name(register)
                },
            };
            if implementation == Implementation::No {
                sample.state = State::Unsupported;
                sample.reason = Reason::HardwareNotImplemented;
            } else if !register.access.readable() {
                sample.reason = Reason::WriteOnly;
                sample.detail = "Architectural write-only operation; no read sent".into();
            } else if sample.owner.is_none() {
                sample.state = State::Unavailable;
                sample.detail = "Register owner is unknown; configure chip/cluster topology".into();
            } else if register.read_side_effect && !manual {
                sample.detail = "Reading has side effects; explicit manual read required".into();
            } else if implementation == Implementation::Unknown
                && !register.conditions.is_empty()
                && !manual
            {
                sample.detail =
                    "Capability is unknown; verify it or request an explicit manual read".into();
            } else {
                match self.read_register_value(register, &catalogue, &mut values) {
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
        values: &mut BTreeMap<String, RawValue>,
    ) -> Result<RawValue, (Reason, String)> {
        if let Some(value) = values.get(&register.id) {
            return Ok(value.clone());
        }
        let value = match &register.reader {
            Reader::Gdb { name } => self.gdb_register_value(name, register.bits)?,
            Reader::Banked { name } => self.read_banked_register(name)?,
            Reader::Alias { source, offset } => {
                let parent = catalogue.register(source).ok_or_else(|| {
                    (
                        Reason::ReaderUnsupported,
                        format!("Unknown alias source {source}"),
                    )
                })?;
                // Alias requests cannot bypass a parent's access or implementation restrictions.
                if !parent.access.readable()
                    || parent.read_side_effect
                    || parent.implementation(&self.effective_register_facts()).0
                        == Implementation::No
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
                let text = self.register_tcl(&command)?;
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
                    let text = self.register_tcl(&command).map_err(|(reason, error)| {
                        if error.contains("adapter protocol unsupported") {
                            (Reason::ReaderUnsupported, error)
                        } else {
                            (reason, error)
                        }
                    })?;
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
                let text = self.register_tcl(&format!(
                    "dict get [get_reg -force [list {}]] {}",
                    crate::live_watch::word(name),
                    crate::live_watch::word(name)
                ))?;
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
        };
        values.insert(register.id.clone(), value.clone());
        Ok(value)
    }
    pub(super) fn gdb_register_value(
        &mut self,
        name: &str,
        bits: u16,
    ) -> Result<RawValue, (Reason, String)> {
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
        let index = self
            .reg_names
            .iter()
            .position(|entry| entry == name)
            .ok_or_else(|| {
                (
                    Reason::ReaderUnsupported,
                    format!("GDB target description does not expose {name}"),
                )
            })?;
        let response = self
            .mi(&format!("-data-list-register-values r {index}"))
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
    pub(super) fn register_tcl(&mut self, operation: &str) -> Result<String, (Reason, String)> {
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
        let target = crate::live_watch::word(target);
        let script = format!(
            "set __dt_old [target current]; set __dt_rc [catch {{targets {target}; if {{[{target} curstate] ne \"halted\"}} {{error \"Physical core is not halted\"}}; {operation}}} __dt_result]; set __dt_restore [catch {{targets $__dt_old; if {{[target current] ne $__dt_old}} {{error \"Target restore readback mismatch\"}}}} __dt_restore_error]; if {{$__dt_restore}} {{error \"Target restoration failed: $__dt_restore_error\"}}; if {{$__dt_rc}} {{error $__dt_result}}; set __dt_result"
        );
        let mut stream = crate::live_watch::connect(&endpoint)
            .map_err(|error| (Reason::TransportError, error))?;
        match crate::live_watch::transact(&mut stream, &script) {
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
}

pub(super) fn route_name(register: &Register) -> String {
    match &register.reader {
        Reader::Gdb { name } => format!("gdb:{name}"),
        Reader::Alias { source, .. } => format!("alias:{source}"),
        Reader::Mmio { component, .. } => format!("mmio:{component}"),
        Reader::Backend { name } => format!("openocd:{name}"),
        Reader::Banked { name } => format!("openocd:aarch64 banked:{name}"),
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
        let packet = worker.join().unwrap();
        assert!(packet.contains("aarch64 mrc 15 0 1 0 0"));
        assert!(packet.contains("target current"));
        assert!(packet.contains("curstate"));
        assert!(packet.contains("targets \\$__dt_old"));
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
    fn unsupported_64bit_register_never_issues_two_mrc_commands() {
        let mut engine = engine();
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
