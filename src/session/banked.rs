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
        let context = self.register_context();
        if context.frame != 0 {
            return Err((
                Reason::AccessRestricted,
                "Banked registers require the current physical frame 0".into(),
            ));
        }
        let services = crate::debug_access::for_project(&self.project)
            .map_err(|error| (Reason::TransportError, error))?;
        let mut leases = services
            .iter()
            .map(|service| service.acquire(false))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| (Reason::TransportError, error))?;
        let thread = self
            .write_thread()
            .map_err(|error| (Reason::AccessRestricted, error))?;
        let frame = self
            .mi("-stack-info-frame")
            .map_err(|error| (Reason::TransportError, error))?;
        if frame
            .data
            .field("frame")
            .is_none_or(|frame| frame.string("level") != "0")
        {
            return Err((
                Reason::AccessRestricted,
                "Actual GDB frame is not the current physical frame 0".into(),
            ));
        }
        let operation = format!(
            "if {{[catch {{aarch64 debugtui_banked_protocol}} __dt_bank_protocol] || $__dt_bank_protocol ne \"{}\"}} {{error \"Banked adapter protocol unsupported\"}}; \
             if {{[catch {{{} {}}} __dt_bank_result]}} {{set __dt_bank_code $::errorCode; \
             if {{[[target current] curstate] ne \"halted\"}} {{error \"Core state restoration failed: banked access left the target state unknown\"}}; \
             if {{$__dt_bank_code eq [list OpenOCD -308] || $__dt_bank_code eq [list OpenOCD -304]}} {{error \"Banked access unavailable: current physical mode/state does not permit this bank\"}}; \
             if {{$__dt_bank_code eq [list OpenOCD -300]}} {{error \"Banked access unsupported: unadapted physical CPU identity/state\"}}; error $__dt_bank_result}}; set __dt_bank_result",
            banked::PROTOCOL,
            self.project.registers.banked_command,
            crate::live_watch::word(name),
        );
        let response = self.register_tcl(&operation);
        if let Some(error) = self.register_access_fault.clone() {
            for lease in &mut leases {
                lease.quarantine(&error);
            }
            self.snapshot.register_probe = None;
            self.state("FAULT");
            return Err((Reason::TransportError, error));
        }
        let final_thread = self.write_thread().map_err(|error| {
            self.snapshot.register_probe = None;
            (Reason::AccessRestricted, error)
        })?;
        let final_frame = self.mi("-stack-info-frame").map_err(|error| {
            self.snapshot.register_probe = None;
            (Reason::TransportError, error)
        })?;
        if thread != final_thread
            || self.register_context() != context
            || final_frame
                .data
                .field("frame")
                .is_none_or(|frame| frame.string("level") != "0")
        {
            self.snapshot.register_probe = None;
            return Err((
                Reason::AccessRestricted,
                "Physical context changed during banked read; sample discarded".into(),
            ));
        }
        let text = response.map_err(|(reason, error)| {
            if error.contains("Banked access unsupported") {
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
        let text = text.trim();
        if text.len() != 10
            || !text.starts_with("0x")
            || !text[2..].bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err((
                Reason::ReaderUnsupported,
                "Banked adapter must return exactly 8 hexadecimal digits".into(),
            ));
        }
        RawValue::parse(text, 32).map_err(|error| (Reason::TransportError, error))
    }
}
