//! Non-intrusive live watch via OpenOCD TCL interface.
//!
//! Resolves global variable addresses from the ELF symbol table, then polls
//! them through a separate TCP connection to the OpenOCD TCL port (default
//! 6666).  This bypasses GDB entirely, so the target can be running freely
//! while values are read — the foundation of live watch on multi-AP targets
//! where AHB-AP3 provides a non-intrusive system-bus window.

use crate::{
    config::LiveWatchConfig,
    logging::Stamp,
    session::Event,
};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    net::TcpStream,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

/// One-shot TCL command sender for use by the coordinator (e.g. sync.open).
/// Connects to the TCL endpoint, sends each command terminated by `\x1a`,
/// reads each response, then disconnects.
pub(crate) fn send_tcl_commands(endpoint: &str, commands: &[String]) -> Result<(), String> {
    let addr: std::net::SocketAddr = endpoint
        .parse()
        .map_err(|e| format!("Invalid TCL endpoint {endpoint}: {e}"))?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))
        .map_err(|e| format!("TCL connect to {endpoint}: {e}"))?;
    stream.set_read_timeout(Some(Duration::from_millis(2000))).ok();
    stream.set_write_timeout(Some(Duration::from_millis(2000))).ok();
    for cmd in commands {
        let pkt = format!("{}\x1a", cmd);
        stream.write_all(pkt.as_bytes()).map_err(|e| format!("TCL write: {e}"))?;
        let mut buf = Vec::with_capacity(256);
        let mut chunk = [0u8; 512];
        loop {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    buf.extend_from_slice(&chunk[..n]);
                    if buf.contains(&0x1a) {
                        break;
                    }
                    if buf.len() > 8192 {
                        return Err("TCL response too large".into());
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => break,
                Err(e) => return Err(format!("TCL read: {e}")),
            }
        }
    }
    Ok(())
}

/// Public handle returned by [`spawn`].  Dropping it cancels the poller.
pub struct LiveWatchHandle {
    pub events: Receiver<Event>,
    pub cancellation: Arc<AtomicBool>,
}

impl Drop for LiveWatchHandle {
    fn drop(&mut self) {
        self.cancellation.store(true, Ordering::Relaxed);
    }
}

/// Launch the live-watch background thread.  Returns `Err` if the ELF cannot
/// be parsed — the caller may proceed without live watch in that case.
pub fn spawn(config: &LiveWatchConfig, watch_names: Vec<String>) -> Result<LiveWatchHandle, String> {
    let symbols = SymbolTable::from_elf(&config.elf)?;
    let (events_tx, events_rx) = mpsc::sync_channel(64);
    let cancellation = Arc::new(AtomicBool::new(false));
    let cancel = cancellation.clone();
    let endpoint = config.tcl_endpoint.clone();
    let bus_target = config.bus_target.clone();
    let interval_ms = config.interval_ms;
    let resolved: Vec<(String, u64)> = watch_names
        .iter()
        .filter_map(|n| symbols.address(n).map(|a| (n.clone(), a)))
        .collect();
    let unresolved: Vec<String> = watch_names
        .into_iter()
        .filter(|n| !symbols.contains(n))
        .collect();
    thread::spawn(move || {
        let started = Instant::now();
        let mut poller = Poller {
            endpoint,
            bus_target,
            interval_ms,
            resolved,
            events: events_tx,
            cancellation: cancel,
            started,
        };
        for name in &unresolved {
            poller.log("live", format!("'{name}' not found in ELF symbol table; skipping"));
        }
        poller.run();
    });
    Ok(LiveWatchHandle {
        events: events_rx,
        cancellation,
    })
}

struct Poller {
    endpoint: String,
    bus_target: String,
    interval_ms: u64,
    resolved: Vec<(String, u64)>,
    events: SyncSender<Event>,
    cancellation: Arc<AtomicBool>,
    started: Instant,
}

