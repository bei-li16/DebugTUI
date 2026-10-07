use super::*;
use crate::registers::{
    RawValue, Reason, SampleView, Scope,
    mpu::m_profile::{IDS, Plan, Region, View},
    provenance::{Acquisition, Provenance, Route},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MRequest {
    context: Context,
    bank: String,
    #[serde(default)]
    read: bool,
}
impl Engine {
    pub(super) fn m_profile_mpu(&mut self, params: &Json) -> Result<Json, String> {
        self.stopped()?;
        let request: MRequest =
            serde_json::from_value(params.clone()).map_err(|e| format!("M MPU request: {e}"))?;
        if request.bank != "m"
            || request.context != self.register_context()
            || request.context.frame != 0
        {
            return Err("M MPU requires the current physical core at frame 0".into());
        }
        if !request.read {
            self.invalidate_mpu_view();
            return Ok(
                json!({"context":request.context,"read":false,"view":self.snapshot.register_mpu}),
            );
        }
        if let Some(view) = &mut self.snapshot.register_mpu {
            view.stale();
        }
        let result = self.read_m_profile_mpu(&request.context);
        if result.is_err() {
            self.publish();
        }
        result
    }
    fn read_m_profile_mpu(&mut self, context: &Context) -> Result<Json, String> {
        self.check_register_read_cancelled()?;
        if let Some(fault) = &self.register_access_fault {
            return Err(format!("Register channel faulted; reconnect: {fault}"));
        }
        let (actual, _) = self
            .register_catalogue
            .as_ref()
            .map_err(Clone::clone)?
            .as_ref()
            .ok_or("Select an adapted Cortex-M catalogue")?
            .clone();
        let builtin = Catalogue::builtin(&actual.cpu)?;
        let facts = self.effective_register_facts();
        let count = *facts
            .get("mpu.regions")
            .ok_or("Probe this physical M core before reading MPU regions")?;
        let probe = self
            .snapshot
            .register_probe
            .as_ref()
            .filter(|p| p.context == *context)
            .ok_or("Probe this physical M core at the current stop")?
            .clone();
        let cpuid: u32 = probe
            .raw("scb.cpuid")
            .ok_or("Physical CPUID missing")?
            .try_into()
            .map_err(|_| "CPUID exceeds 32 bits")?;
        let plan = Plan::new(&builtin, count, cpuid)?;
        for id in IDS {
            let expected = builtin.register(id).unwrap();
            let definition = actual
                .register(id)
                .ok_or_else(|| format!("M MPU definition missing: {id}"))?;
            if definition.reader != expected.reader
                || definition.bits != 32
                || definition.scope != Scope::Core
                || definition.read_side_effect
                || !definition.access.readable()
                || definition.access_rule != expected.access_rule
                || definition.present_if != expected.present_if
            {
                return Err(format!("M MPU requires the verified definition for {id}"));
            }
        }
        let binding = crate::registers::core_private::binding(
            &self.project.registers,
            &self.project.memory_access,
            &context.core,
        )?;
        let channel = self.project.memory_access.iter().find(|c| c.id == binding.channel).cloned()
            .ok_or("M MPU region transactions require an explicit per-core Tcl/AP channel; GDB memory reads can display only the currently selected region")?;
        let services = crate::debug_access::for_project(&self.project)?;
        let mut leases = services
            .iter()
            .map(|s| s.acquire(false))
            .collect::<Result<Vec<_>, _>>()?;
        if self.mpu_physical_context()? != probe.thread {
            return Err("Physical GDB thread changed since M capability probe".into());
        }
        let script = plan.script(&channel.target);
        self.plan_register_value_access(
            Route::TclRegister {
                endpoint: channel.tcl_endpoint.clone(),
                target: channel.target.clone(),
                operation: "M MPU regions: explicit target, RNR select/read/restore".into(),
            },
            script.clone(),
        );
        let mut stream = crate::live_watch::connect(&channel.tcl_endpoint)?;
        let mut progress = crate::live_watch::TransactionProgress::default();
        let response = crate::live_watch::transact_tracked(&mut stream, &script, &mut progress);
        self.register_value_progress(&progress);
        let values = response.and_then(|text| plan.parse(&text));
        let values = match values {
            Ok(values) => values,
            Err(error) => {
                self.snapshot.register_probe = None;
                if error.contains("Selector restoration failed")
                    || !error.starts_with("TCL command failed")
                {
                    for lease in &mut leases {
                        lease.quarantine(&error);
                    }
                    self.register_access_fault = Some(error.clone());
                    self.state(state::FAULT);
                    return Err(format!("M MPU outcome unknown; reconnect: {error}"));
                }
                self.check_register_read_cancelled()?;
                return Err(format!(
                    "M MPU read failed after known restoration: {error}"
                ));
            }
        };
        // Cancellation is observed after the complete Tcl transaction, never during restoration.
        self.check_register_read_cancelled()?;
        let final_thread = self.mpu_physical_context();
        if !final_thread
            .as_ref()
            .is_ok_and(|thread| thread == &probe.thread)
            || self.register_context() != *context
            || self.snapshot.state != state::STOPPED
        {
            self.snapshot.register_probe = None;
            return Err("Physical M MPU context changed; complete batch discarded".into());
        }
        self.check_register_read_cancelled()?;
        let access = self.register_value_access.clone();
        let timestamp_ms = Stamp::now().elapsed_ms(self.session_started);
        let sample = |id: &str, value: RawValue| Sample {
            id: id.into(),
            state: State::Valid,
            implementation: Implementation::Yes,
            reason: Reason::Unknown,
            detail: if plan.count > 0 {
                "M MPU bank transaction; RNR restored and read back"
            } else {
                "M MPU absent; only CPUID/TYPE sampled, no selector access"
            }
            .into(),
            value: Some(value),
            owner: Some(format!("core:{}", context.core)),
            context: context.clone(),
            view: SampleView::PhysicalCore,
            owner_generation: None,
            provenance: Some(Provenance {
                acquisition: Acquisition::MpuRegions,
                catalogue_reader: builtin.register(id).unwrap().reader.clone(),
                access: access.clone(),
                aliases: vec![],
            }),
            last_value_provenance: None,
            eligibility: Some(
                builtin
                    .eligibility(
                        builtin.register(id).unwrap(),
                        &self.project.registers.facts,
                        Some(&probe),
                        context,
                    )
                    .with_catalogue_source(format!("builtin:{}", builtin.cpu)),
            ),
            last_value_eligibility: None,
            timestamp_ms,
            source: format!("openocd:mpu:{}:{}", channel.id, channel.target),
        };
        let view = View {
            context: context.clone(),
            owner: format!("core:{}", context.core),
            cpu: builtin.cpu.clone(),
            count: plan.count,
            state: State::Valid,
            identity: sample("scb.cpuid", values[0].clone()),
            mpu_type: sample("mpu.type", values[1].clone()),
            control: (plan.count > 0).then(|| sample("mpu.ctrl", values[2].clone())),
            original_selector: (plan.count > 0).then(|| values[3].clone()),
            restored_selector: (plan.count > 0).then(|| values[4].clone()),
            regions: (0..plan.count)
                .map(|index| Region {
                    index,
                    base: sample("mpu.rbar", values[5 + usize::from(index) * 2].clone()),
                    attributes: sample("mpu.rasr", values[6 + usize::from(index) * 2].clone()),
                })
                .collect(),
        };
        // Indexed values live in this bank view, not the unindexed current-RNR register cache.
        self.snapshot.register_mpu = Some(view.clone());
        self.publish();
        Ok(json!({"context":context,"read":true,"view":view}))
    }
}
