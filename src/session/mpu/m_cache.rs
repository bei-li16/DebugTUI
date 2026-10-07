use super::*;
use crate::registers::{
    RawValue, Reason, SampleView, Scope,
    m_cache::{Cache, IDS, Plan, View},
    provenance::{Acquisition, Provenance, Route},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheRequest {
    context: Context,
    #[serde(default)]
    read: bool,
}

impl Engine {
    pub(in crate::session) fn m_cache_view(&mut self, params: &Json) -> Result<Json, String> {
        self.stopped()?;
        let request: CacheRequest =
            serde_json::from_value(params.clone()).map_err(|e| format!("M7 cache request: {e}"))?;
        if request.context != self.register_context() || request.context.frame != 0 {
            return Err("M7 cache requires this physical core at frame 0".into());
        }
        if !request.read {
            self.invalidate_mpu_view();
            return Ok(
                json!({"context":request.context,"read":false,"view":self.snapshot.register_cache}),
            );
        }
        if let Some(view) = &mut self.snapshot.register_cache {
            view.stale();
        }
        let result = self.read_m_cache(&request.context);
        if result.is_err() {
            self.publish();
        }
        result
    }
    pub(in crate::session) fn m_cache_selected_value(
        &mut self,
    ) -> Result<RawValue, (Reason, String)> {
        let context = self.register_context();
        let result = self
            .m_cache_view(&json!({"context":context,"read":true}))
            .map_err(|e| {
                (
                    if e.contains("explicit per-core Tcl/AP channel") {
                        Reason::ReaderUnsupported
                    } else {
                        Reason::Unknown
                    },
                    e,
                )
            })?;
        let view: View = serde_json::from_value(result["view"].clone())
            .map_err(|e| (Reason::Unknown, e.to_string()))?;
        let selected = view
            .original_selector
            .as_ref()
            .and_then(|v| v.integer().ok());
        let cache=view.caches.iter().find(|c|Some(c.selector.into())==selected).ok_or((Reason::HardwareNotImplemented,"Current CSSELR selects an unimplemented cache; use :cache to inspect the implemented cache".into()))?;
        self.register_value_access = cache
            .size_id
            .provenance
            .as_ref()
            .and_then(|p| p.access.clone());
        Ok(cache.size_id.value.as_ref().unwrap().clone())
    }
    fn read_m_cache(&mut self, context: &Context) -> Result<Json, String> {
        self.check_register_read_cancelled()?;
        if let Some(fault) = &self.register_access_fault {
            return Err(format!("Register channel faulted; reconnect: {fault}"));
        }
        let (actual, _) = self
            .register_catalogue
            .as_ref()
            .map_err(Clone::clone)?
            .as_ref()
            .ok_or("Select Cortex-M7 catalogue")?
            .clone();
        let builtin = Catalogue::builtin(&actual.cpu)?;
        let facts = self.effective_register_facts();
        let clidr = *facts
            .get("mcache.clidr")
            .ok_or("Probe this M7 core to establish CLIDR before cache reads")?;
        let ctr = *facts
            .get("mcache.ctr")
            .ok_or("Probe this M7 core to establish CTR before cache reads")?;
        let probe = self
            .snapshot
            .register_probe
            .as_ref()
            .filter(|p| p.context == *context)
            .ok_or("Probe this physical M7 core at the current stop")?
            .clone();
        let cpuid: u32 = probe
            .raw("scb.cpuid")
            .ok_or("Physical CPUID missing")?
            .try_into()
            .map_err(|_| "CPUID exceeds 32 bits")?;
        let plan = Plan::new(&builtin, cpuid, clidr as u32, ctr as u32)?;
        for id in IDS {
            let expected = builtin.register(id).unwrap();
            let definition = actual
                .register(id)
                .ok_or_else(|| format!("M7 cache definition missing: {id}"))?;
            if definition.reader != expected.reader
                || definition.bits != 32
                || definition.scope != Scope::Core
                || definition.read_side_effect
                || !definition.access.readable()
                || definition.access_rule != expected.access_rule
                || definition.present_if != expected.present_if
            {
                return Err(format!(
                    "M7 cache requires the verified definition for {id}"
                ));
            }
        }
        let binding = crate::registers::core_private::binding(
            &self.project.registers,
            &self.project.memory_access,
            &context.core,
        )?;
        let channel=self.project.memory_access.iter().find(|c|c.id==binding.channel).cloned().ok_or("M7 cache selector transaction requires an explicit per-core Tcl/AP channel; no GDB-only fallback")?;
        let services = crate::debug_access::for_project(&self.project)?;
        let mut leases = services
            .iter()
            .map(|s| s.acquire(false))
            .collect::<Result<Vec<_>, _>>()?;
        if self.mpu_physical_context()? != probe.thread {
            return Err("Physical GDB thread changed since M7 probe".into());
        }
        let script = plan.script(&channel.target);
        self.plan_register_value_access(
            Route::TclRegister {
                endpoint: channel.tcl_endpoint.clone(),
                target: channel.target.clone(),
                operation: "M7 cache: CSSELR select/read/restore with explicit target".into(),
            },
            script.clone(),
        );
        let mut stream = crate::live_watch::connect(&channel.tcl_endpoint)?;
        let mut progress = crate::live_watch::TransactionProgress::default();
        let response = crate::live_watch::transact_tracked(&mut stream, &script, &mut progress);
        self.register_value_progress(&progress);
        let values = match response.and_then(|text| plan.parse(&text)) {
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
                    return Err(format!("M7 cache outcome unknown; reconnect: {error}"));
                }
                self.check_register_read_cancelled()?;
                return Err(format!("M7 cache failed after known restoration: {error}"));
            }
        };
        self.check_register_read_cancelled()?;
        if !self
            .mpu_physical_context()
            .as_ref()
            .is_ok_and(|thread| thread == &probe.thread)
            || self.register_context() != *context
            || self.snapshot.state != state::STOPPED
        {
            self.snapshot.register_probe = None;
            return Err("Physical M7 cache context changed; complete batch discarded".into());
        }
        self.check_register_read_cancelled()?;
        let access = self.register_value_access.clone();
        let timestamp_ms = Stamp::now().elapsed_ms(self.session_started);
        let sample = |id: &str, value: RawValue, detail: &str| Sample {
            id: id.into(),
            state: State::Valid,
            implementation: Implementation::Yes,
            reason: Reason::Unknown,
            detail: detail.into(),
            value: Some(value),
            owner: Some(format!("core:{}", context.core)),
            context: context.clone(),
            timestamp_ms,
            source: format!("openocd:M7 cache {} {id}", channel.target),
            view: SampleView::PhysicalCore,
            owner_generation: None,
            provenance: Some(Provenance {
                acquisition: Acquisition::Catalogue,
                catalogue_reader: builtin.register(id).unwrap().reader.clone(),
                access: access.clone(),
                aliases: vec![],
            }),
            last_value_provenance: None,
            eligibility: None,
            last_value_eligibility: None,
        };
        let caches = plan
            .selectors
            .iter()
            .zip(&values[5..])
            .map(|(selector, value)| Cache {
                selector: *selector,
                kind: if *selector == 0 {
                    "data"
                } else {
                    "instruction"
                }
                .into(),
                size_id: sample(
                    "scb.ccsidr",
                    value.clone(),
                    "CCSIDR bank sampled within the verified selector restoration transaction",
                ),
            })
            .collect();
        let has_cache = !plan.selectors.is_empty();
        let view = View {
            context: context.clone(),
            owner: format!("core:{}", context.core),
            state: State::Valid,
            identity: sample(
                "scb.cpuid",
                values[0].clone(),
                "Physical identity before/after cache transaction",
            ),
            clidr: sample(
                "scb.clidr",
                values[1].clone(),
                "Implementation is separate from cache enable",
            ),
            ctr: sample("scb.ctr", values[2].clone(), "M7 architecture encoding"),
            original_selector: has_cache.then(|| values[3].clone()),
            restored_selector: has_cache.then(|| values[4].clone()),
            caches,
        };
        self.snapshot.register_cache = Some(view.clone());
        self.publish();
        Ok(json!({"context":context,"read":true,"view":view}))
    }
}
