//! Native bounded R52 read; proof and value belong to the same transaction.
use super::*;
use crate::registers::{RawValue, Reason, r52_core};

impl Engine {
    pub(super) fn read_r52_core_register(
        &mut self,
        request: &r52_core::Request,
    ) -> Result<RawValue, (Reason, String)> {
        let operation = format!(
            "if {{[catch {{aarch64 debugtui_r52_protocol}} __dt_r52_protocol] || $__dt_r52_protocol ne \"{}\"}} {{error \"R52 adapter protocol unsupported\"}}; \
             if {{[catch {{{} {}}} __dt_r52_result]}} {{set __dt_r52_code $::errorCode; \
             if {{[[target current] curstate] ne \"halted\"}} {{error \"Core state restoration failed: R52 target state unknown\"}}; \
             if {{$__dt_r52_code eq [list OpenOCD -300]}} {{error \"R52 reader unsupported: $__dt_r52_result\"}}; error $__dt_r52_result}}; set __dt_r52_result",
            r52_core::PROTOCOL,
            r52_core::COMMAND,
            crate::live_watch::word(&request.name)
        );
        let text = self
            .physical_adapter_read(&operation, &format!("R52 read {}", request.name))
            .map_err(|(reason, error)| {
                let reason = if error.contains("debugtui-r52:access-restricted") {
                    Reason::AccessRestricted
                } else if error.contains("debugtui-r52:access-unknown") {
                    Reason::Unknown
                } else if error.contains("debugtui-r52:not-implemented") {
                    Reason::HardwareNotImplemented
                } else if error.contains("R52 adapter protocol unsupported")
                    || error.contains("R52 reader unsupported")
                {
                    Reason::ReaderUnsupported
                } else {
                    reason
                };
                (reason, error)
            })?;
        let response = r52_core::Response::parse(&text, request)
            .map_err(|error| (Reason::ReaderUnsupported, error))?;
        if let Some(access) = &mut self.register_value_access {
            access.r52_core = Some(response.evidence);
        }
        Ok(response.value)
    }
}