impl Poller {
    fn run(&mut self) {
        let mut stream = match TcpStream::connect_timeout(
            &self.endpoint.parse().unwrap_or_else(|_| "127.0.0.1:6666".parse().unwrap()),
            Duration::from_secs(3),
        ) {
            Ok(s) => {
                let _ = s.set_read_timeout(Some(Duration::from_millis(500)));
                let _ = s.set_write_timeout(Some(Duration::from_millis(500)));
                s
            }
            Err(e) => {
                self.log("live", format!("TCL connect to {} failed: {e}", self.endpoint));
                return;
            }
        };
        self.log("live", format!("Connected to TCL {}; polling {} symbols on {}", self.endpoint, self.resolved.len(), self.bus_target));
        let mut last: HashMap<String, String> = HashMap::new();
        while !self.cancellation.load(Ordering::Relaxed) {
            if self.resolved.is_empty() {
                thread::sleep(Duration::from_millis(self.interval_ms.max(1000)));
                continue;
            }
            let _ = self.send_tcl(&mut stream, &format!("targets {}", self.bus_target));
            for (name, addr) in &self.resolved.clone() {
                if self.cancellation.load(Ordering::Relaxed) {
                    break;
                }
                let cmd = format!("mdw 0x{:x}", addr);
                match self.send_tcl(&mut stream, &cmd) {
                    Ok(text) => {
                        if let Some(value) = parse_mdw(&text) {
                            let changed = last.get(name).is_some_and(|v| v != &value);
                            last.insert(name.clone(), value.clone());
                            let mark = if changed { " *" } else { "" };
                            self.log("live", format!("{} @0x{:08x} = {}{}", name, addr, value, mark));
                        }
                    }
                    Err(e) => {
                        self.log("live", format!("{} read error: {e}", name));
                    }
                }
            }
            let sleep = self.interval_ms.min(60000).max(10);
            thread::sleep(Duration::from_millis(sleep));
        }
    }
    fn send_tcl(&self, stream: &mut TcpStream, command: &str) -> Result<String, String> {
        let pkt = format!("{}\x1a", command);
        stream.write_all(pkt.as_bytes()).map_err(|e| format!("TCL write: {e}"))?;
        let mut buf = Vec::with_capacity(256);
        let mut chunk = [0u8; 512];
        loop {
            match stream.read(&mut chunk) {
                Ok(0) => return Err("TCL connection closed".into()),
                Ok(n) => {
                    buf.extend_from_slice(&chunk[..n]);
                    if buf.contains(&0x1a) {
                        break;
                    }
                    if buf.len() > 8192 {
                        return Err("TCL response too large".into());
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    if buf.is_empty() {
                        return Err("TCL read timed out".into());
                    }
                    break;
                }
                Err(e) => return Err(format!("TCL read: {e}")),
            }
        }
        let text = String::from_utf8_lossy(&buf);
        Ok(text.replace('\x1a', "").trim().to_owned())
    }
    fn log(&self, channel: &str, text: String) {
        let stamp = Stamp::now();
        let elapsed = stamp.elapsed_ms(self.started);
        let _ = self.events.send(Event::Log {
            channel: channel.into(),
            text,
            timestamp: stamp.wall,
            elapsed_ms: elapsed,
        });
    }
}

/// Parse an OpenOCD `mdw` response line like `0x100209bc: 00002bcd`.
fn parse_mdw(text: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("0x").or_else(|| line.strip_prefix("0X")) {
            if let Some((_, val)) = rest.split_once(':') {
                let val = val.trim();
                if !val.is_empty() {
                    return Some(val.to_owned());
                }
            }
        }
        // Also handle bare hex lines without the address prefix
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[0].contains(':') {
            if let Some(word) = parts.get(1) {
                if word.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Some(format!("0x{}", word));
                }
            }
        }
    }
    None
}

// ---- ELF symbol table parser (ELF32 + ELF64, little-endian) ----

struct SymbolTable {
    symbols: HashMap<String, u64>,
}

impl SymbolTable {
    fn from_elf(path: &Path) -> Result<Self, String> {
        let data = fs::read(path).map_err(|e| format!("Read ELF {}: {e}", path.display()))?;
        if data.len() < 52 || &data[0..4] != b"\x7fELF" {
            return Err("Not an ELF file".into());
        }
        let is_64 = data[4] == 2;
        let is_le = data[5] == 1;
        if !is_le {
            return Err("Big-endian ELF not supported".into());
        }
        let symbols = if is_64 {
            parse_symbols_64(&data)?
        } else {
            parse_symbols_32(&data)?
        };
        Ok(Self { symbols })
    }
    fn address(&self, name: &str) -> Option<u64> {
        self.symbols.get(name).copied()
    }
    fn contains(&self, name: &str) -> bool {
        self.symbols.contains_key(name)
    }
}

fn read_u16(data: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([data[off], data[off + 1]])
}
fn read_u32(data: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
}
fn read_u64(data: &[u8], off: usize) -> u64 {
    u64::from_le_bytes([
        data[off], data[off + 1], data[off + 2], data[off + 3],
        data[off + 4], data[off + 5], data[off + 6], data[off + 7],
    ])
}

