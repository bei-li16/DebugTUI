//! Source metadata is queried in one objfile per image, never in the physical
//! inferior's shared substitute-path table. These requests are safe while running.
use super::*;
use crate::images::Loaded;

fn metadata_source(maps: &[crate::config::SourceMap], raw: &str, fullname: &str) -> String {
    let normalized = crate::source::normalized(raw);
    if let Some((map, tail)) = maps.iter().find_map(|map| {
        crate::source::suffix(&normalized, &crate::source::normalized(&map.from))
            .map(|tail| (map, tail))
    }) {
        let expected = map.to.join(tail);
        // Do not accept GDB's basename fallback after a mapping failed.
        if expected.is_file() {
            portable_path(&expected)
        } else {
            raw.into()
        }
    } else if fullname.is_empty() {
        raw.into()
    } else {
        fullname.into()
    }
}

impl Engine {
    fn cached_image_files(&mut self, image: &Loaded) -> Result<Vec<(String, String)>, String> {
        let key = format!("{}:{}", image.id, image.content_revision);
        if let Some(files) = self.image_source_files.get(&key) {
            return Ok(files.clone());
        }
        let record =
            self.with_image_metadata(image, |query| query("-file-list-exec-source-files"))?;
        let entries = record
            .data
            .field("files")
            .map(Value::items)
            .unwrap_or_default();
        if entries.len() > 50000 {
            return Err("Image source inventory exceeds 50000 files".into());
        }
        let maps = crate::images::source_maps(&self.project, &self.image_definition(image));
        let mut files = entries
            .iter()
            .filter_map(|value| {
                let raw = value.string("file");
                let full = value.string("fullname");
                let full = metadata_source(&maps, &raw, &full);
                (!full.is_empty() && !full.chars().any(char::is_control)).then_some((raw, full))
            })
            .collect::<Vec<_>>();
        files.sort();
        files.dedup();
        self.image_source_files.insert(key, files.clone());
        Ok(files)
    }

