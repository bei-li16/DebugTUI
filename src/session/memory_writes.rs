//! Declared RAM and SVD MMIO writers use the same single-use draft protocol.
use super::writes::{DRAFT_LIFETIME, MAX_DRAFTS, NEXT_DRAFT};
use super::*;
use crate::{
    live_watch::{connect, read_only_transaction, transact, word},
    registers::{Context, RawValue, Scope},
    writes::{Input, InputKind, MemoryKind, OnceState, Outcome, Prepared, Region, Selection},
};
use std::path::PathBuf;

pub(super) struct Draft {
    context: Context,
    thread: String,
    elf: PathBuf,
    created: Instant,
    target: Json,
    address: u64,
    region: Region,
    owner: String,
    endpoint: String,
    bus_target: String,
    payload: Payload,
}
enum Payload {
    Ram(Vec<u8>),
    Mmio {
        plan: Box<Prepared>,
        little: bool,
        native_little: bool,
        spec: Json,
    },
}

impl Engine {
    pub(super) fn write_owner(
        &self,
        region: &Region,
        context: &Context,
    ) -> Result<(String, usize), String> {
        let topology = &self.project.registers.topology;
        let owner = topology
            .owner(region.scope, &context.core)
            .or_else(|| {
                (region.scope == Scope::Chip && !self.project.debug.chip.is_empty())
                    .then(|| format!("chip:{}", self.project.debug.chip))
            })
            .ok_or("Write owner topology is unknown")?;
        if region.scope == Scope::Core {
            return Ok((owner, 1));
        }
        let related: Vec<_> = self
            .project
            .cores
            .iter()
            .filter(|c| {
                region.scope == Scope::Chip
                    || topology.clusters.get(&c.name) == topology.clusters.get(&context.core)
            })
            .collect();
        if self.project.cores.is_empty() {
            return Ok((owner, 1));
        }
        if related.is_empty()
            || (region.scope == Scope::Cluster
                && self
                    .project
                    .cores
                    .iter()
                    .any(|c| !topology.clusters.contains_key(&c.name)))
        {
            return Err("Shared write owner membership is not completely declared".into());
        }
        for core in &related {
            if !self
                .write_peers
                .iter()
                .any(|p| p.name == core.name && p.endpoint == core.endpoint && p.state == "STOPPED")
            {
                return Err(format!(
                    "Shared write requires coordinator-confirmed stopped core {}",
                    core.name
                ));
            }
        }
        Ok((owner, related.len()))
    }
    fn write_tcl(&mut self, channel: &str, command: &str, write: bool) -> Result<String, String> {
        if !self.memory_connections.contains_key(channel) {
            let endpoint = &self
                .project
                .memory_access
                .iter()
                .find(|a| a.id == channel)
                .ok_or("Unknown memory channel")?
                .tcl_endpoint;
            self.memory_connections
                .insert(channel.into(), connect(endpoint)?);
        }
        let stream = self.memory_connections.get_mut(channel).unwrap();
        let result = if write {
            transact(stream, command)
        } else {
            read_only_transaction(stream, command)
        };
        if result.is_err() {
            self.memory_connections.remove(channel);
        }
        result
    }
    fn memory_write_checks(
        &mut self,
        region: &Region,
        context: &Context,
    ) -> Result<(String, String, String, String), String> {
        self.stopped()?;
        if context != &self.register_context() || self.cancellation.load(Ordering::Relaxed) {
            return Err("Memory write context expired or cancelled".into());
        }
        let (owner, count) = self.write_owner(region, context)?;
        let thread = self.write_thread()?;
        let (endpoint, target) = if region.channel.is_empty() {
            let permission = self.mi("-gdb-show may-write-memory")?;
            if permission.data.string("value") != "on" {
                return Err("GDB memory writes are disabled (may-write-memory)".into());
            }
            let probe = self.mi("-info-gdb-mi-command data-write-memory-bytes")?;
            if probe
                .data
                .field("command")
                .is_none_or(|c| c.string("exists") != "true")
            {
                return Err("GDB memory byte writer is unavailable".into());
            }
            (self.project.target.endpoint.clone(), context.core.clone())
        } else {
            let access = self
                .project
                .memory_access
                .iter()
                .find(|a| a.id == region.channel)
                .filter(|a| a.cores.is_empty() || a.cores.contains(&context.core))
                .cloned()
                .ok_or("Write channel is unavailable for this core")?;
            if region.halted_targets.len() < count {
                return Err("Declare the physical CPU halted_targets for this bus write owner; an AP state is insufficient".into());
            }
            for cpu in &region.halted_targets {
                let state =
                    self.write_tcl(&region.channel, &format!("{} curstate", word(cpu)), false)?;
                if state.trim() != "halted" {
                    return Err(format!("Physical CPU {cpu} is not halted"));
                }
            }
            (access.tcl_endpoint, access.target)
        };
        if context != &self.register_context() || self.snapshot.state != "STOPPED" {
            return Err("Write context changed during capability checks".into());
        }
        Ok((owner, thread, endpoint, target))
    }
    fn native_endian(&mut self, channel: &str, target: &str) -> Result<bool, String> {
        match self
            .write_tcl(channel, &format!("{} cget -endian", word(target)), false)?
            .trim()
        {
            "little" => Ok(true),
            "big" => Ok(false),
            _ => Err("Bus target byte order is unknown".into()),
        }
    }
    fn svd_write_spec(
        &self,
        target: &Json,
    ) -> Result<(u64, crate::writes::Register, Option<bool>), String> {
        let device = crate::svd::Device::load(&self.project.program.svd)?;
        let peripheral = target["peripheral"]
            .as_str()
            .ok_or("Peripheral name required")?;
        let register = target["register"]
            .as_str()
            .ok_or("SVD register name required")?;
        let item = device
            .peripherals
            .iter()
            .find(|p| p.name == peripheral)
            .and_then(|p| p.registers.iter().find(|r| r.name == register))
            .ok_or("Unknown expanded SVD register")?;
        let mut spec = item.write.clone();
        let name = format!("{peripheral}.{register}");
        if let Some(rule) = self
            .project
            .writes
            .svd_overrides
            .iter()
            .find(|r| r.register == name)
        {
            if let Some(reserved) = rule.reserved {
                spec.reserved = reserved;
            }
            if let Some(ro) = rule.read_only_write {
                spec.read_only_write = ro;
            }
            if let Some(v) = &rule.verification {
                spec.verification = v.clone();
            }
        }
        Ok((item.address, spec, device.little_endian))
    }
    pub(super) fn preview_memory_write(&mut self, p: &Json) -> Result<Json, String> {
        self.stopped()?;
        self.project
            .writes
            .validate(&self.project.memory_access, &self.project.cores)?;
        let context: Context = serde_json::from_value(p["context"].clone())
            .map_err(|e| format!("Write context required: {e}"))?;
        if context != self.register_context() {
            return Err("Write context expired".into());
        }
        let input: Input =
            serde_json::from_value(p["input"].clone()).map_err(|e| format!("Write input: {e}"))?;
        let selection: Selection = serde_json::from_value(p["selection"].clone())
            .map_err(|e| format!("Write selection: {e}"))?;
        let target = &p["target"];
        let channel = target["channel"].as_str().unwrap_or("");
        let (address, bits, spec) = if target["kind"] == "memory" {
            if selection != Selection::Register {
                return Err("RAM writes select a complete value or byte range".into());
            }
            let bits = target["bits"]
                .as_u64()
                .filter(|b| (8..=32768).contains(b) && b.is_multiple_of(8))
                .ok_or("Memory width must be 1..4096 bytes")? as u16;
            (
                super::memory::literal_address(
                    target["address"]
                        .as_str()
                        .ok_or("Literal memory address required")?,
                )?,
                bits,
                None,
            )
        } else {
            let (address, spec, little) = self.svd_write_spec(target)?;
            (address, spec.bits, Some((spec, little)))
        };
        let region = self
            .project
            .writes
            .resolve(address, usize::from(bits / 8), channel, &context.core)?
            .clone();
        let initial_payload = if let Some((spec, svd_little)) = spec {
            if region.kind != MemoryKind::Mmio {
                return Err("SVD writes require a declared MMIO region".into());
            }
            if channel.is_empty() {
                return Err(
                    "SVD MMIO needs a declared TCL channel with a single word writer".into(),
                );
            }
            if !matches!(bits, 8 | 16 | 32)
                || !region.widths.contains(&bits)
                || !address.is_multiple_of(u64::from(bits / 8))
            {
                return Err("MMIO writer requires one declared aligned 8/16/32-bit word".into());
            }
            let little = region
                .little_endian
                .or(svd_little)
                .ok_or("MMIO layout byte order is unknown")?;
            let metadata = serde_json::to_value(&spec).map_err(|e| e.to_string())?;
            let plan = Box::new(spec.prepare(selection, &input, OnceState::Unknown)?);
            Payload::Mmio {
                plan,
                little,
                native_little: false,
                spec: metadata,
            }
        } else {
            match region.kind {
                MemoryKind::Flash => {
                    return Err("Flash writes require the configured Download flow".into());
                }
                MemoryKind::Mmio => {
                    return Err("Select an SVD register and its write semantics for MMIO".into());
                }
                MemoryKind::Ram => {}
            }
            if !region.byte_writable || !region.widths.contains(&8) {
                return Err("RAM byte accesses are not declared writable".into());
            }
            if input.kind != InputKind::Bytes
                && (!region.widths.contains(&bits) || !address.is_multiple_of(u64::from(bits / 8)))
            {
                return Err("Scalar RAM width or alignment is not declared".into());
            }
            Payload::Ram(crate::writes::memory_bytes(
                &input,
                bits,
                region.little_endian,
            )?)
        };
        self.write_drafts
            .1
            .retain(|_, d| d.created.elapsed() < DRAFT_LIFETIME && d.context == context);
        if self.write_drafts.len() >= MAX_DRAFTS {
            return Err("Too many write drafts; cancel unused drafts".into());
        }
        let services = crate::debug_access::for_project(&self.project)?;
        let _leases = services
            .iter()
            .map(|s| s.acquire(false))
            .collect::<Result<Vec<_>, _>>()?;
        let (owner, thread, endpoint, bus_target) = self.memory_write_checks(&region, &context)?;
        let payload = match initial_payload {
            Payload::Mmio {
                plan, little, spec, ..
            } => Payload::Mmio {
                plan,
                little,
                spec,
                native_little: self.native_endian(channel, &bus_target)?,
            },
            payload => payload,
        };
        let token = format!(
            "{:x}-{:x}",
            context.session,
            NEXT_DRAFT.fetch_add(1, Ordering::Relaxed)
        );
        let plan_json = match &payload {
            Payload::Ram(bytes) => {
                json!({"bytes":bytes,"byte_count":bytes.len(),"needs_fresh_read":false})
            }
            Payload::Mmio { plan, .. } => json!(plan),
        };
        let result = json!({"draft":token,"target":target,"address":format!("0x{address:x}"),"bits":bits,
            "owner":owner,"scope":region.scope,"thread":thread,"context":context,"elf":self.project.program.elf,
            "channel":if channel.is_empty() {"gdb"} else {channel},"endpoint":endpoint,"bus_target":bus_target,
            "region":region.id,"plan":plan_json,"expires_in_ms":DRAFT_LIFETIME.as_millis(),"outcome":Outcome::NotSent,
            "warning":"Apply writes this owner and range once. Shared service serialization does not make target accesses atomic; external clients and hardware remain independent."});
        self.write_drafts.1.insert(
            token,
            Draft {
                context,
                thread,
                elf: self.project.program.elf.clone(),
                created: Instant::now(),
                target: target.clone(),
                address,
                region,
                owner,
                endpoint,
                bus_target,
                payload,
            },
        );
        Ok(result)
    }
    fn mmio_value(
        &mut self,
        draft: &Draft,
        bits: u16,
        little: bool,
        native_little: bool,
    ) -> Result<RawValue, String> {
        let response = self.write_tcl(
            &draft.region.channel,
            &format!(
                "{} read_memory 0x{:x} {bits} 1",
                word(&draft.bus_target),
                draft.address
            ),
            false,
        )?;
        let tokens: Vec<_> = response.split_whitespace().collect();
        if tokens.len() != 1 {
            return Err("MMIO response must contain exactly one word".into());
        }
        let native = RawValue::parse(tokens[0], bits)?;
        RawValue::from_bytes(&native.bytes(native_little)?, bits, little)
    }
    pub(super) fn apply_memory_write(&mut self, token: &str) -> Result<Json, String> {
        let draft = self
            .write_drafts
            .1
            .remove(token)
            .ok_or("Write draft expired")?;
        let mut result = json!({"draft":token,"target":draft.target,"owner":draft.owner,"scope":draft.region.scope,
            "context":draft.context,"thread":draft.thread,"channel":if draft.region.channel.is_empty(){"gdb"}else{&draft.region.channel},
            "endpoint":draft.endpoint,"bus_target":draft.bus_target,"address":format!("0x{:x}",draft.address),"atomic":false,"outcome":Outcome::NotSent});
        let services = crate::debug_access::for_project(&self.project)?;
        let mut leases = match services
            .iter()
            .map(|s| s.acquire(false))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(l) => l,
            Err(error) => {
                result["error"] = json!(error);
                return Ok(result);
            }
        };
        let precondition: Result<(String, Option<crate::writes::Resolved>), String> = (|| {
            if draft.created.elapsed() >= DRAFT_LIFETIME || draft.elf != self.project.program.elf {
                return Err("Write draft or ELF expired".into());
            }
            let (owner, thread, endpoint, target) =
                self.memory_write_checks(&draft.region, &draft.context)?;
            if owner != draft.owner
                || thread != draft.thread
                || endpoint != draft.endpoint
                || target != draft.bus_target
            {
                return Err("Write route or owner changed".into());
            }
            if let Payload::Mmio {
                plan,
                little,
                native_little,
                spec,
            } = &draft.payload
            {
                let (address, current, svd_little) = self.svd_write_spec(&draft.target)?;
                if address != draft.address
                    || json!(current) != *spec
                    || draft.region.little_endian.or(svd_little) != Some(*little)
                {
                    return Err("SVD write metadata changed; preview again".into());
                }
                if self.native_endian(&draft.region.channel, &draft.bus_target)? != *native_little {
                    return Err("Bus target byte order changed".into());
                }
                let fresh = if plan.needs_fresh_read {
                    Some(self.mmio_value(
                        &draft,
                        plan.selected_mask.bits,
                        *little,
                        *native_little,
                    )?)
                } else {
                    None
                };
                let resolved = plan.resolve(fresh.as_ref())?;
                result["command_value"] = json!(resolved.command);
                result["verification_mask"] = json!(resolved.verification_mask);
                let native = RawValue::from_bytes(
                    &resolved.command.bytes(*little)?,
                    resolved.command.bits,
                    *native_little,
                )?;
                Ok((
                    format!(
                        "{} write_memory 0x{:x} {} {{{}}}",
                        word(&draft.bus_target),
                        draft.address,
                        native.bits,
                        native.hex
                    ),
                    Some(resolved),
                ))
            } else if let Payload::Ram(bytes) = &draft.payload {
                result["bytes"] = json!(bytes);
                let command = if draft.region.channel.is_empty() {
                    format!(
                        "-data-write-memory-bytes 0x{:x} {}",
                        draft.address,
                        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
                    )
                } else {
                    format!(
                        "{} write_memory 0x{:x} 8 {{{}}}",
                        word(&draft.bus_target),
                        draft.address,
                        bytes
                            .iter()
                            .map(u8::to_string)
                            .collect::<Vec<_>>()
                            .join(" ")
                    )
                };
                Ok((command, None))
            } else {
                unreachable!()
            }
        })();
        let (command, resolved) = match precondition {
            Ok(p) => p,
            Err(error) => {
                result["error"] = json!(error);
                result["code"] = json!("precondition_failed");
                return Ok(result);
            }
        };
        if self.cancellation.load(Ordering::Relaxed)
            || self.register_context() != draft.context
            || self.snapshot.state != "STOPPED"
        {
            result["error"] = json!("Write cancelled or context changed before sending");
            return Ok(result);
        }
        let sent = if draft.region.channel.is_empty() {
            self.mi(&command).map(|_| ())
        } else {
            self.write_tcl(&draft.region.channel, &command, true)
                .map(|_| ())
        };
        let outcome = match sent {
            Err(error) => {
                for lease in &mut leases {
                    lease.quarantine(&error);
                }
                self.register_access_fault = Some(error.clone());
                self.state("FAULT");
                result["error"] = json!(error);
                result["code"] = json!("write_result_unknown");
                Outcome::Unknown
            }
            Ok(()) => {
                let verify: Result<Outcome, String> = (|| {
                    if self.register_context() != draft.context || self.snapshot.state != "STOPPED"
                    {
                        return Err("Write accepted but context changed".into());
                    }
                    match &draft.payload {
                        Payload::Ram(bytes) => {
                            let read = self.read_memory_dump(&json!({"address":format!("0x{:x}",draft.address),"count":bytes.len(),"channel":draft.region.channel,"context":draft.context}))?;
                            result["observed_bytes"] = read["bytes"].clone();
                            if read["address"] != format!("0x{:x}", draft.address) {
                                return Err("Readback address differs from write".into());
                            }
                            Ok(if read["bytes"] == json!(bytes) {
                                Outcome::Verified
                            } else {
                                Outcome::Mismatch
                            })
                        }
                        Payload::Mmio {
                            little,
                            native_little,
                            ..
                        } => {
                            let resolved = resolved.as_ref().unwrap();
                            if resolved.verification_mask.integer()? == 0 {
                                return Ok(Outcome::Accepted);
                            }
                            let value = self.mmio_value(
                                &draft,
                                resolved.command.bits,
                                *little,
                                *native_little,
                            )?;
                            if self.register_context() != draft.context
                                || self.snapshot.state != "STOPPED"
                            {
                                return Err("Context changed during verification".into());
                            }
                            let matches = resolved.matches(&value)?;
                            result["observed"] = json!(value);
                            Ok(if matches {
                                Outcome::Verified
                            } else {
                                Outcome::Mismatch
                            })
                        }
                    }
                })();
                match verify {
                    Ok(o) => o,
                    Err(error) => {
                        result["error"] = json!(error);
                        result["code"] = json!("verification_unavailable");
                        Outcome::Accepted
                    }
                }
            }
        };
        result["outcome"] = json!(outcome);
        self.finish_write(&mut result);
        Ok(result)
    }
}
