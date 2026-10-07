//! Server-owned drafts. Preview performs capability queries, never assignments.
use super::*;
use crate::{
    registers::{Context, Implementation, Scope, Writer},
    writes::{Input, OnceState, Outcome, Prepared, Selection},
};
use std::{collections::BTreeMap, path::PathBuf, sync::atomic::AtomicU64};

pub(super) const MAX_DRAFTS: usize = 128;
pub(super) const DRAFT_LIFETIME: Duration = Duration::from_secs(600);
pub(super) static NEXT_DRAFT: AtomicU64 = AtomicU64::new(1);

#[derive(Default)]
pub(super) struct Drafts(
    BTreeMap<String, Draft>,
    pub(super) BTreeMap<String, super::memory_writes::Draft>,
    pub(super) BTreeMap<String, super::variable_writes::Draft>,
    pub(super) BTreeMap<String, super::vfp_writes::Draft>,
);
impl Drafts {
    pub(super) fn clear(&mut self) {
        self.0.clear();
        self.1.clear();
        self.2.clear();
        self.3.clear();
    }
    pub(super) fn len(&self) -> usize {
        self.0.len() + self.1.len() + self.2.len() + self.3.len()
    }
}
struct Draft {
    context: Context,
    thread: String,
    elf: PathBuf,
    created: Instant,
    register: String,
    gdb_name: String,
    gdb_index: usize,
    plan: Prepared,
}