    fn owned_frame_source(&mut self, image: &Loaded, frame: &Frame) -> String {
        let raw = if frame.debug_file.is_empty() {
            &frame.file
        } else {
            &frame.debug_file
        };
        let raw = crate::images::scoped_expression(raw)
            .map(|(_, f)| f)
            .unwrap_or(raw);
        let normalized = crate::source::normalized(raw);
        if normalized.is_empty() {
            return String::new();
        }
        // Absolute DWARF file records already contain their compilation directory.
        // Relative records need the owning objfile's comp_dir, not the other
        // resident objfile's global substitute-path result or a basename guess.
        if normalized.starts_with('/') || normalized.as_bytes().get(1) == Some(&b':') {
            return crate::images::qualified_source(Some(&image.id), raw);
        }
        let files = match self.cached_image_files(image) {
            Ok(files) => files,
            Err(error) => {
                self.log(
                    "warning",
                    format!("Source metadata for {} unavailable: {error}", image.id),
                );
                return String::new();
            }
        };
        let exact = files
            .iter()
            .filter(|(file, _)| crate::source::normalized(file) == normalized)
            .map(|(_, full)| full.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let candidates = if exact.is_empty() {
            files
                .iter()
                .filter(|(file, _)| {
                    crate::source::normalized(file)
                        .strip_suffix(&normalized)
                        .is_some_and(|head| head.ends_with('/'))
                })
                .map(|(_, full)| full.clone())
                .collect::<std::collections::BTreeSet<_>>()
        } else {
            exact
        };
        if candidates.len() == 1 {
            crate::images::qualified_source(Some(&image.id), candidates.first().unwrap())
        } else {
            self.log(
                "warning",
                format!(
                    "Source {} in {} has {} matching compilation records; no source guessed",
                    raw,
                    image.id,
                    candidates.len()
                ),
            );
            String::new()
        }
    }

    pub(super) fn qualify_image_sources(&mut self) {
        let Some(state) = self.snapshot.image_state.clone() else {
            return;
        };
        let mut frame = self.snapshot.frame.clone();
        if let Some(image) = state
            .images
            .iter()
            .find(|i| Some(&i.id) == state.selected_frame.image.as_ref())
        {
            frame.file = self.owned_frame_source(image, &frame);
        } else {
            frame.file.clear();
        }
        self.snapshot.frame = frame;
        let mut stack = self.snapshot.stack.clone();
        for frame in &mut stack {
            let owner = literal_address(&frame.address).ok().and_then(|pc| {
                crate::images::resolve(
                    &state.images,
                    pc,
                    if frame.level == 0 {
                        state.program_cpsr
                    } else {
                        None
                    },
                )
                .image
            });
            if let Some(image) = state.images.iter().find(|i| Some(&i.id) == owner.as_ref()) {
                frame.file = self.owned_frame_source(image, frame);
            } else {
                frame.file.clear();
                frame.function.clear();
                frame.line = 0;
            }
        }
        self.snapshot.stack = stack;
    }

    pub(super) fn qualify_image_breakpoint_sources(&mut self) {
        let Some(state) = self.snapshot.image_state.clone() else {
            return;
        };
        let mut breakpoints = self.snapshot.breakpoints.clone();
        for breakpoint in &mut breakpoints {
            let Some(image) = state
                .images
                .iter()
                .find(|i| Some(&i.id) == breakpoint.image.as_ref())
            else {
                continue;
            };
            breakpoint.file = self.owned_frame_source(
                image,
                &Frame {
                    file: breakpoint.file.clone(),
                    debug_file: breakpoint.debug_file.clone(),
                    ..Default::default()
                },
            );
        }
        self.snapshot.breakpoints = breakpoints;
    }

    pub(super) fn navigation_images(
        &mut self,
        requested: Option<&str>,
    ) -> Result<Vec<Loaded>, String> {
        let mut images = if self
            .snapshot
            .image_state
            .as_ref()
            .is_some_and(|s| s.symbols_loaded)
        {
            self.guard_image_files()?;
            self.snapshot.image_state.as_ref().unwrap().images.clone()
        } else {
            crate::images::inspect_project(&self.project)?
        };
        if let Some(id) = requested {
            images.retain(|i| i.id == id);
            if images.is_empty() {
                return Err("Image is not configured for this core".into());
            }
        }
        if self.job.is_none() {
            self.job = Some(crate::process::Job::new()?);
        }
        Ok(images)
    }

    pub(super) fn with_image_metadata<T>(
        &mut self,
        loaded: &Loaded,
        action: impl FnOnce(&mut dyn FnMut(&str) -> Result<Record, String>) -> Result<T, String>,
    ) -> Result<T, String> {
        let image = self.image_definition(loaded);
        let mut gdb = guest::Inspector::offline(
            &self.project,
            self.job.as_ref().ok_or("No metadata process job")?,
            self.logs.clone(),
        )?;
        let mut query = |command: &str| {
            gdb.request(command, |_, _| {
                Err("ELF navigation cannot read physical memory".into())
            })
        };
        let result = (|| {
            for command in [
                "-gdb-set pagination off".into(),
                "-gdb-set confirm off".into(),
                "-gdb-set may-call-functions off".into(),
                format!(
                    "-file-exec-and-symbols {}",
                    mi::quote(&portable_path(&image.elf))
                ),
            ] {
                query(&command)?;
            }
            if let Some(architecture) = image_symbols::architecture(&self.project) {
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
                for range in &loaded.executable_ranges {
                    if range
                        .section
                        .chars()
                        .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '"' | '\''))
                    {
                        return Err("Offline section relocation needs section names without whitespace or quotes".into());
                    }
                    query(&format!(
                        "-interpreter-exec console {}",
                        mi::quote(&format!("section {} 0x{:x}", range.section, range.start))
                    ))?;
                }
            }
            if !image.source_root.as_os_str().is_empty() {
                query(&format!(
                    "-environment-directory {}",
                    mi::quote(&portable_path(&image.source_root))
                ))?;
            }
            for map in crate::source::gdb_maps(crate::images::source_maps(&self.project, &image)) {
                query(&format!(
                    "-interpreter-exec console {}",
                    mi::quote(&format!(
                        "set substitute-path {} {}",
                        mi::quote(&map.from),
                        mi::quote(&portable_path(&map.to))
                    ))
                ))?;
            }
            action(&mut query)
        })();
        let result = gdb.finish_result(result);
        if crate::images::changed(loaded, &image)? {
            if self
                .snapshot
                .image_state
                .as_ref()
                .is_some_and(|s| s.symbols_loaded)
            {
                let _ = self.guard_image_files();
            }
            return Err("ELF changed during source navigation; results discarded".into());
        }
        result
    }

    pub(super) fn image_files(&mut self, p: &Json) -> Result<Json, String> {
        let images = self.navigation_images(p["image"].as_str())?;
        let mut files = Vec::new();
        let mut warnings = Vec::new();
        for image in &images {
            for (_, file) in self.cached_image_files(image)? {
                files.push(crate::images::qualified_source(Some(&image.id), &file));
                if files.len() > 50000 {
                    return Err("Image source inventory exceeds 50000 files".into());
                }
            }
            if !image.has_debug_info {
                warnings.push(format!("{} has no DWARF source information", image.id));
            }
        }
        files.sort();
        files.dedup();
        self.snapshot.files = files.into();
        self.publish();
        Ok(
            json!({"files":&*self.snapshot.files,"images":images,"warnings":warnings,"target_io":false}),
        )
    }

    pub(super) fn image_symbols(&mut self, p: &Json) -> Result<Json, String> {
        let (prefix, query) =
            crate::images::scoped_expression(p["query"].as_str().unwrap_or_default())?;
        if prefix.zip(p["image"].as_str()).is_some_and(|(a, b)| a != b) {
            return Err("Symbol prefix and selected image disagree".into());
        }
        let query = query.trim();
        if query.is_empty() || query.chars().count() > 128 || query.chars().any(char::is_control) {
            return Err("Enter 1..128 characters to search symbols".into());
        }
        let images = self.navigation_images(prefix.or(p["image"].as_str()))?;
        let pattern = mi::quote(&crate::search::symbol_pattern(query));
        let mut found = Vec::new();
        let mut warnings = Vec::new();
        let mut truncated = false;
        for image in &images {
            if !image.has_debug_info {
                warnings.push(format!("{} has no DWARF source information", image.id));
            }
            let maps = crate::images::source_maps(&self.project, &self.image_definition(image));
            let (symbols, notes, limited) = self.with_image_metadata(image, |gdb| {
                let mut found = Vec::new();
                let mut notes = Vec::new();
                let mut limited = false;
                let mut supported = 0;
                for (command, kind) in [
                    ("functions", "function"),
                    ("variables", "variable"),
                    ("types", "type"),
                ] {
                    match gdb(&format!(
                        "-symbol-info-{command} --name {pattern} --max-results 200"
                    )) {
                        Ok(r) => {
                            supported += 1;
                            let mut parsed = symbols::parse_symbols_with_path(&r, kind, |file| {
                                metadata_source(
                                    &maps,
                                    &file.string("filename"),
                                    &file.string("fullname"),
                                )
                            });
                            limited |= parsed.len() >= 200;
                            for symbol in &mut parsed {
                                symbol.file =
                                    crate::images::qualified_source(Some(&image.id), &symbol.file);
                                symbol.description =
                                    format!("[{}] {}", image.id, symbol.description);
                            }
                            found.extend(parsed);
                        }
                        Err(e) => notes.push(format!("{} {kind}: {e}", image.id)),
                    }
                }
                if supported == 0 {
                    return Err(format!(
                        "Image {} symbol search unavailable: {}",
                        image.id,
                        notes.join("; ")
                    ));
                }
                Ok((found, notes, limited))
            })?;
            found.extend(symbols);
            warnings.extend(notes);
            truncated |= limited;
        }
        let mut matcher = crate::search::Matcher::new(query);
        found.retain(|s| matcher.score(&s.name).is_some());
        found.sort_by_cached_key(|s| {
            (
                matcher.score(&s.name),
                s.name.clone(),
                s.file.clone(),
                s.line,
                s.kind.clone(),
            )
        });
        found.dedup();
        Ok(
            json!({"symbols":found,"images":images,"warnings":warnings,"truncated":truncated,"target_io":false}),
        )
    }

    pub(super) fn image_disassemble(&mut self, p: &Json) -> Result<Json, String> {
        let raw = p["address"].as_str().unwrap_or_default();
        let (prefix, address) = crate::images::scoped_expression(raw)?;
        if prefix.zip(p["image"].as_str()).is_some_and(|(a, b)| a != b) {
            return Err("Disassembly prefix and selected image disagree".into());
        }
        let images = self.navigation_images(prefix.or(p["image"].as_str()))?;
        let physical = address.is_empty() || address == "$pc";
        let address = if physical {
            self.snapshot.frame.address.clone()
        } else {
            address.into()
        };
        let exact = literal_address(&address).ok();
        let mut found = Vec::new();
        for image in &images {
            let addresses = if let Some(address) = exact {
                vec![address & !1]
            } else if physical {
                vec![]
            } else {
                match self.image_locations(image, &address) {
                    Ok(locations) => locations.iter().map(|l| l.address).collect(),
                    Err(error) if image_symbols::missing_location(&error) => vec![],
                    Err(error) => {
                        return Err(format!("Image {} symbol query failed: {error}", image.id));
                    }
                }
            };
            for address in addresses {
                if let Some(range) = image
                    .executable_ranges
                    .iter()
                    .find(|r| r.start <= address && address < r.end)
                {
                    found.push((
                        image.clone(),
                        address,
                        range.end.min(address.saturating_add(128)),
                    ));
                }
            }
        }
        if found.len() > 1
            && prefix.is_none()
            && p["image"].as_str().is_none()
            && let Some(current) = self
                .snapshot
                .image_state
                .as_ref()
                .and_then(|s| s.selected_frame.image.as_deref())
            && found.iter().any(|(i, _, _)| i.id == current)
        {
            found.retain(|(i, _, _)| i.id == current);
        }
        if found.len() != 1 {
            return Err("ELF disassembly needs one executable image/address; use @image:address or @image:function".into());
        }
        let (image, start, end) = found.pop().unwrap();
        let target_io = p["source"].as_str() != Some("elf")
            && self.snapshot.state == state::STOPPED
            && self
                .snapshot
                .image_state
                .as_ref()
                .and_then(|s| s.selected_frame.image.as_deref())
                == Some(image.id.as_str());
        let command = format!("-data-disassemble -s 0x{start:x} -e 0x{end:x} -- 0");
        let record = if target_io {
            self.mi(&command)?
        } else {
            self.with_image_metadata(&image, |gdb| gdb(&command))?
        };
        let instructions = record
            .data
            .field("asm_insns")
            .map(Value::items)
            .unwrap_or_default()
            .iter()
            .map(|i| {
                format!(
                    "{}  {}",
                    i.string("address"),
                    i.string("inst").replace('\t', "    ")
                )
            })
            .collect::<Vec<_>>();
        let last_known = physical && self.snapshot.state == state::RUNNING;
        self.snapshot.assembly = std::iter::once(if target_io {
            format!("GDB [{}] · selected stopped image", image.id)
        } else {
            format!(
                "ELF [{}] · {} · no target memory read",
                image.id,
                if last_known {
                    "last-known PC"
                } else {
                    "symbol image"
                }
            )
        })
        .chain(instructions)
        .collect();
        self.publish();
        Ok(
            json!({"assembly":self.snapshot.assembly,"image":image.id,"image_revision":image.content_revision,"source":if target_io{"gdb"}else{"elf"},"target_io":target_io,"last_known":last_known,"start":start,"end":end}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_dwarf_files_use_the_owning_compilation_record_and_reject_ambiguous_basenames() {
        let (events, _rx) = mpsc::sync_channel(64);
        let mut engine = Engine::new(Project::default(), events, Arc::new(AtomicBool::new(false)));
        let image = Loaded {
            id: "vm0".into(),
            elf: "unused.elf".into(),
            primary: false,
            domain: crate::images::Domain::Guest,
            bits: 32,
            machine: 40,
            has_debug_info: true,
            content_revision: "revision1".into(),
            executable_ranges: vec![],
            allocated_ranges: vec![],
        };
        engine.image_source_files.insert(
            "vm0:revision1".into(),
            vec![("/ci/project/src/main.c".into(), "D:/vm0/src/main.c".into())],
        );
        let frame = Frame {
            debug_file: "main.c".into(),
            file: "D:/bao/main.c".into(),
            ..Default::default()
        };
        assert_eq!(
            engine.owned_frame_source(&image, &frame),
            "@vm0:D:/vm0/src/main.c"
        );
        engine.snapshot.image_state = Some(crate::images::State {
            images: vec![image.clone()],
            ..Default::default()
        });
        engine.snapshot.breakpoints.push(Breakpoint {
            id: "7".into(),
            image: Some("vm0".into()),
            debug_file: "main.c".into(),
            file: "D:/bao/main.c".into(),
            ..Default::default()
        });
        engine.qualify_image_breakpoint_sources();
        assert_eq!(
            engine.snapshot.breakpoints[0].file,
            "@vm0:D:/vm0/src/main.c"
        );
        let mut later = image.clone();
        later.content_revision = "revision2".into();
        engine.image_source_files.insert(
            "vm0:revision2".into(),
            vec![(
                "/ci/project/other/main.c".into(),
                "D:/vm0/other/main.c".into(),
            )],
        );
        assert_eq!(
            engine.owned_frame_source(&later, &frame),
            "@vm0:D:/vm0/other/main.c"
        );
        engine
            .image_source_files
            .get_mut("vm0:revision1")
            .unwrap()
            .push((
                "/ci/project/extra/main.c".into(),
                "D:/vm0/extra/main.c".into(),
            ));
        assert!(engine.owned_frame_source(&image, &frame).is_empty());
        let frame = Frame {
            debug_file: r"src\main.c".into(),
            ..frame
        };
        assert_eq!(
            engine.owned_frame_source(&image, &frame),
            "@vm0:D:/vm0/src/main.c"
        );
        assert!(
            engine.gdb.is_none(),
            "source ownership does not select a physical inferior"
        );
    }
}
