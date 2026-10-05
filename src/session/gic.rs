//! Explicit GIC adapter; never fall back after selecting a checked protocol.
use super::*;
use crate::registers::{RawValue, Reason, gic};

impl Engine {
    pub(super) fn read_gic_register(
        &mut self,
        name: &str,
        bits: u16,
    ) -> Result<RawValue, (Reason, String)> {
        let operation = format!(
            "if {{[catch {{aarch64 debugtui_gic_protocol}} __dt_gic_protocol] || $__dt_gic_protocol ne \"{}\"}} {{error \"GIC adapter protocol unsupported\"}}; \
             if {{[catch {{{} {}}} __dt_gic_result]}} {{set __dt_gic_code $::errorCode; \
             if {{[[target current] curstate] ne \"halted\"}} {{error \"Core state restoration failed: GIC target state unknown\"}}; \
             if {{$__dt_gic_code eq [list OpenOCD -300]}} {{error \"GIC reader unsupported: $__dt_gic_result\"}}; error $__dt_gic_result}}; set __dt_gic_result",
            gic::PROTOCOL,
            self.project.registers.gic_command,
            crate::live_watch::word(name)
        );
        let text = self
            .physical_adapter_read(&operation, &format!("GIC read {name}"))
            .map_err(|(reason, error)| {
                let reason = if error.contains("debugtui-gic:not-implemented") {
                    Reason::HardwareNotImplemented
                } else if error.contains("debugtui-gic:access-restricted") {
                    Reason::AccessRestricted
                } else if error.contains("debugtui-gic:access-unknown") {
                    Reason::Unknown
                } else if error.contains("GIC adapter protocol unsupported")
                    || error.contains("GIC reader unsupported")
                {
                    Reason::ReaderUnsupported
                } else {
                    reason
                };
                (reason, error)
            })?;
        let response = gic::Response::parse(&text, name, bits)
            .map_err(|error| (Reason::ReaderUnsupported, error))?;
        if let Some(access) = &mut self.register_value_access {
            access.gic = Some(response.evidence);
        }
        Ok(response.value)
    }
}