impl Engine {
    pub(super) fn write_permission(&mut self) -> Result<(), String> {
        let permission = self.mi("-gdb-show may-write-registers")?;
        if permission.data.string("value") != "on" {
            return Err("GDB register writes are disabled (may-write-registers)".into());
        }
        Ok(())
    }
    pub(super) fn write_thread(&mut self) -> Result<String, String> {
        self.stopped()?;
        let response = self.mi("-thread-info")?;
        let threads = response
            .data
            .field("threads")
            .ok_or("GDB omitted thread states")?
            .items();
        let selected = response.data.string("current-thread-id");
        if selected.is_empty()
            || !threads.iter().any(|t| t.string("id") == selected)
            || threads.iter().any(|t| t.string("state") != "stopped")
        {
            return Err(
                "Requires a selected GDB thread and all threads in this core connection stopped"
                    .into(),
            );
        }
        self.stopped()?;
        Ok(selected)
    }
    pub(super) fn preview_write(&mut self, p: &Json) -> Result<Json, String> {
        if let Some(error) = &self.register_access_fault {
            return Err(format!(
                "Debug access is faulted; reconnect before another write: {error}"
            ));
        }
        if p["target"]["kind"] == "variable" {
            return self.preview_variable_write(p);
        }
        if matches!(p["target"]["kind"].as_str(), Some("memory" | "peripheral")) {
            return self.preview_memory_write(p);
        }
        self.stopped()?;
        if self.cancellation.load(Ordering::Relaxed) {
            return Err("Write preview cancelled".into());
        }
        let expected: Context = serde_json::from_value(p["context"].clone())
            .map_err(|e| format!("Write context is required: {e}"))?;
        if expected != self.register_context() {
            return Err("Write context has expired".into());
        }
        if expected.frame != 0 {
            return Err("Select physical frame 0 before editing a core register".into());
        }
        if p["target"]["kind"].as_str() != Some("register") {
            return Err("Writer for this object kind is not adapted yet".into());
        }
        let id = p["target"]["id"].as_str().ok_or("Register ID required")?;
        let catalogue = self
            .register_catalogue
            .as_ref()
            .map_err(Clone::clone)?
            .as_ref()
            .ok_or("Select a register catalogue first")?
            .0
            .clone();
        let register = catalogue.register(id).ok_or("Unknown register ID")?;
        if register.scope != Scope::Core {
            return Err("A GDB register writer must belong to one physical core".into());
        }
        if matches!(register.writer, Some(Writer::Vfp { .. })) {
            return self.preview_vfp_write(p, expected, register);
        }
        let (implementation, evidence) = register.implementation(&self.effective_register_facts());
        if implementation == Implementation::No
            || (implementation == Implementation::Unknown && !register.conditions.is_empty())
        {
            return Err(format!(
                "Register implementation is not confirmed: {evidence}"
            ));
        }
        let (Some(Writer::GdbInteger { name }), Some(spec)) = (&register.writer, &register.write)
        else {
            return Err(
                "No independent writer and write semantics are declared for this register".into(),
            );
        };
        let selection: Selection = serde_json::from_value(p["selection"].clone())
            .map_err(|e| format!("Write selection: {e}"))?;
        let input: Input =
            serde_json::from_value(p["input"].clone()).map_err(|e| format!("Write input: {e}"))?;
        let plan = spec.prepare(selection, &input, OnceState::Unknown)?;
        self.write_drafts.0.retain(|_, draft| {
            draft.created.elapsed() < DRAFT_LIFETIME && draft.context == expected
        });
        if self.write_drafts.len() >= MAX_DRAFTS {
            return Err("Too many outstanding write drafts; cancel unused drafts".into());
        }
        let services = crate::debug_access::for_project(&self.project)?;
        let _leases = services
            .iter()
            .map(|s| s.acquire(false))
            .collect::<Result<Vec<_>, _>>()?;
        let thread = self.write_thread()?;
        self.write_permission()?;
        let command = self.mi("-info-gdb-mi-command data-write-register-values")?;
        if command
            .data
            .field("command")
            .is_none_or(|c| c.string("exists") != "true")
        {
            return Err("This GDB does not implement the register MI writer".into());
        }
        // Keep sparse names and indices exactly as reported by this connection.
        let names = self.mi("-data-list-register-names")?;
        self.reg_names = names
            .data
            .field("register-names")
            .ok_or("GDB omitted register names")?
            .items()
            .iter()
            .map(|v| v.text().to_owned())
            .collect();
        let indices: Vec<_> = self
            .reg_names
            .iter()
            .enumerate()
            .filter(|(_, n)| *n == name)
            .map(|(i, _)| i)
            .collect();
        if indices.len() != 1 {
            return Err("GDB writer register name is missing or ambiguous".into());
        }
        if self.register_context() != expected || self.snapshot.state != state::STOPPED {
            return Err("Write context changed during preview".into());
        }
        let token = format!(
            "{:x}-{:x}",
            expected.session,
            NEXT_DRAFT.fetch_add(1, Ordering::Relaxed)
        );
        let warning = match name.as_str() {
            "pc" | "r15" => {
                "Changing PC changes the next instruction and invalidates source, stack and assembly views."
            }
            "sp" | "r13" => "Changing SP invalidates the stack and local-variable views.",
            _ => "Write applies to this physical core only, including when control scope is All.",
        };
        let result = json!({"draft":token,"target":{"kind":"register","id":id},"owner":expected.core,
            "scope":"core","thread":thread,"context":expected,"elf":self.project.program.elf,
            "channel":"gdb","endpoint":self.project.target.endpoint,"writer":name,"plan":plan,
            "warning":warning,"expires_in_ms":DRAFT_LIFETIME.as_millis(),"outcome":Outcome::NotSent});
        self.write_drafts.0.insert(
            token,
            Draft {
                context: expected,
                thread,
                elf: self.project.program.elf.clone(),
                created: Instant::now(),
                register: id.into(),
                gdb_name: name.clone(),
                gdb_index: indices[0],
                plan,
            },
        );
        Ok(result)
    }
    pub(super) fn cancel_write(&mut self, p: &Json) -> Result<Json, String> {
        let token = p["draft"].as_str().ok_or("Write draft ID required")?;
        let removed = self.write_drafts.0.remove(token).is_some()
            | self.write_drafts.1.remove(token).is_some()
            | self.write_drafts.2.remove(token).is_some()
            | self.write_drafts.3.remove(token).is_some();
        Ok(
            json!({"draft":token,"outcome":if removed {json!(Outcome::NotSent)} else {Json::Null},"cancelled":removed,
            "detail":if removed {"Draft cancelled before sending"} else {"No pending draft; a sent write cannot be withdrawn"}}),
        )
    }
    fn validate_write_draft(&mut self, draft: &Draft) -> Result<(), String> {
        self.stopped()?;
        if self.cancellation.load(Ordering::Relaxed) {
            return Err("Write cancelled before sending".into());
        }
        if draft.created.elapsed() >= DRAFT_LIFETIME
            || draft.context != self.register_context()
            || draft.elf != self.project.program.elf
        {
            return Err(
                "Write draft belongs to an expired core, frame, stop, session or ELF".into(),
            );
        }
        let thread = self.write_thread()?;
        self.write_permission()?;
        if thread != draft.thread || self.register_context() != draft.context {
            return Err("Write thread or context changed".into());
        }
        Ok(())
    }
    pub(super) fn apply_write(&mut self, p: &Json) -> Result<Json, String> {
        let token = p["draft"].as_str().ok_or("Write draft ID required")?;
        if self.write_drafts.3.contains_key(token) {
            return self.apply_vfp_write(token);
        }
        if self.write_drafts.2.contains_key(token) {
            return self.apply_variable_write(token);
        }
        if self.write_drafts.1.contains_key(token) {
            return self.apply_memory_write(token);
        }
        let Some(draft) = self.write_drafts.0.remove(token) else {
            return Ok(
                json!({"draft":token,"outcome":Outcome::NotSent,"error":"No pending write draft; preview again","code":"draft_expired"}),
            );
        };
        let mut result = json!({"draft":token,"target":{"kind":"register","id":draft.register},
            "owner":draft.context.core,"scope":"core","thread":draft.thread,"context":draft.context,
            "channel":"gdb","endpoint":self.project.target.endpoint,"writer":draft.gdb_name,
            "selected_mask":draft.plan.selected_mask,"atomic":false,"outcome":Outcome::NotSent});
        let services = match crate::debug_access::for_project(&self.project) {
            Ok(s) => s,
            Err(error) => {
                result["error"] = json!(error);
                result["code"] = json!("service_unavailable");
                return Ok(result);
            }
        };
        let mut leases = match services
            .iter()
            .map(|s| s.acquire(false))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(l) => l,
            Err(error) => {
                result["error"] = json!(error);
                result["code"] = json!("service_unavailable");
                return Ok(result);
            }
        };
        let resolved = (|| {
            self.validate_write_draft(&draft)?;
            let fresh = if draft.plan.needs_fresh_read {
                Some(
                    self.gdb_register_value(&draft.gdb_name, draft.plan.selected_mask.bits)
                        .map_err(|(_, e)| e)?,
                )
            } else {
                None
            };
            if self.register_context() != draft.context || self.snapshot.state != state::STOPPED {
                return Err("Write context changed during the fresh read".into());
            }
            let resolved = draft.plan.resolve(fresh.as_ref())?;
            if self.cancellation.load(Ordering::Relaxed) {
                return Err("Write cancelled before sending".into());
            }
            Ok::<_, String>(resolved)
        })();
        let resolved = match resolved {
            Ok(r) => r,
            Err(error) => {
                result["error"] = json!(error);
                result["code"] = json!("precondition_failed");
                return Ok(result);
            }
        };
        result["command_value"] = json!(resolved.command);
        result["verification_mask"] = json!(resolved.verification_mask);
        // Only an index and canonical hexadecimal value enter this expression-taking MI command.
        // No user expression, enumeration name, sign, quote or newline reaches GDB.
        let command = format!(
            "-data-write-register-values x {} {}",
            draft.gdb_index, resolved.command.hex
        );
        let outcome = match self.mi(&command) {
            Err(error) => {
                // An error can follow a hardware write (including remote E replies).
                for lease in &mut leases {
                    lease.quarantine(&error);
                }
                self.register_access_fault = Some(error.clone());
                self.state(state::FAULT);
                result["error"] = json!(error);
                result["code"] = json!("write_result_unknown");
                Outcome::Unknown
            }
            Ok(_)
                if self.register_context() != draft.context
                    || self.snapshot.state != state::STOPPED =>
            {
                result["error"] =
                    json!("Write was accepted, but target context changed before verification");
                result["code"] = json!("verification_context_changed");
                Outcome::Accepted
            }
            Ok(_) if resolved.verification_mask.integer()? == 0 => Outcome::Accepted,
            Ok(_) => match self.gdb_register_value(&draft.gdb_name, resolved.command.bits) {
                Ok(value)
                    if self.register_context() == draft.context
                        && self.snapshot.state == state::STOPPED =>
                {
                    result["observed"] = json!(value);
                    if resolved.matches(&value)? {
                        Outcome::Verified
                    } else {
                        Outcome::Mismatch
                    }
                }
                Ok(_) => {
                    result["error"] = json!("Target context changed during verification");
                    Outcome::Accepted
                }
                Err((_, error)) => {
                    result["error"] = json!(error);
                    result["code"] = json!("verification_unavailable");
                    Outcome::Accepted
                }
            },
        };
        result["outcome"] = json!(outcome);
        self.finish_write(&mut result);
        Ok(result)
    }
    pub(super) fn finish_write(&mut self, result: &mut Json) {
        self.invalidate_written_views();
        result["context_after"] = json!(self.register_context());
        self.log("write", serde_json::to_string(&result).unwrap_or_default());
    }
    pub(super) fn invalidate_written_views(&mut self) {
        // Even an unknown result may have changed storage. Invalidate every overlapping view
        // rather than publishing a guessed command value as a fresh target sample.
        self.write_drafts.clear();
        self.snapshot.register_probe = None;
        self.snapshot.generation += 1;
        for sample in &mut self.snapshot.register_samples {
            sample.stale();
        }
        for variable in self
            .snapshot
            .registers
            .iter_mut()
            .chain(&mut self.snapshot.locals)
            .chain(&mut self.snapshot.watches)
        {
            variable.error = true;
            variable.value = "<stale after write>".into();
        }
        self.snapshot.memory.clear();
        self.snapshot.stack.clear();
        self.snapshot.assembly.clear();
        self.refresh_pending = self.snapshot.state == state::STOPPED;
        self.publish();
    }
}
