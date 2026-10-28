//! Saved Guest inspection does not replace the physical inferior or its registers.
//! An auxiliary, bundled GDB consumes an immutable register snapshot through the
//! local read-only RSP adapter. Only the owning worker can service its RAM reads.
use super::*;
use crate::images::{Image, SavedContext};
mod control;
mod rsp;
pub(super) use control::ReturnOperation;

#[derive(Clone, Debug, Serialize)]
struct Saved {
    sequence: u32,
    cpu: u32,
    vm: u32,
    vcpu: u32,
    registers: [u32; 17],
    edge: u32,
    syndrome: u32,
    frame_address: u32,
}

fn decode(bytes: &[u8], expected: &SavedContext) -> Result<Saved, String> {
    if bytes.len() != 112 {
        return Err("Incomplete Bao diagnostic snapshot".into());
    }
    let words: Vec<u32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| u32::from_le_bytes(*b))
        .collect();
    if words[0] != 0x42474331 || words[1] != 1 || words[2] != 112 {
        return Err(
            "Bao diagnostic ABI magic/version/size mismatch; rebuild matching diagnostic firmware"
                .into(),
        );
    }
    if words[3] & 1 != 0 || words[4] != 1 {
        return Err("Guest context is not fully saved, or VM_ENTRY has started; no Guest frame is available".into());
    }
    if (words[5], words[6], words[7]) != (expected.cpu, expected.vm, expected.vcpu) {
        return Err("Saved Guest CPU/VM/vCPU identity differs from the image binding".into());
    }
    if !matches!(
        words[24] & 31,
        0x10 | 0x11 | 0x12 | 0x13 | 0x17 | 0x1b | 0x1f
    ) || !matches!(words[25], 1 | 2)
    {
        return Err("Saved Guest mode or exception edge is invalid".into());
    }
    if !expected
        .memory
        .iter()
        .any(|m| m.contains(u64::from(words[21]), 4))
    {
        return Err(
            "Saved Guest SP is outside configured memory; refusing an invented stack".into(),
        );
    }
    Ok(Saved {
        sequence: words[3],
        cpu: words[5],
        vm: words[6],
        vcpu: words[7],
        registers: words[8..25].try_into().unwrap(),
        edge: words[25],
        syndrome: words[26],
        frame_address: words[27],
    })
}

impl Engine {
    pub(super) fn guest_read(
        &mut self,
        config: &SavedContext,
        address: u64,
        count: u64,
    ) -> Result<Vec<u8>, String> {
        self.drain_memory_notices();
        self.stopped()?;
        if self.cancellation.load(Ordering::Relaxed) || self.read_cancel.load(Ordering::Relaxed) {
            return Err("Guest inspection cancelled".into());
        }
        if !(1..=1024).contains(&count) || address.checked_add(count).is_none() {
            return Err("Invalid Guest memory read".into());
        }
        let core = self.project.preference_core.as_deref().unwrap_or("default");
        let target = self
            .project
            .registers
            .targets
            .get(core)
            .ok_or("Saved Guest inspection requires an explicit physical core target")?
            .clone();
        let access = self
            .project
            .memory_access
            .iter()
            .find(|a| {
                a.id == config.channel && (a.cores.is_empty() || a.cores.iter().any(|c| c == core))
            })
            .cloned()
            .ok_or("Saved Guest memory channel is unavailable on this core")?;
        let command = format!(
            "if {{[{} curstate] ne \"halted\"}} {{error \"Physical core is not halted\"}}; set __dt_guest_bytes [{} read_memory 0x{address:x} 8 {count}]; if {{[{} curstate] ne \"halted\"}} {{error \"Physical core resumed during Guest read\"}}; set __dt_guest_bytes",
            crate::live_watch::word(&target),
            crate::live_watch::word(&access.target),
            crate::live_watch::word(&target)
        );
        self.log(
            "guest-memory>",
            format!("{}: {command}", access.tcl_endpoint),
        );
        let text = self.memory_transaction(&config.channel, &access.tcl_endpoint, &command)?;
        let bytes = text
            .split_whitespace()
            .map(|word| {
                let n = if let Some(h) = word.strip_prefix("0x") {
                    u64::from_str_radix(h, 16)
                } else {
                    word.parse()
                }
                .map_err(|e| e.to_string())?;
                u8::try_from(n).map_err(|_| "Guest memory response exceeds byte width".to_string())
            })
            .collect::<Result<Vec<_>, String>>()?;
        if bytes.len() != count as usize {
            return Err("Incomplete Guest memory read".into());
        }
        Ok(bytes)
    }

