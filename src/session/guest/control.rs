//! Explicit core-local HVC return. A real stop always wins; nothing auto-resumes
//! past a user breakpoint, signal or cancellation to reach an expected image.
use super::*;

pub(in crate::session) struct ReturnOperation {
    id: String,
    pc: u64,
    image: String,
    hit: bool,
    failure: Option<String>,
    running_route: Option<running::Route>,
    cancelled: bool,
    cancellation: Option<Json>,
}

mod running;

#[cfg(test)]
mod tests;

impl Engine {
    pub(in crate::session) fn begin_guest_return(&mut self, p: &Json) -> Result<Json, String> {
        if self.guest_return.is_some() {
            return Err("A Guest return is already pending; cancel it first".into());
        }
        let view = self.guest_context(&json!({"image":p["image"]}))?;
        if view["saved"]["edge"].as_u64() != Some(1)
            || view["saved"]["syndrome"]
                .as_u64()
                .is_none_or(|n| n >> 26 != 0x12)
        {
            return Err("Return Guest currently requires a fully saved HVC32 edge".into());
        }
        let pc = view["saved"]["registers"][15]
            .as_u64()
            .ok_or("No saved return PC")?;
        let image = view["image"].as_str().unwrap_or_default().to_string();
        let running_route = self.prepare_running_return_cancel()?;
        let reply = self.mi(&format!(
            "-break-insert -h -t {}",
            mi::quote(&format!("*0x{pc:x}"))
        ))?;
        let id = reply
            .data
            .field("bkpt")
            .map(|b| b.string("number"))
            .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
            .ok_or("GDB returned no owned return breakpoint")?;
        self.guest_return = Some(ReturnOperation {
            id: id.clone(),
            pc,
            image: image.clone(),
            hit: false,
            failure: None,
            running_route,
            cancelled: false,
            cancellation: None,
        });
        self.set_guest_return_status(
            "running",
            &image,
            pc,
            "Core-local HVC return; any other stop interrupts it",
        );
        if let Err(error) = self.execute(method::CONTINUE, &json!({})) {
            let cleanup = self.cancel_guest_return();
            return Err(format!(
                "Guest return could not continue: {error}; cleanup: {cleanup:?}"
            ));
        }
        Ok(
            json!({"running":self.snapshot.state==state::RUNNING,"image":image,"return_pc":format!("0x{pc:x}"),"owned_temporary_breakpoint":id,"scope":"selected_core"}),
        )
    }
    pub(in crate::session) fn guest_return_stop(&mut self, record: &Record) {
        if let Some(operation) = &mut self.guest_return {
            operation.hit = record.data.string("reason") == "breakpoint-hit"
                && record.data.string("bkptno") == operation.id;
        }
    }
    pub(in crate::session) fn note_guest_return_failure(&mut self, error: &str) {
        let Some(operation) = self.guest_return.as_mut().filter(|o| o.failure.is_none()) else {
            return;
        };
        operation.failure = Some(error.into());
    }
    fn remove_guest_return_breakpoint(&mut self, id: &str) -> Result<(), String> {
        let list = self.mi("-break-list")?;
        let exists = list
            .data
            .field("BreakpointTable")
            .and_then(|v| v.field("body"))
            .map(Value::items)
            .unwrap_or_default()
            .iter()
            .any(|v| v.field("bkpt").unwrap_or(v).string("number") == id);
        if exists {
            self.mi(&format!("-break-delete {id}"))?;
        }
        Ok(())
    }
    pub(in crate::session) fn finish_guest_return(&mut self) -> Result<(), String> {
        let Some(operation) = self.guest_return.take() else {
            return Ok(());
        };
        if let Err(error) = self.remove_guest_return_breakpoint(&operation.id) {
            self.set_guest_return_status("cleanup_failed", &operation.image, operation.pc, &error);
            self.guest_return = Some(operation);
            return Err(error);
        }
        let reached = literal_address(&self.snapshot.frame.address).ok() == Some(operation.pc)
            && operation.hit;
        self.set_guest_return_status(
            if operation.cancelled {
                "cancelled"
            } else if operation.failure.is_some() {
                "failed"
            } else if reached {
                "completed"
            } else {
                "interrupted"
            },
            &operation.image,
            operation.pc,
            &operation
                .failure
                .unwrap_or_else(|| self.snapshot.stop_reason.clone()),
        );
        if let Some(proof) = operation.cancellation {
            self.set_return_cancellation(proof, false);
        }
        Ok(())
    }
    pub(in crate::session) fn cancel_guest_return(&mut self) -> Result<Json, String> {
        self.drain_memory_notices();
        if let Some(operation) = &self.guest_return {
            if operation.cancelled {
                return Ok(
                    json!({"cancelled":true,"already_cancelled":true,"logical_cleanup_pending":true,"target_control":false}),
                );
            }
            if self.snapshot.state == state::RUNNING
                && let Some(route) = operation.running_route.clone()
            {
                return self.cancel_running_guest_return(route);
            }
        }
        let Some(operation) = self.guest_return.take() else {
            return Ok(json!({"cancelled":false,"reason":"No pending Guest return"}));
        };
        if let Err(error) = self.remove_guest_return_breakpoint(&operation.id) {
            self.guest_return = Some(operation);
            return Err(error);
        }
        self.set_guest_return_status(
            "cancelled",
            &operation.image,
            operation.pc,
            "Only the owned temporary breakpoint was removed; no halt/resume was issued",
        );
        self.publish();
        Ok(json!({"cancelled":true,"target_control":false}))
    }
    fn cancel_running_guest_return(&mut self, route: running::Route) -> Result<Json, String> {
        let operation = self
            .guest_return
            .as_ref()
            .ok_or("No pending Guest return")?;
        let (id, pc, image) = (operation.id.clone(), operation.pc, operation.image.clone());
        let before = self.mi("-break-list")?;
        if self.snapshot.state == state::STOPPED {
            self.finish_guest_return()?;
            return Ok(json!({"cancelled":false,"reason":"Target already stopped"}));
        }
        let shared = running::ownership(&before, &id, pc, false)?;
        if self.running_return_backend(&route, pc, shared, false)? == "halted" {
            self.reconcile_stop(Duration::from_millis(self.project.session.timeout_ms))?;
            return Ok(
                json!({"cancelled":false,"reason":"Target already stopped; actual stop takes priority"}),
            );
        }
        let disabled = self.mi(&format!("-break-disable {id}"));
        let after = self.mi("-break-list")?;
        if self.snapshot.state == state::STOPPED {
            self.finish_guest_return()?;
            return Ok(json!({"cancelled":false,"reason":"Target stopped during cancellation"}));
        }
        if let Err(error) = disabled {
            if !error.contains("Cannot execute this command while the target is running") {
                return Err(error);
            }
            self.log("diagnostic", "GDB deferred return breakpoint uninsert; verifying logical disable and completing through OpenOCD.");
        }
        if running::ownership(&after, &id, pc, true)? != shared {
            return Err("User breakpoint ownership changed during cancellation".into());
        }
        let removed = self.running_return_backend(&route, pc, shared, true)?;
        if removed == "halted" {
            self.reconcile_stop(Duration::from_millis(self.project.session.timeout_ms))?;
            return Ok(
                json!({"cancelled":false,"reason":"A real target stop interrupted cancellation"}),
            );
        }
        if removed != if shared { "shared" } else { "removed" } {
            return Err("Unexpected running cancellation proof".into());
        }
        let proof = json!({"source":"openocd_running_breakpoint_control","core":route.core,"target":route.target,
            "shared_user_site":shared,"owned_physical_site_removed":!shared,"gdb_logical_disabled":true,"target_selection_restored":true});
        let operation = self
            .guest_return
            .as_mut()
            .ok_or("Return operation changed during cancellation")?;
        operation.cancelled = true;
        operation.cancellation = Some(proof.clone());
        self.set_guest_return_status("cancelled", &image, pc, "Return cancelled without halt/resume; disabled GDB bookkeeping is deleted at the next real stop");
        self.set_return_cancellation(proof.clone(), true);
        self.publish();
        Ok(
            json!({"cancelled":true,"logical_cleanup_pending":true,"target_control":false,"backend":proof}),
        )
    }
    fn set_return_cancellation(&mut self, proof: Json, pending: bool) {
        if let Some(value) = self
            .snapshot
            .image_state
            .as_mut()
            .and_then(|s| s.return_guest.as_mut())
        {
            value["cancellation"] = proof;
            value["logical_cleanup_pending"] = json!(pending);
        }
    }
    fn set_guest_return_status(&mut self, state: &str, image: &str, pc: u64, detail: &str) {
        if let Some(images) = &mut self.snapshot.image_state {
            images.return_guest = Some(
                json!({"state":state,"image":image,"return_pc":format!("0x{pc:x}"),"detail":detail}),
            );
        }
    }
}