fn parse_symbols_32(data: &[u8]) -> Result<HashMap<String, u64>, String> {
    let e_shoff = read_u32(data, 32) as usize;
    let e_shentsize = read_u16(data, 46) as usize;
    let e_shnum = read_u16(data, 48) as usize;
    let sections = read_section_offsets(data, e_shoff, e_shentsize, e_shnum)?;
    let mut out = HashMap::new();
    for sec in &sections {
        if sec.sh_type == 2 || sec.sh_type == 11 {
            // SHT_SYMTAB or SHT_DYNSYM
            let strtab = sections.get(sec.sh_link).ok_or("Symbol string table missing")?;
            parse_symtab_32(data, sec, strtab, &mut out);
        }
    }
    Ok(out)
}
fn parse_symtab_32(data: &[u8], sec: &Section, strtab: &Section, out: &mut HashMap<String, u64>) {
    let entry = if sec.sh_entsize > 0 { sec.sh_entsize } else { 16 };
    let count = sec.sh_size / entry;
    for i in 0..count {
        let off = sec.sh_offset + i * entry;
        if off + 16 > data.len() {
            break;
        }
        let st_name = read_u32(data, off) as usize;
        let st_value = read_u32(data, off + 4) as u64;
        let st_info = data[off + 12];
        let st_shndx = read_u16(data, off + 14);
        let bind = st_info >> 4;
        let stype = st_info & 0xf;
        if (bind == 1 || bind == 2) && stype <= 3 && st_shndx != 0 {
            insert_symbol(data, strtab.sh_offset, st_name, st_value, out);
        }
    }
}

fn parse_symbols_64(data: &[u8]) -> Result<HashMap<String, u64>, String> {
    let e_shoff = read_u64(data, 40) as usize;
    let e_shentsize = read_u16(data, 58) as usize;
    let e_shnum = read_u16(data, 60) as usize;
    let sections = read_section_offsets(data, e_shoff, e_shentsize, e_shnum)?;
    let mut out = HashMap::new();
    for sec in &sections {
        if sec.sh_type == 2 || sec.sh_type == 11 {
            let strtab = sections.get(sec.sh_link).ok_or("Symbol string table missing")?;
            parse_symtab_64(data, sec, strtab, &mut out);
        }
    }
    Ok(out)
}
fn parse_symtab_64(data: &[u8], sec: &Section, strtab: &Section, out: &mut HashMap<String, u64>) {
    let entry = if sec.sh_entsize > 0 { sec.sh_entsize } else { 24 };
    let count = sec.sh_size / entry;
    for i in 0..count {
        let off = sec.sh_offset + i * entry;
        if off + 24 > data.len() {
            break;
        }
        let st_name = read_u32(data, off) as usize;
        let st_info = data[off + 4];
        let st_shndx = read_u16(data, off + 6);
        let st_value = read_u64(data, off + 8);
        let bind = st_info >> 4;
        let stype = st_info & 0xf;
        if (bind == 1 || bind == 2) && stype <= 3 && st_shndx != 0 {
            insert_symbol(data, strtab.sh_offset, st_name, st_value, out);
        }
    }
}

struct Section {
    sh_type: u32,
    sh_offset: usize,
    sh_size: usize,
    sh_link: usize,
    sh_entsize: usize,
}

fn read_section_offsets(data: &[u8], e_shoff: usize, e_shentsize: usize, e_shnum: usize) -> Result<Vec<Section>, String> {
    if e_shoff == 0 || e_shnum == 0 {
        return Err("No section headers in ELF".into());
    }
    let mut out = Vec::with_capacity(e_shnum);
    for i in 0..e_shnum {
        let off = e_shoff + i * e_shentsize;
        if off + 40 > data.len() {
            break;
        }
        out.push(Section {
            sh_type: read_u32(data, off + 4),
            sh_offset: read_u32(data, off + 16) as usize,
            sh_size: read_u32(data, off + 20) as usize,
            sh_link: read_u32(data, off + 24) as usize,
            sh_entsize: read_u32(data, off + 36) as usize,
        });
    }
    Ok(out)
}

fn insert_symbol(data: &[u8], strtab_off: usize, name_off: usize, addr: u64, out: &mut HashMap<String, u64>) {
    let start = strtab_off + name_off;
    if start >= data.len() {
        return;
    }
    let end = data[start..].iter().position(|&b| b == 0).unwrap_or(data.len() - start);
    if end == 0 {
        return;
    }
    if let Ok(name) = std::str::from_utf8(&data[start..start + end]) {
        if !name.is_empty() {
            out.entry(name.to_owned()).or_insert(addr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mdw_response_parsed() {
        assert_eq!(parse_mdw("0x100209bc: 00002bcd"), Some("00002bcd".into()));
        assert_eq!(parse_mdw("0x100209bc: 00002bcd\nfoo"), Some("00002bcd".into()));
        assert_eq!(parse_mdw("garbage"), None);
    }
}
