//! Explicit, bounded direct MPU reads under the whole service lease. No selector writes.
use super::*;
use crate::registers::{
    Catalogue, Context, Implementation, RawValue, Register, Sample, State,
    mpu::{Bank, View},
    r52_core::{self, CapacityBank, Evidence},
};
mod m_cache;
mod m_profile;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    context: Context,
    bank: Bank,
    #[serde(default)]
    read: bool,
}

// This checks coherence of already validated per-read evidence. It never grants
// permission to a subsequent instruction; the backend proves that anew.
fn native_mpu_proof<'a>(
    raw: &'a super::registers::ReadCache,
    register: &Register,
    context: &Context,
    anchor: Option<&Evidence>,
    capacity: Option<&RawValue>,
) -> Result<&'a Evidence, String> {
    let access = raw
        .provenance
        .get(&register.id)
        .and_then(|p| p.access.as_ref())
        .filter(|a| {
            a.context == *context && a.phase == crate::registers::provenance::Phase::Responded
        })
        .ok_or("MPU read lacks current transaction provenance; samples discarded")?;
    let proof = access
        .r52_core
        .as_ref()
        .ok_or("MPU read lacks current Debug evidence; samples discarded")?;
    let request = r52_core::Request::from_reader(&register.reader)
        .ok_or("MPU definition has no bounded native route")?;
    proof.validate(&request)?;
    if let Some(anchor) = anchor
        && (proof.midr != anchor.midr
            || proof.dspsr != anchor.dspsr
            || proof.dlr != anchor.dlr
            || (proof.dscr.integer()? ^ anchor.dscr.integer()?)
                & u128::from(crate::registers::r52_debug::STATE_MASK)
                != 0)
    {
        return Err("Current Debug evidence changed during MPU read; samples discarded".into());
    }
    if proof.bank != CapacityBank::None && capacity != Some(&proof.capacity) {
        return Err("Fresh MPU bank capacity changed during batch; samples discarded".into());
    }
    Ok(proof)
}

