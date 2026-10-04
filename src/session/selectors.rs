//! Saved/restored MPU and PMU reads on one physical target under service leases.
use super::*;
use crate::registers::{
    Catalogue, Implementation, Reason, Sample, State,
    selector::{Kind, Plan, Request as SelectorRequest, decode_mpu},
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
        let mode = probe.raw("cpsr").ok_or("Physical CPSR mode is not known")? & 31;
        let plan = Plan::new(request.kind, request.index, count, mode)?;
        let operation = plan.script_with_sync(
            &self.project.registers.cp15_command,
            &self.project.registers.selector_command,
            &self.project.registers.isb_command,
        )?;
        let target = self
            .project
            .registers
            .targets
            .get(&request.context.core)
            .ok_or("Declare this core's system-register target")?
            .clone();
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
        let response = self.register_tcl(&operation);
        if let Some(error) = self.register_access_fault.clone() {
            for lease in &mut leases {
                lease.quarantine(&error);
            }
            self.state("FAULT");
            self.log(
                "selector",
                format!(
                    "{} index={} owner={} target={target}: outcome unknown; {error}",
                    plan.selector, plan.index, request.context.core
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
                {
                    self.snapshot.register_probe = None;
                }
                self.check_register_read_cancelled()?;
                let reason = if error.contains("selector synchronization unsupported")
                    || error.contains("adapter protocol unsupported")
                {
                    Reason::ReaderUnsupported
                } else {
                    reason
                };
                let samples: Vec<_> = plan
                    .ids
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
                        timestamp_ms: Stamp::now().elapsed_ms(self.session_started),
                        source: format!(
                            "openocd:selector:{target}:{}:{}",
                            plan.selector, plan.index
                        ),
                    })
                    .collect();
                self.store_register_samples(&samples, &Catalogue::builtin("cortex-r52")?);
                self.log(
                    "selector",
                    format!(
                        "{} index={} read failed after known restoration: {error}",
                        plan.selector, plan.index
                    ),
                );
                self.publish();
                return Err(format!("Selector read {reason:?}: {error}"));
            }
        };
        let mut evidence = match plan.parse(&raw) {
            Ok(evidence) => evidence,
            Err(error) => {
                self.register_access_fault = Some(error.clone());
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
        if !self.project.registers.isb_command.is_empty() {
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
            plan.selector,
            plan.index,
            evidence.original.hex,
            evidence.restored.hex,
            evidence.synchronization
        );
        let samples: Vec<_> = plan
            .ids
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
                timestamp_ms: Stamp::now().elapsed_ms(self.session_started),
                source: format!("openocd:selector:{target}:{}:{}", plan.selector, plan.index),
            })
            .collect();
        self.check_register_read_cancelled()?;
        self.store_register_samples(&samples, &Catalogue::builtin("cortex-r52")?);
        self.log(
            "selector",
            format!("{detail}; owner={} target={target}", request.context.core),
        );
        self.publish();
        Ok(
            json!({"context":request.context,"kind":request.kind,"index":plan.index,"count":plan.count,
            "selector":plan.selector,"evidence":evidence,"samples":samples,"region":region,
            "target":target,"endpoint":self.project.registers.tcl_endpoint,"source":"target-scoped MRC/MCR selector transaction; no counter enable/reset or MPU configuration write"}),
        )
    }
}
