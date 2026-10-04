//! Read-only memory access with explicit transport and running-state capability.
use super::*;
use crate::live_watch::{connect, read_only_transaction as transact, word};

fn integer(text: &str) -> Result<u64, String> {
    let token = text.split_whitespace().next().unwrap_or("");
    if let Some(hex) = token.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).map_err(|e| e.to_string())
    } else {
        token.parse::<u64>().map_err(|e| e.to_string())
    }
}

impl Engine {
    pub(super) fn read_memory_dump(&mut self, p: &Json) -> Result<Json, String> {
        let context = self.register_context();
        if let Some(expected) = p.get("context") {
            let expected: crate::registers::Context = serde_json::from_value(expected.clone())
                .map_err(|e| format!("Memory context: {e}"))?;
            if expected != context {
                return Err(
                    "Memory request belongs to an expired core, frame or stop context".into(),
                );
            }
        }
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
        if self.cancellation.load(Ordering::Relaxed) {
            return Err("Memory read cancelled".into());
        }
        let channel = p["channel"].as_str().unwrap_or("");
        let (base, bytes, target, endpoint, source) = if channel.is_empty() {
            self.stopped()?;
            let record = self.mi(&format!(
                "-data-read-memory-bytes {} {count}",
                mi::quote(address)
            ))?;
            let blocks = record
                .data
                .field("memory")
                .map(Value::items)
                .unwrap_or_default();
            let (base, bytes) = dump_blocks(blocks, count)?;
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
            if !self.memory_connections.contains_key(channel) {
                self.memory_connections
                    .insert(channel.into(), connect(&access.tcl_endpoint)?);
            }
            let command = format!("{} read_memory 0x{base:x} 8 {count}", word(&access.target));
            let text = match transact(self.memory_connections.get_mut(channel).unwrap(), &command) {
                Ok(text) => text,
                Err(error) => {
                    self.memory_connections.remove(channel);
                    return Err(error);
                }
            };
            let bytes = text
                .split_whitespace()
                .map(|token| {
                    integer(token).and_then(|value| {
                        u8::try_from(value).map_err(|_| {
                            "Memory response contains a value wider than one byte".into()
                        })
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            if bytes.len() != count as usize {
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
        if self.register_context() != context
            || (channel.is_empty() && self.snapshot.state != "STOPPED")
        {
            return Err("Target context or running state changed during the memory read".into());
        }
        if self.cancellation.load(Ordering::Relaxed) {
            return Err("Memory read cancelled".into());
        }
        // The caller owns the view cache. Never publish an unqualified memory
        // snapshot that could be applied after a core/frame selection changes.
        Ok(
            json!({"address":format!("0x{base:x}"),"bytes":bytes,"channel":channel,
            "target":target,"endpoint":endpoint,"source":source,"context":context,
            "state":self.snapshot.state,"atomic":false}),
        )
    }
    pub(super) fn memory_channels(&self) -> Result<Json, String> {
        let core = self.project.preference_core.as_deref().unwrap_or("default");
        Ok(
            json!({"core":core,"source":self.project.memory_access_source,"channels":self.project.memory_access.iter().map(|access| json!({"configuration":access,"available_for_core":access.cores.is_empty() || access.cores.iter().any(|name| name == core),"while_running_declared":access.while_running})).collect::<Vec<_>>()}),
        )
    }
    pub(super) fn read_memory_channel(&mut self, p: &Json) -> Result<Json, String> {
        let channel = p["channel"].as_str().unwrap_or("");
        if channel.is_empty() {
            return self.execute("peripheral_read", p);
        }
        if !matches!(self.snapshot.state.as_str(), "STOPPED" | "RUNNING") {
            return Err("Memory access requires a connected target".into());
        }
        let access = self
            .project
            .memory_access
            .iter()
            .find(|a| a.id == channel)
            .filter(|a| {
                a.cores.is_empty()
                    || self
                        .project
                        .preference_core
                        .as_ref()
                        .is_some_and(|name| a.cores.contains(name))
            })
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
        if self.cancellation.load(Ordering::Relaxed) {
            return Err("Memory read cancelled".into());
        }
        if !self.memory_connections.contains_key(channel) {
            self.memory_connections
                .insert(channel.into(), connect(&access.tcl_endpoint)?);
        }
        // 64-bit values use two 32-bit bus transactions, never an unsupported AP width.
        let width = bits.min(32);
        let count = bits / width;
        let command = format!(
            "{} read_memory 0x{address:x} {width} {count}",
            word(&access.target)
        );
        let result = transact(self.memory_connections.get_mut(channel).unwrap(), &command);
        let text = match result {
            Ok(text) => text,
            Err(error) => {
                self.memory_connections.remove(channel);
                return Err(error);
            }
        };
        let values = text
            .split_whitespace()
            .map(integer)
            .collect::<Result<Vec<_>, _>>()?;
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
        self.log(
            "diagnostic",
            format!(
                "Memory [{channel}] target={} endpoint={} source={} address=0x{address:x} bits={bits} value=0x{value:x}",
                access.target, access.tcl_endpoint, self.project.memory_access_source
            ),
        );
        Ok(
            json!({"value":value,"raw":crate::registers::RawValue::from_integer(u128::from(value),bits as u16)?,"address":address,"bits":bits,"channel":channel,"target":access.target,"endpoint":access.tcl_endpoint,"source":self.project.memory_access_source,"state":self.snapshot.state,"atomic":false}),
        )
    }

    pub(super) fn resolve_watch(&mut self, p: &Json) -> Result<Json, String> {
        self.stopped()?;
        let expression = p["expression"]
            .as_str()
            .ok_or("Watch expression is required")?;
        if !self.watch_names.iter().any(|name| name == expression) {
            return Err("Watch no longer exists".into());
        }
        let path: Vec<usize> = serde_json::from_value(p.get("path").cloned().unwrap_or(json!([])))
            .map_err(|_| "Invalid child path")?;
        if path.len() > 8 {
            return Err("Watch path is too deep".into());
        }
        let created = self.mi(&format!("-var-create - * {}", mi::quote(expression)))?;
        let root = created.data.string("name");
        let result = (|| {
            let mut node = created.data.clone();
            for index in path {
                let end = index.checked_add(1).ok_or("Child index overflow")?;
                let record = self.mi(&format!(
                    "-var-list-children --no-values {} {index} {end}",
                    mi::quote(&node.string("name"))
                ))?;
                node = record
                    .data
                    .field("children")
                    .and_then(|c| c.items().first())
                    .map(|c| c.field("child").unwrap_or(c).clone())
                    .ok_or("Watch child is unavailable")?;
            }
            if node.string("numchild").parse::<usize>().unwrap_or(0) > 0
                && !node.string("type").contains('*')
            {
                return Err("Expand the aggregate to read its visible scalar members".into());
            }
            let record = self.mi(&format!(
                "-var-info-path-expression {}",
                mi::quote(&node.string("name"))
            ))?;
            let expression = record.data.string("path_expr");
            let address = self.mi(&format!(
                "-data-evaluate-expression {}",
                mi::quote(&format!("(unsigned long long)&({expression})"))
            ))?;
            let address = integer(&address.data.string("value")).map_err(
                |_| "Expression has no stable memory address (register, bitfield or temporary)",
            )?;
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
                json!({"address":address,"bits":bits,"float":float,"signed":signed,"little_endian":little,"type":node.string("type"),"expression":expression}),
            )
        })();
        let _ = self.mi(&format!("-var-delete {}", mi::quote(&root)));
        result
    }
}

pub(super) fn literal_address(address: &str) -> Result<u64, String> {
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
        assert!(worker.join().unwrap().contains("read_memory 0x1000 8 4"));
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
