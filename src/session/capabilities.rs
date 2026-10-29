//! Explicit, bounded M/R identity probes; no implementation guesses on failure.
use super::*;
use crate::registers::{
    Catalogue, Context, Implementation, Reason, Sample, State,
    capabilities::{PROBE_IDS, Probe},
};
use std::collections::BTreeMap;
impl Engine {
    pub(super) fn effective_register_facts(&self) -> BTreeMap<String, u64> {
        if self.snapshot.state == state::STOPPED
            && let Ok(Some((catalogue, _))) = &self.register_catalogue
        {
            return catalogue.observation_facts_for_owners(
                &self.project.registers.facts,
                self.snapshot.register_probe.as_ref(),
                &self.snapshot.register_samples,
                &self.register_context(),
                &self.register_topology(),
            );
        }
        self.snapshot
            .register_probe
            .as_ref()
            .filter(|p| {
                p.context == self.register_context() && self.snapshot.state == state::STOPPED
            })
            .map(|p| p.effective(&self.project.registers.facts))
            .unwrap_or_else(|| self.project.registers.facts.clone())
    }
    pub(super) fn probe_register_capabilities(&mut self, p: &Json) -> Result<Json, String> {
        self.stopped()?;
        let context: Context = serde_json::from_value(p["context"].clone())
            .map_err(|e| format!("Probe context required: {e}"))?;
        if context != self.register_context() || context.frame != 0 {
            return Err("Capabilities require the current physical core at frame 0".into());
        }
        let current = self
            .register_catalogue
            .as_ref()
            .map_err(Clone::clone)?
            .as_ref()
            .ok_or("Select an adapted M3/M4/M7 or R52 register catalogue")?;
        let is_m = crate::registers::m_profile::adapted_cpu(&current.0.cpu);
        if !is_m && current.0.architecture != "armv8-r-aarch32" {
            return Err("This capability probe is adapted for M3/M4/M7 and R52 AArch32".into());
        }
        let catalogue = current.0.clone();
        let catalogue_source = current.1.clone();
        // Legacy named R52 IDs are an alternate route only for the exact
        // architectural reader, width and owner. Chip policies still apply.
        let r52_routes = if is_m {
            None
        } else {
            Some(Catalogue::builtin("cortex-r52")?)
        };
        let services = crate::debug_access::for_project(&self.project)?;
        let mut leases = services
            .iter()
            .map(|s| s.acquire(false))
            .collect::<Result<Vec<_>, _>>()?;
        self.check_register_read_cancelled()?;
        let thread = self.write_thread()?;
        let frame = self.mi("-stack-info-frame")?;
        if frame
            .data
            .field("frame")
            .is_none_or(|f| f.string("level") != "0")
        {
            return Err("GDB selected frame is not physical frame 0".into());
        }
        // Replace current evidence only on a complete probe. Physical faults
        // and context changes still invalidate it through their normal guards.
        let mut probe=Probe {context:context.clone(),thread,identity:None,facts:BTreeMap::new(),samples:vec![],nvic:None,gdb_names:vec![],notes:vec![
            "GDB names prove visibility only; unspecified widths and unprobed classes remain unknown".into(),
            "FPU presence, FPEXC.EN, banked-register and genuine MRRC capabilities are not inferred from CPACR or a read failure".into(),
            "Identity/MPU/GIC/VFP decoding uses Cortex-R52 TRM 100026_0104_01_en §§4.3, 10.3, 16.5–16.6 and DDI 0568 D1.3; target responses retained separately".into()]};
        if is_m {
            probe.notes = vec!["Cortex-M ID probe uses each core's explicit CorePrivate route; raw samples remain separate from decoded capacities".into(),
                "CPUID must match the adapted selected model; errors, invalid encodings and unenabled DWT remain Unknown, never trigger writes".into(),
                "CMSIS pinned sources and Arm DDI 0403/0439/0489 define the M ID fields; ICTR is an upper bound, not a real IRQ list".into()];
        }
        let mut values = super::registers::ReadCache::default();
        let core_ids = if is_m {
            crate::registers::m_profile::PROBE_IDS
        } else {
            PROBE_IDS
        };
        let mut ids: Vec<&str> = core_ids.to_vec();
        for id in &catalogue.discovery.registers {
            if !ids.contains(&id.as_str()) {
                ids.push(id);
            }
        }
        for id in ids {
            self.check_register_read_cancelled()?;
            if catalogue.register(id).is_none() {
                continue;
            }
            let register = catalogue
                .register(id)
                .ok_or_else(|| format!("Built-in probe register missing: {id}"))?;
            let mut sample = Sample {
                id: id.into(),
                state: State::NotRead,
                implementation: Implementation::Unknown,
                reason: Reason::Unknown,
                detail: String::new(),
                value: None,
                owner: Some(format!("core:{}", context.core)),
                context: context.clone(),
                view: crate::registers::SampleView::PhysicalCore,
                owner_generation: None,
                provenance: Some(crate::registers::provenance::Provenance::declared(
                    &register.reader,
                )),
                last_value_provenance: None,
                eligibility: None,
                last_value_eligibility: None,
                timestamp_ms: Stamp::now().elapsed_ms(self.session_started),
                source: super::registers::route_name(register),
            };
            sample.provenance.as_mut().unwrap().acquisition =
                crate::registers::provenance::Acquisition::CapabilityProbe;
            let hyp = probe.raw("cpsr").is_some_and(|n| n & 31 == 0x1a);
            let native_r52 = !is_m
                && self.project.registers.cp15_command == crate::registers::r52_core::COMMAND
                && crate::registers::r52_core::Request::from_reader(&register.reader).is_some();
            let gic = probe
                .facts
                .get("gic.system_interface")
                .is_some_and(|f| f.value == 1);
            let denied = if is_m {
                crate::registers::m_profile::probe_denial(&probe, &catalogue, id)
            } else {
                match id {
                _ if !matches!(id, "cpsr" | "midr")
                    && !probe
                        .identity
                        .as_ref()
                        .is_some_and(|i| i.model.as_deref() == Some("Cortex-R52")) =>
                {
                    Some((
                        if probe.identity.is_some() {
                            Reason::ReaderUnsupported
                        } else {
                            Reason::Unknown
                        },
                        "Actual MIDR has not established an adapted Cortex-R52 identity; selected catalogue alone does not authorize optional probes",
                    ))
                }
                "hmpuir" if !hyp && !native_r52 => Some((
                    Reason::AccessRestricted,
                    "EL2 access is not proven in this physical CPSR mode",
                )),
                "icc_ctlr" | "ich_vtr" if self.project.registers.gic_command.is_empty() && !hyp => {
                    Some((
                        Reason::AccessRestricted,
                        "Physical ICC / Hyp ICH interface requires proven Hyp mode; no virtual alias guess",
                    ))
                }
                "icc_ctlr" | "ich_vtr" if self.project.registers.gic_command.is_empty() && !gic => {
                    Some((
                        Reason::Unknown,
                        "GIC system-register capability has not been established",
                    ))
                }
                "pmcr"
                    if self.project.registers.pmu_command.is_empty()
                        && !probe.facts.get("pmu.present").is_some_and(|f| f.value == 1) =>
                {
                    Some((
                        Reason::Unknown,
                        "A known implemented PMU architecture has not been observed",
                    ))
                }
                _ => None,
            }.map(|(reason, detail)| (reason, detail.to_string()))
            };
            values.facts = Some(catalogue.observation_facts_for_owners(
                &self.project.registers.facts,
                Some(&probe),
                &[],
                &context,
                &self.register_topology(),
            ));
            let eligibility = catalogue
                .eligibility_with_owners(
                    register,
                    &self.project.registers.facts,
                    Some(&probe),
                    &[],
                    &context,
                    &self.register_topology(),
                )
                .with_catalogue_source(&catalogue_source);
            sample.implementation = eligibility.implementation;
            sample.eligibility = Some(eligibility);
            let denied = catalogue
                .declared_read_denial(register)
                .or(denied)
                .or_else(|| {
                    crate::registers::discovery::denial(
                        &catalogue,
                        register,
                        values.facts.as_ref().unwrap(),
                    )
                })
                .or_else(|| {
                    self.register_access_denial(
                        &catalogue,
                        register,
                        values.facts.as_ref().unwrap(),
                    )
                });
            if let Some((reason, detail)) = denied {
                sample.state = if matches!(
                    reason,
                    Reason::ReaderUnsupported | Reason::HardwareNotImplemented
                ) {
                    State::Unsupported
                } else if reason == Reason::WriteOnly {
                    State::NotRead
                } else {
                    State::Unavailable
                };
                sample.reason = reason;
                sample.detail = detail;
            } else {
                // A genuine named GDB register is an independent read route.
                self.register_value_access = None;
                if is_m || native_r52 {
                    values.facts = Some(catalogue.observation_facts_for_owners(
                        &self.project.registers.facts,
                        Some(&probe),
                        &[],
                        &context,
                        &self.register_topology(),
                    ));
                }
                let native = native_r52
                    || (id == "pmcr" && !self.project.registers.pmu_command.is_empty())
                    || (matches!(id, "icc_ctlr" | "ich_vtr")
                        && !self.project.registers.gic_command.is_empty());
                let result = if native {
                    sample.source = self.register_sample_origin(register, &catalogue).1;
                    self.read_register_value(register, &catalogue, &mut values)
                } else if r52_routes
                    .as_ref()
                    .and_then(|c| c.register(id))
                    .is_some_and(|r| {
                        r.reader == register.reader
                            && r.bits == register.bits
                            && r.scope == register.scope
                    })
                    && self.gdb_register_index(id).is_ok()
                {
                    sample.source = format!("gdb:{id}");
                    self.gdb_register_value(id, register.bits)
                } else {
                    self.read_register_value(register, &catalogue, &mut values)
                };
                let provenance = sample.provenance.as_mut().unwrap();
                provenance.acquisition = crate::registers::provenance::Acquisition::CapabilityProbe;
                provenance.access = self.register_value_access.clone();
                match result {
                    Ok(value)
                        if self.register_context() == context
                            && self.snapshot.state == state::STOPPED =>
                    {
                        sample.state = State::Valid;
                        sample.value = Some(value);
                    }
                    Ok(_) => {
                        sample.state = State::Stale;
                        sample.detail = "Context changed during capability inspection".into();
                    }
                    Err((reason, error)) => {
                        sample.reason = reason;
                        sample.detail = error;
                        sample.state = if reason == Reason::ReaderUnsupported {
                            State::Unsupported
                        } else if reason == Reason::TransportError {
                            State::Error
                        } else {
                            State::Unavailable
                        };
                    }
                }
            }
            probe.samples.push(sample);
            probe.decode_with_catalogue(&catalogue, &self.register_topology());
            if self.register_context() != context || self.snapshot.state != state::STOPPED {
                self.snapshot.register_probe = None;
                return Err("Context changed; discarded capability probe".into());
            }
            if self.register_access_fault.is_some() {
                break;
            }
        }
        if !is_m && self.project.registers.mmio_probe && self.register_access_fault.is_none() {
            self.probe_mmio_capabilities(&mut probe)?;
        }
        if !is_m && self.register_access_fault.is_none() {
            self.probe_stm_capabilities(&mut probe)?;
        }
        if is_m {
            let source = self.project.program.svd.to_string_lossy().into_owned();
            let device = if source.is_empty() {
                None
            } else {
                Some(crate::svd::Device::load_shared(&self.project.program.svd))
            };
            let mut nvic = crate::registers::m_profile::nvic_metadata(
                device.as_ref().and_then(|d| d.as_deref().ok()),
                &source,
                self.project
                    .registers
                    .facts
                    .get("nvic.priority_bits")
                    .copied(),
                &catalogue.cpu,
                probe.facts.get("nvic.lines_upper_bound").map(|f| f.value),
            );
            if let Some(Err(error)) = device {
                nvic.notes
                    .push(format!("SVD metadata unavailable: {error}"));
            }
            probe.notes.extend(nvic.notes.iter().cloned());
            probe.nvic = Some(nvic);
        }
        probe.gdb_names = self.reg_names.clone();
        probe.notes.sort();
        probe.notes.dedup();
        for (key, fact) in &probe.facts {
            if let Some(declared) = self.project.registers.facts.get(key)
                && *declared != fact.value
            {
                probe.notes.push(format!("{key}: configuration declared {declared}, observed {}; observation wins only for this context",fact.value));
            }
        }
        if let Some(error) = self.register_access_fault.clone() {
            self.log("capabilities", "Probe discarded after an uncertain channel result; the following evidence is not a current capability cache");
            for line in probe.report_lines() {
                self.log("capabilities", line);
            }
            for lease in &mut leases {
                lease.quarantine(&error);
            }
            self.state(state::FAULT);
            return Err(format!("Capability channel faulted; reconnect: {error}"));
        }
        let final_thread = self.write_thread()?;
        let final_frame = self.mi("-stack-info-frame")?;
        if final_thread != probe.thread
            || final_frame
                .data
                .field("frame")
                .is_none_or(|f| f.string("level") != "0")
            || self.register_context() != context
            || self.snapshot.state != state::STOPPED
        {
            self.snapshot.register_probe = None;
            return Err("Physical GDB thread/frame changed; discarded capability probe".into());
        }
        self.check_register_read_cancelled()?;
        self.store_register_samples(&probe.samples, &catalogue);
        self.snapshot.register_probe = Some(probe.clone());
        for line in probe.report_lines() {
            self.log("capabilities", line);
        }
        self.publish();
        Ok(
            json!({"context":context,"statistics":crate::registers::statistics::summarize(&catalogue, &probe.samples),"probe":probe,"facts":self.effective_register_facts(),"configured_facts":self.project.registers.facts,"fact_source":"configuration_and_current_target_observation",
                "configured_tools":{"gdb":self.project.gdb.executable,"service":self.project.service.as_ref().map(|s| &s.command),"source":"effective worker configuration; executable versions and hashes require host verification"}}),
        )
    }
}
