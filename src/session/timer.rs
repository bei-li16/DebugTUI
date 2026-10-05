//! Explicit Timer adapter; never fall back after selecting a checked protocol.
use super::*;
use crate::registers::{RawValue, Reason, timer};

impl Engine {
    pub(super) fn read_timer_register(
        &mut self,
        name: &str,
        bits: u16,
    ) -> Result<RawValue, (Reason, String)> {
        let operation = format!(
            "if {{[catch {{aarch64 debugtui_timer_protocol}} __dt_timer_protocol] || $__dt_timer_protocol ne \"{}\"}} {{error \"Timer adapter protocol unsupported\"}}; \
             if {{[catch {{{} {}}} __dt_timer_result]}} {{set __dt_timer_code $::errorCode; \
             if {{[[target current] curstate] ne \"halted\"}} {{error \"Core state restoration failed: Timer target state unknown\"}}; \
             if {{$__dt_timer_code eq [list OpenOCD -300]}} {{error \"Timer reader unsupported: $__dt_timer_result\"}}; error $__dt_timer_result}}; set __dt_timer_result",
            timer::PROTOCOL,
            self.project.registers.timer_command,
            crate::live_watch::word(name)
        );
        let text = self
            .physical_adapter_read(&operation, &format!("Timer read {name}"))
            .map_err(|(reason, error)| {
                let reason = if error.contains("debugtui-timer:access-restricted") {
                    Reason::AccessRestricted
                } else if error.contains("debugtui-timer:access-unknown") {
                    Reason::Unknown
                } else if error.contains("Timer adapter protocol unsupported")
                    || error.contains("Timer reader unsupported")
                {
                    Reason::ReaderUnsupported
                } else {
                    reason
                };
                (reason, error)
            })?;
        let response = timer::Response::parse(&text, name, bits)
            .map_err(|error| (Reason::ReaderUnsupported, error))?;
        if let Some(access) = &mut self.register_value_access {
            access.timer = Some(response.evidence);
        }
        Ok(response.value)
    }
}
