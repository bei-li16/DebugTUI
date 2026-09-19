//! Read-only memory access with explicit transport and running-state capability.
use super::*;
use crate::live_watch::{connect, transact, word};

fn integer(text: &str) -> Result<u64, String> {
    let token = text.split_whitespace().next().unwrap_or("");
    if let Some(hex) = token.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).map_err(|e| e.to_string())
    } else {
        token.parse::<u64>().map_err(|e| e.to_string())
    }
}
impl Engine {
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
                "Memory [{channel}] target={} address=0x{address:x} bits={bits} value=0x{value:x}",
                access.target
            ),
        );
        Ok(
            json!({"value":value,"address":address,"bits":bits,"channel":channel,"state":self.snapshot.state}),
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
