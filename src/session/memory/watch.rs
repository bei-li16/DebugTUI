//! Resolve a typed Watch address only in a proven stopped thread/frame.
use super::*;
#[derive(Clone)]
pub(in crate::session) struct WatchBinding {
    context: Context,
    epoch: u64,
    thread: String,
    pc: String,
    root: String,
    path: Vec<usize>,
    elf: std::path::PathBuf,
    endpoint: Option<String>,
    address: u64,
    bits: u64,
    little: bool,
}

impl Engine {
    pub(in crate::session) fn resolve_watch(&mut self, p: &Json) -> Result<Json, String> {
        let boundary = self.begin_memory_read(p)?;
        self.stopped()?;
        if let Some(fault) = &self.register_access_fault {
            return Err(format!(
                "Debug access is faulted; reconnect before resolving Watch: {fault}"
            ));
        }
        let expression = p["expression"]
            .as_str()
            .ok_or("Watch expression is required")?;
        expression::validate(expression)?;
        if !self.watch_names.iter().any(|name| name == expression) {
            return Err("Watch no longer exists".into());
        }
        let path: Vec<usize> = serde_json::from_value(p.get("path").cloned().unwrap_or(json!([])))
            .map_err(|_| "Invalid child path")?;
        if path.len() > 8 || path.iter().any(|i| *i >= 4096) {
            return Err("Watch child path exceeds its bounds".into());
        }
        let before = self.selected_register_frame()?;
        if before.1 != boundary.context.frame {
            return Err("Selected GDB frame does not match the requested Watch context".into());
        }
        literal_address(&before.2).map_err(|_| "Cannot verify the selected frame address")?;
        self.finish_memory_read(&boundary)?;
        let mut result = self.without_target_calls(|engine| {
            engine.resolve_watch_object(expression, &path, &boundary)
        })?;
        // Restoration and object cleanup finish before a cancellation/context
        // change can discard the result. No binding escapes either failure.
        self.finish_memory_read(&boundary)?;
        let after = self.selected_register_frame()?;
        if before != after {
            return Err(
                "Selected GDB thread, frame or frame address changed during Watch resolution"
                    .into(),
            );
        }
        self.finish_memory_read(&boundary)?;
        result["context"] = json!(boundary.context);
        result["thread"] = json!(after.0);
        result["frame_address"] = json!(after.2);
        result["source"] = json!("gdb_typed_address");
        result["state"] = json!(state::STOPPED);
        let context = self.register_context();
        self.watch_bindings.retain(|_, proof| {
            proof.context == context
                && proof.epoch == self.snapshot.memory_selection_epoch
                && !(proof.root == expression && proof.path == path)
        });
        if self.watch_bindings.len() >= 256 {
            self.watch_bindings.clear();
        }
        self.watch_binding_serial = self.watch_binding_serial.wrapping_add(1);
        let id = format!("{}:{}", self.register_session, self.watch_binding_serial);
        self.watch_bindings.insert(
            id.clone(),
            WatchBinding {
                context,
                epoch: self.snapshot.memory_selection_epoch,
                thread: after.0,
                pc: after.2,
                root: expression.into(),
                path,
                elf: self.project.program.elf.clone(),
                endpoint: self.connected_gdb_endpoint.clone(),
                address: result["address"].as_u64().unwrap(),
                bits: result["bits"].as_u64().unwrap(),
                little: result["little_endian"].as_bool().unwrap(),
            },
        );
        result["binding_id"] = json!(id);
        result["selection_epoch"] = json!(self.snapshot.memory_selection_epoch);
        Ok(result)
    }

    pub(super) fn check_watch_binding(
        &mut self,
        p: &Json,
        boundary: &ReadBoundary,
    ) -> Result<(), String> {
        let Some(id) = p.get("watch_binding") else {
            return Ok(());
        };
        let id = id.as_str().ok_or("Watch binding must be a string")?;
        let proof = self
            .watch_bindings
            .get(id)
            .cloned()
            .ok_or("Watch binding expired; resolve the selected Watch while stopped")?;
        if proof.context != boundary.context
            || proof.epoch != self.snapshot.memory_selection_epoch
            || proof.elf != self.project.program.elf
            || proof.endpoint != self.connected_gdb_endpoint
            || !self.watch_names.contains(&proof.root)
            || p["address"].as_u64() != Some(proof.address)
            || p["bits"].as_u64() != Some(proof.bits)
            || p["little_endian"].as_bool() != Some(proof.little)
        {
            return Err(
                "Watch binding no longer matches its context, program or typed address".into(),
            );
        }
        if self.snapshot.state == state::STOPPED {
            let frame = self.selected_register_frame()?;
            if frame != (proof.thread, proof.context.frame, proof.pc) {
                self.snapshot.memory_selection_epoch =
                    self.snapshot.memory_selection_epoch.wrapping_add(1);
                self.watch_bindings.clear();
                self.publish();
                return Err(
                    "Watch binding belongs to a different selected thread, frame or PC".into(),
                );
            }
            self.finish_memory_read(boundary)?;
        }
        Ok(())
    }

