//! Some remote GDBs acknowledge resume before hardware insertion fails, then
//! omit *stopped. Reconcile their actual thread state without interrupting them.
use super::*;

impl Engine {
    pub(super) fn execution_diagnostic(&mut self, text: &str) {
        if self.snapshot.state != state::RUNNING || self.execution_failure.is_some() {
            return;
        }
        let text = text.trim();
        if !text.starts_with("Cannot insert hardware breakpoint ")
            && !text.starts_with("Could not insert hardware breakpoints:")
            && !text
                .starts_with("You may have requested too many hardware breakpoints/watchpoints.")
        {
            return;
        }
        let error = format!("Hardware breakpoint insertion failed: {text}");
        self.note_guest_return_failure(&error);
        self.execution_failure = Some(error);
    }

    pub(super) fn reconcile_execution_failure(&mut self) -> Result<(), String> {
        let Some(error) = self.execution_failure.take() else {
            return Ok(());
        };
        self.log("error", error);
        // Only the event loop calls this, outside nested MI record handling.
        // Diagnostic text alone never establishes a physical stopped state.
        if self.snapshot.state == state::RUNNING
            && !self.reconcile_stop(Duration::from_millis(self.project.session.timeout_ms))?
        {
            self.log("error", "GDB still reports running threads after the insertion failure; the target was not interrupted.");
        }
        if self.snapshot.state == state::STOPPED
            && self.snapshot.stop_reason == "state-synchronized"
        {
            self.snapshot.stop_reason = "hardware-breakpoint-insertion-failed".into();
        }
        Ok(())
    }
}
