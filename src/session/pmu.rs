//! Explicit PMU adapter; never fall back after selecting a checked protocol.
use super::*;
use crate::registers::{RawValue, Reason, pmu};

impl Engine {
    pub(super) fn read_pmu_register(
        &mut self,
        name: &str,
        bits: u16,
    ) -> Result<RawValue, (Reason, String)> {
        let operation = format!(
            "if {{[catch {{aarch64 debugtui_pmu_protocol}} __dt_pmu_protocol] || $__dt_pmu_protocol ne \"{}\"}} {{error \"PMU adapter protocol unsupported\"}}; \
             if {{[catch {{{} {}}} __dt_pmu_result]}} {{set __dt_pmu_code $::errorCode; \
             if {{[[target current] curstate] ne \"halted\"}} {{error \"Core state restoration failed: PMU target state unknown\"}}; \
             if {{$__dt_pmu_code eq [list OpenOCD -300]}} {{error \"PMU reader unsupported: $__dt_pmu_result\"}}; error $__dt_pmu_result}}; set __dt_pmu_result",
            pmu::PROTOCOL,
            self.project.registers.pmu_command,
            crate::live_watch::word(name)
        );
        let text = self
            .physical_adapter_read(&operation, &format!("PMU read {name}"))
            .map_err(|(reason, error)| {
                let reason = if error.contains("debugtui-pmu:access-restricted") {
                    Reason::AccessRestricted
                } else if error.contains("debugtui-pmu:access-unknown") {
                    Reason::Unknown
                } else if error.contains("PMU adapter protocol unsupported")
                    || error.contains("PMU reader unsupported")
                {
                    Reason::ReaderUnsupported
                } else {
                    reason
                };
                (reason, error)
            })?;
        let response = pmu::Response::parse(&text, name, bits)
            .map_err(|error| (Reason::ReaderUnsupported, error))?;
        if let Some(access) = &mut self.register_value_access {
            access.pmu = Some(response.evidence);
        }
        Ok(response.value)
    }
}
