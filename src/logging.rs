//! Timestamped, non-overwriting session logs. No runtime time-zone dependency.
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::Instant,
};

#[derive(Clone)]
pub(crate) struct Stamp {
    pub wall: String,
    pub at: Instant,
}
impl Stamp {
    pub fn now() -> Self {
        Self {
            wall: wall_time(),
            at: Instant::now(),
        }
    }
    pub fn filename(&self) -> String {
        format!(
            "{}-{}-{}",
            self.wall[..10].replace('-', ""),
            self.wall[11..19].replace(':', ""),
            &self.wall[20..23]
        )
    }
    pub fn elapsed_ms(&self, start: Instant) -> u64 {
        self.at
            .saturating_duration_since(start)
            .as_millis()
            .min(u64::MAX as u128) as u64
    }
}
#[cfg(windows)]
fn wall_time() -> String {
    #[repr(C)]
    #[derive(Default)]
    struct SystemTime {
        year: u16,
        month: u16,
        weekday: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        millis: u16,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLocalTime(time: *mut SystemTime);
    }
    let mut time = SystemTime::default();
    // SYSTEMTIME is an initialized, correctly aligned buffer owned by this call.
    unsafe { GetLocalTime(&mut time) };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        time.year, time.month, time.day, time.hour, time.minute, time.second, time.millis
    )
}
#[cfg(not(windows))]
fn wall_time() -> String {
    utc(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default())
}
#[cfg(any(not(windows), test))]
fn utc(elapsed: std::time::Duration) -> String {
    let secs = elapsed.as_secs();
    let z = (secs / 86400) as i64 + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}.{:03}Z",
        secs / 3600 % 24,
        secs / 60 % 60,
        secs % 60,
        elapsed.subsec_millis()
    )
}
pub(crate) struct Trace {
    file: File,
    pub path: PathBuf,
    pub started: Instant,
    bytes: usize,
}
impl Trace {
    pub fn open(dir: &Path, stamp: &Stamp) -> io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        for serial in 0..10000 {
            let suffix = if serial == 0 {
                String::new()
            } else {
                format!("-{serial:03}")
            };
            let path = dir.join(format!("session-{}{suffix}.log", stamp.filename()));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => {
                    return Ok(Self {
                        file,
                        path,
                        started: stamp.at,
                        bytes: 0,
                    });
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Cannot allocate a unique session log",
        ))
    }
    pub fn write(&mut self, stamp: &Stamp, channel: &str, text: &str) {
        const LIMIT: usize = 8 * 1024 * 1024;
        let ms = stamp.elapsed_ms(self.started);
        let prefix = format!(
            "[{}] [+{}.{:03}s] [{channel}] ",
            stamp.wall,
            ms / 1000,
            ms % 1000
        );
        let text = text.trim_end_matches(['\r', '\n']);
        // Including empty records: every physical line carries its own timestamp/channel.
        for line in text.split('\n') {
            let line = line.trim_end_matches('\r');
            let size = prefix.len() + line.len() + 1;
            if self.bytes.saturating_add(size) <= LIMIT {
                let _ = writeln!(self.file, "{prefix}{line}");
                self.bytes += size;
            } else if self.bytes <= LIMIT {
                let _ = writeln!(
                    self.file,
                    "[{}] [+{}.{:03}s] [session] Trace limit reached (8 MiB); further disk logging disabled.",
                    stamp.wall,
                    ms / 1000,
                    ms % 1000
                );
                self.bytes = LIMIT + 1;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn calendar_and_clock_shape() {
        assert_eq!(utc(Duration::ZERO), "1970-01-01 00:00:00.000Z");
        assert_eq!(
            utc(Duration::from_millis(1709251199123)),
            "2024-02-29 23:59:59.123Z"
        );
        let stamp = Stamp::now();
        assert_eq!(&stamp.wall[10..11], " ");
        assert_eq!(stamp.filename().len(), 19);
        assert!(
            stamp
                .filename()
                .chars()
                .all(|c| c.is_ascii_digit() || c == '-')
        );
    }
    #[test]
    fn collisions_do_not_overwrite_and_multiline_records_keep_capture_time() {
        let dir = std::env::temp_dir().join(format!("debugtui-log-test-{}", std::process::id()));
        let stamp = Stamp {
            wall: "2026-09-17 17:04:05.006".into(),
            at: Instant::now(),
        };
        let mut first = Trace::open(&dir, &stamp).unwrap();
        let second = Trace::open(&dir, &stamp).unwrap();
        assert_ne!(first.path, second.path);
        let later = Stamp {
            wall: "2026-09-17 17:04:06.240".into(),
            at: stamp.at + Duration::from_millis(1234),
        };
        first.write(&later, "gdb", "first\r\nsecond\n\nthird\n");
        let content = std::fs::read_to_string(&first.path).unwrap();
        assert_eq!(content.lines().count(), 4);
        assert!(
            content
                .lines()
                .all(|s| s.starts_with("[2026-09-17 17:04:06.240] [+1.234s] [gdb]"))
        );
        let paths = [first.path.clone(), second.path.clone()];
        drop(first);
        drop(second);
        for p in paths {
            std::fs::remove_file(p).unwrap();
        }
        let _ = std::fs::remove_dir(dir);
    }
}
