//! Global types come from one objfile; bytes come from the explicitly selected
//! physical memory channel. No physical register/frame is replaced or invented.
use super::*;
use crate::images::{Image, Loaded, SavedContext};
mod expression;
mod frame;

impl Engine {
    pub(super) fn validate_image_break_condition(&self, condition: &str) -> Result<(), String> {
        if condition.is_empty() {
            return Ok(());
        }
        if let Some(state) = &self.snapshot.image_state {
            let names = condition
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>();
            for loaded in &state.images {
                let objects =
                    crate::live_watch::image_objects(&self.image_definition(loaded), loaded)?;
                if names.iter().any(|n| objects.contains_key(*n)) {
                    return Err("Named globals in multi-image breakpoint conditions are not objfile-safe; use a local parameter or an explicit typed address expression".into());
                }
            }
        }
        Ok(())
    }
    pub(super) fn image_global_selection(
        &mut self,
        expression: &str,
        requested: Option<&str>,
    ) -> Result<Option<(Loaded, String)>, String> {
        let (qualified, expression) = crate::images::scoped_expression(expression)?;
        if qualified.zip(requested).is_some_and(|(a, b)| a != b) {
            return Err("Expression prefix and selected image disagree".into());
        }
        let requested = qualified.or(requested);
        let Some(state) = &self.snapshot.image_state else {
            return if requested.is_some() {
                Err("Image expressions require configured resident images".into())
            } else {
                Ok(None)
            };
        };
        let images = state.images.clone();
        let current = state.selected_frame.image.clone();
        let (syntax, registers) = expression::syntax(expression)?;
        let roots = memory::expression::roots(&syntax)?;
        if requested.is_none() && registers && roots.is_empty() {
            return Ok(None);
        }
        let local = roots
            .iter()
            .any(|name| self.snapshot.locals.iter().any(|v| v.name == *name));
        let mut matches = Vec::new();
        for loaded in &images {
            let image = self.image_definition(loaded);
            let objects = crate::live_watch::image_objects(&image, loaded)?;
            if roots.iter().any(|name| objects.contains_key(*name)) {
                matches.push(loaded.id.as_str());
            }
        }
        if requested.is_none() && local {
            // A plain local retains its physical frame. Mixed expressions use
            // one objfile plus registers observed from this same physical frame.
            let global_roots = roots.iter().any(|name| {
                !self.snapshot.locals.iter().any(|v| v.name == *name)
                    && images.iter().any(|loaded| {
                        crate::live_watch::image_objects(&self.image_definition(loaded), loaded)
                            .is_ok_and(|objects| objects.contains_key(*name))
                    })
            });
            if !global_roots {
                return Ok(None);
            }
        }
        let id = requested
            .or(current.as_deref())
            .ok_or("Global expression has no verified frame image; use @image:expression")?;
        let loaded = images
            .iter()
            .find(|i| i.id == id)
            .cloned()
            .ok_or("Image is not configured for this core")?;
        if requested.is_none() && !matches.is_empty() && !matches.contains(&id) {
            return Err("Global symbol belongs to another image; use @image:expression".into());
        }
        let expression = if current.as_deref() != Some(id) {
            if registers {
                return Err("Another image has no current physical register frame; inspect its saved Guest context instead".into());
            }
            memory::expression::qualify_globals(expression)?
        } else {
            expression.into()
        };
        Ok(Some((loaded, expression)))
    }

