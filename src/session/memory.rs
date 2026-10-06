//! Read-only memory access with explicit transport and running-state capability.
use super::*;
use crate::live_watch::{connect, word};
use crate::registers::{
    Context,
    provenance::{ByteOrder, Route},
};
mod expression;
mod watch;
pub(super) use watch::WatchBinding;

struct ReadBoundary {
    context: Context,
    epoch: u64,
    state: String,
}

fn integer(text: &str) -> Result<u64, String> {
    let token = text.split_whitespace().next().unwrap_or("");
    if let Some(hex) = token.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).map_err(|e| e.to_string())
    } else {
        token.parse::<u64>().map_err(|e| e.to_string())
    }
}

impl Engine {
    fn drain_memory_notices(&mut self) {
        // Do not send MI queries here: AP reads may legitimately run while GDB
        // cannot service a stopped-context query. Consume already queued notices.
        while let Some(gdb) = &self.gdb {
            match gdb.records.try_recv() {
                Ok(incoming) => self.record(incoming),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.record(Incoming::Closed);
                    break;
                }
            }
        }
    }
    fn begin_memory_read(&mut self, p: &Json) -> Result<ReadBoundary, String> {
        self.register_value_access = None;
        self.drain_memory_notices();
        let context = self.register_context();
        if let Some(expected) = p.get("context") {
            let expected: Context = serde_json::from_value(expected.clone())
                .map_err(|e| format!("Memory context: {e}"))?;
            if expected != context {
                return Err(
                    "Memory request belongs to an expired core, frame or stop context".into(),
                );
            }
        }
        self.memory_read_not_cancelled()?;
        if let Some(epoch) = p.get("selection_epoch")
            && epoch.as_u64() != Some(self.snapshot.memory_selection_epoch)
        {
            return Err("Memory request belongs to an expired thread selection".into());
        }
        Ok(ReadBoundary {
            context,
            epoch: self.memory_context_epoch,
            state: self.snapshot.state.clone(),
        })
    }
    fn memory_read_not_cancelled(&self) -> Result<(), String> {
        if self.cancellation.load(Ordering::Relaxed) || self.read_cancel.load(Ordering::Relaxed) {
            Err("Memory read cancelled".into())
        } else {
            Ok(())
        }
    }
    fn finish_memory_read(&mut self, boundary: &ReadBoundary) -> Result<(), String> {
        self.drain_memory_notices();
        if self.register_context() != boundary.context
            || self.memory_context_epoch != boundary.epoch
            || self.snapshot.state != boundary.state
        {
            return Err("Target context or running state changed during the memory read".into());
        }
        self.memory_read_not_cancelled()
    }
    fn memory_transaction(
        &mut self,
        channel: &str,
        endpoint: &str,
        command: &str,
    ) -> Result<String, String> {
        if !self.memory_connections.contains_key(channel) {
            self.memory_connections
                .insert(channel.into(), connect(endpoint)?);
        }
        let mut progress = crate::live_watch::TransactionProgress::default();
        let result = crate::live_watch::read_only_transaction_tracked(
            self.memory_connections.get_mut(channel).unwrap(),
            command,
            &mut progress,
        );
        self.register_value_progress(&progress);
        if result.is_err() {
            self.memory_connections.remove(channel);
        }
        result
    }
    pub(super) fn read_memory_dump(&mut self, p: &Json) -> Result<Json, String> {
        let boundary = self.begin_memory_read(p)?;
        let context = &boundary.context;
        let count = p["count"].as_u64().ok_or("Memory byte count is required")?;
        if !(1..=4096).contains(&count) {
            return Err("Read 1..4096 memory bytes per request".into());
        }
        let address = p["address"].as_str().ok_or("Memory address is required")?;
        if address.is_empty() || address.len() > 256 || address.chars().any(char::is_control) {
            return Err("Invalid memory address".into());
        }
        if let Ok(base) = literal_address(address) {
            base.checked_add(count).ok_or("Memory address overflow")?;
        }
        let channel = memory_channel(p)?;
        let (base, bytes, target, endpoint, source) = if channel.is_empty() {
            self.stopped()?;
            let command = format!("-data-read-memory-bytes {} {count}", mi::quote(address));
            self.plan_register_value_access(
                Route::GdbMemory {
                    endpoint: self.connected_gdb_endpoint.clone(),
                    configured_endpoint: self.project.target.endpoint.clone(),
                    address: address.into(),
                    bits: (count * 8) as u16,
                    byte_order: ByteOrder::Little,
                },
                command.clone(),
            );
            let record = self.mi(&command)?;
            let blocks = record
                .data
                .field("memory")
                .map(Value::items)
                .unwrap_or_default();
            let (base, bytes) = dump_blocks(blocks, count)?;
            if literal_address(address).is_ok_and(|expected| expected != base) {
                return Err("Memory response belongs to a different address".into());
            }
            (
                base,
                bytes,
                context.core.clone(),
                self.project.target.endpoint.clone(),
                "GDB".into(),
            )
        } else {
            if !matches!(self.snapshot.state.as_str(), "STOPPED" | "RUNNING") {
                return Err("Memory access requires a connected target".into());
            }
            let access = self
                .project
                .memory_access
                .iter()
                .find(|access| access.id == channel)
                .filter(|access| access.cores.is_empty() || access.cores.contains(&context.core))
                .cloned()
                .ok_or("Memory channel is not available for this core")?;
            if self.snapshot.state == "RUNNING" && !access.while_running {
                return Err("This memory channel requires a stopped core".into());
            }
            let base = literal_address(address)?;
            base.checked_add(count).ok_or("Memory address overflow")?;
            let command = format!("{} read_memory 0x{base:x} 8 {count}", word(&access.target));
            self.plan_register_value_access(
                Route::TclMemory {
                    endpoint: access.tcl_endpoint.clone(),
                    target: access.target.clone(),
                    channel: channel.into(),
                    configuration_source: self.project.memory_access_source.clone(),
                    address: format!("0x{base:x}"),
                    bits: (count * 8) as u16,
                    bus_width: 8,
                    count: count as u16,
                    byte_order: ByteOrder::Little,
                    atomic: false,
                },
                command.clone(),
            );
            let text = self.memory_transaction(channel, &access.tcl_endpoint, &command)?;
            let bytes = text
                .split_whitespace()
                .map(|token| {
                    integer(token).and_then(|value| {
                        u8::try_from(value).map_err(|_| {
                            "Memory response contains a value wider than one byte".into()
                        })
                    })
                })
                .collect::<Result<Vec<_>, String>>();
            let bytes = match bytes {
                Ok(bytes) => bytes,
                Err(error) => {
                    self.memory_connections.remove(channel);
                    return Err(error);
                }
            };
            if bytes.len() != count as usize {
                self.memory_connections.remove(channel);
                return Err("Incomplete memory response".into());
            }
            (
                base,
                bytes,
                access.target,
                access.tcl_endpoint,
                self.project.memory_access_source.clone(),
            )
        };
        self.finish_memory_read(&boundary)?;
        // The caller owns the view cache. Never publish an unqualified memory
        // snapshot that could be applied after a core/frame selection changes.
        Ok(
            json!({"address":format!("0x{base:x}"),"bytes":bytes,"channel":channel,
            "target":target,"endpoint":endpoint,"source":source,"context":context,
            "state":self.snapshot.state,"atomic":false,"selection_epoch":self.snapshot.memory_selection_epoch,"access":self.register_value_access}),
        )
    }
    pub(super) fn memory_channels(&self) -> Result<Json, String> {
        let core = self.project.preference_core.as_deref().unwrap_or("default");
        Ok(
            json!({"core":core,"source":self.project.memory_access_source,"channels":self.project.memory_access.iter().map(|access| json!({"configuration":access,"available_for_core":access.cores.is_empty() || access.cores.iter().any(|name| name == core),"while_running_declared":access.while_running})).collect::<Vec<_>>()}),
        )
    }
    pub(super) fn read_memory_channel(&mut self, p: &Json) -> Result<Json, String> {
        let boundary = self.begin_memory_read(p)?;
        self.check_watch_binding(p, &boundary)?;
        let channel = memory_channel(p)?;
        if channel.is_empty() {
            return self.read_memory_gdb_scalar(p, &boundary);
        }
        if !matches!(self.snapshot.state.as_str(), "STOPPED" | "RUNNING") {
            return Err("Memory access requires a connected target".into());
        }
        let access = self
            .project
            .memory_access
            .iter()
            .find(|a| a.id == channel)
            .filter(|a| a.cores.is_empty() || a.cores.contains(&boundary.context.core))
            .cloned()
            .ok_or("Memory channel is not available for this core")?;
        if self.snapshot.state == "RUNNING" && !access.while_running {
            return Err("This memory channel requires a stopped core".into());
        }
        let address = p["address"].as_u64().ok_or("Memory address is required")?;
        let bits = p["bits"].as_u64().ok_or("Memory width is required")?;
        let little = p["little_endian"]
            .as_bool()
            .ok_or("Memory byte order is required")?;
        if !matches!(bits, 8 | 16 | 32 | 64) || !address.is_multiple_of(bits / 8) {
            return Err("Unsupported or unaligned memory width".into());
        }
        if address.checked_add(bits / 8).is_none() {
            return Err("Memory address overflow".into());
        }
        self.memory_read_not_cancelled()?;
        // 64-bit values use two 32-bit bus transactions, never an unsupported AP width.
        let width = bits.min(32);
        let count = bits / width;
        let command = format!(
            "{} read_memory 0x{address:x} {width} {count}",
            word(&access.target)
        );
        self.plan_register_value_access(
            crate::registers::provenance::Route::TclMemory {
                endpoint: access.tcl_endpoint.clone(),
                target: access.target.clone(),
                channel: channel.into(),
                configuration_source: self.project.memory_access_source.clone(),
                address: format!("0x{address:x}"),
                bits: bits as u16,
                bus_width: width as u16,
                count: count as u16,
                atomic: false,
                byte_order: if little {
                    crate::registers::provenance::ByteOrder::Little
                } else {
                    crate::registers::provenance::ByteOrder::Big
                },
            },
            command.clone(),
        );
        let text = self.memory_transaction(channel, &access.tcl_endpoint, &command)?;
        let values = text
            .split_whitespace()
            .map(integer)
            .collect::<Result<Vec<_>, _>>();
        let values = match values {
            Ok(values) => values,
            Err(error) => {
                self.memory_connections.remove(channel);
                return Err(error);
            }
        };
        if values.len() != count as usize
            || values.iter().any(|v| width < 64 && *v >= (1u64 << width))
        {
            self.memory_connections.remove(channel);
            return Err("Invalid memory response width/count".into());
        }
        let value = if bits == 64 {
            if little {
                values[0] | values[1] << 32
            } else {
                values[0] << 32 | values[1]
            }
        } else {
            values[0]
        };
        self.finish_memory_read(&boundary)?;
        self.log(
            "diagnostic",
            format!(
                "Memory [{channel}] target={} endpoint={} source={} address=0x{address:x} bits={bits} value=0x{value:x}",
                access.target, access.tcl_endpoint, self.project.memory_access_source
            ),
        );
        Ok(
            json!({"value":value,"raw":crate::registers::RawValue::from_integer(u128::from(value),bits as u16)?,"address":address,"bits":bits,"channel":channel,"target":access.target,"endpoint":access.tcl_endpoint,"source":self.project.memory_access_source,"state":self.snapshot.state,"atomic":false,"context":boundary.context,"selection_epoch":self.snapshot.memory_selection_epoch,"access":self.register_value_access}),
        )
    }

    fn read_memory_gdb_scalar(
        &mut self,
        p: &Json,
        boundary: &ReadBoundary,
    ) -> Result<Json, String> {
        self.stopped()?;
        let address = p["address"].as_u64().ok_or("Register address required")?;
        let bits = p["bits"].as_u64().ok_or("Register width required")?;
        let little = p["little_endian"]
            .as_bool()
            .ok_or("SVD byte order is not specified")?;
        if !matches!(bits, 8 | 16 | 32 | 64) || !address.is_multiple_of(bits / 8) {
            return Err("Unsupported or unaligned register width".into());
        }
        address
            .checked_add(bits / 8)
            .ok_or("Memory address overflow")?;
        let command = format!("-data-read-memory-bytes 0x{address:x} {}", bits / 8);
        self.plan_register_value_access(
            Route::GdbMemory {
                endpoint: self.connected_gdb_endpoint.clone(),
                configured_endpoint: self.project.target.endpoint.clone(),
                address: format!("0x{address:x}"),
                bits: bits as u16,
                byte_order: if little {
                    ByteOrder::Little
                } else {
                    ByteOrder::Big
                },
            },
            command.clone(),
        );
        let record = self.mi(&command)?;
        let blocks = record
            .data
            .field("memory")
            .map(Value::items)
            .unwrap_or_default();
        let (base, bytes) = dump_blocks(blocks, bits / 8)?;
        if base != address {
            return Err("Memory response belongs to a different address".into());
        }
        let value = bytes.iter().enumerate().fold(0u64, |value, (i, byte)| {
            value | u64::from(*byte) << (8 * if little { i } else { bytes.len() - 1 - i })
        });
        self.finish_memory_read(boundary)?;
        Ok(
            json!({"value":value,"raw":crate::registers::RawValue::from_integer(u128::from(value), bits as u16)?,
            "address":address,"bits":bits,"channel":"","target":boundary.context.core,
            "endpoint":self.project.target.endpoint,"source":"GDB","state":self.snapshot.state,
            "atomic":false,"context":boundary.context,"selection_epoch":self.snapshot.memory_selection_epoch,"access":self.register_value_access}),
        )
    }
}