    pub(super) fn guest_context(&mut self, p: &Json) -> Result<Json, String> {
        self.stopped()?;
        let image_id = p["image"].as_str().ok_or("Guest image id is required")?;
        let image: Image = crate::images::applicable(&self.project)
            .into_iter()
            .find(|i| i.id == image_id)
            .cloned()
            .ok_or("Guest image is not bound to this core")?;
        let config = image
            .saved_context
            .as_ref()
            .ok_or(
                "Image has no saved context provider; configure the optional Bao diagnostic ABI",
            )?
            .clone();
        let physical = self.register_context();
        let state = self
            .snapshot
            .image_state
            .as_ref()
            .ok_or("No resident images")?;
        if !state.error.is_empty() || state.program_cpsr.is_none_or(|c| c & 31 != 0x1a) {
            return Err(
                "Saved Guest inspection requires current, verified stopped Bao Hyp context".into(),
            );
        }
        let loaded = state
            .images
            .iter()
            .find(|i| i.id == image_id)
            .ok_or("Guest symbols are not loaded")?
            .clone();
        if loaded.machine != 40 || loaded.bits != 32 || !loaded.has_debug_info {
            return Err("Saved Guest DWARF inspection requires ARM32 ELF debug information".into());
        }
        if std::fs::read(&image.elf).map_err(|e| e.to_string())?.get(5) != Some(&1) {
            return Err("Bao v1 saved registers currently require a little-endian ELF".into());
        }
        let frame = p.get("level").and_then(Json::as_u64).unwrap_or(0);
        if frame > 31 {
            return Err("Guest frame level must be 0..31".into());
        }
        let expressions = p
            .get("expressions")
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default();
        if expressions.len() > 32 {
            return Err("At most 32 Guest expressions per inspection".into());
        }
        for expression in &expressions {
            memory::expression::validate(
                expression.as_str().ok_or("Guest expression must be text")?,
            )?;
        }
        // The lease prevents other in-process core workers from changing target
        // control while auxiliary GDB is unwinding. curstate and sequence checks
        // also reject an external resume or an incomplete exception boundary.
        let services = crate::debug_access::for_project(&self.project)?;
        let _leases = services
            .iter()
            .map(|s| s.acquire(false))
            .collect::<Result<Vec<_>, _>>()?;
        let address = self
            .mi(&format!(
                "-data-evaluate-expression {}",
                mi::quote(&format!(
                    "(unsigned long)&{}[{}]",
                    config.symbol, config.index
                ))
            ))?
            .data
            .string("value");
        let address = literal_address(address.split_whitespace().next().unwrap_or(""))?;
        let bytes = self.guest_read(&config, address, 112)?;
        let saved = decode(&bytes, &config)?;
        let resolution = crate::images::resolve(
            std::slice::from_ref(&loaded),
            u64::from(saved.registers[15]),
            Some(saved.registers[16]),
        );
        if resolution.image.as_deref() != Some(image_id) {
            return Err("Saved Guest PC/mode does not identify its configured image".into());
        }
        if self.guest_read(&config, address, 112)? != bytes {
            return Err("Saved Guest context changed while being captured".into());
        }
        let mut inspector = Inspector::new(
            &self.project,
            self.job.as_ref().ok_or("Process job missing")?,
            saved.registers,
            self.logs.clone(),
        )?;
        let endpoint = inspector.bridge.as_ref().unwrap().endpoint.clone();
        let maps = crate::source::gdb_maps(crate::images::source_maps(&self.project, &image));
        let architecture = image_symbols::architecture(&self.project).map(str::to_owned);
        let mut reads = 0u64;
        let mut read_bytes = 0u64;
        let (stack, locals, values) = {
            let mut query = |command: &str| {
                inspector.request(command, |a, n| {
                    reads += 1;
                    read_bytes += n;
                    if reads > 512 || read_bytes > 65536 {
                        return Err("Guest inspection memory budget exceeded".into());
                    }
                    if !config.memory.iter().any(|r| r.contains(a, n)) {
                        return Err(
                            "Guest memory read is outside configured code/RAM ranges".into()
                        );
                    }
                    self.guest_read(&config, a, n)
                })
            };
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
            for map in maps {
                query(&format!(
                    "-interpreter-exec console {}",
                    mi::quote(&format!(
                        "set substitute-path {} {}",
                        mi::quote(&map.from),
                        mi::quote(&portable_path(&map.to))
                    ))
                ))?;
            }
            // This is a local synthetic endpoint, never the physical OpenOCD port.
            query(&format!("-target-select remote {endpoint}"))?;
            let frames = query("-stack-list-frames 0 31")?;
            let stack: Vec<Frame> = frames
                .data
                .field("stack")
                .map(Value::items)
                .unwrap_or_default()
                .iter()
                .filter_map(|v| v.field("frame"))
                .map(Frame::from_mi)
                .collect();
            if !stack.iter().any(|f| u64::from(f.level) == frame) {
                return Err("Guest stack does not contain the requested recoverable frame".into());
            }
            query(&format!("-stack-select-frame {frame}"))?;
            let locals = query("-stack-list-variables --all-values")?;
            let locals: Vec<Variable> = locals
                .data
                .field("variables")
                .map(Value::items)
                .unwrap_or_default()
                .iter()
                .take(128)
                .map(|v| Variable {
                    name: v.string("name"),
                    value: v.string("value"),
                    ..Default::default()
                })
                .collect();
            let mut values = Vec::new();
            for expression in expressions {
                let text = expression.as_str().unwrap();
                let value = match query(&format!("-data-evaluate-expression {}", mi::quote(text))) {
                    Ok(r) => json!({"expression":text,"value":r.data.string("value")}),
                    Err(e) => json!({"expression":text,"error":e}),
                };
                values.push(value);
            }
            query("-target-disconnect")?;
            query("-gdb-exit")?;
            (stack, locals, values)
        };
        inspector.gdb.wait_for_exit()?;
        if self.guest_read(&config, address, 112)? != bytes || self.register_context() != physical {
            return Err("Saved Guest context or physical stop changed during inspection".into());
        }
        if crate::images::changed(&loaded, &image)? {
            return Err("Guest ELF changed during inspection; results discarded".into());
        }
        Ok(
            json!({"image":image.id,"image_revision":loaded.content_revision,"context":physical,
            "origin":"bao_v1_saved_guest","read_only":true,"physical_context_unchanged":true,
            "snapshot_address":format!("0x{address:x}"),"saved":saved,"stack":stack,
            "selected_frame":stack.iter().find(|f|u64::from(f.level)==frame),"locals":locals,"expressions":values,
            "memory":{"channel":config.channel,"reads":reads,"bytes":read_bytes,"atomic":false},
            "limitations":"Only captured r0-r15/CPSR are available. Optimized-out values and unavailable floating-point registers are not synthesized."}),
        )
    }
}