    pub(super) fn with_image_globals<T>(
        &mut self,
        loaded: &Loaded,
        action: impl FnOnce(&mut dyn FnMut(&str) -> Result<Record, String>) -> Result<T, String>,
    ) -> Result<(T, Json), String> {
        self.stopped()?;
        self.guard_image_files()?;
        if loaded.machine != 40 || loaded.bits != 32 || !loaded.has_debug_info {
            return Err(
                "Isolated image globals currently require ARM32 ELF debug information".into(),
            );
        }
        let image: Image = self.image_definition(loaded);
        if std::fs::read(&image.elf).map_err(|e| e.to_string())?.get(5) != Some(&1) {
            return Err("Isolated image globals currently require little-endian ELF".into());
        }
        let channel = if image.memory_channel.is_empty() {
            image
                .saved_context
                .as_ref()
                .map(|s| s.channel.as_str())
                .unwrap_or_default()
        } else {
            &image.memory_channel
        };
        if channel.is_empty() {
            return Err(format!(
                "Image {} needs memory_channel for isolated global reads",
                image.id
            ));
        }
        let config = SavedContext {
            channel: channel.into(),
            ..Default::default()
        };
        let physical = self.register_context();
        let cache_key = self.image_cache_key();
        let architecture = image_symbols::architecture(&self.project).map(str::to_owned);
        let pc = literal_address(&self.snapshot.frame.address)
            .ok()
            .and_then(|n| u32::try_from(n).ok());
        let cpsr = self
            .snapshot
            .image_state
            .as_ref()
            .and_then(|s| s.program_cpsr);
        let services = crate::debug_access::for_project(&self.project)?;
        let _leases = services
            .iter()
            .map(|s| s.acquire(false))
            .collect::<Result<Vec<_>, _>>()?;
        let current = self
            .snapshot
            .image_state
            .as_ref()
            .and_then(|s| s.selected_frame.image.as_deref())
            == Some(image.id.as_str());
        let selected_frame = self.snapshot.frame.clone();
        let (mut inspector, registers) = if current {
            let (values, available) = self.image_frame_registers(loaded)?;
            (
                guest::Inspector::observed(
                    &self.project,
                    self.job.as_ref().ok_or("Process job missing")?,
                    values,
                    self.logs.clone(),
                )?,
                available,
            )
        } else {
            (
                guest::Inspector::globals(
                    &self.project,
                    self.job.as_ref().ok_or("Process job missing")?,
                    pc,
                    cpsr,
                    self.logs.clone(),
                )?,
                Vec::new(),
            )
        };
        let endpoint = inspector.endpoint()?;
        let mut reads = 0u64;
        let mut bytes = 0u64;
        let mut query = |command: &str| {
            inspector.request(command, |address, count| {
                reads += 1;
                bytes += count;
                if reads > 512 || bytes > 65536 {
                    return Err("Image globals memory budget exceeded".into());
                }
                let in_section = loaded.allocated_ranges.iter().any(|r| {
                    r.start <= address && address.checked_add(count).is_some_and(|end| end <= r.end)
                });
                let in_ram = image
                    .saved_context
                    .as_ref()
                    .is_some_and(|s| s.memory.iter().any(|r| r.contains(address, count)));
                if !in_section && !in_ram {
                    return Err(
                        "Image global read is outside its configured allocated sections/RAM".into(),
                    );
                }
                self.guest_read(&config, address, count)
            })
        };
        let result = (|| {
            for command in [
                "-gdb-set pagination off".into(),
                "-gdb-set confirm off".into(),
                "-gdb-set may-call-functions off".into(),
                "-gdb-set trust-readonly-sections on".into(),
                format!(
                    "-file-exec-and-symbols {}",
                    mi::quote(&portable_path(&image.elf))
                ),
            ] {
                query(&command)?;
            }
            if let Some(architecture) = &architecture {
                query(&format!("-gdb-set architecture {architecture}"))?;
            }
            if image.offset != 0 || !image.sections.is_empty() {
                query("-interpreter-exec console \"symbol-file\"")?;
                let mut command = format!(
                    "add-symbol-file {} -o {}",
                    mi::quote(&portable_path(&image.elf)),
                    image.offset
                );
                for (section, address) in &image.sections {
                    command.push_str(&format!(" -s {} 0x{address:x}", mi::quote(section)));
                }
                query(&format!(
                    "-interpreter-exec console {}",
                    mi::quote(&command)
                ))?;
            }
            query(&format!("-target-select remote {endpoint}"))?;
            if current {
                // Select inside the isolated GDB after a genuine unwind from
                // the observed top frame. GDB treats caller return addresses
                // differently from a frame-zero PC when finding local scopes.
                query(&format!("-stack-select-frame {}", selected_frame.level))?;
                let frame = query("-stack-info-frame")?;
                let frame = frame
                    .data
                    .field("frame")
                    .ok_or("Isolated GDB omitted the selected frame")?;
                let observed = Frame::from_mi(frame);
                if observed.level != selected_frame.level
                    || literal_address(&observed.address)? & !1
                        != literal_address(&selected_frame.address)? & !1
                {
                    return Err("Isolated unwind does not match the selected physical frame".into());
                }
            }
            action(&mut query)
        })();
        // GDB owns only the local read-only adapter; exiting cannot resume the
        // physical target, even on an unsuccessful expression.
        let result = inspector.finish_result(result);
        self.guard_image_files()?;
        if self.register_context() != physical || self.image_cache_key() != cache_key {
            return Err(
                "Physical/image context changed during global inspection; result discarded".into(),
            );
        }
        let provenance = json!({"image":image.id,"image_revision":loaded.content_revision,"context":physical,
            "source":if current {"isolated_current_frame"} else {"isolated_image_globals"},"read_only":true,"physical_context_unchanged":true,
            "observed_registers":registers,
            "register_frame":if current {Some(0)} else {None},"frame_unwind_verified":current,
            "memory":{"channel":config.channel,"reads":reads,"bytes":bytes,"atomic":false}});
        result.map(|r| (r, provenance))
    }

    pub(super) fn evaluate_image_global(
        &mut self,
        loaded: &Loaded,
        expression: &str,
    ) -> Result<Json, String> {
        let (value, mut metadata) = self.with_image_globals(loaded, |query| {
            query(&format!(
                "-data-evaluate-expression {}",
                mi::quote(expression)
            ))
            .map(|r| r.data.string("value"))
        })?;
        metadata["value"] = json!(value);
        Ok(metadata)
    }

    pub(super) fn read_image_watch_tree(
        &mut self,
        loaded: &Loaded,
        name: &str,
        expression: &str,
    ) -> Result<Variable, String> {
        let expansions = self.watch_expansions.get(name).cloned().unwrap_or_default();
        let (mut tree, provenance) = self.with_image_globals(loaded, |query| {
            let created = query(&format!("-var-create - * {}", mi::quote(expression)))?;
            let object = created.data.string("name");
            if object.is_empty() {
                return Err("Isolated GDB omitted Watch object identity".into());
            }
            let value = watch::read_node(query, name, &created.data, vec![], &expansions, &mut 256);
            query(&format!("-var-delete {}", mi::quote(&object)))?;
            Ok(value)
        })?;
        let origin = ImageValueOrigin {
            image: loaded.id.clone(),
            revision: loaded.content_revision.clone(),
            channel: provenance["memory"]["channel"]
                .as_str()
                .unwrap_or_default()
                .into(),
        };
        fn stamp(value: &mut Variable, origin: &ImageValueOrigin) {
            value.image_origin = Some(origin.clone());
            if let Some(tree) = &mut value.tree {
                for child in &mut tree.children {
                    stamp(child, origin);
                }
            }
        }
        stamp(&mut tree, &origin);
        Ok(tree)
    }
}