fn memory_channel(p: &Json) -> Result<&str, String> {
    match p.get("channel") {
        None => Ok(""),
        Some(channel) => channel
            .as_str()
            .ok_or_else(|| "Memory channel must be a string; no implicit GDB fallback".into()),
    }
}

pub(crate) fn literal_address(address: &str) -> Result<u64, String> {
    let digits = address
        .strip_prefix("0x")
        .or_else(|| address.strip_prefix("0X"));
    let value = if let Some(digits) = digits {
        u64::from_str_radix(digits, 16)
    } else {
        address.parse()
    };
    value.map_err(|_| "Bus memory access requires a literal hexadecimal or decimal address".into())
}

fn dump_blocks(blocks: &[Value], count: u64) -> Result<(u64, Vec<u8>), String> {
    let first = blocks.first().ok_or("Incomplete memory response")?;
    let base = literal_address(&first.string("begin"))?;
    base.checked_add(count).ok_or("Memory address overflow")?;
    let mut bytes = Vec::with_capacity(count as usize);
    for block in blocks {
        if literal_address(&block.string("begin"))? != base + bytes.len() as u64 {
            return Err("Memory response contains a gap or overlapping blocks".into());
        }
        let contents = block.string("contents");
        if !contents.len().is_multiple_of(2)
            || !contents.bytes().all(|b| b.is_ascii_hexdigit())
            || contents.len() / 2 > count as usize - bytes.len()
        {
            return Err("Invalid memory response bytes".into());
        }
        for pair in contents.as_bytes().as_chunks::<2>().0 {
            bytes.push(u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap());
        }
    }
    if bytes.len() != count as usize {
        return Err("Incomplete memory response".into());
    }
    Ok((base, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn bus_read_reports_exact_64bit_value_and_actual_route_without_halt_or_target_selection() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap().to_string();
        let mut project = Project {
            memory_access_source: "profile:fixture.toml".into(),
            ..Default::default()
        };
        project.memory_access.push(crate::config::MemoryAccess {
            id: "ap".into(),
            target: "bus0".into(),
            tcl_endpoint: endpoint.clone(),
            while_running: true,
            ..Default::default()
        });
        let (events, _) = mpsc::sync_channel(512);
        let mut engine = Engine::new(project, events, Arc::new(AtomicBool::new(false)));
        engine.snapshot.state = "RUNNING".into();
        let listed = engine.memory_channels().unwrap();
        assert_eq!(listed["source"], "profile:fixture.toml");
        assert_eq!(listed["channels"][0]["available_for_core"], true);
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut packet = vec![];
            let mut byte = [0];
            loop {
                stream.read_exact(&mut byte).unwrap();
                if byte[0] == 0x1a {
                    break;
                }
                packet.push(byte[0]);
            }
            stream
                .write_all(b"__DEBUGTUI_RPC__0:0x76543210 0xfedcba98\x1a")
                .unwrap();
            String::from_utf8(packet).unwrap()
        });
        let result = engine
            .read_memory_channel(
                &json!({"channel":"ap","address":536870912,"bits":64,"little_endian":true}),
            )
            .unwrap();
        assert_eq!(result["raw"]["hex"], "0xfedcba9876543210");
        assert_eq!(result["target"], "bus0");
        assert_eq!(result["endpoint"], endpoint);
        assert_eq!(result["state"], "RUNNING");
        assert_eq!(result["atomic"], false);
        assert_eq!(result["access"]["phase"], "responded");
        assert_eq!(result["access"]["context"], result["context"]);
        assert_eq!(result["access"]["route"]["target"], "bus0");
        assert_eq!(result["access"]["route"]["bits"], 64);
        assert_eq!(result["access"]["route"]["bus_width"], 32);
        assert_eq!(result["access"]["route"]["count"], 2);
        assert!(
            result["access"]["completed_ms"].as_u64().unwrap()
                >= result["access"]["timestamp_ms"].as_u64().unwrap()
        );
        let command = worker.join().unwrap();
        assert!(command.contains("read_memory 0x20000000 32 2"));
        assert!(
            !command.contains("halt")
                && !command.contains("targets")
                && !command.contains("target current")
        );
    }

    fn memory_fixture(response: &'static [u8]) -> (String, thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap().to_string();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut packet = vec![];
            let mut byte = [0];
            loop {
                stream.read_exact(&mut byte).unwrap();
                if byte[0] == 0x1a {
                    break;
                }
                packet.push(byte[0]);
            }
            stream.write_all(response).unwrap();
            String::from_utf8(packet).unwrap()
        });
        (endpoint, worker)
    }

    fn memory_engine(endpoint: &str) -> Engine {
        let mut project = Project {
            preference_core: Some("core1".into()),
            memory_access_source: "project:fixture.toml".into(),
            ..Default::default()
        };
        project.memory_access.push(crate::config::MemoryAccess {
            id: "ap".into(),
            target: "soc.bus".into(),
            tcl_endpoint: endpoint.into(),
            while_running: true,
            cores: vec!["core1".into()],
            ..Default::default()
        });
        let (events, _) = mpsc::sync_channel(512);
        let mut engine = Engine::new(project, events, Arc::new(AtomicBool::new(false)));
        engine.snapshot.state = "RUNNING".into();
        engine
    }

    #[test]
    fn bus_dump_preserves_address_order_and_reports_non_atomic_explicit_core_route() {
        let (endpoint, worker) = memory_fixture(b"__DEBUGTUI_RPC__0:0x00 0x7f 0x80 0xff\x1a");
        let mut engine = memory_engine(&endpoint);
        let result=engine.read_memory_dump(&json!({"address":"0x100000008","count":4,"channel":"ap","context":engine.register_context()})).unwrap();
        assert_eq!(result["address"], "0x100000008");
        assert_eq!(result["bytes"], json!([0, 127, 128, 255]));
        assert_eq!(result["context"]["core"], "core1");
        assert_eq!(result["target"], "soc.bus");
        assert_eq!(result["endpoint"], endpoint);
        assert_eq!(result["source"], "project:fixture.toml");
        assert_eq!(result["atomic"], false);
        assert_eq!(result["access"]["phase"], "responded");
        assert_eq!(result["access"]["context"], result["context"]);
        assert_eq!(result["access"]["route"]["address"], "0x100000008");
        assert_eq!(result["access"]["route"]["bits"], 32);
        assert_eq!(result["access"]["route"]["bus_width"], 8);
        assert_eq!(result["access"]["route"]["count"], 4);
        assert!(engine.snapshot.memory.is_empty());
        let command = worker.join().unwrap();
        assert!(
            command.contains("soc.bus") && command.contains("read_memory 0x100000008 8 4"),
            "{command}"
        );
        assert!(
            !command.contains("halt")
                && !command.contains("targets")
                && !command.contains("target current")
        );
    }

    #[test]
    fn invalid_dump_context_limits_channels_and_capabilities_touch_no_transport() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut engine = memory_engine(&listener.local_addr().unwrap().to_string());
        for request in [
            json!({"address":"0x1000","count":0,"channel":"ap"}),
            json!({"address":"0x1000","count":4097,"channel":"ap"}),
            json!({"address":"0xffffffffffffffff","count":1,"channel":"ap"}),
            json!({"address":"$sp","count":4,"channel":"ap"}),
            json!({"address":"0x1000","count":4,"channel":"missing"}),
            json!({"address":"0x1000","count":4,"channel":"ap","context":{"session":0,"generation":0,"core":"core0","frame":0}}),
        ] {
            assert!(engine.read_memory_dump(&request).is_err());
            assert!(engine.memory_connections.is_empty());
        }
        engine.project.memory_access[0].while_running = false;
        assert!(
            engine
                .read_memory_dump(&json!({"address":"0x1000","count":4,"channel":"ap"}))
                .is_err()
        );
        engine.project.memory_access[0].while_running = true;
        engine.project.preference_core = Some("core0".into());
        assert!(
            engine
                .read_memory_dump(&json!({"address":"0x1000","count":4,"channel":"ap"}))
                .is_err()
        );
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn incomplete_bus_dump_is_an_error_without_gdb_fallback_or_retry() {
        let (endpoint, worker) = memory_fixture(b"__DEBUGTUI_RPC__0:0xaa\x1a");
        let mut engine = memory_engine(&endpoint);
        assert_eq!(
            engine
                .read_memory_dump(&json!({"address":"0x1000","count":4,"channel":"ap"}))
                .unwrap_err(),
            "Incomplete memory response"
        );
        assert!(engine.gdb.is_none());
        assert!(engine.snapshot.memory.is_empty());
        assert!(engine.memory_connections.is_empty());
        assert!(worker.join().unwrap().contains("read_memory 0x1000 8 4"));
    }

    // Inject into the real engine's MI receive queue only after the TCP fixture
    // observes dispatch. This does not depend on a sleep or stdout scheduling.
    fn notification_queue(engine: &mut Engine) -> mpsc::Sender<Incoming> {
        let mut child = Command::new("node")
            .args(["-e", "process.stdin.resume()"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let (sender, records) = mpsc::channel();
        engine.gdb = Some(Gdb {
            child,
            input,
            records,
            token: 0,
        });
        sender
    }

    #[test]
    fn bus_scalar_and_dump_discard_mid_read_mi_notifications_without_querying_or_halting() {
        for notices in [
            vec!["*running,thread-id=\"all\""],
            vec!["*stopped,reason=\"signal-received\",frame={level=\"0\"}"],
            vec!["=thread-selected,id=\"2\",frame={level=\"0\"}"],
            vec!["closed"],
            vec![
                "*running,thread-id=\"all\"",
                "*stopped,reason=\"signal-received\",frame={level=\"0\"}",
            ],
        ] {
            for dump in [false, true] {
                let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                let mut engine = memory_engine(&listener.local_addr().unwrap().to_string());
                engine.snapshot.state = "STOPPED".into();
                let sender = notification_queue(&mut engine);
                let dispatch_sender = sender.clone();
                let injected = notices.clone();
                let worker = thread::spawn(move || {
                    let (mut stream, _) = listener.accept().unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .unwrap();
                    let mut packet = vec![];
                    loop {
                        let mut byte = [0];
                        stream.read_exact(&mut byte).unwrap();
                        if byte[0] == 0x1a {
                            break;
                        }
                        packet.push(byte[0]);
                    }
                    for notice in injected {
                        dispatch_sender
                            .send(if notice == "closed" {
                                Incoming::Closed
                            } else {
                                Incoming::Record(mi::parse(notice).unwrap().unwrap())
                            })
                            .unwrap();
                    }
                    stream.write_all(b"__DEBUGTUI_RPC__0:0xaa\x1a").unwrap();
                    String::from_utf8(packet).unwrap()
                });
                let context = engine.register_context();
                let request = json!({"channel":"ap","address":if dump {json!("0x1000")} else {json!(4096)},
                    "count":1,"bits":8,"little_endian":true,"context":context});
                let result = if dump {
                    engine.read_memory_dump(&request)
                } else {
                    engine.read_memory_channel(&request)
                };
                assert_eq!(
                    result.unwrap_err(),
                    "Target context or running state changed during the memory read",
                    "{notices:?}/{dump}"
                );
                assert_eq!(
                    engine.register_value_access.as_ref().unwrap().phase,
                    crate::registers::provenance::Phase::Responded
                );
                assert_eq!(
                    engine.gdb.as_ref().unwrap().token,
                    0,
                    "AP boundary sent a hidden MI query"
                );
                assert!(engine.snapshot.memory.is_empty());
                if notices[0].starts_with("=thread-selected") {
                    assert_eq!(
                        engine.register_context(),
                        context,
                        "fixture must exercise a same-frame thread change"
                    );
                }
                let command = worker.join().unwrap();
                assert!(command.contains("read_memory 0x1000 8 1"));
                assert!(
                    !command.contains("halt")
                        && !command.contains("targets")
                        && !command.contains("resume")
                );
                drop(sender);
            }
        }
    }

    #[test]
    fn bus_reads_cancelled_after_dispatch_do_not_publish_a_valid_value() {
        for dump in [false, true] {
            for per_request in [false, true] {
                let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                let mut engine = memory_engine(&listener.local_addr().unwrap().to_string());
                let cancellation = if per_request {
                    engine.read_cancel.clone()
                } else {
                    engine.cancellation.clone()
                };
                let worker = thread::spawn(move || {
                    let (mut stream, _) = listener.accept().unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .unwrap();
                    loop {
                        let mut byte = [0];
                        stream.read_exact(&mut byte).unwrap();
                        if byte[0] == 0x1a {
                            break;
                        }
                    }
                    cancellation.store(true, Ordering::Relaxed);
                    stream.write_all(b"__DEBUGTUI_RPC__0:0xaa\x1a").unwrap();
                });
                let request = json!({"channel":"ap","address":if dump {json!("0x1000")} else {json!(4096)},"count":1,"bits":8,"little_endian":true});
                let result = if dump {
                    engine.read_memory_dump(&request)
                } else {
                    engine.read_memory_channel(&request)
                };
                assert_eq!(result.unwrap_err(), "Memory read cancelled");
                assert_eq!(
                    engine.register_value_access.as_ref().unwrap().phase,
                    crate::registers::provenance::Phase::Responded
                );
                assert!(engine.snapshot.memory.is_empty());
                worker.join().unwrap();
            }
        }
    }

    #[test]
    fn scalar_expired_context_and_pre_dispatch_cancel_touch_no_transport() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut engine = memory_engine(&listener.local_addr().unwrap().to_string());
        let mut context = engine.register_context();
        context.core = "core0".into();
        for channel in ["", "ap"] {
            let request = json!({"channel":channel,"address":4096,"bits":32,"little_endian":true,"context":context});
            assert!(
                engine
                    .read_memory_channel(&request)
                    .unwrap_err()
                    .contains("expired core")
            );
            assert!(engine.register_value_access.is_none());
        }
        engine.cancellation.store(true, Ordering::Relaxed);
        assert_eq!(
            engine
                .read_memory_channel(
                    &json!({"channel":"ap","address":4096,"bits":32,"little_endian":true})
                )
                .unwrap_err(),
            "Memory read cancelled"
        );
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert!(engine.memory_connections.is_empty());
    }

    #[test]
    fn failed_generated_bus_read_reconnects_only_on_the_next_explicit_request() {
        for dump in [false, true] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let mut engine = memory_engine(&listener.local_addr().unwrap().to_string());
            let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let dispatched = calls.clone();
            let server = thread::spawn(move || {
                for reply in [None, Some(b"__DEBUGTUI_RPC__0:0xaa\x1a".as_slice())] {
                    let (mut stream, _) = listener.accept().unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .unwrap();
                    let mut packet = vec![];
                    loop {
                        let mut byte = [0];
                        stream.read_exact(&mut byte).unwrap();
                        if byte[0] == 0x1a {
                            break;
                        }
                        packet.push(byte[0]);
                    }
                    assert!(
                        String::from_utf8(packet)
                            .unwrap()
                            .contains("read_memory 0x1000 8 1")
                    );
                    dispatched.fetch_add(1, Ordering::SeqCst);
                    if let Some(reply) = reply {
                        stream.write_all(reply).unwrap();
                    }
                }
            });
            let request = json!({"channel":"ap","address":if dump {json!("0x1000")} else {json!(4096)},"count":1,"bits":8,"little_endian":true});
            let first = if dump {
                engine.read_memory_dump(&request)
            } else {
                engine.read_memory_channel(&request)
            };
            assert!(first.unwrap_err().contains("connection closed"));
            assert!(engine.memory_connections.is_empty());
            assert_eq!(
                calls.load(Ordering::SeqCst),
                1,
                "failed request was retried automatically"
            );
            let second = if dump {
                engine.read_memory_dump(&request)
            } else {
                engine.read_memory_channel(&request)
            }
            .unwrap();
            assert_eq!(
                if dump {
                    second["bytes"][0].clone()
                } else {
                    second["value"].clone()
                },
                json!(170)
            );
            assert_eq!(second["access"]["phase"], "responded");
            assert_eq!(calls.load(Ordering::SeqCst), 2);
            assert!(engine.gdb.is_none());
            server.join().unwrap();
        }
    }

    #[test]
    fn malformed_bus_payload_drops_connection_without_retry_or_gdb_fallback() {
        for payload in [
            b"__DEBUGTUI_RPC__0:nan\x1a".as_slice(),
            b"__DEBUGTUI_RPC__0:0x100\x1a".as_slice(),
        ] {
            for dump in [false, true] {
                let (endpoint, worker) = memory_fixture(payload);
                let mut engine = memory_engine(&endpoint);
                let request = json!({"channel":"ap","address":if dump {json!("0x1000")} else {json!(4096)},"count":1,"bits":8,"little_endian":true});
                let result = if dump {
                    engine.read_memory_dump(&request)
                } else {
                    engine.read_memory_channel(&request)
                };
                assert!(result.is_err());
                assert!(engine.memory_connections.is_empty());
                assert!(engine.gdb.is_none());
                assert!(worker.join().unwrap().contains("read_memory 0x1000 8 1"));
            }
        }
    }

    #[test]
    fn gdb_dump_requires_complete_contiguous_valid_bytes_without_truncating_high_addresses() {
        let parse_blocks = |contents: &str| {
            mi::parse(&format!("1^done,memory={contents}"))
                .unwrap()
                .unwrap()
                .data
                .field("memory")
                .unwrap()
                .items()
                .to_vec()
        };
        let blocks = parse_blocks(
            r#"[{begin="0x100000008",contents="007f"},{begin="0x10000000a",contents="80ff"}]"#,
        );
        assert_eq!(
            dump_blocks(&blocks, 4).unwrap(),
            (0x100000008, vec![0, 127, 128, 255])
        );
        for contents in [
            r#"[{begin="0x1000",contents="00"}]"#,
            r#"[{begin="0x1000",contents="00"},{begin="0x1000",contents="7f80ff"}]"#,
            r#"[{begin="0x1000",contents="00"},{begin="0x1002",contents="7f80ff"}]"#,
            r#"[{begin="0x1000",contents="007f80fg"}]"#,
            r#"[{begin="0x1000",contents="007f80f"}]"#,
            r#"[{begin="0xffffffffffffffff",contents="007f80ff"}]"#,
        ] {
            assert!(
                dump_blocks(&parse_blocks(contents), 4).is_err(),
                "{contents}"
            );
        }
    }
}
