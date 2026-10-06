//! Recreate typed variable objects at preview/apply; never assign a raw address.
use super::writes::{DRAFT_LIFETIME, MAX_DRAFTS, NEXT_DRAFT};
use super::*;
use crate::{
    registers::{Context, RawValue, Scope},
    writes::{Input, MemoryKind, Outcome, ScalarType, Selection, variable_lvalue},
};
use std::path::PathBuf;
mod bitfields;
mod literals;
use bitfields::BitfieldMetadata;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Metadata {
    expression: String,
    type_name: String,
    reference: bool,
    declared_bits: u16,
    bitfield: Option<BitfieldMetadata>,
    scalar: ScalarType,
    address: Option<String>,
    region: Option<String>,
    owner: String,
    scope: Scope,
}
impl Metadata {
    fn assignment_type(&self) -> String {
        if self.reference {
            format!("__typeof__(*(&({})))", self.expression)
        } else {
            format!("__typeof__({})", self.expression)
        }
    }
}
struct Storage {
    address: Option<String>,
    region: Option<String>,
    owner: String,
    scope: Scope,
}
pub(super) struct Draft {
    context: Context,
    thread: String,
    elf: PathBuf,
    created: Instant,
    target: Json,
    metadata: Metadata,
    raw: RawValue,
    assignment: Assignment,
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", content = "expression", rename_all = "snake_case")]
enum Assignment {
    Expression(String),
    ExactFloatBits,
}

impl Engine {
    fn check_variable_literal(
        &mut self,
        metadata: &Metadata,
        raw: &RawValue,
        assignment: &str,
    ) -> Result<RawValue, String> {
        let expression = format!("({})({assignment})", metadata.assignment_type());
        let created = self.mi(&format!("-var-create - * {}", mi::quote(&expression)))?;
        let name = created.data.string("name");
        if name.is_empty() {
            return Err("GDB omitted the numeric literal object".into());
        }
        let result = self.variable_raw(&name, raw.bits);
        let cleanup = self.mi(&format!("-var-delete {}", mi::quote(&name)));
        match (result, cleanup) {
            (Ok(observed), Ok(_)) => Ok(observed),
            (Err(error), Ok(_)) => Err(error),
            (Err(error), Err(cleanup)) => Err(format!(
                "{error}; numeric literal cleanup failed: {cleanup}"
            )),
            (Ok(_), Err(error)) => Err(format!("Numeric literal cleanup failed: {error}")),
        }
    }
    fn resolve_variable_assignment(
        &mut self,
        metadata: &Metadata,
        input: &Input,
    ) -> Result<(RawValue, Assignment), String> {
        // A GDB may know a DWARF 128-bit type while rejecting the compiler's
        // spelling. Only the validated object supplies this type expression.
        let (raw, assignment) = if metadata.bitfield.is_some() {
            metadata.scalar.bitfield_assignment(input)?
        } else {
            metadata
                .scalar
                .assignment_with_type(input, Some(&metadata.assignment_type()))?
        };
        if !metadata.scalar.requires_literal_probe(&raw) {
            return Ok((raw, Assignment::Expression(assignment)));
        }
        let observed = self.check_variable_literal(metadata, &raw, &assignment)?;
        if observed == raw {
            return Ok((raw, Assignment::Expression(assignment)));
        }
        if !metadata.scalar.float {
            return Err(format!(
                "GDB 128-bit literal mismatch: requested {}, observed {}; no write sent",
                raw.hex, observed.hex
            ));
        }
        self.with_exact_float_literal(metadata, &raw, |_, _| Ok(()))?;
        Ok((raw, Assignment::ExactFloatBits))
    }
    pub(super) fn without_target_calls<T>(
        &mut self,
        body: impl FnOnce(&mut Self) -> Result<T, String>,
    ) -> Result<T, String> {
        let previous = self
            .mi("-gdb-show may-call-functions")?
            .data
            .string("value");
        if !matches!(previous.as_str(), "on" | "off") {
            return Err("Cannot verify GDB target function-call policy".into());
        }
        if previous == "on" {
            self.mi("-gdb-set may-call-functions off")?;
        }
        let result = body(self);
        if previous == "on"
            && let Err(error) = self.mi("-gdb-set may-call-functions on")
        {
            self.register_access_fault = Some(error.clone());
            self.state("FAULT");
            return Err(format!(
                "GDB function-call policy restoration failed: {error}; operation: {}",
                result
                    .as_ref()
                    .err()
                    .map(String::as_str)
                    .unwrap_or("completed")
            ));
        }
        result
    }
    fn variable_target(&self, target: &Json) -> Result<(String, Vec<usize>), String> {
        let expression = target["expression"]
            .as_str()
            .ok_or("Variable root expression required")?;
        variable_lvalue(expression)?;
        match target["pane"].as_str() {
            Some("watch") if self.watch_names.iter().any(|e| e == expression) => {}
            Some("locals") if self.snapshot.locals.iter().any(|v| v.name == expression) => {}
            _ => {
                return Err(
                    "Variable root no longer exists in the selected Watch/Locals pane".into(),
                );
            }
        }
        if target["channel"]
            .as_str()
            .is_some_and(|c| !c.is_empty() && c != "gdb")
        {
            return Err(
                "Typed variable assignment requires the GDB channel; no implicit AP fallback"
                    .into(),
            );
        }
        let path: Vec<usize> =
            serde_json::from_value(target.get("path").cloned().unwrap_or(json!([])))
                .map_err(|_| "Invalid variable child path")?;
        if path.len() > 8 || path.iter().any(|i| *i >= 4096) {
            return Err("Variable child path exceeds its bounds".into());
        }
        if target["pane"] == "watch" {
            let core = self.project.preference_core.as_deref().unwrap_or("single");
            let leaf = if path.is_empty() {
                format!("watch:{expression}")
            } else {
                format!("watch-child:{}", json!([expression, path, false]))
            };
            if self
                .project
                .refresh_policy(core, &leaf, Some(&format!("watch:{expression}")))
                .is_some_and(|p| !p.channel.is_empty() && p.channel != "gdb")
            {
                return Err("This Watch uses an explicit bus channel; typed assignment cannot silently switch to GDB".into());
            }
        }
        Ok((expression.into(), path))
    }
    fn with_variable<T>(
        &mut self,
        target: &Json,
        body: impl FnOnce(&mut Self, Value) -> Result<T, String>,
    ) -> Result<T, String> {
        let (expression, path) = self.variable_target(target)?;
        // var-create may fetch a scalar value. Classify root storage first so
        // a writer preview never uses var-create to probe Flash or MMIO.
        let context = self.register_context();
        let root_type = self.variable_type(&expression)?;
        let signature = ScalarType::type_signature(&root_type)?;
        let pointer = signature.rfind('*');
        let qualifiers = pointer.map_or(signature.as_str(), |i| &signature[i + 1..]);
        let tokens = qualifiers
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .collect::<Vec<_>>();
        if tokens.contains(&"volatile") || (pointer.is_none() && tokens.contains(&"const")) {
            return Err("Const or volatile root storage cannot be edited".into());
        }
        let bytes = self.variable_size(&expression)?;
        self.variable_storage(&expression, bytes, None, &context)?;
        let created = self.mi(&format!("-var-create - * {}", mi::quote(&expression)))?;
        let root = created.data.string("name");
        if root.is_empty() {
            return Err("GDB omitted the root variable object name".into());
        }
        let result = (|| {
            let mut node = created.data;
            for index in path {
                if node.string("dynamic") == "1" {
                    return Err(
                        "Dynamic pretty-printer children do not provide a stable writer".into(),
                    );
                }
                let count = node
                    .string("numchild")
                    .parse::<usize>()
                    .map_err(|_| "Unknown child count")?;
                if index >= count {
                    return Err("Variable child path is no longer in range".into());
                }
                let response = self.mi(&format!(
                    "-var-list-children --no-values {} {index} {}",
                    mi::quote(&node.string("name")),
                    index + 1
                ))?;
                let children = response
                    .data
                    .field("children")
                    .ok_or("GDB omitted child list")?
                    .items();
                if children.len() != 1 {
                    return Err("Variable child selection is ambiguous".into());
                }
                node = children[0].field("child").unwrap_or(&children[0]).clone();
            }
            body(self, node)
        })();
        let deleted = self.mi(&format!("-var-delete {}", mi::quote(&root)));
        if let Err(error) = &deleted {
            self.register_access_fault = Some(error.clone());
            self.state("FAULT");
        }
        match (result, deleted) {
            (Ok(value), Ok(_)) => Ok(value),
            (Err(error), Ok(_)) => Err(error),
            (Err(error), Err(cleanup)) => Err(format!(
                "{error}; variable object cleanup failed: {cleanup}"
            )),
            (_, Err(error)) => Err(format!("Variable object cleanup failed: {error}")),
        }
    }
    fn variable_raw(&mut self, name: &str, bits: u16) -> Result<RawValue, String> {
        let value = self
            .mi(&format!(
                "-var-evaluate-expression -f hexadecimal {}",
                mi::quote(name)
            ))?
            .data
            .string("value");
        RawValue::parse(value.split_whitespace().next().unwrap_or(""), bits).map_err(|e| {
            format!("Variable is optimized out, unavailable or not a raw scalar: {e}; {value}")
        })
    }
    fn variable_console(&mut self, command: &str) -> Result<String, String> {
        if self.console_capture.is_some() {
            return Err("Nested type capture is unavailable".into());
        }
        self.console_capture = Some(String::new());
        let printed = self.console(command);
        let output = self.console_capture.take().unwrap_or_default();
        printed?;
        if output.contains('\0') {
            return Err("Expanded type exceeds the capture limit".into());
        }
        Ok(output)
    }
    fn variable_type(&mut self, expression: &str) -> Result<String, String> {
        let output = self.variable_console(&format!("ptype /r {expression}"))?;
        Ok(output
            .trim()
            .strip_prefix("type = ")
            .filter(|s| !s.is_empty())
            .ok_or("GDB omitted the expanded type")?
            .trim()
            .to_owned())
    }
    fn variable_size(&mut self, expression: &str) -> Result<usize, String> {
        self.mi(&format!(
            "-data-evaluate-expression {}",
            mi::quote(&format!("sizeof({expression})"))
        ))?
        .data
        .string("value")
        .parse::<usize>()
        .ok()
        .filter(|b| *b > 0)
        .ok_or_else(|| "Unknown object byte width".into())
    }
    fn variable_metadata(&mut self, node: &Value, context: &Context) -> Result<Metadata, String> {
        if node.string("name").is_empty() || node.string("dynamic") == "1" {
            return Err("Variable object has no stable writer identity".into());
        }
        let name = node.string("name");
        let attributes = self.mi(&format!("-var-show-attributes {}", mi::quote(&name)))?;
        if attributes.data.string("attr") != "editable" {
            return Err("GDB reports that this variable is not assignable".into());
        }
        let expression = self
            .mi(&format!("-var-info-path-expression {}", mi::quote(&name)))?
            .data
            .string("path_expr");
        variable_lvalue(&expression)?;
        let member = if let Some((parent, field)) = ScalarType::member_parent(&expression)? {
            let parent_type = self.variable_type(&parent)?;
            ScalarType::validate_member(&parent_type, &field)?
                .map(|bits| (parent, parent_type, field, bits))
        } else {
            None
        };
        let type_name = self.variable_type(&expression)?;
        let (pointer, float, boolean) = ScalarType::validate_type(&type_name)?;
        let reference = ScalarType::type_signature(&type_name)?
            .trim_end()
            .ends_with('&');
        let assignment_type = if reference {
            format!("__typeof__(*(&({expression})))")
        } else {
            format!("__typeof__({expression})")
        };
        if !pointer && node.string("numchild").parse::<usize>().unwrap_or(0) > 0 {
            return Err("Select a scalar member of the aggregate".into());
        }
        let bytes = self.variable_size(&expression)?;
        let declared_bits = u16::try_from(bytes)
            .ok()
            .and_then(|b| b.checked_mul(8))
            .filter(|b| matches!(b, 8 | 16 | 32 | 64 | 128))
            .ok_or("Typed scalar width is unsupported")?;
        let bits = member.as_ref().map_or(declared_bits, |m| m.3);
        let signed = if pointer || float || boolean {
            false
        } else {
            let value = self
                .mi(&format!(
                    "-data-evaluate-expression {}",
                    mi::quote(&format!("(({assignment_type})-1) < (({assignment_type})0)"))
                ))?
                .data
                .string("value");
            match value.as_str() {
                "1" | "true" => true,
                "0" | "false" => false,
                _ => return Err("Cannot determine the scalar signed range".into()),
            }
        };
        let (storage, bitfield) = if let Some((parent, parent_type, field, bits)) = member {
            if pointer || float || reference {
                return Err("Bitfield requires a plain integer/boolean type".into());
            }
            let (storage, metadata) =
                self.bitfield_metadata(&parent, &parent_type, &field, bits, bytes, context)?;
            (storage, Some(metadata))
        } else {
            (
                self.variable_storage(&expression, bytes, Some(bits), context)?,
                None,
            )
        };
        if reference && storage.address.is_none() {
            return Err(
                "Reference writer requires the actual referent's declared RAM address".into(),
            );
        }
        let metadata = Metadata {
            expression,
            type_name,
            reference,
            declared_bits,
            bitfield,
            scalar: ScalarType {
                bits,
                signed,
                float,
                pointer,
                boolean,
            },
            address: storage.address,
            region: storage.region,
            owner: storage.owner,
            scope: storage.scope,
        };
        self.variable_value(&name, &metadata)?;
        self.bitfield_before(&metadata, &name, context)?;
        if self.register_context() != *context || self.snapshot.state != "STOPPED" {
            return Err("Variable context changed while resolving its type/storage".into());
        }
        Ok(metadata)
    }
    fn variable_storage(
        &mut self,
        expression: &str,
        bytes: usize,
        bits: Option<u16>,
        context: &Context,
    ) -> Result<Storage, String> {
        let address_response = self.mi(&format!(
            "-data-evaluate-expression {}",
            mi::quote(&format!("(unsigned long long)&({expression})"))
        ));
        match address_response {
            Ok(record) => {
                let value = record.data.string("value");
                let address =
                    super::memory::literal_address(value.split_whitespace().next().unwrap_or(""))?;
                let region = self
                    .project
                    .writes
                    .resolve(address, bytes, "", &context.core)?
                    .clone();
                if region.kind != MemoryKind::Ram {
                    return Err("Typed variables require a declared RAM region; Flash/MMIO need their own writer".into());
                }
                if bits.is_some_and(|b| {
                    !region.widths.contains(&b) || !address.is_multiple_of(bytes as u64)
                }) {
                    return Err(
                        "Typed scalar width/alignment is not declared for this RAM region".into(),
                    );
                }
                if self.mi("-gdb-show may-write-memory")?.data.string("value") != "on" {
                    return Err("GDB memory writes are disabled".into());
                }
                let (owner, _) = self.write_owner(&region, context)?;
                Ok(Storage {
                    address: Some(format!("0x{address:x}")),
                    region: Some(region.id),
                    owner,
                    scope: region.scope,
                })
            }
            Err(error) if error.contains("is in register") && context.frame == 0 => {
                if self
                    .mi("-gdb-show may-write-registers")?
                    .data
                    .string("value")
                    != "on"
                {
                    return Err("GDB register writes are disabled".into());
                }
                Ok(Storage {
                    address: None,
                    region: None,
                    owner: format!("core:{}", context.core),
                    scope: Scope::Core,
                })
            }
            Err(error) => Err(format!(
                "Variable has no proven writable storage (bitfields, temporaries and saved registers require a separate writer): {error}"
            )),
        }
    }
    fn variable_context(&mut self, context: &Context) -> Result<String, String> {
        self.stopped()?;
        if self.register_context() != *context || self.cancellation.load(Ordering::Relaxed) {
            return Err("Variable edit context expired or cancelled".into());
        }
        let thread = self.write_thread()?;
        let frame = self.mi("-stack-info-frame")?;
        if frame
            .data
            .field("frame")
            .is_none_or(|f| f.string("level").parse::<u32>().ok() != Some(context.frame))
        {
            return Err("GDB selected frame differs from the edit context".into());
        }
        self.stopped()?;
        if self.register_context() != *context || self.cancellation.load(Ordering::Relaxed) {
            return Err("Variable edit context changed during frame inspection".into());
        }
        Ok(thread)
    }
    pub(super) fn preview_variable_write(&mut self, p: &Json) -> Result<Json, String> {
        let context: Context = serde_json::from_value(p["context"].clone())
            .map_err(|e| format!("Write context required: {e}"))?;
        self.project
            .writes
            .validate(&self.project.memory_access, &self.project.cores)?;
        let selection: Selection =
            serde_json::from_value(p["selection"].clone()).map_err(|e| e.to_string())?;
        if selection != Selection::Register {
            return Err("Variable fields use the native member path, not register masks".into());
        }
        self.variable_target(&p["target"])?;
        let input: Input = serde_json::from_value(p["input"].clone()).map_err(|e| e.to_string())?;
        let services = crate::debug_access::for_project(&self.project)?;
        let _leases = services
            .iter()
            .map(|s| s.acquire(false))
            .collect::<Result<Vec<_>, _>>()?;
        let thread = self.variable_context(&context)?;
        if self
            .mi("-info-gdb-mi-command var-assign")?
            .data
            .field("command")
            .is_none_or(|c| c.string("exists") != "true")
        {
            return Err("GDB typed variable writer is unavailable".into());
        }
        let (metadata, raw, assignment) = self.without_target_calls(|engine| {
            engine.with_variable(&p["target"], |engine, node| {
                let metadata = engine.variable_metadata(&node, &context)?;
                let (raw, assignment) = engine.resolve_variable_assignment(&metadata, &input)?;
                Ok((metadata, raw, assignment))
            })
        })?;
        self.variable_context(&context)?;
        self.write_drafts
            .2
            .retain(|_, d| d.created.elapsed() < DRAFT_LIFETIME && d.context == context);
        if self.write_drafts.len() >= MAX_DRAFTS {
            return Err("Too many write drafts; cancel unused drafts".into());
        }
        let token = format!(
            "{:x}-{:x}",
            context.session,
            NEXT_DRAFT.fetch_add(1, Ordering::Relaxed)
        );
        let mask = RawValue::from_integer(
            if raw.bits == 128 {
                u128::MAX
            } else {
                (1u128 << raw.bits) - 1
            },
            raw.bits,
        )?;
        let result = json!({"draft":token,"target":p["target"],"context":context,"thread":thread,"owner":metadata.owner,"scope":metadata.scope,"metadata":metadata,
            "channel":"gdb","endpoint":self.project.target.endpoint,"plan":{"value":raw,"selected_mask":mask,"needs_fresh_read":metadata.bitfield.is_some()},"outcome":Outcome::NotSent,
            "literal":assignment,"warning":"Apply recreates and rechecks type/storage with target calls disabled. Bitfields re-read parent bytes immediately before one typed assignment and verify all neighbouring bits; special float and 128-bit literals are checked against exact preview bits.","expires_in_ms":DRAFT_LIFETIME.as_millis()});
        self.write_drafts.2.insert(
            token,
            Draft {
                context,
                thread,
                elf: self.project.program.elf.clone(),
                created: Instant::now(),
                target: p["target"].clone(),
                metadata,
                raw,
                assignment,
            },
        );
        Ok(result)
    }
    pub(super) fn apply_variable_write(&mut self, token: &str) -> Result<Json, String> {
        let draft = self
            .write_drafts
            .2
            .remove(token)
            .ok_or("Variable draft expired")?;
        let mut result = json!({"draft":token,"target":draft.target,"context":draft.context,"thread":draft.thread,"owner":draft.metadata.owner,"scope":draft.metadata.scope,
            "metadata":draft.metadata,"channel":"gdb","endpoint":self.project.target.endpoint,"command_value":draft.raw,"atomic":false,"outcome":Outcome::NotSent});
        let services = crate::debug_access::for_project(&self.project)?;
        let mut leases = match services
            .iter()
            .map(|s| s.acquire(false))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(l) => l,
            Err(e) => {
                result["error"] = json!(e);
                return Ok(result);
            }
        };
        let mut sent = false;
        let mut outcome = Outcome::NotSent;
        let operation = (|| {
            if draft.created.elapsed() >= DRAFT_LIFETIME || self.project.program.elf != draft.elf {
                return Err("Variable draft or ELF expired".into());
            }
            if self.variable_context(&draft.context)? != draft.thread {
                return Err("Variable thread changed".into());
            }
            self.without_target_calls(|engine| {
                engine.with_variable(&draft.target, |engine, node| {
                    let metadata = engine.variable_metadata(&node, &draft.context)?;
                    if metadata != draft.metadata {
                        return Err(
                            "Variable type, member, owner or storage changed; preview again".into(),
                        );
                    }
                    if engine.variable_context(&draft.context)? != draft.thread {
                        return Err("Variable thread changed before assignment".into());
                    }
                    let mut assign = |engine: &mut Self, assignment: &str| {
                        let before = engine.bitfield_before(
                            &metadata,
                            &node.string("name"),
                            &draft.context,
                        )?;
                        // Literal probing also drains asynchronous MI records.
                        // Recheck after it, immediately before the sole assignment.
                        if engine.variable_context(&draft.context)? != draft.thread {
                            return Err("Variable thread changed during literal inspection".into());
                        }
                        let expression = format!("({})({assignment})", metadata.assignment_type());
                        sent = true;
                        outcome = Outcome::Unknown;
                        engine.mi(&format!(
                            "-var-assign {} {}",
                            mi::quote(&node.string("name")),
                            mi::quote(&expression)
                        ))?;
                        outcome = Outcome::Accepted;
                        if engine.register_context() != draft.context
                            || engine.snapshot.state != "STOPPED"
                        {
                            return Err(
                                "Assignment accepted but context changed before verification"
                                    .into(),
                            );
                        }
                        let observed = engine.variable_value(&node.string("name"), &metadata)?;
                        result["observed"] = json!(observed);
                        let neighbours = if let Some(before) = before {
                            let field = metadata.bitfield.as_ref().unwrap();
                            let after = engine.bitfield_bytes(&metadata, &draft.context)?;
                            let expected =
                                field
                                    .layout
                                    .expected(&before, &draft.raw, field.little_endian)?;
                            let matched = after == expected;
                            let original_field =
                                field.layout.extract(&before, field.little_endian)?;
                            let neighbours_preserved = field.layout.expected(
                                &after,
                                &original_field,
                                field.little_endian,
                            )? == before;
                            result["gdb_observed"] = json!(observed);
                            result["observed"] =
                                json!(field.layout.extract(&after, field.little_endian)?);
                            result["parent_before_bytes"] = json!(before);
                            result["parent_after_bytes"] = json!(after);
                            result["parent_expected_bytes"] = json!(expected);
                            result["neighbours_preserved"] = json!(neighbours_preserved);
                            result["parent_matches_expected"] = json!(matched);
                            matched
                        } else {
                            true
                        };
                        if engine.register_context() != draft.context
                            || engine.snapshot.state != "STOPPED"
                        {
                            return Err("Context changed during variable verification".into());
                        }
                        outcome = if observed == draft.raw && neighbours {
                            Outcome::Verified
                        } else {
                            Outcome::Mismatch
                        };
                        Ok(())
                    };
                    match &draft.assignment {
                        Assignment::Expression(expression) => {
                            if metadata.scalar.requires_literal_probe(&draft.raw)
                                && engine
                                    .check_variable_literal(&metadata, &draft.raw, expression)?
                                    != draft.raw
                            {
                                return Err(
                                    "GDB numeric literal changed since preview; no write sent"
                                        .into(),
                                );
                            }
                            assign(engine, expression)
                        }
                        Assignment::ExactFloatBits => {
                            engine.with_exact_float_literal(&metadata, &draft.raw, assign)
                        }
                    }
                })
            })
        })();
        if let Err(error) = operation {
            result["error"] = json!(error);
            result["code"] = json!(if !sent {
                "precondition_failed"
            } else if outcome == Outcome::Unknown {
                "write_result_unknown"
            } else {
                "verification_or_cleanup_unavailable"
            });
            if outcome == Outcome::Unknown || self.snapshot.state == "FAULT" {
                for lease in &mut leases {
                    lease.quarantine(&error);
                }
                self.register_access_fault = Some(error);
                self.state("FAULT");
            }
        }
        result["outcome"] = json!(outcome);
        if sent {
            self.finish_write(&mut result);
        }
        Ok(result)
    }
}
