//! Physical bank access never uses unwound frames or the legacy get_reg path.
use super::*;
use crate::registers::{RawValue, Reason, banked};

impl Engine {
    pub(super) fn read_banked_register(
        &mut self,
        name: &str,
    ) -> Result<RawValue, (Reason, String)> {
        if self.project.registers.banked_command.is_empty() {
            return Err((
                Reason::ReaderUnsupported,
                "Configure registers.banked_command for the verified no-mode-change adapter".into(),
            ));
        }
        let operation = format!(
            "if {{[catch {{aarch64 debugtui_banked_protocol}} __dt_bank_protocol] || $__dt_bank_protocol ne \"{}\"}} {{error \"Banked adapter protocol unsupported\"}}; \
             if {{[catch {{{} {}}} __dt_bank_result]}} {{set __dt_bank_code $::errorCode; \
             if {{[[target current] curstate] ne \"halted\"}} {{error \"Core state restoration failed: banked access left the target state unknown\"}}; \
             if {{[string match \"*debugtui-banked:access-unknown*\" $__dt_bank_result]}} {{error $__dt_bank_result}}; \
             if {{$__dt_bank_code eq [list OpenOCD -308] || $__dt_bank_code eq [list OpenOCD -304]}} {{error \"Banked access unavailable: current physical mode/state does not permit this bank\"}}; \
             if {{$__dt_bank_code eq [list OpenOCD -300]}} {{error \"Banked access unsupported: unadapted physical CPU identity/state\"}}; error $__dt_bank_result}}; set __dt_bank_result",
            banked::PROTOCOL,
            self.project.registers.banked_command,
            crate::live_watch::word(name),
        );
        let response = self.physical_adapter_read(&operation, &format!("banked read {name}"));
        let text = response.map_err(|(reason, error)| {
            if error.contains("debugtui-banked:access-unknown") {
                (Reason::Unknown, error)
            } else if error.contains("Banked access unsupported") {
                self.snapshot.register_probe = None;
                (Reason::ReaderUnsupported, error)
            } else if error.contains("Banked adapter protocol unsupported") {
                (Reason::ReaderUnsupported, error)
            } else if error.contains("Banked access unavailable") {
                self.snapshot.register_probe = None;
                (Reason::AccessRestricted, error)
            } else {
                (reason, error)
            }
        })?;
        let response = banked::Response::parse(&text, name)
            .map_err(|error| (Reason::ReaderUnsupported, error))?;
        if let Some(access) = &mut self.register_value_access {
            access.banked = Some(response.evidence);
        }
        Ok(response.value)
    }
}