    fn resolve_watch_object(
        &mut self,
        expression: &str,
        path: &[usize],
        boundary: &ReadBoundary,
    ) -> Result<Json, String> {
        self.finish_memory_read(boundary)?;
        let created = self.mi(&format!("-var-create - * {}", mi::quote(expression)))?;
        let root = created.data.string("name");
        if root.is_empty() {
            return Err("GDB omitted the Watch variable object identity".into());
        }
        let result = (|| {
            let mut node = created.data.clone();
            for &index in path {
                self.finish_memory_read(boundary)?;
                stable_node(&node)?;
                let count = child_count(&node)?;
                if index >= count {
                    return Err("Watch child path is no longer in range".into());
                }
                let record = self.mi(&format!(
                    "-var-list-children --no-values {} {index} {}",
                    mi::quote(&node.string("name")),
                    index + 1
                ))?;
                let children = record
                    .data
                    .field("children")
                    .ok_or("Watch child is unavailable")?
                    .items();
                if children.len() != 1 {
                    return Err("Watch child selection is ambiguous".into());
                }
                node = children[0].field("child").unwrap_or(&children[0]).clone();
            }
            stable_node(&node)?;
            if child_count(&node)? > 0 && !node.string("type").contains('*') {
                return Err("Expand the aggregate to read its visible scalar members".into());
            }
            self.finish_memory_read(boundary)?;
            let record = self.mi(&format!(
                "-var-info-path-expression {}",
                mi::quote(&node.string("name"))
            ))?;
            let expression = record.data.string("path_expr");
            expression::validate(&expression)?;
            self.finish_memory_read(boundary)?;
            let address = self.mi(&format!(
                "-data-evaluate-expression {}",
                mi::quote(&format!("(unsigned long long)&({expression})"))
            ))?;
            let address = integer(&address.data.string("value")).map_err(
                |_| "Expression has no stable memory address (register, bitfield or temporary)",
            )?;
            self.finish_memory_read(boundary)?;
            let size = self.mi(&format!(
                "-data-evaluate-expression {}",
                mi::quote(&format!("sizeof({expression})"))
            ))?;
            let bits = integer(&size.data.string("value"))?
                .checked_mul(8)
                .ok_or("Size overflow")?;
            if !matches!(bits, 8 | 16 | 32 | 64) {
                return Err("Live reads support scalar 8/16/32/64-bit values".into());
            }
            let cast = self.mi(&format!(
                "-data-evaluate-expression {}",
                mi::quote(&format!("(__typeof__({expression}))1.5"))
            ));
            let float = cast.as_ref().is_ok_and(|r| r.data.string("value") == "1.5");
            self.finish_memory_read(boundary)?;
            let negative = self.mi(&format!(
                "-data-evaluate-expression {}",
                mi::quote(&format!("(__typeof__({expression}))-1"))
            ));
            let signed = negative
                .as_ref()
                .is_ok_and(|r| r.data.string("value").starts_with('-'));
            let mut header = [0u8; 6];
            std::fs::File::open(&self.project.program.elf)
                .and_then(|mut f| f.read_exact(&mut header))
                .map_err(|e| e.to_string())?;
            let little = if &header[..4] == b"\x7fELF" {
                match header[5] {
                    1 => true,
                    2 => false,
                    _ => return Err("Unknown ELF byte order".into()),
                }
            } else if &header[..2] == b"MZ" {
                true
            } else {
                return Err("Cannot determine program byte order".into());
            };
            Ok(
                json!({"address":address,"bits":bits,"float":float,"signed":signed,
                "little_endian":little,"type":node.string("type"),"expression":expression}),
            )
        })();
        let cleanup = self
            .mi(&format!("-var-delete {}", mi::quote(&root)))
            .and_then(|r| {
                if r.data
                    .string("ndeleted")
                    .parse::<usize>()
                    .is_ok_and(|n| n > 0)
                {
                    Ok(())
                } else {
                    Err("GDB did not confirm Watch object deletion".into())
                }
            });
        if let Err(error) = &cleanup {
            self.register_access_fault = Some(error.clone());
            self.state(state::FAULT);
        }
        match (result, cleanup) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(error), Ok(())) => Err(error),
            (Err(error), Err(cleanup)) => {
                Err(format!("{error}; Watch object cleanup failed: {cleanup}"))
            }
            (_, Err(error)) => Err(format!("Watch object cleanup failed: {error}")),
        }
    }
}

fn child_count(node: &Value) -> Result<usize, String> {
    node.string("numchild")
        .parse()
        .map_err(|_| "GDB omitted a valid Watch child count".into())
}
fn stable_node(node: &Value) -> Result<(), String> {
    if node.string("name").is_empty() || node.string("dynamic") == "1" {
        Err("Watch object has no stable identity or uses dynamic pretty-printer children".into())
    } else {
        Ok(())
    }
}
