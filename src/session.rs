use crate::{
    config::{Project, portable_path},
    mi::{self, Record, Value},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use std::{
    collections::VecDeque,
    fs::{self, File},
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Frame {
    pub level: u32,
    pub function: String,
    pub file: String,
    pub line: u32,
    pub address: String,
}
impl Frame {
    fn from_mi(v: &Value) -> Self {
        Self {
            level: v.string("level").parse().unwrap_or(0),
            function: v.string("func"),
            file: {
                let s = v.string("fullname");
                if s.is_empty() { v.string("file") } else { s }
            },
            line: v.string("line").parse().unwrap_or(0),
            address: v.string("addr"),
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Variable {
    pub name: String,
    pub value: String,
    pub changed: bool,
    pub error: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Breakpoint {
    pub id: String,
    pub location: String,
    pub kind: String,
    pub enabled: bool,
    pub temporary: bool,
    pub file: String,
    pub line: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub state: String,
    pub stop_reason: String,
    pub frame: Frame,
    pub stack: Vec<Frame>,
    pub locals: Vec<Variable>,
    pub watches: Vec<Variable>,
    pub registers: Vec<Variable>,
    pub breakpoints: Vec<Breakpoint>,
    pub files: Vec<String>,
    pub assembly: Vec<String>,
    pub memory: Vec<String>,
    pub generation: u64,
    pub async_supported: bool,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            state: "DISCONNECTED".into(),
            stop_reason: String::new(),
            frame: Frame::default(),
            stack: vec![],
            locals: vec![],
            watches: vec![],
            registers: vec![],
            breakpoints: vec![],
            files: vec![],
            assembly: vec![],
            memory: vec![],
            generation: 0,
            async_supported: false,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Json,
}
impl Request {
    pub fn new(id: u64, method: &str, params: Json) -> Self {
        Self {
            id,
            method: method.into(),
            params,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Snapshot {
        snapshot: Box<Snapshot>,
    },
    Log {
        channel: String,
        text: String,
    },
    Response {
        id: u64,
        ok: bool,
        result: Json,
        error: Option<String>,
    },
    Exit,
}
pub struct EngineHandle {
    commands: Sender<Request>,
    pub events: Receiver<Event>,
    pub cancellation: Arc<AtomicBool>,
}
impl EngineHandle {
    pub fn send(&self, request: Request) -> Result<(), String> {
        if request.method == "quit" {
            self.cancellation.store(true, Ordering::Relaxed);
        }
        self.commands.send(request).map_err(|e| e.to_string())
    }
}
impl Drop for EngineHandle {
    fn drop(&mut self) {
        self.cancellation.store(true, Ordering::Relaxed);
    }
}

pub fn spawn(project: Project) -> EngineHandle {
    let (commands, requests) = mpsc::channel();
    let (events, receiver) = mpsc::sync_channel(512);
    let cancellation = Arc::new(AtomicBool::new(false));
    let worker_cancellation = cancellation.clone();
    thread::spawn(move || {
        let mut e = Engine::new(project, events, worker_cancellation);
        e.run(requests);
    });
    EngineHandle {
        commands,
        events: receiver,
        cancellation,
    }
}

fn hidden(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
}
fn tool_environment(command: &mut Command) {
    if let Some(windows) = std::env::var_os("SystemRoot") {
        let root = Path::new(&windows);
        command.env(
            "PATH",
            format!("{};{}", root.join("System32").display(), root.display()),
        );
    }
    command
        .env_remove("PYTHONHOME")
        .env_remove("PYTHONPATH")
        .env("LC_ALL", "C");
}

#[derive(Clone)]
struct Logs(Arc<Mutex<VecDeque<(String, String)>>>);
impl Logs {
    fn push(&self, channel: &str, text: String) {
        if let Ok(mut q) = self.0.lock() {
            if q.len() >= 512 {
                q.pop_front();
            }
            q.push_back((channel.into(), text));
        }
    }
    fn drain(&self) -> Vec<(String, String)> {
        self.0
            .lock()
            .map(|mut q| q.drain(..).collect())
            .unwrap_or_default()
    }
}
fn log_reader(stream: impl Read + Send + 'static, channel: &'static str, logs: Logs) {
    thread::spawn(move || {
        let mut r = BufReader::new(stream);
        let mut b = Vec::new();
        loop {
            b.clear();
            match (&mut r).take(1024 * 1024).read_until(b'\n', &mut b) {
                Ok(0) | Err(_) => break,
                Ok(_) => logs.push(channel, String::from_utf8_lossy(&b).trim_end().into()),
            }
        }
    });
}
enum Incoming {
    Record(Record),
    Closed,
    Invalid(String),
}
struct Gdb {
    child: Child,
    input: ChildStdin,
    records: Receiver<Incoming>,
    token: u64,
}
impl Drop for Gdb {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}
struct ServerProcess {
    child: Child,
}
impl Drop for ServerProcess {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

struct Engine {
    project: Project,
    events: SyncSender<Event>,
    snapshot: Snapshot,
    gdb: Option<Gdb>,
    server: Option<ServerProcess>,
    logs: Logs,
    trace: Option<File>,
    trace_bytes: usize,
    refresh_pending: bool,
    watch_names: Vec<String>,
    saved_breakpoints: Vec<String>,
    reg_names: Vec<String>,
    exiting: bool,
    job: Option<crate::process::Job>,
    cancellation: Arc<AtomicBool>,
}
impl Engine {
    fn new(project: Project, events: SyncSender<Event>, cancellation: Arc<AtomicBool>) -> Self {
        let watch_names = project.watch.clone();
        let saved_breakpoints = project.breakpoints.clone();
        Self {
            project,
            events,
            snapshot: Snapshot::default(),
            gdb: None,
            server: None,
            logs: Logs(Arc::new(Mutex::new(VecDeque::new()))),
            trace: None,
            trace_bytes: 0,
            refresh_pending: false,
            watch_names,
            saved_breakpoints,
            reg_names: vec![],
            exiting: false,
            job: None,
            cancellation,
        }
    }
    fn emit(&self, event: Event) {
        let _ = self.events.send(event);
    }
    fn publish(&self) {
        self.emit(Event::Snapshot {
            snapshot: Box::new(self.snapshot.clone()),
        });
    }
    fn state(&mut self, state: &str) {
        self.snapshot.state = state.into();
        self.publish();
    }
    fn log(&mut self, channel: &str, text: impl Into<String>) {
        let text = text.into();
        if let Some(file) = &mut self.trace {
            const LIMIT: usize = 8 * 1024 * 1024;
            let size = channel.len() + text.len() + 4;
            if self.trace_bytes + size <= LIMIT {
                let _ = writeln!(file, "[{channel}] {text}");
                self.trace_bytes += size;
            } else if self.trace_bytes <= LIMIT {
                let _ = writeln!(
                    file,
                    "[session] Trace limit reached (8 MiB); further disk logging disabled."
                );
                self.trace_bytes = LIMIT + 1;
            }
        }
        self.emit(Event::Log {
            channel: channel.into(),
            text,
        });
    }
    fn flush_logs(&mut self) {
        for (channel, text) in self.logs.drain() {
            self.log(&channel, text);
        }
    }
    fn run(&mut self, requests: Receiver<Request>) {
        self.publish();
        while !self.exiting {
            self.flush_logs();
            if self.gdb.is_some() {
                loop {
                    let incoming = self.gdb.as_ref().unwrap().records.try_recv();
                    match incoming {
                        Ok(r) => self.record(r),
                        Err(_) => break,
                    }
                }
                if self.refresh_pending && self.snapshot.state == "STOPPED" {
                    self.refresh_pending = false;
                    if let Err(e) = self.refresh() {
                        self.log("error", e);
                    }
                }
            }
            match requests.recv_timeout(Duration::from_millis(20)) {
                Ok(request) => {
                    let result = self.execute(&request.method, &request.params);
                    if let Err(error) = &result {
                        self.log("error", error.clone());
                    }
                    self.emit(Event::Response {
                        id: request.id,
                        ok: result.is_ok(),
                        result: result.clone().unwrap_or(Json::Null),
                        error: result.err(),
                    });
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let _ = self.disconnect();
                    break;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
        self.emit(Event::Exit);
    }
    fn record(&mut self, incoming: Incoming) {
        match incoming {
            Incoming::Record(r) => {
                if r.kind == '*' && r.class == "running" {
                    self.state("RUNNING");
                } else if r.kind == '*' && r.class == "stopped" {
                    self.snapshot.generation += 1;
                    self.snapshot.state = "STOPPED".into();
                    self.snapshot.stop_reason = r.data.string("reason");
                    if let Some(frame) = r.data.field("frame") {
                        self.snapshot.frame = Frame::from_mi(frame);
                    }
                    if let Some(value) = r.data.field("value") {
                        let expression = r
                            .data
                            .field("wpt")
                            .map(|v| v.string("exp"))
                            .unwrap_or_default();
                        self.log(
                            "stop",
                            format!(
                                "{expression}: {} -> {}",
                                value.string("old"),
                                value.string("new")
                            ),
                        );
                    }
                    self.refresh_pending = true;
                    self.publish();
                } else if matches!(r.kind, '~' | '@' | '&') {
                    self.log(
                        if r.kind == '~' {
                            "gdb"
                        } else if r.kind == '@' {
                            "target"
                        } else {
                            "diagnostic"
                        },
                        r.data.text(),
                    );
                } else if r.kind == '=' && r.class.starts_with("breakpoint-") {
                    self.refresh_pending = true;
                }
            }
            Incoming::Closed => {
                if !matches!(
                    self.snapshot.state.as_str(),
                    "DISCONNECTING" | "DISCONNECTED"
                ) {
                    self.state("FAULT");
                    self.log(
                        "error",
                        "GDB closed the connection. Target state is unknown.",
                    );
                }
            }
            Incoming::Invalid(s) => self.log("protocol", s),
        }
    }
    fn request(&mut self, command: &str, timeout: Duration) -> Result<Record, String> {
        let gdb = self.gdb.as_mut().ok_or("Not connected")?;
        gdb.token += 1;
        let token = gdb.token;
        writeln!(gdb.input, "{token}{command}")
            .and_then(|_| gdb.input.flush())
            .map_err(|e| format!("GDB input: {e}"))?;
        self.log("mi>", format!("{token}{command}"));
        let deadline = Instant::now() + timeout;
        loop {
            self.flush_logs();
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.state("FAULT");
                return Err(format!(
                    "GDB request timed out; reconnect to recover: {command}"
                ));
            }
            let incoming = self
                .gdb
                .as_ref()
                .ok_or("GDB gone")?
                .records
                .recv_timeout(remaining.min(Duration::from_millis(50)));
            match incoming {
                Ok(Incoming::Record(r)) if r.kind == '^' && r.token == Some(token) => {
                    self.log(
                        "mi<",
                        format!(
                            "{token}^{} {}",
                            r.class,
                            serde_json::to_string(&r.data).unwrap_or_default()
                        ),
                    );
                    if r.class == "error" {
                        return Err(r.data.string("msg"));
                    }
                    return Ok(r);
                }
                Ok(Incoming::Closed) => {
                    self.record(Incoming::Closed);
                    return Err("GDB exited".into());
                }
                Ok(other) => self.record(other),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("GDB reader disconnected".into());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }
    fn mi(&mut self, command: &str) -> Result<Record, String> {
        self.request(
            command,
            Duration::from_millis(self.project.session.timeout_ms),
        )
    }
    fn console(&mut self, command: &str) -> Result<Record, String> {
        self.mi(&format!("-interpreter-exec console {}", mi::quote(command)))
    }
    fn stopped(&self) -> Result<(), String> {
        if self.snapshot.state == "STOPPED" {
            Ok(())
        } else {
            Err(format!(
                "Target must be stopped (currently {})",
                self.snapshot.state
            ))
        }
    }
    fn start_server(&mut self) -> Result<(), String> {
        if self.project.server.mode == "external" {
            return Ok(());
        }
        self.state("STARTING SERVER");
        let mut c = Command::new(
            self.project
                .tools
                .root
                .join("bin/jlink/JLinkGDBServerCL.exe"),
        );
        c.args([
            "-device",
            &self.project.server.device,
            "-if",
            &self.project.server.interface,
            "-speed",
            &self.project.server.speed_khz.to_string(),
            "-port",
            &self.project.server.port.to_string(),
            "-select",
        ]);
        c.arg(
            self.project
                .server
                .serial
                .as_ref()
                .map(|s| format!("USB={s}"))
                .unwrap_or("USB".into()),
        );
        c.args(["-localhostonly", "-nogui", "-halt", "-singlerun"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        hidden(&mut c);
        tool_environment(&mut c);
        let mut child = c.spawn().map_err(|e| format!("Start J-Link Server: {e}"))?;
        self.job
            .as_ref()
            .ok_or("Process job missing")?
            .attach(&mut child)?;
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let mut stdout = child.stdout.take().unwrap();
        let logs = self.logs.clone();
        // The CLI server's final readiness prompt may not end with a newline.
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            let mut pending = String::new();
            loop {
                let n = match stdout.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                pending.push_str(&String::from_utf8_lossy(&buf[..n]));
                if pending.contains("Waiting for GDB connection")
                    || pending.contains("Connected to target")
                {
                    let _ = ready_tx.try_send(());
                }
                while let Some(end) = pending.find('\n') {
                    let line = pending[..end].trim_end_matches('\r').to_owned();
                    pending.drain(..=end);
                    logs.push("server", line);
                }
                if pending.len() > 1024 * 1024 {
                    logs.push("server", std::mem::take(&mut pending));
                }
            }
            if !pending.is_empty() {
                logs.push("server", pending);
            }
        });
        log_reader(
            child.stderr.take().unwrap(),
            "server-error",
            self.logs.clone(),
        );
        self.server = Some(ServerProcess { child });
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            self.flush_logs();
            if ready_rx.try_recv().is_ok() {
                return Ok(());
            }
            if let Some(status) = self
                .server
                .as_mut()
                .unwrap()
                .child
                .try_wait()
                .map_err(|e| e.to_string())?
            {
                return Err(format!("Server exited ({status}); see Server Log"));
            }
            if Instant::now() > deadline {
                return Err("Server startup timed out; see Server Log".into());
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
    fn connect(&mut self) -> Result<Json, String> {
        if self.gdb.is_some() {
            return Err("A session already exists. Disconnect before reconnecting.".into());
        }
        self.project.resolve_tools()?;
        if self.job.is_none() {
            self.job = Some(crate::process::Job::new()?);
        }
        if let Some(dir) = &self.project.session.log_dir {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            self.trace = Some(
                File::create(dir.join(format!(
                        "session-{}.log",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis()
                    )))
                .map_err(|e| e.to_string())?,
            );
            self.trace_bytes = 0;
        }
        let result = self.connect_inner();
        if result.is_err() {
            self.gdb.take();
            self.server.take();
            self.state("FAULT");
        }
        result
    }
    fn connect_inner(&mut self) -> Result<Json, String> {
        self.start_server()?;
        self.state("STARTING GDB");
        let root = self.project.tools.root.join("bin/gdb");
        let data = root.join("arm-none-eabi/share/gdb");
        let mut c = Command::new(root.join("bin/arm-none-eabi-gdb.exe"));
        c.args(["-nx", "-q", "--interpreter=mi2"])
            .arg(format!("--data-directory={}", portable_path(&data)));
        for setting in [
            format!(
                "set debug-file-directory {}",
                portable_path(&root.join("lib/debug"))
            ),
            format!(
                "set auto-load scripts-directory {}",
                portable_path(&data.join("auto-load"))
            ),
            format!(
                "set auto-load safe-path {}",
                portable_path(&data.join("auto-load"))
            ),
        ] {
            c.arg("-iex").arg(setting);
        }
        c.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        hidden(&mut c);
        tool_environment(&mut c);
        let mut child = c.spawn().map_err(|e| format!("Start GDB: {e}"))?;
        self.job
            .as_ref()
            .ok_or("Process job missing")?
            .attach(&mut child)?;
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        log_reader(child.stderr.take().unwrap(), "gdb-error", self.logs.clone());
        let (tx, rx) = mpsc::sync_channel(256);
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = Vec::new();
            loop {
                line.clear();
                match (&mut reader).take(1024 * 1024).read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if n >= 1024 * 1024 {
                            let _ = tx.send(Incoming::Invalid("MI record exceeds 1 MiB".into()));
                            break;
                        }
                        let parsed = mi::parse(&String::from_utf8_lossy(&line));
                        match parsed {
                            Ok(Some(r)) => {
                                if tx.send(Incoming::Record(r)).is_err() {
                                    return;
                                }
                            }
                            Ok(None) => {}
                            Err(e) => {
                                if tx.send(Incoming::Invalid(e)).is_err() {
                                    return;
                                }
                            }
                        }
                    }
                }
            }
            let _ = tx.send(Incoming::Closed);
        });
        self.gdb = Some(Gdb {
            child,
            input,
            records: rx,
            token: 0,
        });
        for command in [
            "-gdb-set pagination off",
            "-gdb-set confirm off",
            "-gdb-set mi-async on",
            "-gdb-set remotetimeout 5",
            "-gdb-set tcp connect-timeout 5",
        ] {
            self.mi(command)?;
        }
        self.mi(&format!(
            "-file-exec-and-symbols {}",
            mi::quote(&portable_path(&self.project.program.elf))
        ))?;
        for map in self.project.source_map.clone() {
            self.console(&format!(
                "set substitute-path {} {}",
                mi::quote(&map.from),
                mi::quote(&portable_path(&map.to))
            ))?;
        }
        self.state("CONNECTING");
        // -target-select forwards its target arguments verbatim; quoting an endpoint
        // makes this GDB interpret it as a serial-device filename.
        self.mi(&format!(
            "-target-select extended-remote {}:{}",
            self.project.server.host, self.project.server.port
        ))?;
        self.console("monitor halt")?;
        self.console("maintenance flush register-cache")?;
        let features = self.mi("-list-target-features")?;
        self.snapshot.async_supported = features
            .data
            .field("features")
            .is_some_and(|v| v.items().iter().any(|x| x.text() == "async"));
        self.log(
            "session",
            format!("MI target async support: {}", self.snapshot.async_supported),
        );
        self.snapshot.state = "STOPPED".into();
        self.snapshot.stop_reason = "connected".into();
        self.snapshot.generation += 1;
        let names = self.mi("-data-list-register-names")?;
        self.reg_names = names
            .data
            .field("register-names")
            .map(|v| v.items().iter().map(|x| x.text().to_owned()).collect())
            .unwrap_or_default();
        for location in self.saved_breakpoints.clone() {
            if let Err(e) = self.mi(&format!("-break-insert {}", mi::quote(&location))) {
                self.log("error", format!("Restore breakpoint {location}: {e}"));
            }
        }
        self.refresh_pending = false;
        self.refresh()?;
        Ok(
            json!({"connected":true,"async_supported":self.snapshot.async_supported,"tools":portable_path(&self.project.tools.root)}),
        )
    }
    fn wait_stopped(&mut self, timeout: Duration) -> Result<Json, String> {
        let deadline = Instant::now() + timeout;
        loop {
            if self.snapshot.state == "STOPPED" {
                self.refresh_pending = false;
                self.refresh()?;
                return Ok(json!({"reason":self.snapshot.stop_reason,"frame":self.snapshot.frame}));
            }
            if self.snapshot.state == "FAULT" {
                return Err("Target connection lost".into());
            }
            if Instant::now() >= deadline {
                return Err("Timed out waiting for target to stop; it may still be running".into());
            }
            let incoming = self
                .gdb
                .as_ref()
                .ok_or("Not connected")?
                .records
                .recv_timeout(Duration::from_millis(25));
            if let Ok(r) = incoming {
                self.record(r);
            }
            self.flush_logs();
        }
    }
    fn refresh(&mut self) -> Result<(), String> {
        self.stopped()?;
        let r = self.mi("-stack-info-frame")?;
        if let Some(f) = r.data.field("frame") {
            self.snapshot.frame = Frame::from_mi(f);
        }
        if let Ok(r) = self.mi("-stack-list-frames 0 31") {
            self.snapshot.stack = r
                .data
                .field("stack")
                .map(|v| {
                    v.items()
                        .iter()
                        .filter_map(|f| f.field("frame"))
                        .map(Frame::from_mi)
                        .collect()
                })
                .unwrap_or_default();
        }
        if let Ok(r) = self.mi("-stack-list-variables --simple-values") {
            self.snapshot.locals = r
                .data
                .field("variables")
                .map(|v| {
                    v.items()
                        .iter()
                        .take(128)
                        .map(|x| Variable {
                            name: x.string("name"),
                            value: {
                                let value = x.string("value");
                                if value.is_empty() {
                                    format!("<{}>", x.string("type"))
                                } else {
                                    value
                                }
                            },
                            ..Default::default()
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
        let old = self.snapshot.watches.clone();
        let mut watches = Vec::new();
        for name in self.watch_names.clone().into_iter().take(64) {
            let result = self.mi(&format!("-data-evaluate-expression {}", mi::quote(&name)));
            let (value, error) = match result {
                Ok(r) => (r.data.string("value"), false),
                Err(e) => (e, true),
            };
            let changed = old
                .iter()
                .find(|v| v.name == name)
                .is_some_and(|v| v.value != value);
            watches.push(Variable {
                name,
                value,
                changed,
                error,
            });
        }
        self.snapshot.watches = watches;
        let indices: Vec<usize> = self
            .reg_names
            .iter()
            .enumerate()
            .filter(|(_, n)| {
                matches!(
                    n.as_str(),
                    "r0" | "r1"
                        | "r2"
                        | "r3"
                        | "r4"
                        | "r5"
                        | "r6"
                        | "r7"
                        | "r8"
                        | "r9"
                        | "r10"
                        | "r11"
                        | "r12"
                        | "sp"
                        | "lr"
                        | "pc"
                        | "xpsr"
                        | "msp"
                        | "psp"
                )
            })
            .map(|(i, _)| i)
            .collect();
        if !indices.is_empty() {
            let cmd = format!(
                "-data-list-register-values x {}",
                indices
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            if let Ok(r) = self.mi(&cmd) {
                let old = self.snapshot.registers.clone();
                self.snapshot.registers = r
                    .data
                    .field("register-values")
                    .map(|v| {
                        v.items()
                            .iter()
                            .map(|r| {
                                let name = self
                                    .reg_names
                                    .get(r.string("number").parse::<usize>().unwrap_or(usize::MAX))
                                    .cloned()
                                    .unwrap_or_default();
                                let value = r.string("value");
                                let changed = old
                                    .iter()
                                    .find(|x| x.name == name)
                                    .is_some_and(|x| x.value != value);
                                Variable {
                                    name,
                                    value,
                                    changed,
                                    error: false,
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
            }
        }
        self.refresh_breakpoints()?;
        self.publish();
        Ok(())
    }
    fn refresh_breakpoints(&mut self) -> Result<(), String> {
        let r = self.mi("-break-list")?;
        self.snapshot.breakpoints = r
            .data
            .field("BreakpointTable")
            .and_then(|t| t.field("body"))
            .map(|v| {
                v.items()
                    .iter()
                    .map(|x| x.field("bkpt").unwrap_or(x))
                    .map(|b| Breakpoint {
                        id: b.string("number"),
                        location: {
                            let s = b.string("original-location");
                            if s.is_empty() { b.string("what") } else { s }
                        },
                        kind: b.string("type"),
                        enabled: b.string("enabled") == "y",
                        temporary: b.string("disp") == "del",
                        file: b.string("fullname"),
                        line: b.string("line").parse().unwrap_or(0),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(())
    }
    fn disconnect(&mut self) -> Result<Json, String> {
        let mut failure = None;
        if self.gdb.is_some() {
            if self.snapshot.state == "RUNNING"
                && let Err(e) = self
                    .mi("-exec-interrupt --all")
                    .and_then(|_| self.wait_stopped(Duration::from_secs(5)).map(|_| ()))
            {
                failure = Some(e);
            }
            if self.snapshot.state == "STOPPED" {
                self.saved_breakpoints = self
                    .snapshot
                    .breakpoints
                    .iter()
                    .filter(|b| !b.kind.contains("watchpoint") && !b.temporary)
                    .map(|b| b.location.clone())
                    .filter(|s| !s.is_empty())
                    .collect();
                if let Err(e) = self.console("delete breakpoints") {
                    failure = Some(e);
                }
                if let Err(e) = self.console("monitor go") {
                    failure = Some(e);
                }
            }
            self.state("DISCONNECTING");
            if let Err(e) = self.mi("-target-detach") {
                failure = Some(e);
            }
            let _ = self.mi("-gdb-exit");
            self.gdb.take();
        }
        if let Some(server) = &mut self.server {
            let deadline = Instant::now() + Duration::from_secs(3);
            while server.child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(25));
            }
        }
        self.server.take();
        self.refresh_pending = false;
        self.flush_logs();
        if let Err(e) = self
            .project
            .save_preferences(self.watch_names.clone(), self.saved_breakpoints.clone())
        {
            self.log("error", format!("Save preferences: {e}"));
        }
        self.state(if failure.is_some() {
            "FAULT"
        } else {
            "DISCONNECTED"
        });
        if let Some(e) = failure {
            Err(format!(
                "Disconnected with errors; target state must be checked: {e}"
            ))
        } else {
            Ok(json!({"disconnected":true}))
        }
    }
    fn execute(&mut self, method: &str, p: &Json) -> Result<Json, String> {
        let text = |key: &str| {
            p.get(key)
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        match method {
            "connect" => self.connect(),
            "disconnect" => self.disconnect(),
            "quit" => {
                let result = self.disconnect();
                self.exiting = true;
                result
            }
            "status" => Ok(serde_json::to_value(&self.snapshot).unwrap()),
            "continue" => {
                self.stopped()?;
                let generation = self.snapshot.generation;
                self.mi("-exec-continue")?;
                if self.snapshot.generation == generation {
                    self.state("RUNNING");
                }
                Ok(json!({"running":self.snapshot.state=="RUNNING"}))
            }
            "pause" => {
                if self.snapshot.state == "STOPPED" {
                    return Ok(json!({"stopped":true}));
                }
                self.mi("-exec-interrupt --all")?;
                self.wait_stopped(Duration::from_secs(5))
            }
            "wait_stopped" => self.wait_stopped(Duration::from_millis(
                p.get("timeout_ms")
                    .and_then(Json::as_u64)
                    .unwrap_or(10000)
                    .min(60000),
            )),
            "step" | "next" | "stepi" | "finish" => {
                self.stopped()?;
                let generation = self.snapshot.generation;
                let cmd = match method {
                    "step" => "-exec-step",
                    "next" => "-exec-next",
                    "stepi" => "-exec-step-instruction",
                    _ => "-exec-finish",
                };
                self.mi(cmd)?;
                if self.snapshot.generation == generation {
                    self.state("RUNNING");
                }
                Ok(json!({"running":self.snapshot.state=="RUNNING"}))
            }
            "restart" => {
                self.stopped()?;
                self.console("monitor reset")?;
                self.console("monitor halt")?;
                self.console("maintenance flush register-cache")?;
                self.snapshot.generation += 1;
                self.refresh()?;
                Ok(json!({"reset":true,"state":"STOPPED"}))
            }
            "evaluate" => {
                self.stopped()?;
                let r = self.mi(&format!(
                    "-data-evaluate-expression {}",
                    mi::quote(&text("expression"))
                ))?;
                Ok(json!({"value":r.data.string("value")}))
            }
            "watch" => {
                let expr = text("expression");
                if expr.is_empty() {
                    return Err("Expression is required".into());
                }
                if self.watch_names.len() >= 64 {
                    return Err("Watch limit is 64 expressions".into());
                }
                if !self.watch_names.contains(&expr) {
                    self.watch_names.push(expr);
                }
                if self.snapshot.state == "STOPPED" {
                    self.refresh()?;
                }
                Ok(json!({"watches":self.watch_names}))
            }
            "unwatch" => {
                self.watch_names.retain(|s| s != &text("expression"));
                self.snapshot
                    .watches
                    .retain(|v| self.watch_names.contains(&v.name));
                self.publish();
                Ok(json!({"watches":self.watch_names}))
            }
            "break" => {
                self.stopped()?;
                let location = text("location");
                let temporary = p.get("temporary").and_then(Json::as_bool).unwrap_or(false);
                let r = self.mi(&format!(
                    "-break-insert {} {}",
                    if temporary { "-t" } else { "" },
                    mi::quote(&location)
                ))?;
                self.refresh_breakpoints()?;
                self.publish();
                Ok(json!({"breakpoint":r.data}))
            }
            "data_break" => {
                self.stopped()?;
                let r = self.mi(&format!("-break-watch {}", mi::quote(&text("expression"))))?;
                self.refresh_breakpoints()?;
                self.publish();
                Ok(json!({"breakpoint":r.data}))
            }
            "delete_break" => {
                self.stopped()?;
                let id = text("number");
                if id.is_empty() {
                    self.console("delete breakpoints")?;
                } else {
                    if !id.chars().all(|c| c.is_ascii_digit() || c == '.') {
                        return Err("Invalid breakpoint number".into());
                    }
                    self.mi(&format!("-break-delete {id}"))?;
                }
                self.refresh_breakpoints()?;
                self.publish();
                Ok(json!({"deleted":true}))
            }
            "frame" => {
                self.stopped()?;
                let index = p.get("level").and_then(Json::as_u64).unwrap_or(0);
                self.mi(&format!("-stack-select-frame {index}"))?;
                self.refresh()?;
                Ok(json!({"frame":self.snapshot.frame}))
            }
            "refresh" => {
                self.refresh()?;
                Ok(json!({"refreshed":true}))
            }
            "memory" => {
                self.stopped()?;
                let count = p
                    .get("count")
                    .and_then(Json::as_u64)
                    .unwrap_or(256)
                    .clamp(1, 4096);
                let r = self.mi(&format!(
                    "-data-read-memory-bytes {} {count}",
                    mi::quote(&text("address"))
                ))?;
                self.snapshot.memory.clear();
                if let Some(memory) = r.data.field("memory") {
                    for block in memory.items() {
                        let base =
                            u64::from_str_radix(block.string("begin").trim_start_matches("0x"), 16)
                                .unwrap_or(0);
                        let bytes = block.string("contents");
                        for (i, row) in bytes.as_bytes().chunks(32).enumerate() {
                            let hex = String::from_utf8_lossy(row);
                            let pairs = hex
                                .as_bytes()
                                .chunks(2)
                                .map(|p| String::from_utf8_lossy(p).to_string())
                                .collect::<Vec<_>>()
                                .join(" ");
                            self.snapshot
                                .memory
                                .push(format!("{:08x}  {pairs}", base + i as u64 * 16));
                        }
                    }
                }
                self.publish();
                Ok(json!({"memory":r.data}))
            }
            "disassemble" => {
                self.stopped()?;
                let address = text("address");
                let address = if address.is_empty() {
                    "$pc".into()
                } else {
                    address
                };
                let r = self.mi(&format!(
                    "-data-disassemble -s {} -e {} -- 0",
                    mi::quote(&address),
                    mi::quote(&format!("({address})+128"))
                ))?;
                self.snapshot.assembly = r
                    .data
                    .field("asm_insns")
                    .map(|v| {
                        v.items()
                            .iter()
                            .map(|i| format!("{}  {}", i.string("address"), i.string("inst")))
                            .collect()
                    })
                    .unwrap_or_default();
                self.publish();
                Ok(json!({"assembly":self.snapshot.assembly}))
            }
            "files" => {
                let r = self.mi("-file-list-exec-source-files")?;
                self.snapshot.files = r
                    .data
                    .field("files")
                    .map(|v| {
                        v.items()
                            .iter()
                            .map(|f| {
                                let full = f.string("fullname");
                                if full.is_empty() {
                                    f.string("file")
                                } else {
                                    full
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                self.snapshot.files.sort();
                self.snapshot.files.dedup();
                self.publish();
                Ok(json!({"files":self.snapshot.files}))
            }
            "download" => {
                self.stopped()?;
                self.state("DOWNLOADING");
                let result = self.request("-target-download", Duration::from_secs(90));
                if self.snapshot.state != "FAULT" {
                    self.state("STOPPED");
                }
                result?;
                self.console("monitor reset")?;
                self.console("monitor halt")?;
                self.console("maintenance flush register-cache")?;
                self.refresh()?;
                Ok(json!({"downloaded":true}))
            }
            "console" => {
                let command = text("command");
                let trimmed = command.trim();
                match trimmed {
                    "c" | "continue" | "monitor go" => {
                        return self.execute("continue", &Json::Null);
                    }
                    "s" | "step" => return self.execute("step", &Json::Null),
                    "n" | "next" => return self.execute("next", &Json::Null),
                    "si" | "stepi" => return self.execute("stepi", &Json::Null),
                    "interrupt" | "monitor halt" => return self.execute("pause", &Json::Null),
                    "q" | "quit" => return self.execute("quit", &Json::Null),
                    "monitor reset" => return self.execute("restart", &Json::Null),
                    "run" | "r" => {
                        self.execute("restart", &Json::Null)?;
                        return self.execute("continue", &Json::Null);
                    }
                    _ => {}
                }
                self.stopped()?;
                let r = self.console(&command)?;
                self.refresh_pending = true;
                Ok(json!({"result":r.data}))
            }
            "set_elf" => {
                if self.gdb.is_some() {
                    return Err("Disconnect before changing ELF".into());
                }
                self.project.program.elf = text("path").into();
                Ok(json!({"elf":self.project.program.elf}))
            }
            "build" => {
                if self.gdb.is_some() {
                    return Err("Disconnect before building and replacing ELF".into());
                }
                let b = self
                    .project
                    .build
                    .clone()
                    .ok_or("No [build] command configured")?;
                let mut c = Command::new(&b.command);
                c.args(&b.args)
                    .current_dir(&b.cwd)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                hidden(&mut c);
                let build_job = crate::process::Job::new()?;
                let mut child = c.spawn().map_err(|e| e.to_string())?;
                build_job.attach(&mut child)?;
                self.state("BUILDING");
                log_reader(child.stdout.take().unwrap(), "build", self.logs.clone());
                log_reader(child.stderr.take().unwrap(), "build", self.logs.clone());
                let deadline = Instant::now() + Duration::from_secs(300);
                loop {
                    self.flush_logs();
                    if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                        self.state("DISCONNECTED");
                        if status.success() {
                            return Ok(json!({"built":true}));
                        }
                        return Err(format!("Build exited: {status}"));
                    }
                    if Instant::now() > deadline || self.cancellation.load(Ordering::Relaxed) {
                        let _ = child.kill();
                        let _ = child.wait();
                        self.state("DISCONNECTED");
                        return Err(if self.cancellation.load(Ordering::Relaxed) {
                            "Build cancelled during exit".into()
                        } else {
                            "Build timed out".into()
                        });
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            }
            _ => Err(format!("Unknown method: {method}")),
        }
    }
}
