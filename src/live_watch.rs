//! Optional OpenOCD raw scalar monitoring. No GDB or single-core dependency.
use crate::{config::LiveWatchConfig, logging::Stamp, session::Event};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};

pub(crate) fn connect(endpoint: &str) -> Result<TcpStream, String> {
    let addresses = endpoint
        .to_socket_addrs()
        .map_err(|e| format!("Invalid TCL endpoint {endpoint}: {e}"))?;
    let mut error = format!("No address for TCL endpoint {endpoint}");
    for addr in addresses.take(4) {
        match TcpStream::connect_timeout(&addr, Duration::from_millis(500)) {
            Ok(s) => {
                s.set_read_timeout(Some(Duration::from_millis(500)))
                    .map_err(|e| e.to_string())?;
                s.set_write_timeout(Some(Duration::from_millis(500)))
                    .map_err(|e| e.to_string())?;
                return Ok(s);
            }
            Err(e) => error = format!("TCL connect to {endpoint}: {e}"),
        }
    }
    Err(error)
}
// Quote one Tcl word, including arbitrary configured command scripts.
pub(crate) fn word(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '\\' | '"' | '$' | '[' | ']' => {
                out.push('\\');
                out.push(c);
            }
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}
pub(crate) const RPC_MARKER: &str = "__DEBUGTUI_RPC__";
/// OpenOCD also forwards TCL return text to GDB target output. Keep tagged RPC
/// echoes (and the separate newline some versions emit) out of the Console.
#[derive(Default)]
pub(crate) struct RpcEcho {
    newline: bool,
}
impl RpcEcho {
    pub(crate) fn internal(&mut self, text: &str) -> bool {
        if text.starts_with(RPC_MARKER) {
            self.newline = true;
            return true;
        }
        let echo = self.newline && text.trim().is_empty();
        self.newline = false;
        echo
    }
}
pub(crate) fn transact(stream: &mut TcpStream, command: &str) -> Result<String, String> {
    if command.contains('\x1a') {
        return Err("TCL command contains a frame terminator".into());
    }
    // RPC normally returns only text, not Tcl's return code. Explicitly frame it.
    let packet = format!(
        "set __dt_code [catch {} __dt_value]; format \"{RPC_MARKER}%d:%s\" $__dt_code $__dt_value\x1a",
        word(command)
    );
    stream
        .write_all(packet.as_bytes())
        .map_err(|e| format!("TCL write: {e}"))?;
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if Instant::now() >= deadline {
            return Err("TCL response timed out".into());
        }
        match stream.read(&mut byte) {
            Ok(0) => return Err("TCL connection closed before complete response".into()),
            Ok(_) => {
                if byte[0] == 0x1a {
                    break;
                }
                buf.push(byte[0]);
                if buf.len() > 8192 {
                    return Err("TCL response too large".into());
                }
            }
            Err(e) => return Err(format!("TCL read: {e}")),
        }
    }
    let text = String::from_utf8(buf).map_err(|e| format!("Invalid TCL response: {e}"))?;
    let text = text.strip_prefix(RPC_MARKER).unwrap_or(&text);
    let (code, value) = text.split_once(':').ok_or("Missing TCL response status")?;
    if code != "0" {
        return Err(format!("TCL command failed ({code}): {value}"));
    }
    Ok(value.trim().to_owned())
}
pub(crate) fn send_tcl_commands(
    endpoint: &str,
    commands: &[String],
    cancellation: &AtomicBool,
) -> Result<(), String> {
    if cancellation.load(Ordering::Relaxed) {
        return Err("Synchronization cancelled".into());
    }
    let mut stream = connect(endpoint)?;
    for cmd in commands {
        if cancellation.load(Ordering::Relaxed) {
            return Err("Synchronization cancelled".into());
        }
        transact(&mut stream, cmd)?;
    }
    Ok(())
}
pub struct LiveWatchHandle {
    pub events: Receiver<Event>,
    pub cancellation: Arc<AtomicBool>,
}
impl Drop for LiveWatchHandle {
    fn drop(&mut self) {
        self.cancellation.store(true, Ordering::Relaxed);
    }
}

