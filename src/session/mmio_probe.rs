//! Explicit read-only aperture proof; no unlock, control write, or inferred base.
use super::*;
use crate::registers::{
    Implementation, RawValue, Reader, Reason, Sample, SampleView, Scope, State,
    capabilities::Probe,
    mmio_probe::{self, Descriptor},
    provenance::{Acquisition, Provenance},
};

impl Engine {
    pub(super) fn mmio_identity_sample(
        &mut self,
        probe: &mut Probe,
        d: &Descriptor,
        after: bool,
    ) -> Result<bool, String> {
        self.check_register_read_cancelled()?;
        if self.register_context() != probe.context || self.snapshot.state != "STOPPED" {
            self.snapshot.register_probe = None;
            return Err("Context changed; discarded MMIO capability probe".into());
        }
        let scope = if crate::registers::stm::component(d.component) {
            Scope::Chip
        } else if d.component == "gicd" {
            Scope::Cluster
        } else {
            Scope::Core
        };
        let mut topology = self.project.registers.topology.clone();
        if topology.chip.is_empty() {
            topology.chip = self.project.debug.chip.clone();
        }
        let owner = topology.owner(scope, &probe.context.core);
        let reader = Reader::Mmio {
            component: d.component.into(),
            offset: d.offset,
            require_owner_mapping: true,
        };
        let mut provenance = Provenance::declared(&reader);
        provenance.acquisition = Acquisition::CapabilityProbe;
        let mut sample = Sample {
            id: mmio_probe::sample_id(d, after),
            state: State::NotRead,
            implementation: Implementation::Unknown,
            reason: Reason::Unknown,
            detail: String::new(),
            value: None,
            owner,
            context: probe.context.clone(),
            view: SampleView::PhysicalCore,
            owner_generation: None,
            provenance: Some(provenance),
            last_value_provenance: None,
            eligibility: None,
            last_value_eligibility: None,
            timestamp_ms: Stamp::now().elapsed_ms(self.session_started),
            source: format!("mmio:{}", d.component),
        };
        self.register_value_access = None;
        let result = (|| {
            let binding = self
                .project
                .registers
                .component(d.component, sample.owner.as_deref(), true)
                .map_err(|e| (Reason::ReaderUnsupported, e))?
                .clone();
            let address = binding.base.checked_add(d.offset).ok_or((
                Reason::ReaderUnsupported,
                "Component address overflows".into(),
            ))?;
            let response = self.read_memory_channel(&json!({"channel":binding.channel,"address":address,"bits":d.bits,"little_endian":binding.little_endian}))
                .map_err(|e| (Reason::TransportError,e))?;
            let raw = response["value"].as_u64().ok_or((
                Reason::TransportError,
                "MMIO response lacks exact integer".into(),
            ))?;
            RawValue::from_integer(u128::from(raw), d.bits).map_err(|e| (Reason::TransportError, e))
        })();
        sample.provenance.as_mut().unwrap().access = self.register_value_access.clone();
        match result {
            Ok(raw) => {
                sample.state = State::Valid;
                sample.reason = Reason::Unknown;
                sample.value = Some(raw);
                sample.detail = "Read-only identity/capacity observation; permission and board owner association remain separate".into();
            }
            Err((reason, error)) => {
                sample.state = if reason == Reason::ReaderUnsupported {
                    State::Unsupported
                } else {
                    State::Error
                };
                sample.reason = reason;
                sample.detail = error;
            }
        }
        let valid = sample.state == State::Valid;
        probe.samples.push(sample);
        self.check_register_read_cancelled()?;
        if self.register_context() != probe.context || self.snapshot.state != "STOPPED" {
            self.snapshot.register_probe = None;
            return Err("Context changed; discarded MMIO capability probe".into());
        }
        Ok(valid)
    }
    fn mmio_identity_stage(
        &mut self,
        probe: &mut Probe,
        component: &str,
        after: bool,
    ) -> Result<bool, String> {
        for d in mmio_probe::descriptors(component) {
            if !self.mmio_identity_sample(probe, d, after)? {
                return Ok(false);
            }
            if !after && let Err(error) = mmio_probe::guard(probe, d) {
                probe.notes.push(format!(
                    "MMIO stage rejected: {error}; later {component} addresses not read"
                ));
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub(super) fn probe_mmio_capabilities(&mut self, probe: &mut Probe) -> Result<(), String> {
        if !self.mmio_identity_stage(probe, "debug_external", false)? {
            probe.decode();
            return Ok(());
        }
        for component in ["gicd", "gicr"] {
            if self.mmio_identity_stage(probe, component, false)? {
                self.mmio_identity_stage(probe, component, true)?;
            }
        }
        self.mmio_identity_stage(probe, "debug_external", true)?;
        probe.decode();
        Ok(())
    }
}