#[cfg(test)]
mod tests;

pub(super) struct Inspector {
    gdb: Gdb,
    bridge: Option<rsp::Bridge>,
    logs: Logs,
    namespace: &'static str,
}
impl Inspector {
    fn new(
        project: &Project,
        job: &crate::process::Job,
        registers: [u32; 17],
        logs: Logs,
    ) -> Result<Self, String> {
        let bridge = rsp::Bridge::new(registers, logs.clone())?;
        Self::spawn(project, job, Some(bridge), logs, "guest")
    }
    pub(super) fn offline(
        project: &Project,
        job: &crate::process::Job,
        logs: Logs,
    ) -> Result<Self, String> {
        Self::spawn(project, job, None, logs, "symbol")
    }
    pub(super) fn globals(
        project: &Project,
        job: &crate::process::Job,
        pc: Option<u32>,
        cpsr: Option<u32>,
        logs: Logs,
    ) -> Result<Self, String> {
        // No invented SP or register file. Only already-observed PC/CPSR are
        // exposed; every other register is explicitly unavailable in RSP.
        let mut registers = [None; 17];
        registers[15] = pc;
        registers[16] = cpsr;
        Self::observed(project, job, registers, logs)
    }
    /// An immutable read-only view of registers observed in one real physical
    /// GDB frame. Missing/unwind-unavailable registers stay unavailable.
    pub(super) fn observed(
        project: &Project,
        job: &crate::process::Job,
        registers: [Option<u32>; 17],
        logs: Logs,
    ) -> Result<Self, String> {
        let bridge = rsp::Bridge::partial(registers, logs.clone())?;
        Self::spawn(project, job, Some(bridge), logs, "global")
    }
    pub(super) fn endpoint(&self) -> Result<String, String> {
        self.bridge
            .as_ref()
            .map(|b| b.endpoint.clone())
            .ok_or_else(|| "Offline GDB has no target endpoint".into())
    }
    fn spawn(
        project: &Project,
        job: &crate::process::Job,
        bridge: Option<rsp::Bridge>,
        logs: Logs,
        namespace: &'static str,
    ) -> Result<Self, String> {
        let mut c = Command::new(&project.gdb.executable);
        c.args(["-nx", "-q", "--interpreter=mi2"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        hidden(&mut c);
        tool_environment(&mut c, &project.gdb.env, &project.gdb.unset_env);
        let mut child = c
            .spawn()
            .map_err(|e| format!("Start read-only Guest GDB: {e}"))?;
        job.attach(&mut child)?;
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        log_reader(
            child.stderr.take().unwrap(),
            "guest-gdb-error",
            logs.clone(),
        );
        let (tx, records) = mpsc::sync_channel(256);
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = Vec::new();
            loop {
                line.clear();
                match (&mut reader).take(1024 * 1024).read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(n) if n >= 1024 * 1024 => break,
                    Ok(_) => (),
                }
                if let Ok(Some(record)) = mi::parse(&String::from_utf8_lossy(&line))
                    && tx.send(Incoming::Record(record)).is_err()
                {
                    return;
                }
            }
            let _ = tx.send(Incoming::Closed);
        });
        Ok(Self {
            gdb: Gdb {
                child,
                input,
                records,
                token: 0,
            },
            bridge,
            logs,
            namespace,
        })
    }
    pub(super) fn request(
        &mut self,
        command: &str,
        mut read: impl FnMut(u64, u64) -> Result<Vec<u8>, String>,
    ) -> Result<Record, String> {
        self.gdb.token += 1;
        let token = self.gdb.token;
        writeln!(self.gdb.input, "{token}{command}")
            .and_then(|_| self.gdb.input.flush())
            .map_err(|e| {
                format!(
                    "Read-only {} GDB input closed during {command}: {e}; exit {:?}",
                    self.namespace,
                    self.gdb.child.try_wait().ok().flatten()
                )
            })?;
        self.logs.push(
            &format!("{}-mi>", self.namespace),
            format!("{token}{command}"),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            while let Some(Ok(request)) = self.bridge.as_ref().map(|b| b.reads.try_recv()) {
                let result = read(request.address, request.count);
                let _ = request.reply.send(result);
            }
            match self.gdb.records.recv_timeout(Duration::from_millis(5)) {
                Ok(Incoming::Record(record)) => {
                    self.logs.push(
                        &format!("{}-mi<", self.namespace),
                        serde_json::to_string(&record).unwrap_or_default(),
                    );
                    if record.kind == '&'
                        && (record.data.text().contains("internal-error:")
                            || record.data.text().contains("virtual memory exhausted"))
                    {
                        return Err(format!(
                            "Read-only {} GDB backend failed: {}",
                            self.namespace,
                            record.data.text().chars().take(2048).collect::<String>()
                        ));
                    }
                    if record.kind == '^' && record.token == Some(token) {
                        return if record.class == "error" {
                            Err(record.data.string("msg"))
                        } else {
                            Ok(record)
                        };
                    }
                }
                Ok(Incoming::Closed) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("Read-only Guest GDB exited".into());
                }
                _ => (),
            }
        }
        Err(format!("Read-only Guest GDB timed out: {command}"))
    }
    pub(super) fn finish(&mut self) -> Result<(), String> {
        self.request("-gdb-exit", |_, _| {
            Err("Offline symbol query cannot read memory".into())
        })?;
        self.gdb.wait_for_exit()
    }
    pub(super) fn finish_result<T>(&mut self, result: Result<T, String>) -> Result<T, String> {
        match (result, self.finish()) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(error), Ok(())) => Err(error),
            (Err(error), Err(cleanup)) => Err(format!("{error}; parser shutdown: {cleanup}")),
            (Ok(_), Err(error)) => Err(format!("Parser shutdown failed: {error}")),
        }
    }
}
