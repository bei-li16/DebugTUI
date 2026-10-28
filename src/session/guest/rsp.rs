//! A local, read-only RSP view of a captured ARM32 register set. This endpoint
//! never connects to the physical GDB port. All memory requests go back to the
//! owning Engine for its stop/sequence/channel checks and service serialization.
use super::super::*;
use std::net::{TcpListener, TcpStream};

pub(super) struct MemoryRead {
    pub address: u64,
    pub count: u64,
    pub reply: SyncSender<Result<Vec<u8>, String>>,
}

pub(super) struct Bridge {
    pub endpoint: String,
    pub reads: Receiver<MemoryRead>,
    cancel: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Bridge {
    pub fn new(registers: [u32; 17], logs: Logs) -> Result<Self, String> {
        Self::partial(registers.map(Some), logs)
    }
    pub fn partial(registers: [Option<u32>; 17], logs: Logs) -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let endpoint = listener
            .local_addr()
            .map_err(|e| e.to_string())?
            .to_string();
        let (tx, reads) = mpsc::sync_channel(8);
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let worker = thread::spawn(move || {
            let result = serve(listener, registers, &tx, &stop, &logs);
            if let Err(e) = result {
                logs.push("guest-rsp", e);
            }
        });
        Ok(Self {
            endpoint,
            reads,
            cancel,
            worker: Some(worker),
        })
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn xml() -> String {
    let mut text = String::from(
        "<?xml version=\"1.0\"?><!DOCTYPE target SYSTEM \"gdb-target.dtd\"><target><architecture>arm</architecture><feature name=\"org.gnu.gdb.arm.core\">",
    );
    for i in 0..17 {
        let (name, kind) = match i {
            13 => ("sp".into(), "data_ptr"),
            14 => ("lr".into(), "uint32"),
            15 => ("pc".into(), "code_ptr"),
            16 => ("cpsr".into(), "uint32"),
            n => (format!("r{n}"), "uint32"),
        };
        text.push_str(&format!(
            "<reg name=\"{name}\" bitsize=\"32\" regnum=\"{i}\" type=\"{kind}\"/>"
        ));
    }
    text.push_str("</feature></target>");
    text
}

pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn packet_reply(
    packet: &str,
    regs: &[Option<u32>; 17],
    memory: impl FnOnce(u64, u64) -> Result<Vec<u8>, String>,
) -> String {
    if packet.starts_with("qSupported") {
        return "PacketSize=1000;qXfer:features:read+".into();
    }
    if let Some(range) = packet.strip_prefix("qXfer:features:read:target.xml:") {
        let Some((a, b)) = range.split_once(',') else {
            return "E01".into();
        };
        let (Ok(offset), Ok(count)) = (usize::from_str_radix(a, 16), usize::from_str_radix(b, 16))
        else {
            return "E01".into();
        };
        let xml = xml();
        let offset = offset.min(xml.len());
        let end = offset.saturating_add(count.min(4096)).min(xml.len());
        return format!(
            "{}{}",
            if end == xml.len() { 'l' } else { 'm' },
            &xml[offset..end]
        );
    }
    match packet {
        "?" => return "T05thread:1;".into(),
        "qC" => return "QC1".into(),
        "qfThreadInfo" => return "m1".into(),
        "qsThreadInfo" => return "l".into(),
        "qAttached" => return "1".into(),
        "qOffsets" => return "Text=0;Data=0;Bss=0".into(),
        "Hg0" | "Hg1" | "Hc-1" | "Hc0" | "Hc1" | "T1" | "D" | "D;1" => return "OK".into(),
        "g" => {
            return regs
                .iter()
                .map(|r| {
                    r.map(|r| hex(&r.to_le_bytes()))
                        .unwrap_or_else(|| "xxxxxxxx".into())
                })
                .collect();
        }
        _ => (),
    }
    if let Some(n) = packet.strip_prefix('p') {
        return usize::from_str_radix(n, 16)
            .ok()
            .and_then(|n| regs.get(n))
            .map(|r| {
                r.map(|r| hex(&r.to_le_bytes()))
                    .unwrap_or_else(|| "xxxxxxxx".into())
            })
            .unwrap_or_else(|| "E01".into());
    }
    if let Some(request) = packet.strip_prefix('m') {
        let Some((a, n)) = request.split_once(',') else {
            return "E01".into();
        };
        let (Ok(a), Ok(n)) = (u64::from_str_radix(a, 16), u64::from_str_radix(n, 16)) else {
            return "E01".into();
        };
        if !(1..=1024).contains(&n) {
            return "E01".into();
        }
        return memory(a, n)
            .ok()
            .filter(|bytes| bytes.len() == n as usize)
            .map(|bytes| hex(&bytes))
            .unwrap_or_else(|| "E01".into());
    }
    // Explicitly reject every mutating/execute packet, even if GDB probes one.
    if packet.starts_with([
        'G', 'P', 'M', 'X', 'Z', 'z', 'c', 'C', 's', 'S', 'k', 'R', '!',
    ]) || packet.starts_with("vCont;")
        || packet.starts_with("vRun")
        || packet.starts_with("qRcmd")
    {
        return "E01".into();
    }
    String::new()
}

fn serve(
    listener: TcpListener,
    regs: [Option<u32>; 17],
    reads: &SyncSender<MemoryRead>,
    cancel: &AtomicBool,
    logs: &Logs,
) -> Result<(), String> {
    let mut socket = loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        match listener.accept() {
            Ok((s, _)) => break s,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5))
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    socket
        .set_read_timeout(Some(Duration::from_millis(50)))
        .map_err(|e| e.to_string())?;
    socket
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    loop {
        let Some(b) = read_byte(&mut socket, cancel)? else {
            return Ok(());
        };
        if b != b'$' {
            continue;
        }
        let mut data = Vec::new();
        loop {
            let Some(b) = read_byte(&mut socket, cancel)? else {
                return Ok(());
            };
            if b == b'#' {
                break;
            }
            data.push(b);
            if data.len() > 16384 {
                return Err("Guest RSP packet exceeds limit".into());
            }
        }
        let Some(a) = read_byte(&mut socket, cancel)? else {
            return Ok(());
        };
        let Some(b) = read_byte(&mut socket, cancel)? else {
            return Ok(());
        };
        let expected = std::str::from_utf8(&[a, b])
            .ok()
            .and_then(|s| u8::from_str_radix(s, 16).ok());
        if expected != Some(data.iter().fold(0u8, |a, b| a.wrapping_add(*b))) {
            socket.write_all(b"-").map_err(|e| e.to_string())?;
            continue;
        }
        socket.write_all(b"+").map_err(|e| e.to_string())?;
        let packet = String::from_utf8(data).map_err(|e| e.to_string())?;
        logs.push("guest-rsp>", packet.clone());
        let reply = packet_reply(&packet, &regs, |address, count| {
            let (tx, rx) = mpsc::sync_channel(1);
            reads
                .send(MemoryRead {
                    address,
                    count,
                    reply: tx,
                })
                .map_err(|e| e.to_string())?;
            let deadline = Instant::now() + Duration::from_secs(5);
            while !cancel.load(Ordering::Relaxed) && Instant::now() < deadline {
                match rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(value) => return value,
                    Err(mpsc::RecvTimeoutError::Timeout) => (),
                    Err(e) => return Err(e.to_string()),
                }
            }
            Err("Guest memory request cancelled or timed out".into())
        });
        // XML and hexadecimal replies contain no RSP reserved characters.
        let checksum = reply.bytes().fold(0u8, |a, b| a.wrapping_add(b));
        socket
            .write_all(format!("${reply}#{checksum:02x}").as_bytes())
            .map_err(|e| e.to_string())?;
        if packet == "D" || packet == "D;1" {
            return Ok(());
        }
    }
}
fn read_byte(socket: &mut TcpStream, cancel: &AtomicBool) -> Result<Option<u8>, String> {
    let mut b = [0];
    while !cancel.load(Ordering::Relaxed) {
        match socket.read(&mut b) {
            Ok(0) => return Ok(None),
            Ok(_) => return Ok(Some(b[0])),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_registers_are_distinct_and_all_mutators_are_denied() {
        let regs = std::array::from_fn(|i| Some(i as u32 + 0x12345600));
        assert_eq!(packet_reply("pd", &regs, |_, _| panic!()), "0d563412");
        assert_eq!(packet_reply("g", &regs, |_, _| panic!()).len(), 136);
        for command in [
            "G00",
            "P0=ff",
            "M100,1:ff",
            "X100,1:a",
            "Z1,100,4",
            "z1,100,4",
            "c",
            "s",
            "vCont;c",
            "qRcmd,00",
            "!",
        ] {
            assert_eq!(
                packet_reply(command, &regs, |_, _| panic!()),
                "E01",
                "{command}"
            );
        }
        assert_eq!(
            packet_reply("m1234,2", &regs, |a, n| {
                assert_eq!((a, n), (0x1234, 2));
                Ok(vec![1, 2])
            }),
            "0102"
        );
        assert_eq!(packet_reply("m1234,8000", &regs, |_, _| panic!()), "E01");
    }
    #[test]
    fn global_view_never_fabricates_stack_or_general_registers() {
        let mut regs = [None; 17];
        regs[15] = Some(0x10020164);
        regs[16] = Some(0x600001d3);
        assert_eq!(packet_reply("pd", &regs, |_, _| panic!()), "xxxxxxxx");
        assert_eq!(packet_reply("pf", &regs, |_, _| panic!()), "64010210");
        assert_eq!(packet_reply("p11", &regs, |_, _| panic!()), "E01");
        assert_eq!(
            packet_reply("g", &regs, |_, _| panic!()),
            format!("{}64010210d3010060", "xxxxxxxx".repeat(15))
        );
    }
}
