use super::*;
use crate::images;

impl Engine {
    pub(super) fn image_cache_key(&self) -> String {
        self.snapshot
            .image_state
            .as_ref()
            .map(images::State::cache_key)
            .unwrap_or_default()
    }

    /// A file changed between stops must invalidate typed operations too, even
    /// when the user has not requested another Source refresh yet.
    pub(super) fn guard_image_context(&mut self) -> Result<(), String> {
        self.guard_image_files()?;
        if self
            .snapshot
            .image_state
            .as_ref()
            .is_some_and(|s| s.selected_frame.state != "resolved")
        {
            return Err("Selected frame has unknown or ambiguous image identity; typed values and writes are unavailable".into());
        }
        Ok(())
    }
    pub(super) fn guard_image_files(&mut self) -> Result<(), String> {
        let Some(state) = &self.snapshot.image_state else {
            return Ok(());
        };
        if !state.symbols_loaded {
            return Err("Resident image symbols are not loaded in a connected session".into());
        }
        if !state.error.is_empty() {
            return Err(state.error.clone());
        }
        let error=state.images.iter().find_map(|loaded| {
            let definition=self.project.images.iter().find(|i|i.id==loaded.id).cloned().unwrap_or_else(||images::Image{id:loaded.id.clone(),elf:loaded.elf.clone(),..Default::default()});
            match images::changed(loaded,&definition) {
                Ok(false)=>None,Ok(true)=>Some(format!("ELF content changed for {}; symbols are stale. Verify firmware before reloading",loaded.id)),Err(e)=>Some(e)
            }
        });
        if let Some(error) = error {
            let state = self.snapshot.image_state.as_mut().unwrap();
            state.error = error.clone();
            state.symbol_epoch += 1;
            state.execution = images::Resolution {
                state: "stale".into(),
                detail: error.clone(),
                ..Default::default()
            };
            state.selected_frame = state.execution.clone();
            self.snapshot.locals.clear();
            self.snapshot.watches.clear();
            self.snapshot.frame.file.clear();
            self.snapshot.frame.function.clear();
            self.snapshot.frame.line = 0;
            self.snapshot.stack.clear();
            self.invalidate_register_boundary();
            return Err(error);
        }
        Ok(())
    }
    /// Validate every local image before opening the service or touching the target.
    pub(super) fn prepare_images(&mut self) -> Result<(), String> {
        self.image_locations.clear();
        self.image_source_files.clear();
        let images = images::inspect_project(&self.project)?;
        self.snapshot.image_state = (!self.project.images.is_empty()).then(|| images::State {
            images,
            symbol_epoch: self.snapshot.generation,
            ..Default::default()
        });
        Ok(())
    }

    pub(super) fn load_images(&mut self) -> Result<(), String> {
        let definitions = images::applicable(&self.project)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        for image in definitions {
            if image.elf != self.project.program.elf {
                let mut command = format!(
                    "add-symbol-file {} -o {}",
                    mi::quote(&portable_path(&image.elf)),
                    image.offset
                );
                for (section, address) in &image.sections {
                    command.push_str(&format!(" -s {} 0x{address:x}", mi::quote(section)));
                }
                self.console(&command)
                    .map_err(|e| format!("Load image {}: {e}", image.id))?;
            }
            // Per-image source rules belong to UI references and isolated
            // metadata queries. A physical GDB has only one global rule table.
        }
        if let Some(state) = &mut self.snapshot.image_state {
            state.symbols_loaded = true;
        }
        Ok(())
    }

    pub(super) fn image_inventory(&self) -> Result<Json, String> {
        let inventory = match &self.snapshot.image_state {
            Some(state) => state.clone(),
            None => images::State {
                images: images::inspect_project(&self.project)?,
                ..Default::default()
            },
        };
        Ok(
            json!({"context":self.register_context(),"state":inventory,"configured":self.project.images,
            "target_io":false,"running":self.snapshot.state == state::RUNNING}),
        )
    }

    pub(super) fn refresh_images(&mut self) -> Result<(), String> {
        let Some(mut state) = self.snapshot.image_state.take() else {
            return Ok(());
        };
        let changed = state.images.iter().find_map(|loaded| {
            let configured = self.project.images.iter().find(|i| i.id == loaded.id).cloned().unwrap_or_else(|| images::Image {
                id:loaded.id.clone(),elf:loaded.elf.clone(),..Default::default()
            });
            match images::changed(loaded, &configured) {
                Ok(false) => None,
                Ok(true) => Some(format!("ELF content changed for {}; symbols are stale. Verify the flashed image before explicitly reloading", loaded.id)),
                Err(error) => Some(error),
            }
        });
        if let Some(error) = changed {
            if state.error.is_empty() {
                self.invalidate_register_boundary();
                state.symbol_epoch += 1;
            }
            state.error = error;
            state.execution = images::Resolution {
                state: "stale".into(),
                detail: state.error.clone(),
                ..Default::default()
            };
            state.selected_frame = state.execution.clone();
            self.snapshot.image_state = Some(state);
            return Ok(());
        }
        let selected = self.snapshot.frame.address.clone();
        let frame_level = self.snapshot.frame.level;
        let thread = self.write_thread().ok();
        let physical = if frame_level == 0 {
            image_integer(&selected)
        } else {
            self.image_frame_register(thread.as_deref(), 0, "$pc")
        };
        let mode = self.image_frame_cpsr(thread.as_deref(), 0);
        state.program_cpsr = mode;
        let frame_mode = if frame_level == 0 {
            mode
        } else {
            self.image_frame_cpsr(thread.as_deref(), frame_level)
        };
        state.execution = physical
            .map(|pc| images::resolve(&state.images, pc, mode))
            .unwrap_or_default();
        state.selected_frame = image_integer(&selected)
            .map(|pc| images::resolve(&state.images, pc, frame_mode))
            .unwrap_or_default();
        self.snapshot.image_state = Some(state);
        Ok(())
    }
    fn image_frame_cpsr(&mut self, thread: Option<&str>, frame: u32) -> Option<u32> {
        self.image_frame_register(thread, frame, "$cpsr")
            .and_then(|n| u32::try_from(n).ok())
    }
    fn image_frame_register(
        &mut self,
        thread: Option<&str>,
        frame: u32,
        register: &str,
    ) -> Option<u64> {
        let thread = thread?;
        // MI option values are parsed as bare decimal integers by GDB. Quoting
        // a thread id makes real GDB reject it (the mock must not hide this).
        if thread.is_empty() || !thread.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        self.mi(&format!(
            "-data-evaluate-expression --thread {thread} --frame {frame} \"{register}\""
        ))
        .ok()
        .and_then(|r| image_integer(&r.data.string("value")))
    }
}

fn image_integer(text: &str) -> Option<u64> {
    let value = text.split_whitespace().next()?;
    value
        .strip_prefix("0x")
        .map(|n| u64::from_str_radix(n, 16).ok())
        .unwrap_or_else(|| value.parse().ok())
}
