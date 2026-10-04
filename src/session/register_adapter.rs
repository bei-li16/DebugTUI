//! Leases and physical context guards shared by explicitly verified adapters.
use super::*;
use crate::registers::Reason;

impl Engine {
    pub(super) fn physical_adapter_read(
        &mut self,
        operation: &str,
        label: &str,
    ) -> Result<String, (Reason, String)> {
        let context = self.register_context();
        if context.frame != 0 {
            return Err((
                Reason::AccessRestricted,
                format!("{label} requires current physical frame 0"),
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
                "Actual GDB frame is not current physical frame 0".into(),
            ));
        }
        let response = self.register_tcl(operation);
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
                format!("Physical context changed during {label}; sample discarded"),
            ));
        }
        response
    }
}
