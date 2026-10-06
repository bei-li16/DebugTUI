//! Explicit STM component mappings opt into identity reads during Probe caps.
use super::*;
use crate::registers::{Scope, capabilities::Probe, stm};

impl Engine {
    fn stm_stage(
        &mut self,
        probe: &mut Probe,
        component: &str,
        after: bool,
    ) -> Result<bool, String> {
        for descriptor in stm::descriptors(component) {
            if !self.mmio_identity_sample(probe, descriptor, after)? {
                return Ok(false);
            }
            if let Err(error) = if after {
                stm::consistent_pair(probe, descriptor)
            } else {
                stm::guard(probe, descriptor)
            } {
                probe.notes.push(format!(
                    "STM stage rejected: {error}; later {component} addresses not read"
                ));
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub(super) fn probe_stm_capabilities(&mut self, probe: &mut Probe) -> Result<(), String> {
        let mut topology = self.project.registers.topology.clone();
        if topology.chip.is_empty() {
            topology.chip = self.project.debug.chip.clone()
        }
        let owner = topology.owner(Scope::Chip, &probe.context.core);
        let Ok(binding) = self
            .project
            .registers
            .component("stm", owner.as_deref(), true)
            .cloned()
        else {
            return Ok(());
        };
        if !binding.base.is_multiple_of(0x1000) || binding.base.checked_add(0xfff).is_none() {
            probe.notes.push(
                "STM mapping rejected before I/O: explicit aligned 4 KiB control aperture required"
                    .into(),
            );
            return Ok(());
        }
        if !self.stm_stage(probe, "stm", false)? {
            probe.decode();
            return Ok(());
        }
        for name in ["stm_hwe", "stm_dma"] {
            if let Ok(optional) = self
                .project
                .registers
                .component(name, owner.as_deref(), true)
            {
                if optional.base != binding.base
                    || optional.channel != binding.channel
                    || optional.little_endian != binding.little_endian
                {
                    probe.notes.push(format!("STM {name} mapping rejected before I/O: must explicitly select the same control aperture and route"));
                    continue;
                }
                if self.stm_stage(probe, name, false)? {
                    self.stm_stage(probe, name, true)?;
                }
            }
        }
        self.stm_stage(probe, "stm", true)?;
        probe.decode();
        Ok(())
    }
}
