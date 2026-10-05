//! Raw VFP drafts use an independent, explicitly enabled physical writer.
use super::writes::{DRAFT_LIFETIME, MAX_DRAFTS, NEXT_DRAFT};
use super::*;
use crate::{
    registers::{
        Context, Implementation, RawValue, Register, Writer,
        vfp::{self, WriteResponse, WriteView},
    },
    writes::{Input, OnceState, Outcome, Prepared, Selection},
};
use std::path::PathBuf;

#[derive(Clone, PartialEq, Eq)]
struct Route {
    endpoint: String,
    target: String,
    reader: String,
    writer: String,
}
pub(super) struct Draft {
    context: Context,
    thread: String,
    elf: PathBuf,
    created: Instant,
    register: Register,
    view: WriteView,
    route: Route,
    plan: Prepared,
}
impl Engine {
    fn vfp_write_route(&self) -> Result<Route, String> {
        let config = &self.project.registers;
        if config.vfp_write_command != "aarch64 vfp_write" || config.vfp_command != "aarch64 vfp" {
            return Err("Configure registers.vfp_write_command = aarch64 vfp_write independently of the reader".into());
        }
        if config.tcl_endpoint.is_empty() {
            return Err("VFP writer requires an explicit TCL endpoint".into());
        }
        Ok(Route {
            endpoint: config.tcl_endpoint.clone(),
            target: config
                .targets
                .get(&self.register_context().core)
                .ok_or("Declare this physical core's VFP target")?
                .clone(),
            reader: config.vfp_command.clone(),
            writer: config.vfp_write_command.clone(),
        })
    }
    fn vfp_write_context(&mut self, expected: &Context) -> Result<String, String> {
        self.stopped()?;
        if self.cancellation.load(Ordering::Relaxed) {
            return Err("VFP write cancelled before sending".into());
        }
        if self.register_access_fault.is_some() {
            return Err("Debug access is faulted; reconnect before writing".into());
        }
        if expected != &self.register_context() || expected.frame != 0 {
            return Err("VFP write requires the current physical context at frame 0".into());
        }
        self.write_permission()?;
        let thread = self.write_thread()?;
        let frame = self.mi("-stack-info-frame")?;
        if frame
            .data
            .field("frame")
            .is_none_or(|frame| frame.string("level") != "0")
        {
            return Err("Actual GDB frame is not physical frame 0".into());
        }
        Ok(thread)
    }
    fn vfp_write_pair(&mut self, register: &Register, view: WriteView) -> Result<RawValue, String> {
        let mut values = super::registers::ReadCache::default();
        self.read_vfp_register(&view.reader_name(), &mut values)
            .map_err(|(_, error)| error)?;
        let m0 = values
            .get(&format!(":vfp_mvfr0:{}", view.pair))
            .ok_or("VFP evidence omitted MVFR0")?
            .integer()?;
        let m1 = values
            .get(&format!(":vfp_mvfr1:{}", view.pair))
            .ok_or("VFP evidence omitted MVFR1")?
            .integer()?;
        let features =
            vfp::features(m0 as u64, m1 as u64).ok_or("VFP capacity is not confirmed")?;
        let mut facts = self.effective_register_facts();
        // A successful physical data read independently proves capacity and EN.
        facts.extend([
            ("vfp.present".into(), 1),
            ("vfp.enabled".into(), 1),
            ("vfp.d_registers".into(), features.d_registers),
            ("vfp.neon".into(), u64::from(features.neon)),
            (
                "vfp.double_precision".into(),
                u64::from(features.double_precision),
            ),
        ]);
        let (implementation, evidence) = register.implementation(&facts);
        if implementation == Implementation::No
            || (implementation == Implementation::Unknown && !register.conditions.is_empty())
        {
            return Err(format!("VFP implementation is not confirmed: {evidence}"));
        }
        values
            .remove(&format!(":vfp_pair:{}", view.pair))
            .ok_or("VFP adapter omitted the physical pair".into())
    }
    pub(super) fn preview_vfp_write(
        &mut self,
        p: &Json,
        context: Context,
        register: &Register,
    ) -> Result<Json, String> {
        let Some(Writer::Vfp { name }) = &register.writer else {
            return Err("No VFP writer declared".into());
        };
        let view = WriteView::parse(name).ok_or("Unadapted VFP writer")?;
        let spec = register
            .write
            .as_ref()
            .ok_or("No VFP write semantics declared")?;
        let selection: Selection = serde_json::from_value(p["selection"].clone())
            .map_err(|e| format!("Write selection: {e}"))?;
        let input: Input =
            serde_json::from_value(p["input"].clone()).map_err(|e| format!("Write input: {e}"))?;
        let plan = spec.prepare(selection, &input, OnceState::Unknown)?;
        plan.resolve(None)?; // Plain storage only; no unadapted write side effects.
        let route = self.vfp_write_route()?;
        self.write_drafts.3.retain(|_, draft| {
            draft.created.elapsed() < DRAFT_LIFETIME && draft.context == context
        });
        if self.write_drafts.len() >= MAX_DRAFTS {
            return Err("Too many outstanding write drafts; cancel unused drafts".into());
        }
        let services = crate::debug_access::for_project(&self.project)?;
        let _leases = services
            .iter()
            .map(|s| s.acquire(false))
            .collect::<Result<Vec<_>, _>>()?;
        let thread = self.vfp_write_context(&context)?;
        let protocol = self
            .physical_adapter_read(
                "aarch64 debugtui_vfp_write_protocol",
                "VFP writer capability",
            )
            .map_err(|(_, error)| error)?;
        if protocol != vfp::WRITE_PROTOCOL {
            return Err("Independent VFP writer protocol unsupported".into());
        }
        let before = self.vfp_write_pair(register, view)?;
        if self.vfp_write_context(&context)? != thread || self.vfp_write_route()? != route {
            return Err("VFP context or route changed during preview".into());
        }
        let token = format!(
            "{:x}-{:x}",
            context.session,
            NEXT_DRAFT.fetch_add(1, Ordering::Relaxed)
        );
        let result = json!({"draft":token,"target":{"kind":"register","id":register.id},"context":context,
            "owner":context.core,"scope":"core","thread":thread,"elf":self.project.program.elf,
            "channel":"register_tcl","endpoint":route.endpoint,"target_name":route.target,"writer":name,
            "plan":plan,"observed_before":view.view(&before)?,"physical_pair_before":before,"atomic":false,
            "warning":"Writes this physical core only. S/D preserve fresh sibling bits; Q uses two non-atomic writes. Raw values preserve NaN payloads. Unknown results are never retried or rolled back.",
            "expires_in_ms":DRAFT_LIFETIME.as_millis(),"outcome":Outcome::NotSent});
        self.write_drafts.3.insert(
            token,
            Draft {
                context,
                thread,
                elf: self.project.program.elf.clone(),
                created: Instant::now(),
                register: register.clone(),
                view,
                route,
                plan,
            },
        );
        Ok(result)
    }
    pub(super) fn apply_vfp_write(&mut self, token: &str) -> Result<Json, String> {
        let Some(draft) = self.write_drafts.3.remove(token) else {
            return Err("VFP draft expired".into());
        };
        let Some(Writer::Vfp { name }) = &draft.register.writer else {
            return Err("VFP writer disappeared".into());
        };
        let mut result = json!({"draft":token,"target":{"kind":"register","id":draft.register.id},"context":draft.context,
            "owner":draft.context.core,"scope":"core","thread":draft.thread,"channel":"register_tcl",
            "endpoint":draft.route.endpoint,"target_name":draft.route.target,"writer":name,
            "selected_mask":draft.plan.selected_mask,"atomic":false,"outcome":Outcome::NotSent});
        let services = match crate::debug_access::for_project(&self.project) {
            Ok(services) => services,
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
            Ok(leases) => leases,
            Err(error) => {
                result["error"] = json!(error);
                result["code"] = json!("service_unavailable");
                return Ok(result);
            }
        };
        let resolved = (|| {
            if draft.created.elapsed() >= DRAFT_LIFETIME
                || draft.elf != self.project.program.elf
                || draft.route != self.vfp_write_route()?
                || self.vfp_write_context(&draft.context)? != draft.thread
            {
                return Err("VFP draft belongs to an expired context, thread, ELF or route".into());
            }
            self.vfp_write_pair(&draft.register, draft.view)?;
            if self.vfp_write_context(&draft.context)? != draft.thread {
                return Err("VFP context changed before sending".into());
            }
            draft.plan.resolve(None)
        })();
        let resolved = match resolved {
            Ok(resolved) => resolved,
            Err(error) => {
                result["error"] = json!(error);
                result["code"] = json!("precondition_failed");
                return Ok(result);
            }
        };
        result["command_value"] = json!(resolved.command);
        result["verification_mask"] = json!(resolved.verification_mask);
        // Only fixed validated names and canonical bits enter the atomic Tcl evaluation.
        let operation = format!(
            "if {{[catch {{aarch64 debugtui_vfp_write_protocol}} __dt_fp_write_protocol] || $__dt_fp_write_protocol ne \"{}\"}} {{set __dt_fp_write_result \"outcome not_sent reason writer-unsupported\"}} else {{set __dt_fp_write_result [{} {} {}]}}; set __dt_fp_write_result",
            vfp::WRITE_PROTOCOL,
            draft.route.writer,
            crate::live_watch::word(name),
            resolved.command.hex
        );
        let mut submitted = false;
        let response = self.register_tcl_tracked(&operation, &mut submitted);
        let parsed = match response {
            Ok(text) => WriteResponse::parse(&text, draft.view, &resolved.command),
            Err((_, error)) if !submitted => {
                result["error"] = json!(error);
                result["code"] = json!("precondition_failed");
                return Ok(result);
            }
            Err((_, error)) => Err(error),
        };
        match parsed {
            Ok(WriteResponse::NotSent(reason)) => {
                result["code"] = json!("writer_refused");
                result["error"] = json!(reason);
                self.snapshot.register_probe = None;
                return Ok(result);
            }
            Err(error) => {
                for lease in &mut leases {
                    lease.quarantine(&error);
                }
                self.register_access_fault = Some(error.clone());
                self.state("FAULT");
                result["error"] = json!(error);
                result["code"] = json!("write_result_unknown");
                result["outcome"] = json!(Outcome::Unknown);
            }
            Ok(WriteResponse::Completed {
                outcome,
                before,
                expected,
                observed,
            }) => {
                result["physical_pair_before"] = json!(before);
                result["physical_pair_expected"] = json!(expected);
                result["physical_pair_observed"] = json!(observed);
                result["observed"] = json!(draft.view.view(&observed)?);
                result["neighbours_preserved"] =
                    json!((before.integer()? ^ observed.integer()?) & !draft.view.mask() == 0);
                // The backend returned a checked physical readback. Changed GDB/UI context
                // prevents publishing it as a current selected-thread sample.
                let after = (|| {
                    if self.write_thread()? != draft.thread
                        || self.register_context() != draft.context
                    {
                        return Err("VFP context changed after sending".into());
                    }
                    let frame = self.mi("-stack-info-frame")?;
                    if frame
                        .data
                        .field("frame")
                        .is_none_or(|frame| frame.string("level") != "0")
                    {
                        return Err("GDB frame changed after sending".into());
                    }
                    Ok::<_, String>(())
                })();
                result["outcome"] = json!(if let Err(error) = after {
                    result["error"] = json!(error);
                    result["code"] = json!("verification_context_changed");
                    if outcome == Outcome::Verified {
                        Outcome::Accepted
                    } else {
                        outcome
                    }
                } else {
                    outcome
                });
            }
        }
        self.finish_write(&mut result);
        Ok(result)
    }
}