impl Engine {
    pub(super) fn mpu_regions(&mut self, params: &Json) -> Result<Json, String> {
        if params["bank"] == "m" {
            return self.m_profile_mpu(params);
        }
        self.stopped()?;
        let request: Request =
            serde_json::from_value(params.clone()).map_err(|e| format!("MPU request: {e}"))?;
        if request.context != self.register_context() || request.context.frame != 0 {
            return Err("MPU view requires the current physical core at frame 0".into());
        }
        let probe = self
            .snapshot
            .register_probe
            .as_ref()
            .filter(|p| p.context == request.context)
            .ok_or("Probe this physical R52 core at the current stop before reading MPU regions")?
            .clone();
        if probe
            .identity
            .as_ref()
            .is_none_or(|i| i.model.as_deref() != Some("Cortex-R52"))
        {
            return Err("Actual Cortex-R52 identity is not adapted".into());
        }
        let count = request.bank.validate_count(
            probe
                .facts
                .get(request.bank.count_fact())
                .ok_or("MPU count has not been observed from this physical core")?
                .value,
        )?;
        if !request.read {
            return Ok(
                json!({"view":View::from_samples(request.bank, u64::from(count), &request.context, &self.snapshot.register_samples)?, "context":request.context, "read":false}),
            );
        }
        if let Some(fault) = &self.register_access_fault {
            return Err(format!("Register channel faulted; reconnect: {fault}"));
        }
        if self.project.registers.cp15_command.is_empty() {
            return Err("Declare a verified CP15 MRC command for direct MPU reads".into());
        }
        let builtin = Catalogue::builtin("cortex-r52")?;
        let (catalogue, catalogue_source) = self
            .register_catalogue
            .as_ref()
            .map_err(Clone::clone)?
            .as_ref()
            .ok_or("Select an R52 register catalogue")?
            .clone();
        let native = self.project.registers.cp15_command == r52_core::COMMAND;
        let reading_catalogue = if native { &catalogue } else { &builtin };
        let native_facts = native.then(|| self.effective_register_facts());
        let mut ids = vec![
            "cpsr".to_owned(),
            "midr".to_owned(),
            request.bank.count_id().to_owned(),
        ];
        ids.extend(request.bank.read_ids(count)?);
        // A user catalogue cannot redirect the fixed MPU action to a side-effect register,
        // a different encoding, a shared owner or an alias. Validate every ID before I/O.
        for id in &ids {
            let expected = builtin
                .register(id)
                .ok_or("Built-in MPU definition missing")?;
            let actual = catalogue
                .register(id)
                .ok_or_else(|| format!("MPU catalogue entry missing: {id}"))?;
            if actual.reader != expected.reader
                || actual.bits != 32
                || actual.scope != crate::registers::Scope::Core
                || actual.read_side_effect
                || !actual.access.readable()
            {
                return Err(format!(
                    "MPU action requires the verified direct definition for {id}"
                ));
            }
            if let Some(facts) = &native_facts {
                let denial = if id == "cpsr" {
                    catalogue.access_denial(actual, facts, true, None)
                } else {
                    catalogue.checked_el2_backend_denial(actual, facts, true)
                };
                if let Some((reason, detail)) = denial {
                    return Err(format!(
                        "MPU action {id}: {reason:?}: {detail}; no read sent"
                    ));
                }
                if catalogue.has_presence_rule(actual)
                    && catalogue.implementation(actual, facts).0 != Implementation::Yes
                {
                    return Err(format!(
                        "MPU action {id}: presence not proven; no read sent"
                    ));
                }
            }
        }
        let services = crate::debug_access::for_project(&self.project)?;
        let mut leases = services
            .iter()
            .map(|s| s.acquire(false))
            .collect::<Result<Vec<_>, _>>()?;
        let result = (|| -> Result<Json, String> {
            self.check_register_read_cancelled()?;
            let thread = self.mpu_physical_context()?;
            if thread != probe.thread {
                return Err("Actual physical GDB thread changed since capability probe".into());
            }
            let mut raw = super::registers::ReadCache::default();
            // CPSR is stopped program state. Native permissions come from the
            // current Debug proof, independently of this saved mode.
            self.read_register_value(
                reading_catalogue.register("cpsr").unwrap(),
                reading_catalogue,
                &mut raw,
            )
            .map_err(|(_, e)| e)?;
            let program_status = raw["cpsr"].integer()?;
            let mode = program_status & 31;
            if !native
                && (probe
                    .raw("cpsr")
                    .is_none_or(|old| u128::from(old & 31) != mode)
                    || !matches!(mode, 0x11 | 0x12 | 0x13 | 0x17 | 0x1a | 0x1b | 0x1f)
                    || (request.bank == Bank::El2 && mode != 0x1a))
            {
                return Err("MPU bank is inaccessible in the actual physical CPSR mode".into());
            }
            self.read_register_value(
                reading_catalogue.register("midr").unwrap(),
                reading_catalogue,
                &mut raw,
            )
            .map_err(|(_, e)| e)?;
            if raw["midr"].integer()? & 0xff0ffff0 != 0x410fd130
                || (native && probe.raw("midr").map(u128::from) != Some(raw["midr"].integer()?))
            {
                return Err("Physical MIDR changed; probe again before optional MPU reads".into());
            }
            let anchor = if native {
                Some(
                    native_mpu_proof(
                        &raw,
                        reading_catalogue.register("midr").unwrap(),
                        &request.context,
                        None,
                        None,
                    )?
                    .clone(),
                )
            } else {
                None
            };
            self.read_register_value(
                reading_catalogue.register(request.bank.count_id()).unwrap(),
                reading_catalogue,
                &mut raw,
            )
            .map_err(|(_, e)| e)?;
            if request.bank.count(&raw[request.bank.count_id()])? != count {
                return Err(
                    "Physical MIDR or MPU count changed; probe again before indexed reads".into(),
                );
            }
            let capacity = raw[request.bank.count_id()].clone();
            if native {
                native_mpu_proof(
                    &raw,
                    reading_catalogue.register(request.bank.count_id()).unwrap(),
                    &request.context,
                    anchor.as_ref(),
                    Some(&capacity),
                )?;
            }
            let mut samples = vec![];
            for id in &ids {
                self.check_register_read_cancelled()?;
                if self.register_context() != request.context || self.snapshot.state != "STOPPED" {
                    return Err("Physical MPU context changed; samples discarded".into());
                }
                let register = reading_catalogue.register(id).unwrap();
                let eligibility = reading_catalogue
                    .eligibility(
                        register,
                        &self.project.registers.facts,
                        Some(&probe),
                        &request.context,
                    )
                    .with_catalogue_source(if native {
                        &catalogue_source
                    } else {
                        "builtin:cortex-r52"
                    });
                let result = self.read_register_value(register, reading_catalogue, &mut raw);
                if native {
                    result.as_ref().map_err(|(_, e)| e.clone())?;
                    if id != "cpsr" {
                        native_mpu_proof(
                            &raw,
                            register,
                            &request.context,
                            anchor.as_ref(),
                            Some(&capacity),
                        )?;
                    }
                }
                let mut sample = Sample {
                    id: id.clone(),
                    state: State::Valid,
                    implementation: Implementation::Yes,
                    reason: crate::registers::Reason::Unknown,
                    detail: String::new(),
                    value: None,
                    owner: Some(format!("core:{}", request.context.core)),
                    context: request.context.clone(),
                    view: crate::registers::SampleView::PhysicalCore,
                    owner_generation: None,
                    provenance: raw.provenance.get(id).cloned().map(|mut p| {
                        p.acquisition = crate::registers::provenance::Acquisition::MpuRegions;
                        p
                    }),
                    last_value_provenance: None,
                    eligibility: Some(eligibility),
                    last_value_eligibility: None,
                    timestamp_ms: Stamp::now().elapsed_ms(self.session_started),
                    source: if native && id != "cpsr" {
                        format!("openocd:{}", r52_core::COMMAND)
                    } else {
                        super::registers::route_name(register)
                    },
                };
                match result {
                    Ok(value) => sample.value = Some(value),
                    Err((reason, error)) => {
                        sample.state = State::Unavailable;
                        sample.reason = reason;
                        sample.detail = error;
                        if let Some(old) = self
                            .snapshot
                            .register_samples
                            .iter()
                            .find(|old| old.id == *id && old.value.is_some())
                        {
                            sample.timestamp_ms = old.timestamp_ms;
                            sample
                                .detail
                                .push_str(&format!("; last raw sample at {} ms", old.timestamp_ms));
                        }
                    }
                }
                samples.push(sample);
                if let Some(fault) = self.register_access_fault.clone() {
                    self.state("FAULT");
                    return Err(format!("MPU read outcome unknown; reconnect: {fault}"));
                }
            }
            if native {
                // A new cache forces actual final reads under the same whole
                // service lease. The values here validate, never replace, the
                // original samples and their request provenance.
                let mut final_raw = super::registers::ReadCache::default();
                for id in ["midr", request.bank.count_id()] {
                    let register = reading_catalogue.register(id).unwrap();
                    self.read_register_value(register, reading_catalogue, &mut final_raw)
                        .map_err(|(_, e)| e)?;
                    native_mpu_proof(
                        &final_raw,
                        register,
                        &request.context,
                        anchor.as_ref(),
                        Some(&capacity),
                    )?;
                }
                if final_raw[request.bank.count_id()] != capacity {
                    return Err(
                        "Fresh MPU bank capacity changed at final validation; samples discarded"
                            .into(),
                    );
                }
            }
            let final_thread = self.mpu_physical_context()?;
            let final_status = self
                .gdb_register_value("cpsr", 32)
                .map_err(|(_, e)| e)?
                .integer()?;
            if final_thread != thread
                || (native && final_status != program_status)
                || (!native && final_status & 31 != mode)
                || self.register_context() != request.context
                || self.snapshot.state != "STOPPED"
            {
                return Err("Physical MPU context changed; samples discarded".into());
            }
            let view =
                View::from_samples(request.bank, u64::from(count), &request.context, &samples)?;
            self.check_register_read_cancelled()?;
            self.store_register_samples(&samples, &catalogue);
            self.log(
                "mpu",
                format!(
                    "{:?} {count} regions; owner={}; direct MRCs; no selector writes",
                    request.bank, view.owner
                ),
            );
            self.publish();
            Ok(
                json!({"context":request.context,"view":view,"samples":samples,"read":true,"source":"current physical core, direct indexed MPU reads; selectors unchanged; sequential sample"}),
            )
        })();
        let cancelled = result
            .as_ref()
            .err()
            .is_some_and(|e| e.starts_with("Register read cancelled;"));
        if result.is_err() && !cancelled {
            self.snapshot.register_probe = None;
            for sample in &mut self.snapshot.register_samples {
                sample.stale();
            }
            self.invalidate_register_samples();
            self.publish();
        }
        if let Some(fault) = self.register_access_fault.clone() {
            for lease in &mut leases {
                lease.quarantine(&fault);
            }
            self.state("FAULT");
            return Err(format!("MPU read outcome unknown; reconnect: {fault}"));
        }
        result
    }
    pub(super) fn mpu_physical_context(&mut self) -> Result<String, String> {
        let thread = self.write_thread()?;
        if self
            .mi("-stack-info-frame")?
            .data
            .field("frame")
            .is_none_or(|f| f.string("level") != "0")
        {
            return Err("Actual GDB frame is not physical frame 0".into());
        }
        Ok(thread)
    }
}
