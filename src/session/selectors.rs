//! Saved/restored MPU and PMU reads on one physical target under service leases.
use super::*;
use crate::registers::{
    Catalogue, Implementation, Reason, Sample, State,
    selector::{Kind, NativePlan, Plan, Request as SelectorRequest, decode_mpu},
};

impl Engine {
    pub(super) fn read_selected_registers(&mut self, params: &Json) -> Result<Json, String> {
        self.stopped()?;
        if let Some(error) = &self.register_access_fault {
            return Err(format!("Register channel faulted; reconnect: {error}"));
        }
        let request: SelectorRequest =
            serde_json::from_value(params.clone()).map_err(|e| format!("Selector request: {e}"))?;
        if request.context != self.register_context() || request.context.frame != 0 {
            return Err(
                "Selector request requires current physical frame 0 and stop context".into(),
            );
        }
        let probe = self
            .snapshot
            .register_probe
            .as_ref()
            .filter(|p| p.context == request.context)
            .ok_or("Probe this physical R52 core at the current stop before using a selector")?
            .clone();
        if probe
            .identity
            .as_ref()
            .is_none_or(|i| i.model.as_deref() != Some("Cortex-R52"))
        {
            return Err("Actual Cortex-R52 identity is not adapted".into());
        }
        let native = self.project.registers.cp15_command == crate::registers::r52_core::COMMAND;
        if native && request.kind == Kind::Pmu {
            return Err("Native selector only supports adapted R52 MPU banks and indices".into());
        }
        let key = match request.kind {
            Kind::MpuEl1 => "mpu.el1.regions",
            Kind::MpuEl2 => "mpu.el2.regions",
            Kind::Pmu => "pmu.counters",
        };
        let count = probe
            .facts
            .get(key)
            .ok_or("Selector count has not been observed from this core")?
            .value;
        let native_plan = if native {
            if self.project.registers.selector_command != crate::registers::selector::NATIVE_COMMAND
            {
                return Err("Declare aarch64 r52_select for the native R52 MPU selector".into());
            }
            Some(NativePlan::new(request.kind, request.index, count)?)
        } else {
            None
        };
        let legacy_plan = if native {
            None
        } else {
            let mode = probe.raw("cpsr").ok_or("Physical CPSR mode is not known")? & 31;
            Some(Plan::new(request.kind, request.index, count, mode)?)
        };
        let (plan_index, plan_count, plan_selector, plan_ids, operation) =
            if let Some(plan) = &native_plan {
                (
                    plan.index,
                    plan.count,
                    plan.selector,
                    plan.ids.clone(),
                    plan.script(),
                )
            } else {
                let plan = legacy_plan.as_ref().unwrap();
                (
                    plan.index,
                    plan.count,
                    plan.selector,
                    plan.ids.clone(),
                    plan.script_with_sync(
                        &self.project.registers.cp15_command,
                        &self.project.registers.selector_command,
                        &self.project.registers.isb_command,
                    )?,
                )
            };
        let target = self
            .project
            .registers
            .targets
            .get(&request.context.core)
            .ok_or("Declare this core's system-register target")?
            .clone();
        let builtin = Catalogue::builtin("cortex-r52")?;
        let (catalogue, catalogue_source) = if native {
            self.register_catalogue
                .as_ref()
                .map_err(Clone::clone)?
                .as_ref()
                .ok_or("Select an R52 register catalogue")?
                .clone()
        } else {
            (builtin.clone(), "builtin:cortex-r52".into())
        };
        if native {
            let facts = self.effective_register_facts();
            let count_id = if request.kind == Kind::MpuEl2 {
                "hmpuir"
            } else {
                "mpuir"
            };
            for id in ["midr", count_id, plan_selector, &plan_ids[0], &plan_ids[1]] {
                let actual = catalogue
                    .register(id)
                    .ok_or_else(|| format!("Selector catalogue entry missing: {id}"))?;
                let expected = builtin
                    .register(id)
                    .ok_or("Built-in MPU selector definition missing")?;
                if actual.reader != expected.reader
                    || actual.bits != 32
                    || actual.scope != crate::registers::Scope::Core
                    || actual.read_side_effect
                    || !actual.access.readable()
                {
                    return Err(format!(
                        "Selector action requires the verified direct definition for {id}"
                    ));
                }
                if let Some((reason, detail)) =
                    catalogue.checked_el2_backend_denial(actual, &facts, true)
                {
                    return Err(format!(
                        "Selector read {reason:?}: selector action {id}: {detail}; no read sent"
                    ));
                }
                if catalogue.has_presence_rule(actual)
                    && catalogue.implementation(actual, &facts).0 != Implementation::Yes
                {
                    return Err(format!(
                        "Selector action {id}: presence not proven; no read sent"
                    ));
                }
            }
        }
        let eligibility: std::collections::BTreeMap<_, _> = plan_ids
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    catalogue
                        .eligibility(
                            catalogue.register(id).unwrap(),
                            &self.project.registers.facts,
                            Some(&probe),
                            &request.context,
                        )
                        .with_catalogue_source(&catalogue_source),
                )
            })
            .collect();
        let services = crate::debug_access::for_project(&self.project)?;
        let mut leases = services
            .iter()
            .map(|s| s.acquire(false))
            .collect::<Result<Vec<_>, _>>()?;
        self.check_register_read_cancelled()?;
        let thread = self.write_thread()?;
        let frame = self.mi("-stack-info-frame")?;
        if thread != probe.thread
            || frame
                .data
                .field("frame")
                .is_none_or(|f| f.string("level") != "0")
        {
            return Err("Actual physical GDB context changed since capability probe".into());
        }
        self.register_value_access = None;
        let label = format!("Selector {plan_selector} index {plan_index}");
        // The outer leases and GDB guards protect this whole operation. Parse
        // restoration evidence before the final context guard: a frame change
        // must not hide an uncertain selector outcome.
        let response = self.register_tcl_value(&operation, &label);
        let mut access = self.register_value_access.clone();
        if let Some(error) = self.register_access_fault.clone() {
            self.snapshot.register_probe = None;
            for lease in &mut leases {
                lease.quarantine(&error);
            }
            self.state("FAULT");
            self.log(
                "selector",
                format!(
                    "{} index={} owner={} target={target}: outcome unknown; {error}",
                    plan_selector, plan_index, request.context.core
                ),
            );
            return Err(format!(
                "Selector outcome unknown; reconnect before further access: {error}"
            ));
        }
        let raw = match response {
            Ok(raw) => raw,
            Err((reason, error)) => {
                if error.contains("Physical capability count changed")
                    || error.contains("Physical MIDR is not an adapted R52")
                    || error.contains("debugtui-r52:capacity-changed")
                {
                    self.snapshot.register_probe = None;
                }
                self.check_register_read_cancelled()?;
                let reason = if error.contains("selector synchronization unsupported")
                    || error.contains("adapter protocol unsupported")
                {
                    Reason::ReaderUnsupported
                } else if error.contains("debugtui-r52:access-restricted") {
                    Reason::AccessRestricted
                } else if error.contains("debugtui-r52:not-implemented") {
                    Reason::HardwareNotImplemented
                } else if error.contains("debugtui-r52:access-unknown")
                    || error.contains("debugtui-r52:selector-invalid")
                    || error.contains("debugtui-r52:capacity-changed")
                {
                    Reason::Unknown
                } else if error.contains("R52 selector reader unsupported") {
                    Reason::ReaderUnsupported
                } else {
                    reason
                };
                let samples: Vec<_> = plan_ids
                    .iter()
                    .map(|id| Sample {
                        id: id.clone(),
                        state: State::Unavailable,
                        implementation: Implementation::Unknown,
                        reason,
                        detail: self
                            .snapshot
                            .register_samples
                            .iter()
                            .find(|old| old.id == *id && old.value.is_some())
                            .map(|old| format!("{error}; last sample at {} ms", old.timestamp_ms))
                            .unwrap_or_else(|| error.clone()),
                        value: None,
                        owner: Some(format!("core:{}", request.context.core)),
                        context: request.context.clone(),
                        view: crate::registers::SampleView::PhysicalCore,
                        owner_generation: None,
                        provenance: Some(crate::registers::provenance::Provenance {
                            acquisition: crate::registers::provenance::Acquisition::SelectorBank,
                            catalogue_reader: catalogue.register(id).unwrap().reader.clone(),
                            access: access.clone(),
                            aliases: vec![],
                        }),
                        last_value_provenance: None,
                        eligibility: eligibility.get(id).cloned(),
                        last_value_eligibility: None,
                        timestamp_ms: Stamp::now().elapsed_ms(self.session_started),
                        source: format!(
                            "openocd:selector:{target}:{}:{}",
                            plan_selector, plan_index
                        ),
                    })
                    .collect();
                self.store_register_samples(&samples, &catalogue);
                self.log(
                    "selector",
                    format!(
                        "{} index={} read failed after known restoration: {error}",
                        plan_selector, plan_index
                    ),
                );
                self.publish();
                return Err(format!("Selector read {reason:?}: {error}"));
            }
        };
        let parsed = if let Some(plan) = &native_plan {
            plan.parse(&raw).and_then(|(evidence, proof)| {
                if proof.midr.integer()?
                    != u128::from(probe.raw("midr").ok_or("Probe MIDR missing")?)
                {
                    return Err("Selector physical identity changed since probe".into());
                }
                let provenance = access
                    .as_mut()
                    .filter(|a| {
                        a.context == request.context
                            && a.phase == crate::registers::provenance::Phase::Responded
                    })
                    .ok_or("Selector lacks current physical transaction provenance")?;
                provenance.r52_core = Some(proof);
                Ok(evidence)
            })
        } else {
            legacy_plan.as_ref().unwrap().parse(&raw)
        };
        let mut evidence = match parsed {
            Ok(evidence) => evidence,
            Err(error) => {
                self.register_access_fault = Some(error.clone());
                self.snapshot.register_probe = None;
                for lease in &mut leases {
                    lease.quarantine(&error);
                }
                self.state("FAULT");
                self.log(
                    "selector",
                    format!("Invalid selector evidence: {raw}; {error}"),
                );
                return Err(format!("Selector outcome unknown; reconnect: {error}"));
            }
        };
        if !native && !self.project.registers.isb_command.is_empty() {
            evidence.synchronization =
                "Genuine ISB via debugtui-armv8-1; selector restore readback verified".into();
        }
        self.check_register_read_cancelled()?;
        let final_thread = self.write_thread()?;
        let final_frame = self.mi("-stack-info-frame")?;
        if final_thread != thread
            || final_frame
                .data
                .field("frame")
                .is_none_or(|f| f.string("level") != "0")
            || self.register_context() != request.context
            || self.snapshot.state != "STOPPED"
        {
            self.snapshot.register_probe = None;
            return Err(
                "Selector was restored, but physical context changed; samples discarded".into(),
            );
        }
        let region = if request.kind != Kind::Pmu {
            Some(decode_mpu(
                request.kind,
                &evidence.values[0],
                &evidence.values[1],
            )?)
        } else {
            None
        };
        let detail = format!(
            "{} index={} saved={} restored={}; {}",
            plan_selector,
            plan_index,
            evidence.original.hex,
            evidence.restored.hex,
            evidence.synchronization
        );
        let samples: Vec<_> = plan_ids
            .iter()
            .zip(&evidence.values)
            .map(|(id, value)| Sample {
                id: id.clone(),
                state: State::Valid,
                implementation: Implementation::Yes,
                reason: Reason::Unknown,
                detail: detail.clone(),
                value: Some(value.clone()),
                owner: Some(format!("core:{}", request.context.core)),
                context: request.context.clone(),
                view: crate::registers::SampleView::PhysicalCore,
                owner_generation: None,
                provenance: Some(crate::registers::provenance::Provenance {
                    acquisition: crate::registers::provenance::Acquisition::SelectorBank,
                    catalogue_reader: catalogue.register(id).unwrap().reader.clone(),
                    access: access.clone(),
                    aliases: vec![],
                }),
                last_value_provenance: None,
                eligibility: eligibility.get(id).cloned(),
                last_value_eligibility: None,
                timestamp_ms: Stamp::now().elapsed_ms(self.session_started),
                source: format!("openocd:selector:{target}:{plan_selector}:{plan_index}"),
            })
            .collect();
        self.check_register_read_cancelled()?;
        self.store_register_samples(&samples, &catalogue);
        self.log(
            "selector",
            format!("{detail}; owner={} target={target}", request.context.core),
        );
        self.publish();
        Ok(
            json!({"context":request.context,"kind":request.kind,"index":plan_index,"count":plan_count,
            "selector":plan_selector,"evidence":evidence,"samples":samples,"region":region,
            "target":target,"endpoint":self.project.registers.tcl_endpoint,"source": if native {
                "native R52 current Debug EL2 MPU selector transaction; temporary selector writes restored and verified"
            } else { "target-scoped MRC/MCR selector transaction; no counter enable/reset or MPU configuration write" }}),
        )
    }
}