pub fn spawn(
    config: &LiveWatchConfig,
    watch_names: Vec<String>,
) -> Result<LiveWatchHandle, String> {
    let symbols = SymbolTable::from_elf(&config.elf)?;
    let (tx, rx) = mpsc::sync_channel(64);
    let cancellation = Arc::new(AtomicBool::new(false));
    let cancel = cancellation.clone();
    let config = config.clone();
    thread::spawn(move || {
        let started = Instant::now();
        let log = |text: String| {
            let stamp = Stamp::now();
            let _ = tx.try_send(Event::Log {
                channel: "live".into(),
                text,
                elapsed_ms: stamp.elapsed_ms(started),
                timestamp: stamp.wall,
            });
        };
        let resolved: Vec<_> = watch_names
            .iter()
            .filter_map(|n| match symbols.symbols.get(n) {
                Some(s) => Some((n.clone(), *s)),
                None => {
                    log(format!(
                        "'{n}' is not a unique 8/16/32-bit global object; live read skipped"
                    ));
                    None
                }
            })
            .collect();
        if resolved.is_empty() {
            return;
        }
        let mut stream = None;
        let mut last = HashMap::new();
        let mut failure = None;
        while !cancel.load(Ordering::Relaxed) {
            let result = (|| -> Result<(), String> {
                if stream.is_none() {
                    stream = Some(connect(&config.tcl_endpoint)?);
                }
                for (name, symbol) in &resolved {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    // Target-specific command does not change OpenOCD's global target.
                    let command = format!(
                        "{} read_memory 0x{:x} {} 1",
                        word(&config.bus_target),
                        symbol.address,
                        symbol.size * 8
                    );
                    let text = transact(stream.as_mut().unwrap(), &command)?;
                    let value = parse_value(&text)
                        .ok_or_else(|| format!("Invalid memory value for {name}: {text}"))?;
                    if last.insert(name.clone(), value) != Some(value) {
                        log(format!(
                            "{name} @0x{:x} = 0x{value:x} (raw {}-bit)",
                            symbol.address,
                            symbol.size * 8
                        ));
                    }
                }
                Ok(())
            })();
            match result {
                Ok(()) => {
                    failure = None;
                    wait(&cancel, config.interval_ms.clamp(10, 60_000));
                }
                Err(e) => {
                    stream = None;
                    if failure.as_ref() != Some(&e) {
                        log(e.clone());
                        failure = Some(e);
                    }
                    wait(&cancel, 1000);
                }
            }
        }
    });
    Ok(LiveWatchHandle {
        events: rx,
        cancellation,
    })
}
fn wait(cancel: &AtomicBool, ms: u64) {
    let deadline = Instant::now() + Duration::from_millis(ms);
    while !cancel.load(Ordering::Relaxed) && Instant::now() < deadline {
        thread::sleep(
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(25)),
        );
    }
}
fn parse_value(text: &str) -> Option<u64> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else {
        text.parse().ok()
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Symbol {
    address: u64,
    size: u64,
}
struct SymbolTable {
    symbols: HashMap<String, Symbol>,
}
impl SymbolTable {
    fn from_elf(path: &Path) -> Result<Self, String> {
        let data = fs::read(path).map_err(|e| format!("Read ELF {}: {e}", path.display()))?;
        Self::parse(&data)
    }
    fn parse(data: &[u8]) -> Result<Self, String> {
        if data.get(..4) != Some(b"\x7fELF") {
            return Err("Not an ELF file".into());
        }
        let is64 = match data.get(4) {
            Some(1) => false,
            Some(2) => true,
            _ => return Err("Invalid ELF class".into()),
        };
        if data.get(5) != Some(&1) {
            return Err("Live Watch requires little-endian ELF".into());
        }
        let (offset, stride, count) = if is64 {
            (
                number(data, 40, 8)?,
                number(data, 58, 2)?,
                number(data, 60, 2)?,
            )
        } else {
            (
                number(data, 32, 4)?,
                number(data, 46, 2)?,
                number(data, 48, 2)?,
            )
        };
        if offset == 0 || count == 0 || stride < (if is64 { 64 } else { 40 }) {
            return Err("Invalid or unsupported ELF section headers".into());
        }
        let section_data = range(
            data,
            offset,
            stride.checked_mul(count).ok_or("ELF section overflow")?,
        )?;
        let mut sections = Vec::new();
        for i in 0..count {
            let s = range(section_data, i * stride, stride)?;
            sections.push(if is64 {
                Section {
                    kind: number(s, 4, 4)?,
                    offset: number(s, 24, 8)?,
                    size: number(s, 32, 8)?,
                    link: number(s, 40, 4)?,
                    stride: number(s, 56, 8)?,
                }
            } else {
                Section {
                    kind: number(s, 4, 4)?,
                    offset: number(s, 16, 4)?,
                    size: number(s, 20, 4)?,
                    link: number(s, 24, 4)?,
                    stride: number(s, 36, 4)?,
                }
            });
        }
        let mut symbols = HashMap::new();
        let mut ambiguous = std::collections::HashSet::new();
        for sec in &sections {
            if !matches!(sec.kind, 2 | 11) {
                continue;
            }
            if sec.stride < (if is64 { 24 } else { 16 }) || sec.size % sec.stride != 0 {
                return Err("Invalid ELF symbol entry size".into());
            }
            let table = range(data, sec.offset, sec.size)?;
            let strings = sections
                .get(usize::try_from(sec.link).map_err(|_| "ELF link overflow")?)
                .ok_or("ELF string table missing")?;
            if strings.kind != 3 {
                return Err("Invalid ELF string table".into());
            }
            let strings = range(data, strings.offset, strings.size)?;
            for i in 0..sec.size / sec.stride {
                let s = range(table, i * sec.stride, sec.stride)?;
                let name = number(s, 0, 4)?;
                let (info, index, address, size) = if is64 {
                    (
                        number(s, 4, 1)?,
                        number(s, 6, 2)?,
                        number(s, 8, 8)?,
                        number(s, 16, 8)?,
                    )
                } else {
                    (
                        number(s, 12, 1)?,
                        number(s, 14, 2)?,
                        number(s, 4, 4)?,
                        number(s, 8, 4)?,
                    )
                };
                if !matches!(info >> 4, 1 | 2)
                    || info & 15 != 1
                    || index == 0
                    || index >= 0xff00
                    || !matches!(size, 1 | 2 | 4)
                {
                    continue;
                }
                let name = usize::try_from(name)
                    .ok()
                    .and_then(|n| strings.get(n..))
                    .ok_or("ELF symbol name outside string table")?;
                let end = name
                    .iter()
                    .position(|&b| b == 0)
                    .ok_or("Unterminated ELF symbol name")?;
                let name =
                    std::str::from_utf8(&name[..end]).map_err(|_| "Invalid ELF symbol name")?;
                if name.is_empty() {
                    continue;
                }
                let value = Symbol { address, size };
                if let Some(old) = symbols.insert(name.to_owned(), value)
                    && old != value
                {
                    ambiguous.insert(name.to_owned());
                }
            }
        }
        for name in ambiguous {
            symbols.remove(&name);
        }
        Ok(Self { symbols })
    }
}
struct Section {
    kind: u64,
    offset: u64,
    size: u64,
    link: u64,
    stride: u64,
}
fn range(data: &[u8], offset: u64, size: u64) -> Result<&[u8], String> {
    let end = offset.checked_add(size).ok_or("ELF offset overflow")?;
    let start = usize::try_from(offset).map_err(|_| "ELF offset overflow")?;
    let end = usize::try_from(end).map_err(|_| "ELF offset overflow")?;
    data.get(start..end)
        .ok_or_else(|| "Truncated ELF data".into())
}
fn number(data: &[u8], offset: u64, size: u64) -> Result<u64, String> {
    Ok(range(data, offset, size)?
        .iter()
        .enumerate()
        .fold(0, |n, (i, b)| n | ((*b as u64) << (8 * i))))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn elf(is64: bool) -> Vec<u8> {
        let header = if is64 { 64 } else { 52 };
        let stride = if is64 { 64 } else { 40 };
        let entry = if is64 { 24 } else { 16 };
        let sym = header + stride * 3;
        let strings = sym + entry * 2;
        let mut data = vec![0; strings + 9];
        data[..6].copy_from_slice(&[0x7f, b'E', b'L', b'F', if is64 { 2 } else { 1 }, 1]);
        let mut put = |offset: usize, value: u64, size: usize| {
            data[offset..offset + size].copy_from_slice(&value.to_le_bytes()[..size]);
        };
        put(
            if is64 { 40 } else { 32 },
            header as u64,
            if is64 { 8 } else { 4 },
        );
        put(if is64 { 58 } else { 46 }, stride as u64, 2);
        put(if is64 { 60 } else { 48 }, 3, 2);
        for (index, kind, offset, size, link, entsize) in
            [(1, 2, sym, entry * 2, 2, entry), (2, 3, strings, 9, 0, 0)]
        {
            let s = header + index * stride;
            put(s + 4, kind, 4);
            put(
                s + if is64 { 24 } else { 16 },
                offset as u64,
                if is64 { 8 } else { 4 },
            );
            put(
                s + if is64 { 32 } else { 20 },
                size as u64,
                if is64 { 8 } else { 4 },
            );
            put(s + if is64 { 40 } else { 24 }, link, 4);
            put(
                s + if is64 { 56 } else { 36 },
                entsize as u64,
                if is64 { 8 } else { 4 },
            );
        }
        let s = sym + entry;
        put(s, 1, 4);
        put(s + if is64 { 4 } else { 12 }, 0x11, 1);
        put(s + if is64 { 6 } else { 14 }, 1, 2);
        put(
            s + if is64 { 8 } else { 4 },
            if is64 { 0x1_2000_0000 } else { 0x2000_0000 },
            if is64 { 8 } else { 4 },
        );
        put(s + if is64 { 16 } else { 8 }, 4, if is64 { 8 } else { 4 });
        data[strings..].copy_from_slice(b"\0counter\0");
        data
    }
    #[test]
    fn elf32_and_elf64_global_objects_and_bounds() {
        for is64 in [false, true] {
            let data = elf(is64);
            let symbols = SymbolTable::parse(&data).unwrap();
            assert_eq!(
                symbols.symbols["counter"],
                Symbol {
                    address: if is64 { 0x1_2000_0000 } else { 0x2000_0000 },
                    size: 4
                }
            );
            for len in 0..data.len() {
                assert!(
                    SymbolTable::parse(&data[..len]).is_err(),
                    "accepted truncation {len}"
                );
            }
        }
    }
    #[test]
    fn truncated_elf_never_panics() {
        for class in [1, 2, 3] {
            for len in 0..256 {
                let mut d = vec![0xff; len];
                for (i, b) in b"\x7fELF".iter().chain([class, 1].iter()).enumerate() {
                    if i < len {
                        d[i] = *b;
                    }
                }
                assert!(SymbolTable::parse(&d).is_err());
            }
        }
    }
    #[test]
    fn rpc_requires_complete_success_response() {
        for (response, ok) in [
            ("0:42\x1a", true),
            ("__DEBUGTUI_RPC__0:42\x1a", true),
            ("1:invalid target\x1a", false),
            ("0:partial", false),
            ("unframed\x1a", false),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let worker = thread::spawn(move || {
                let (mut s, _) = listener.accept().unwrap();
                let mut b = [0];
                while s.read_exact(&mut b).is_ok() && b[0] != 0x1a {}
                s.write_all(response.as_bytes()).unwrap();
            });
            let mut client = connect(&addr.to_string()).unwrap();
            assert_eq!(transact(&mut client, "test").is_ok(), ok);
            worker.join().unwrap();
        }
    }
    #[test]
    fn scalar_values_and_quoting() {
        assert_eq!(parse_value("0xFF"), Some(255));
        assert_eq!(parse_value("42"), Some(42));
        assert_eq!(parse_value("1 2"), None);
        assert_eq!(word("a; $x [b]"), "\"a; \\$x \\[b\\]\"");
    }
    #[test]
    fn rpc_echo_filter_preserves_real_target_messages_and_errors() {
        let mut echo = RpcEcho::default();
        assert!(!echo.internal("target application message\n"));
        assert!(echo.internal("__DEBUGTUI_RPC__0:0x123\n"));
        assert!(echo.internal("\n"));
        assert!(!echo.internal("\n"));
        assert!(echo.internal("__DEBUGTUI_RPC__1:AP failed"));
        assert!(!echo.internal("HardFault in application\n"));
        assert!(!echo.internal("\n"));
        assert!(!echo.internal("0:ordinary target data"));
    }
    #[test]
    fn live_poller_reconnects_and_never_selects_global_target() {
        let file = std::env::temp_dir().join(format!("debugtui-live-{}.elf", std::process::id()));
        fs::write(&file, elf(false)).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let config = LiveWatchConfig {
            tcl_endpoint: format!("localhost:{}", listener.local_addr().unwrap().port()),
            bus_target: "AHB_3".into(),
            interval_ms: 10,
            elf: file.clone(),
        };
        let server = thread::spawn(move || {
            for value in [1, 2] {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut packet = Vec::new();
                let mut byte = [0];
                while stream.read_exact(&mut byte).is_ok() && byte[0] != 0x1a {
                    packet.push(byte[0]);
                }
                let command = String::from_utf8(packet).unwrap();
                assert!(command.contains("read_memory 0x20000000 32 1"));
                assert!(!command.contains("targets "));
                stream
                    .write_all(format!("0:0x{value:x}\x1a").as_bytes())
                    .unwrap();
            }
        });
        let handle = spawn(&config, vec!["counter".into()]).unwrap();
        let deadline = Instant::now() + Duration::from_secs(6);
        let mut values = Vec::new();
        while values.len() < 2 {
            let event = handle
                .events
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
            if let Event::Log { text, .. } = event
                && text.contains("(raw 32-bit)")
            {
                values.push(text);
            }
        }
        assert!(values[0].contains("= 0x1"));
        assert!(values[1].contains("= 0x2"));
        drop(handle);
        server.join().unwrap();
        fs::remove_file(file).unwrap();
    }
    #[test]
    fn incomplete_rpc_times_out_instead_of_succeeding() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut b = [0];
            while stream.read_exact(&mut b).is_ok() && b[0] != 0x1a {}
            stream.write_all(b"0:partial").unwrap();
            thread::sleep(Duration::from_millis(750));
        });
        let mut stream = connect(&addr.to_string()).unwrap();
        let start = Instant::now();
        assert!(transact(&mut stream, "test").is_err());
        assert!(start.elapsed() < Duration::from_secs(2));
        server.join().unwrap();
    }
}
