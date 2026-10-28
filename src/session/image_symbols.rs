//! Resolve locations in a single local objfile. No physical target is selected,
//! halted or queried here; only the final exact address reaches physical GDB.
use super::*;
use crate::images::{Image, Loaded};

#[derive(Clone, Debug, Serialize)]
pub(super) struct Location {
    pub address: u64,
    pub function: String,
    pub file: String,
    pub line: u32,
}
pub(super) fn missing_location(error: &str) -> bool {
    if error.contains("; parser shutdown:") {
        return false;
    }
    (error.starts_with("Function ") && error.contains("not defined"))
        || error.starts_with("No source file named ")
        || error.starts_with("No line ")
        || (error.starts_with("No symbol ") && error.contains("in current context"))
}

/// Replay only the declarative architecture choice. Other init commands may
/// connect/reset/write the physical target and never belong in an inspector.
pub(super) fn architecture(project: &Project) -> Option<&str> {
    project.gdb.init.iter().rev().find_map(|line| {
        let value = line.trim().strip_prefix("set architecture ")?.trim();
        (!value.is_empty()
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_:-".contains(&b)))
        .then_some(value)
    })
}

impl Engine {
    pub(super) fn image_location_inventory(&mut self, p: &Json) -> Result<Json, String> {
        let id = p["image"].as_str().ok_or("Image id is required")?;
        let location = p["location"].as_str().ok_or("Location is required")?;
        let images = if self
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
        let image = images
            .into_iter()
            .find(|i| i.id == id)
            .ok_or("Image is not configured for this core")?;
        if self.job.is_none() {
            self.job = Some(crate::process::Job::new()?);
        }
        let locations = self.image_locations(&image, location)?;
        Ok(
            json!({"image":id,"image_revision":image.content_revision,"locations":locations,"target_io":false,"context":self.register_context()}),
        )
    }
    pub(super) fn image_definition(&self, loaded: &Loaded) -> Image {
        self.project
            .images
            .iter()
            .find(|i| i.id == loaded.id)
            .cloned()
            .unwrap_or_else(|| Image {
                id: loaded.id.clone(),
                elf: loaded.elf.clone(),
                source_root: self.project.program.source_root.clone(),
                ..Default::default()
            })
    }
    pub(super) fn image_locations(
        &mut self,
        loaded: &Loaded,
        location: &str,
    ) -> Result<Vec<Location>, String> {
        let key = format!("{}:{}:{location}", loaded.id, loaded.content_revision);
        if let Some(result) = self.image_locations.get(&key) {
            return result.clone();
        }
        let definition = self.image_definition(loaded);
        let result = self.query_image_locations(&definition, loaded, location);
        if self.image_locations.len() >= 512 {
            self.image_locations.clear();
        }
        if result.is_ok() || result.as_ref().is_err_and(|e| missing_location(e)) {
            self.image_locations.insert(key, result.clone());
        }
        result
    }
    fn query_image_locations(
        &self,
        image: &Image,
        loaded: &Loaded,
        location: &str,
    ) -> Result<Vec<Location>, String> {
        if location.is_empty() || location.len() > 1024 || location.chars().any(char::is_control) {
            return Err("Breakpoint location must be a nonempty single-line string of at most 1024 characters".into());
        }
        if location.starts_with('*') {
            return Err("Use a literal address after '*' for an exact image breakpoint".into());
        }
        let mut gdb = guest::Inspector::offline(
            &self.project,
            self.job.as_ref().ok_or("GDB process job missing")?,
            self.logs.clone(),
        )?;
        let mut query = |command: &str| {
            gdb.request(command, |_, _| {
                Err("Offline image resolver cannot read target memory".into())
            })
        };
        query("-gdb-set pagination off")?;
        query("-gdb-set confirm off")?;
        query("-gdb-set may-call-functions off")?;
        query(&format!(
            "-file-exec-and-symbols {}",
            mi::quote(&portable_path(&image.elf))
        ))?;
        if let Some(architecture) = architecture(&self.project) {
            query(&format!("-gdb-set architecture {architecture}"))?;
        }
        for map in crate::source::gdb_maps(crate::images::source_maps(&self.project, image)) {
            query(&format!(
                "-interpreter-exec console {}",
                mi::quote(&format!(
                    "set substitute-path {} {}",
                    mi::quote(&map.from),
                    mi::quote(&portable_path(&map.to))
                ))
            ))?;
        }
        let reply = query(&format!("-break-insert -d {}", mi::quote(location)));
        // Lookup errors are local to this objfile. Caller may try another image.
        let result = reply.and_then(|reply| {
            let bp = reply
                .data
                .field("bkpt")
                .ok_or("GDB omitted the offline breakpoint")?;
            let entries = bp
                .field("locations")
                .map(Value::items)
                .map(|a| a.iter().collect::<Vec<_>>())
                .unwrap_or_else(|| vec![bp]);
            let mut linked = image.clone();
            linked.offset = 0;
            linked.sections.clear();
            let linked = crate::images::inspect(&linked, false)?;
            let mut locations = Vec::new();
            for value in entries {
                let value = value.field("bkpt").unwrap_or(value);
                let Some(address) = literal_address(&value.string("addr")).ok() else {
                    continue;
                };
                let normalized = address & !1;
                let Some(original) = linked
                    .executable_ranges
                    .iter()
                    .find(|r| r.start <= normalized && normalized < r.end)
                else {
                    continue;
                };
                let runtime = loaded
                    .executable_ranges
                    .iter()
                    .find(|r| r.section == original.section)
                    .ok_or("Image executable section disappeared")?;
                let relocated = runtime
                    .start
                    .checked_add(normalized - original.start)
                    .ok_or("Breakpoint relocation overflow")?;
                locations.push(Location {
                    address: relocated,
                    function: value.string("func"),
                    file: {
                        let full = value.string("fullname");
                        if full.is_empty() {
                            value.string("file")
                        } else {
                            full
                        }
                    },
                    line: value.string("line").parse().unwrap_or(0),
                });
            }
            locations.sort_by_key(|l| l.address);
            locations.dedup_by_key(|l| l.address);
            Ok(locations)
        });
        // Always close the isolated parser normally, including missing names.
        gdb.finish_result(result)
    }

    /// Returns (exact GDB location, image binding). A unique current-frame image
    /// is the default for a name found in more than one resident objfile.
    pub(super) fn resolve_image_breakpoint(
        &mut self,
        location: &str,
        requested: Option<&str>,
    ) -> Result<(String, Option<String>), String> {
        let (qualified, location) = crate::images::scoped_expression(location)?;
        if qualified.zip(requested).is_some_and(|(a, b)| a != b) {
            return Err("Location prefix and selected image disagree".into());
        }
        let requested = qualified.or(requested);
        let Some(state) = &self.snapshot.image_state else {
            if requested.is_some() {
                return Err("Image breakpoints require configured resident images".into());
            }
            return Ok((location.into(), None));
        };
        let images = state.images.clone();
        let current = state.selected_frame.image.clone();
        self.guard_image_files()?;
        if let Some(id) = requested
            && !images.iter().any(|i| i.id == id)
        {
            return Err(format!("Image {id} is not configured for this core"));
        }
        let explicit = location
            .strip_prefix('*')
            .map(literal_address)
            .transpose()?;
        let mut matches = Vec::new();
        let mut errors = Vec::new();
        for image in images
            .iter()
            .filter(|i| requested.is_none_or(|id| id == i.id))
        {
            let locations = if let Some(address) = explicit {
                if image
                    .executable_ranges
                    .iter()
                    .any(|r| r.start <= address && address < r.end)
                {
                    vec![Location {
                        address,
                        function: String::new(),
                        file: String::new(),
                        line: 0,
                    }]
                } else {
                    vec![]
                }
            } else {
                match self.image_locations(image, location) {
                    Ok(found) => found,
                    Err(error) => {
                        if !missing_location(&error) {
                            return Err(format!("Image {} symbol query failed: {error}", image.id));
                        }
                        errors.push(format!("{}: {error}", image.id));
                        vec![]
                    }
                }
            };
            if !locations.is_empty() {
                matches.push((image.id.clone(), locations));
            }
        }
        if matches.len() > 1
            && requested.is_none()
            && let Some(id) = current
            && matches.iter().any(|(image, _)| image == &id)
        {
            matches.retain(|(image, _)| image == &id);
        }
        if matches.len() != 1 {
            if matches.is_empty() && explicit.is_some() && requested.is_none() {
                return Ok((location.into(), None));
            }
            return Err(format!(
                "Location '{location}' requires one image; candidates: {}. Select an image or exact address. {}",
                matches
                    .iter()
                    .map(|(id, _)| id.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                errors.join("; ")
            ));
        }
        let (id, locations) = matches.pop().unwrap();
        if locations.len() != 1 {
            return Err(format!(
                "Image {id} has {} code locations for '{location}'; choose an exact address: {}",
                locations.len(),
                locations
                    .iter()
                    .map(|l| format!("0x{:x}", l.address))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let address = locations[0].address;
        // A physical address breakpoint cannot distinguish overlapping VM address
        // spaces. Never label it image-specific without such a backend identity.
        if images
            .iter()
            .filter(|i| {
                i.executable_ranges
                    .iter()
                    .any(|r| r.start <= address && address < r.end)
            })
            .count()
            > 1
        {
            return Err("Breakpoint address belongs to overlapping images; a backend address-space identity is required".into());
        }
        Ok((format!("*0x{address:x}"), Some(id)))
    }
}

#[cfg(test)]
mod architecture_tests {
    use super::*;

    #[test]
    fn inspectors_inherit_only_a_standalone_architecture_declaration() {
        let mut project = Project::default();
        project.gdb.init = vec![
            "monitor reset halt".into(),
            "set architecture armv8-r".into(),
            "target remote 127.0.0.1:3333".into(),
        ];
        assert_eq!(architecture(&project), Some("armv8-r"));
        project.gdb.init.push("set architecture auto".into());
        assert_eq!(architecture(&project), Some("auto"));
        project.gdb.init = vec!["set architecture armv8-r; monitor reset halt".into()];
        assert!(architecture(&project).is_none());
    }
}
