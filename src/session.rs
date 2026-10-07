pub use crate::wake::Doorbell;
use crate::{
    config::{Project, portable_path},
    logging::{Stamp, Trace},
    mi::{self, Record, Value},
    wake::RingOnDrop,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

mod banked;
mod breakpoints;
mod capabilities;
mod gic;
mod memory;
pub(crate) use memory::literal_address;
mod memory_writes;
mod mmio_probe;
mod mpu;
mod pmu;
mod r52_core;
mod register_adapter;
mod register_matrix;
mod registers;
mod selectors;
mod stm;
mod symbols;
mod timer;
mod variable_writes;
mod vfp;
mod vfp_writes;
mod watch;
mod writes;
pub(crate) use symbols::Symbol;

pub(crate) fn execution_alias(command: &str) -> Option<&'static str> {
    Some(match command {
        "c" | "continue" => method::CONTINUE,
        "r" | "run" => method::RUN,
        "s" | "step" => method::STEP,
        "n" | "next" => method::NEXT,
        "si" | "stepi" => "step-instruction",
        "ni" | "nexti" => "next-instruction",
        "fin" | "finish" => method::FINISH,
        _ => return None,
    })
}

fn register_indices(names: &[String], selected: &[String]) -> Result<Vec<usize>, String> {
    for name in selected {
        if name.is_empty() || !names.contains(name) {
            return Err(format!(
                "Configured GDB register {name:?} is not exposed by this target"
            ));
        }
    }
    Ok(names
        .iter()
        .enumerate()
        .filter(|(_, n)| !n.is_empty() && (selected.is_empty() || selected.contains(n)))
        .map(|(i, _)| i)
        .collect())
}

#[cfg(test)]
mod register_selection_tests {
    use super::*;
    #[test]
    fn excludes_unavailable_banks_without_renumbering_and_rejects_typos() {
        let names = ["r0", "", "pc", "d0", "s0"].map(String::from);
        assert_eq!(register_indices(&names, &[]).unwrap(), [0, 2, 3, 4]);
        assert_eq!(
            register_indices(&names, &["pc".into(), "r0".into()]).unwrap(),
            [0, 2]
        );
        assert!(register_indices(&names, &["missing".into()]).is_err());
    }
}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree: Option<WatchTree>,
}
/// Native GDB variable-object metadata. Child paths are relative to a Watch root,
/// never GDB object names (which are deliberately short-lived).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WatchTree {
    pub path: Vec<usize>,
    pub type_name: String,
    pub child_count: usize,
    pub expanded: bool,
    pub children: Vec<Variable>,
    pub has_more: bool,
    pub limited: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Breakpoint {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cores: Vec<usize>,
    pub id: String,
    pub location: String,
    pub kind: String,
    pub enabled: bool,
    pub temporary: bool,
    pub file: String,
    pub line: u32,
    pub condition: String,
    pub ignore_count: u32,
    pub hit_count: u64,
    pub address: String,
    pub pending: bool,
    pub restore_error: String,
}
/// Request method names, shared by the UI, the coordinator, the session
/// worker and the JSON interface.
pub mod method {
    pub const BREAK: &str = "break";
    pub const BREAK_APPLY: &str = "break_apply";
    pub const BREAK_CORES: &str = "break_cores";
    pub const BUILD: &str = "build";
    pub const COMPLETE: &str = "complete";
    pub const CONNECT: &str = "connect";
    pub const CONSOLE: &str = "console";
    pub const CONTINUE: &str = "continue";
    pub const CONTROL_SCOPE: &str = "control_scope";
    pub const CORES: &str = "cores";
    pub const DATA_BREAK: &str = "data_break";
    pub const DELETE_BREAK: &str = "delete_break";
    pub const DISASSEMBLE: &str = "disassemble";
    pub const DISCONNECT: &str = "disconnect";
    pub const DOWNLOAD: &str = "download";
    pub const ENABLE_BREAK: &str = "enable_break";
    pub const EVALUATE: &str = "evaluate";
    pub const FILES: &str = "files";
    pub const FINISH: &str = "finish";
    pub const FRAME: &str = "frame";
    pub const LOCAL_EXPAND: &str = "local_expand";
    pub const MEMORY: &str = "memory";
    pub const MEMORY_CHANNELS: &str = "memory_channels";
    pub const MEMORY_DUMP: &str = "memory_dump";
    pub const MEMORY_READ: &str = "memory_read";
    pub const NEXT: &str = "next";
    pub const PAUSE: &str = "pause";
    pub const PERIPHERAL_READ: &str = "peripheral_read";
    pub const QUIT: &str = "quit";
    pub const RECONNECT: &str = "reconnect";
    pub const REFRESH: &str = "refresh";
    pub const REGISTER_BOUNDARY: &str = "register_boundary";
    pub const REGISTER_PREFERENCES: &str = "register_preferences";
    pub const REGISTER_SHARED_INVALIDATE: &str = "register_shared_invalidate";
    pub const REGISTERS_CACHE: &str = "registers_cache";
    pub const REGISTERS_LIST: &str = "registers_list";
    pub const REGISTERS_MATRIX: &str = "registers_matrix";
    pub const REGISTERS_MPU: &str = "registers_mpu";
    pub const REGISTERS_PROBE: &str = "registers_probe";
    pub const REGISTERS_READ: &str = "registers_read";
    pub const REGISTERS_SELECT: &str = "registers_select";
    pub const RESTART: &str = "restart";
    pub const RESTART_SHARED: &str = "restart_shared";
    pub const RUN: &str = "run";
    pub const SELECT_CORE: &str = "select_core";
    pub const SET_ELF: &str = "set_elf";
    pub const STATUS: &str = "status";
    pub const STEP: &str = "step";
    pub const STEPI: &str = "stepi";
    pub const SYMBOLS: &str = "symbols";
    pub const SYNCHRONIZE: &str = "synchronize";
    pub const UI_PREFERENCES: &str = "ui_preferences";
    pub const UNWATCH: &str = "unwatch";
    pub const UPDATE_BREAK: &str = "update_break";
    pub const WAIT_STOPPED: &str = "wait_stopped";
    pub const WATCH: &str = "watch";
    pub const WATCH_EXPAND: &str = "watch_expand";
    pub const WATCH_RESOLVE: &str = "watch_resolve";
    pub const WRITE_APPLY: &str = "write_apply";
    pub const WRITE_CANCEL: &str = "write_cancel";
    pub const WRITE_DISCARD: &str = "write_discard";
    pub const WRITE_INVALIDATE: &str = "write_invalidate";
    pub const WRITE_PREVIEW: &str = "write_preview";
}
/// Target states as snapshots and the JSON interface spell them.
pub mod state {
    pub const DISCONNECTED: &str = "DISCONNECTED";
    pub const STARTING_SERVER: &str = "STARTING SERVER";
    pub const STARTING_GDB: &str = "STARTING GDB";
    pub const CONNECTING: &str = "CONNECTING";
    pub const READY: &str = "READY";
    pub const RUNNING: &str = "RUNNING";
    pub const STOPPED: &str = "STOPPED";
    pub const DISCONNECTING: &str = "DISCONNECTING";
    pub const BUILDING: &str = "BUILDING";
    pub const DOWNLOADING: &str = "DOWNLOADING";
    pub const FAULT: &str = "FAULT";
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
    #[serde(default)]
    pub register_samples: Vec<crate::registers::Sample>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub register_owner_generations: std::collections::BTreeMap<String, u64>,
    pub breakpoints: Vec<Breakpoint>,
    pub files: Arc<[String]>,
    pub assembly: Vec<String>,
    pub memory: Vec<String>,
    pub generation: u64,
    #[serde(default)]
    pub register_session: u64,
    #[serde(default)]
    pub memory_selection_epoch: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub register_generation: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub register_probe: Option<crate::registers::capabilities::Probe>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub register_mpu: Option<crate::registers::mpu::m_profile::View>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub register_cache: Option<crate::registers::m_cache::View>,
    pub async_supported: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub core: Option<CoreStatus>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cores: Vec<CoreStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_scope: Option<crate::config::ControlScope>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreStatus {
    pub index: usize,
    pub name: String,
    pub endpoint: String,
    pub state: String,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            state: state::DISCONNECTED.into(),
            stop_reason: String::new(),
            frame: Frame::default(),
            stack: vec![],
            locals: vec![],
            watches: vec![],
            registers: vec![],
            register_samples: vec![],
            register_owner_generations: std::collections::BTreeMap::new(),
            breakpoints: vec![],
            files: Arc::default(),
            assembly: vec![],
            memory: vec![],
            generation: 0,
            register_session: 0,
            memory_selection_epoch: 0,
            register_generation: None,
            register_probe: None,
            register_mpu: None,
            register_cache: None,
            async_supported: false,
            core: None,
            cores: vec![],
            control_scope: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Json,
    /// Supplied only by the coordinator, never accepted from a headless client.
    #[serde(skip)]
    pub(crate) write_peers: Vec<CoreStatus>,
    /// Local request cancellation; never interrupts transport or restoration.
    #[serde(skip)]
    pub(crate) read_cancel: Arc<AtomicBool>,
}
impl Request {
    pub fn is_quit(&self) -> bool {
        self.method == method::QUIT
            || (self.method == method::CONSOLE
                && self
                    .params
                    .get("command")
                    .and_then(Json::as_str)
                    .is_some_and(|s| matches!(s.trim(), "q" | "quit")))
    }
    pub fn new(id: u64, method: &str, params: Json) -> Self {
        Self {
            id,
            method: method.into(),
            params,
            write_peers: vec![],
            read_cancel: Arc::new(AtomicBool::new(false)),
        }
    }
    /// Cancel a scoped register, Watch address, scalar or range memory request
    /// through a retained clone.
    /// The current transaction completes before results are discarded. This
    /// does not cancel writes, other requests, or the debugging session.
    pub fn cancel_read(&self) {
        self.read_cancel.store(true, Ordering::Relaxed);
    }
    pub(crate) fn is_register_read(&self) -> bool {
        matches!(
            self.method.as_str(),
            method::REGISTERS_READ
                | method::REGISTERS_PROBE
                | method::REGISTERS_SELECT
                | method::REGISTERS_MPU
                | method::REGISTERS_CACHE
        )
    }
    pub(crate) fn is_cancellable_read(&self) -> bool {
        self.is_register_read()
            || matches!(
                self.method.as_str(),
                method::WATCH_RESOLVE
                    | method::MEMORY_READ
                    | method::MEMORY_DUMP
                    | method::PERIPHERAL_READ
            )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    LiveWatch {
        sample: crate::live_watch::LiveWatchSample,
    },
    Snapshot {
        snapshot: Box<Snapshot>,
    },
    Log {
        channel: String,
        text: String,
        timestamp: String,
        elapsed_ms: u64,
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
    pub(crate) commands: Sender<Request>,
    pub events: Receiver<Event>,
    pub cancellation: Arc<AtomicBool>,
    /// The worker's bell. Declared last: dropping the handle rings it only
    /// after `commands` is gone, so the worker wakes to a disconnected channel.
    pub(crate) bell: RingOnDrop,
}
/// Cancels a worker's current operation from another thread. Setting the
/// flag alone would not wake a coordinator that is asleep on its bell.
#[derive(Clone)]
pub struct Canceller {
    flag: Arc<AtomicBool>,
    bell: Doorbell,
}
impl Canceller {
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Relaxed);
        self.bell.ring();
    }
}
impl EngineHandle {
    pub fn canceller(&self) -> Canceller {
        Canceller {
            flag: self.cancellation.clone(),
            bell: self.bell.0.clone(),
        }
    }
    pub fn send(&self, request: Request) -> Result<(), String> {
        if request.is_quit() {
            self.cancellation.store(true, Ordering::Relaxed);
        }
        self.commands.send(request).map_err(|e| e.to_string())?;
        self.bell.0.ring();
        Ok(())
    }
}
impl Drop for EngineHandle {
    fn drop(&mut self) {
        self.cancellation.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
pub(crate) fn test_channel() -> (EngineHandle, Receiver<Request>) {
    let (commands, requests) = mpsc::channel();
    let (_, events) = mpsc::channel();
    (
        EngineHandle {
            commands,
            events,
            cancellation: Arc::new(AtomicBool::new(false)),
            bell: Default::default(),
        },
        requests,
    )
}

pub fn spawn(project: Project) -> EngineHandle {
    spawn_with(project, Doorbell::default())
}

/// Like `spawn`, ringing `notify` after each event so the caller can sleep
/// until there is something to read instead of polling.
pub fn spawn_with(project: Project, notify: Doorbell) -> EngineHandle {
    let (commands, requests) = mpsc::channel();
    let (events, receiver) = mpsc::sync_channel(512);
    let cancellation = Arc::new(AtomicBool::new(false));
    let worker_cancellation = cancellation.clone();
    let inbox = Doorbell::default();
    let worker_inbox = inbox.clone();
    thread::spawn(move || {
        Engine::new(project, events, worker_cancellation)
            .with_bells(worker_inbox, notify)
            .run(requests);
    });
    EngineHandle {
        commands,
        events: receiver,
        cancellation,
        bell: RingOnDrop(inbox),
    }
}

pub(crate) fn hidden(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
}
pub(crate) fn tool_environment(
    command: &mut Command,
    values: &std::collections::BTreeMap<String, String>,
    unset: &[String],
) {
    command.env("LC_ALL", "C");
    for name in unset {
        command.env_remove(name);
    }
    command.envs(values);
}

#[derive(Clone, Default)]
struct Logs {
    queue: Arc<Mutex<VecDeque<(Stamp, String, String)>>>,
    /// The worker's bell: reader threads wake it for each line.
    bell: Doorbell,
}
impl Logs {
    fn push(&self, channel: &str, text: String) {
        if let Ok(mut q) = self.queue.lock() {
            if q.len() >= 512 {
                q.pop_front();
            }
            q.push_back((Stamp::now(), channel.into(), text));
        }
        self.bell.ring();
    }
    fn drain(&self) -> Vec<(Stamp, String, String)> {
        self.queue
            .lock()
            .map(|mut q| q.drain(..).collect())
            .unwrap_or_default()
    }
}
fn log_reader(
    stream: impl Read + Send + 'static,
    channel: &'static str,
    logs: Logs,
) -> thread::JoinHandle<()> {
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
    })
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
impl Gdb {
    fn wait_for_exit(&mut self) -> Result<(), String> {
        // MI ^exit acknowledges the command before GDB finishes its cleanup.
        // Let the process exit normally; Drop remains the bounded fallback.
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match self
                .child
                .try_wait()
                .map_err(|e| format!("Wait for GDB exit: {e}"))?
            {
                Some(status) if status.success() => return Ok(()),
                Some(status) => return Err(format!("GDB exited unsuccessfully: {status}")),
                None if Instant::now() >= deadline => {
                    return Err("GDB did not exit after ^exit".into());
                }
                None => thread::sleep(Duration::from_millis(10)),
            }
        }
    }
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
    /// Rung by requests, GDB output and log lines; the idle worker sleeps on it.
    inbox: Doorbell,
    /// The consumer's bell, rung after every event.
    notify: Doorbell,
    trace: Option<Trace>,
    session_started: Instant,
    refresh_pending: bool,
    watch_names: Vec<String>,
    watch_expansions: watch::Expansions,
    memory_connections: std::collections::HashMap<String, std::net::TcpStream>,
    memory_context_epoch: u64,
    watch_bindings: std::collections::HashMap<String, memory::WatchBinding>,
    watch_binding_serial: u64,
    rpc_echo: crate::live_watch::RpcEcho,
    saved_breakpoints: Vec<crate::config::BreakpointSpec>,
    unresolved_breakpoints: Vec<Breakpoint>,
    breakpoint_groups: std::collections::HashMap<String, String>,
    reg_names: Vec<String>,
    connected_gdb_endpoint: Option<String>,
    register_value_access: Option<crate::registers::provenance::Access>,
    register_catalogue: Result<Option<(crate::registers::Catalogue, String)>, String>,
    register_session: u64,
    register_access_fault: Option<String>,
    write_drafts: writes::Drafts,
    write_peers: Vec<CoreStatus>,
    console_capture: Option<String>,
    exiting: bool,
    job: Option<crate::process::Job>,
    cancellation: Arc<AtomicBool>,
    read_cancel: Arc<AtomicBool>,
}
impl Engine {
    fn project_task(&mut self, kind: &'static str) -> Result<Json, String> {
        let script = if kind == "build" {
            &self.project.tasks.build
        } else {
            &self.project.tasks.download
        };
        let (mut command, cwd, description) = if !script.trim().is_empty() {
            #[cfg(windows)]
            let command = {
                use std::os::windows::process::CommandExt;
                let mut c =
                    Command::new(std::env::var_os("COMSPEC").unwrap_or_else(|| "cmd.exe".into()));
                c.args(["/D", "/V:OFF", "/S", "/C"])
                    .raw_arg(format!("\"{script}\""));
                c
            };
            #[cfg(not(windows))]
            let command = {
                let mut c = Command::new("sh");
                c.args(["-c", script]);
                c
            };
            (
                command,
                self.project.program.source_root.clone(),
                script.clone(),
            )
        } else if kind == "build" {
            let b = self
                .project
                .build
                .as_ref()
                .ok_or("Build is not configured. Set Build command in F2 Setup.")?;
            let mut c = Command::new(&b.command);
            c.args(&b.args);
            (c, b.cwd.clone(), format!("{} {:?}", b.command, b.args))
        } else {
            return Err("Download is not configured. Set Download command in F2 Setup.".into());
        };
        let cwd = if cwd.as_os_str().is_empty() {
            std::env::current_dir().map_err(|e| e.to_string())?
        } else {
            cwd
        };
        if !cwd.is_dir() {
            return Err(format!(
                "Task working directory does not exist: {}",
                cwd.display()
            ));
        }
        if self.cancellation.load(Ordering::Relaxed) {
            return Err(format!("{kind} cancelled during exit"));
        }
        let reconnect = self.gdb.is_some();
        if reconnect || self.server.is_some() {
            self.log(
                kind,
                "Releasing the debug session before running the command.",
            );
            self.disconnect()?;
        }
        command
            .current_dir(&cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        hidden(&mut command);
        self.log(kind, format!("cwd: {}", portable_path(&cwd)));
        self.log(kind, format!("> {description}"));
        let job = crate::process::Job::new()?;
        let mut child = command.spawn().map_err(|e| format!("Start {kind}: {e}"))?;
        job.attach(&mut child)?;
        self.state(if kind == "build" {
            state::BUILDING
        } else {
            state::DOWNLOADING
        });
        let stdout = log_reader(child.stdout.take().unwrap(), kind, self.logs.clone());
        let stderr = log_reader(child.stderr.take().unwrap(), kind, self.logs.clone());
        let started = Instant::now();
        let outcome = loop {
            self.flush_logs();
            match child.try_wait() {
                Ok(Some(status)) => {
                    break if status.success() {
                        Ok(())
                    } else {
                        Err(format!("{kind} exited: {status}"))
                    };
                }
                Err(e) => break Err(format!("Wait for {kind}: {e}")),
                Ok(None) => {}
            }
            if self.cancellation.load(Ordering::Relaxed) {
                break Err(format!("{kind} cancelled during exit"));
            }
            if started.elapsed() >= Duration::from_millis(self.project.tasks.timeout_ms) {
                break Err(format!("{kind} timed out"));
            }
            thread::sleep(Duration::from_millis(25));
        };
        if outcome.is_err() {
            let _ = child.kill();
        }
        let _ = child.wait();
        // Close descendants before joining readers: inherited pipe handles must not hold up Exit.
        drop(job);
        let _ = stdout.join();
        let _ = stderr.join();
        self.flush_logs();
        self.state(state::DISCONNECTED);
        outcome?;
        self.log(kind, "Command completed successfully.");
        if reconnect && !self.cancellation.load(Ordering::Relaxed) {
            self.connect()
                .map_err(|e| format!("{kind} succeeded, but reconnect failed: {e}"))?;
        }
        Ok(if kind == "build" {
            json!({"built":true,"task":kind,"completed":true,"state":self.snapshot.state})
        } else {
            json!({"downloaded":true,"task":kind,"completed":true,"state":self.snapshot.state})
        })
    }
    fn new(project: Project, events: SyncSender<Event>, cancellation: Arc<AtomicBool>) -> Self {
        let register_catalogue = project.registers.load();
        let watch_names = project.watch.clone();
        let saved_breakpoints = project.breakpoints.clone();
        Self {
            project,
            events,
            snapshot: Snapshot::default(),
            gdb: None,
            server: None,
            logs: Logs::default(),
            inbox: Doorbell::default(),
            notify: Doorbell::default(),
            trace: None,
            session_started: Instant::now(),
            refresh_pending: false,
            watch_names,
            watch_expansions: Default::default(),
            memory_connections: Default::default(),
            memory_context_epoch: 0,
            watch_bindings: Default::default(),
            watch_binding_serial: 0,
            rpc_echo: Default::default(),
            saved_breakpoints,
            unresolved_breakpoints: vec![],
            breakpoint_groups: Default::default(),
            reg_names: vec![],
            connected_gdb_endpoint: None,
            register_value_access: None,
            register_catalogue,
            register_session: registers::new_session(),
            register_access_fault: None,
            write_drafts: Default::default(),
            write_peers: vec![],
            console_capture: None,
            exiting: false,
            job: None,
            cancellation,
            read_cancel: Arc::new(AtomicBool::new(false)),
        }
    }
    fn with_bells(mut self, inbox: Doorbell, notify: Doorbell) -> Self {
        // Readers clone `logs` only once GDB or a service starts, after this.
        self.logs.bell = inbox.clone();
        self.inbox = inbox;
        self.notify = notify;
        self
    }
    fn emit(&self, event: Event) {
        let _ = self.events.send(event);
        self.notify.ring();
    }
    fn publish(&self) {
        self.emit(Event::Snapshot {
            snapshot: Box::new(self.snapshot.clone()),
        });
    }
    fn state(&mut self, state: &str) {
        self.write_drafts.clear();
        self.snapshot.state = if self.register_access_fault.is_some()
            && matches!(state, state::STOPPED | state::RUNNING | state::READY)
        {
            state::FAULT.into()
        } else {
            state.into()
        };
        self.invalidate_register_samples();
        self.publish();
    }
    fn log(&mut self, channel: &str, text: impl Into<String>) {
        self.log_at(channel, text.into(), Stamp::now());
    }
    fn log_at(&mut self, channel: &str, text: String, stamp: Stamp) {
        let channel = if text.trim_start().starts_with(crate::live_watch::RPC_MARKER) {
            "diagnostic"
        } else {
            channel
        };
        if let Some(trace) = &mut self.trace {
            trace.write(&stamp, channel, &text);
        }
        self.emit(Event::Log {
            channel: channel.into(),
            text,
            elapsed_ms: stamp.elapsed_ms(self.session_started),
            timestamp: stamp.wall,
        });
    }
    fn flush_logs(&mut self) {
        for (stamp, channel, text) in self.logs.drain() {
            self.log_at(&channel, text, stamp);
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
                if self.refresh_pending && self.snapshot.state == state::STOPPED {
                    self.refresh_pending = false;
                    if let Err(e) = self.refresh() {
                        self.log("error", e);
                    }
                }
            }
            match requests.try_recv() {
                Ok(request) => {
                    self.read_cancel = if request.is_cancellable_read() {
                        request.read_cancel.clone()
                    } else {
                        Arc::new(AtomicBool::new(false))
                    };
                    self.write_peers = request.write_peers;
                    let result = self.execute(&request.method, &request.params);
                    self.write_peers.clear();
                    self.read_cancel = Arc::new(AtomicBool::new(false));
                    if let Err(error) = &result
                        && !matches!(request.method.as_str(), method::COMPLETE | method::SYMBOLS)
                    {
                        self.log(
                            if matches!(
                                request.method.as_str(),
                                method::WATCH_RESOLVE | method::MEMORY_READ
                            ) {
                                "diagnostic"
                            } else {
                                "error"
                            },
                            error.clone(),
                        );
                    }
                    self.emit(Event::Response {
                        id: request.id,
                        ok: result.is_ok(),
                        result: result.clone().unwrap_or(Json::Null),
                        error: result.err(),
                    });
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    let _ = self.disconnect();
                    break;
                }
                // Requests, GDB records and log lines all ring the inbox; none
                // arrived since the drain above, so sleep until one does.
                Err(mpsc::TryRecvError::Empty) => {
                    self.inbox.wait(None);
                }
            }
        }
        self.emit(Event::Exit);
    }
    fn record(&mut self, incoming: Incoming) {
        if matches!(&incoming, Incoming::Closed)
            || matches!(&incoming, Incoming::Record(r) if
                (r.kind == '*' && matches!(r.class.as_str(), "running" | "stopped"))
                || (r.kind == '=' && matches!(r.class.as_str(), "thread-selected" | "thread-exited" | "thread-group-exited")))
        {
            // A thread selection can change without changing the frame level or
            // stop generation. Fence reads that were already in flight.
            self.memory_context_epoch = self.memory_context_epoch.wrapping_add(1);
        }
        if matches!(&incoming, Incoming::Record(r) if r.kind == '=' && matches!(r.class.as_str(), "thread-selected" | "thread-exited" | "thread-group-exited"))
        {
            self.snapshot.memory_selection_epoch =
                self.snapshot.memory_selection_epoch.wrapping_add(1);
            self.watch_bindings.clear();
            self.publish();
        }
        if matches!(&incoming, Incoming::Record(r) if r.class == "thread-selected") {
            self.write_drafts.clear();
        }
        match incoming {
            Incoming::Record(r) => {
                if matches!(r.kind, '*' | '=') {
                    self.log("mi<", serde_json::to_string(&r).unwrap_or_default());
                }
                if r.kind == '+' && r.class == "download" {
                    let sent = r.data.string("total-sent").parse::<u64>().ok();
                    let total = r.data.string("total-size").parse::<u64>().ok();
                    if let Some((sent, total)) = sent.zip(total).filter(|(_, total)| *total > 0) {
                        self.log("progress",json!({"percent": (sent as u128 * 100 / total as u128).min(100) as u64}).to_string());
                    }
                }
                if r.kind == '*' && r.class == "running" {
                    self.state(state::RUNNING);
                } else if r.kind == '*' && r.class == "stopped" {
                    if r.data.string("reason").starts_with("exited") {
                        self.snapshot.generation += 1;
                        self.snapshot.frame = Frame::default();
                        self.snapshot.stop_reason = r.data.string("reason");
                        self.refresh_pending = false;
                        self.state(state::READY);
                        return;
                    }
                    self.snapshot.generation += 1;
                    self.snapshot.state = if self.register_access_fault.is_some() {
                        state::FAULT.into()
                    } else {
                        state::STOPPED.into()
                    };
                    self.snapshot.assembly.clear();
                    self.snapshot.memory.clear();
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
                    self.refresh_pending = self.register_access_fault.is_none();
                    self.invalidate_register_samples();
                    self.publish();
                } else if matches!(r.kind, '~' | '@' | '&') {
                    if r.kind == '~'
                        && let Some(capture) = &mut self.console_capture
                    {
                        if capture.len() + r.data.text().len() <= 65536 {
                            capture.push_str(r.data.text());
                        } else if !capture.contains('\0') {
                            capture.push('\0');
                        }
                    }
                    let internal = r.kind == '@' && self.rpc_echo.internal(r.data.text());
                    self.log(
                        if internal {
                            "diagnostic"
                        } else if r.kind == '~' {
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
                    state::DISCONNECTING | state::DISCONNECTED
                ) {
                    self.state(state::FAULT);
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
        let services = crate::debug_access::for_project(&self.project)?;
        let cleanup = matches!(
            command,
            "-gdb-exit" | "-target-disconnect" | "-target-detach"
        );
        let mut leases = services
            .iter()
            .map(|service| service.acquire(cleanup))
            .collect::<Result<Vec<_>, _>>()?;
        let gdb = self.gdb.as_mut().ok_or("Not connected")?;
        gdb.token += 1;
        let token = gdb.token;
        if command.starts_with("-interpreter-exec ")
            || command.starts_with("-target-select ")
            || matches!(
                command,
                "-target-disconnect" | "-target-detach" | "-gdb-exit"
            )
        {
            // Opaque CLI/configuration commands can redirect a GDB connection.
            self.connected_gdb_endpoint = None;
        }
        if let Some(access) = self.register_value_access.as_mut()
            && access.command == command
        {
            access.phase = crate::registers::provenance::Phase::Started;
            access.timestamp_ms = Stamp::now().elapsed_ms(self.session_started);
            access.completed_ms = None;
        }
        writeln!(gdb.input, "{token}{command}")
            .and_then(|_| gdb.input.flush())
            .map_err(|e| format!("GDB input: {e}"))?;
        self.log("mi>", format!("{token}{command}"));
        let deadline = Instant::now() + timeout;
        loop {
            self.flush_logs();
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                for lease in &mut leases {
                    lease.quarantine("GDB request timed out; target state is unknown");
                }
                self.state(state::FAULT);
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
                    if let Some(access) = self.register_value_access.as_mut()
                        && access.command == command
                    {
                        access.phase = crate::registers::provenance::Phase::Responded;
                        access.completed_ms = Some(Stamp::now().elapsed_ms(self.session_started));
                    }
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
    fn commands(&mut self, commands: &[String], timeout: Duration) -> Result<(), String> {
        for command in commands {
            // CLI execution commands can leave GDB in foreground mode and make
            // subsequent MI interrupts hang, even with mi-async enabled.
            let command = if let Some(method) = execution_alias(command.trim()) {
                format!("-exec-{method}")
            } else if command.starts_with('-') {
                command.clone()
            } else {
                format!("-interpreter-exec console {}", mi::quote(command))
            };
            self.request(&command, timeout)?;
        }
        Ok(())
    }
    fn sync_target_state(&mut self) -> Result<(), String> {
        let threads = self.mi("-thread-info")?;
        let running = threads
            .data
            .field("threads")
            .is_some_and(|v| v.items().iter().any(|t| t.string("state") == "running"));
        if running {
            self.state(state::RUNNING);
        } else if self.mi("-stack-info-frame").is_ok() {
            self.snapshot.generation += 1;
            self.state(state::STOPPED);
            self.refresh_pending = false;
            self.refresh()?;
        } else {
            // Breakpoints exist before a local inferior is started too.
            self.refresh_breakpoints()?;
            self.state(state::READY);
        }
        Ok(())
    }
    fn stopped(&self) -> Result<(), String> {
        if self.snapshot.state == state::STOPPED {
            Ok(())
        } else {
            Err(format!(
                "Target must be stopped (currently {})",
                self.snapshot.state
            ))
        }
    }
    fn inactive(&self) -> Result<(), String> {
        if self.gdb.is_some()
            && matches!(self.snapshot.state.as_str(), state::READY | state::STOPPED)
        {
            Ok(())
        } else {
            Err("Operation requires a connected inactive or stopped target".into())
        }
    }
    fn start_server(&mut self) -> Result<(), String> {
        let Some(service) = self.project.service.clone() else {
            return Ok(());
        };
        if !service.enabled {
            return Ok(());
        }
        self.state(state::STARTING_SERVER);
        let mut c = Command::new(&service.command);
        c.args(&service.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(cwd) = &service.cwd {
            c.current_dir(cwd);
        }
        hidden(&mut c);
        tool_environment(&mut c, &service.env, &service.unset_env);
        let mut child = c.spawn().map_err(|e| {
            format!(
                "Start environment service {}: {e}",
                service.command.display()
            )
        })?;
        self.job
            .as_ref()
            .ok_or("Process job missing")?
            .attach(&mut child)?;
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        if service.ready.is_empty() {
            let _ = ready_tx.try_send(());
        }
        let streams: [(Box<dyn Read + Send>, &str); 2] = [
            (Box::new(child.stdout.take().unwrap()), "server"),
            (Box::new(child.stderr.take().unwrap()), "server-error"),
        ];
        for (mut stream, channel) in streams {
            let logs = self.logs.clone();
            let ready_markers = service.ready.clone();
            let ready_tx = ready_tx.clone();
            // The CLI server's final readiness prompt may not end with a newline.
            thread::spawn(move || {
                let mut buf = [0u8; 4096];
                let mut pending = String::new();
                loop {
                    let n = match stream.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    pending.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if ready_markers.iter().any(|marker| pending.contains(marker)) {
                        let _ = ready_tx.try_send(());
                    }
                    while let Some(end) = pending.find('\n') {
                        let line = pending[..end].trim_end_matches('\r').to_owned();
                        pending.drain(..=end);
                        logs.push(channel, line);
                    }
                    if pending.len() > 1024 * 1024 {
                        logs.push(channel, std::mem::take(&mut pending));
                    }
                }
                if !pending.is_empty() {
                    logs.push(channel, pending);
                }
            });
        }
        self.server = Some(ServerProcess { child });
        let deadline = Instant::now() + Duration::from_millis(service.timeout_ms);
        loop {
            if self.cancellation.load(Ordering::Relaxed) {
                return Err("Connection cancelled".into());
            }
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
        self.project.prepare()?;
        crate::debug_access::recover(&self.project)?;
        self.register_session = registers::new_session();
        self.watch_bindings.clear();
        self.register_access_fault = None;
        self.register_catalogue = self.project.registers.load();
        self.snapshot = Snapshot::default();
        self.snapshot.register_session = self.register_session;
        self.reg_names.clear();
        self.refresh_pending = false;
        if self.job.is_none() {
            self.job = Some(crate::process::Job::new()?);
        }
        // Drain the preceding session before rotating its file on reconnect.
        self.flush_logs();
        let stamp = Stamp::now();
        self.trace = self
            .project
            .session
            .log_dir
            .as_ref()
            .map(|dir| Trace::open(dir, &stamp))
            .transpose()
            .map_err(|e| e.to_string())?;
        self.session_started = stamp.at;
        if let Some(trace) = &self.trace {
            self.log(
                "session",
                format!(
                    "Log: {} (wall clock; +elapsed uses a monotonic clock)",
                    trace.path.display()
                ),
            );
        }
        let result = self.connect_inner();
        if result.is_err() {
            self.gdb.take();
            self.connected_gdb_endpoint = None;
            self.server.take();
            self.state(state::FAULT);
        }
        result
    }
    fn connect_inner(&mut self) -> Result<Json, String> {
        self.start_server()?;
        self.state(state::STARTING_GDB);
        let mut c = Command::new(&self.project.gdb.executable);
        c.args(&self.project.gdb.args)
            .args(["-nx", "-q", "--interpreter=mi2"]);
        if let Some(cwd) = &self.project.gdb.cwd {
            c.current_dir(cwd);
        }
        c.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        hidden(&mut c);
        tool_environment(&mut c, &self.project.gdb.env, &self.project.gdb.unset_env);
        let mut child = c.spawn().map_err(|e| format!("Start GDB: {e}"))?;
        self.job
            .as_ref()
            .ok_or("Process job missing")?
            .attach(&mut child)?;
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        log_reader(child.stderr.take().unwrap(), "gdb-error", self.logs.clone());
        let (tx, rx) = mpsc::sync_channel(256);
        let bell = self.inbox.clone();
        thread::spawn(move || {
            let send = |incoming| {
                let sent = tx.send(incoming).is_ok();
                bell.ring();
                sent
            };
            let mut reader = BufReader::new(stdout);
            let mut line = Vec::new();
            loop {
                line.clear();
                match (&mut reader).take(1024 * 1024).read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if n >= 1024 * 1024 {
                            send(Incoming::Invalid("MI record exceeds 1 MiB".into()));
                            break;
                        }
                        let parsed = mi::parse(&String::from_utf8_lossy(&line));
                        match parsed {
                            Ok(Some(r)) => {
                                if !send(Incoming::Record(r)) {
                                    return;
                                }
                            }
                            Ok(None) => {}
                            Err(e) => {
                                if !send(Incoming::Invalid(e)) {
                                    return;
                                }
                            }
                        }
                    }
                }
            }
            send(Incoming::Closed);
        });
        self.gdb = Some(Gdb {
            child,
            input,
            records: rx,
            token: 0,
        });
        for command in ["-gdb-set pagination off", "-gdb-set confirm off"] {
            self.mi(command)?;
        }
        if let Err(e) = self.mi("-gdb-set mi-async on") {
            self.log(
                "capability",
                format!("Asynchronous execution unavailable: {e}"),
            );
        }
        if !self.project.program.elf.as_os_str().is_empty() {
            self.mi(&format!(
                "-file-exec-and-symbols {}",
                mi::quote(&portable_path(&self.project.program.elf))
            ))?;
        }
        for map in crate::source::gdb_maps(self.project.effective_source_maps()) {
            self.console(&format!(
                "set substitute-path {} {}",
                mi::quote(&map.from),
                mi::quote(&portable_path(&map.to))
            ))?;
        }
        self.commands(
            &self.project.gdb.init.clone(),
            Duration::from_millis(self.project.session.timeout_ms),
        )?;
        self.state(state::CONNECTING);
        // -target-select forwards its target arguments verbatim; quoting an endpoint
        // makes this GDB interpret it as a serial-device filename.
        if self.project.target.mode != "local" {
            self.mi(&format!(
                "-target-select {} {}",
                self.project.target.mode, self.project.target.endpoint
            ))?;
            self.connected_gdb_endpoint = Some(self.project.target.endpoint.clone());
        }
        self.commands(
            &self.project.target.after_connect.clone(),
            Duration::from_millis(self.project.session.timeout_ms),
        )?;
        self.snapshot.async_supported = self.mi("-list-target-features").ok().is_some_and(|r| {
            r.data
                .field("features")
                .is_some_and(|v| v.items().iter().any(|x| x.text() == "async"))
        });
        self.log(
            "session",
            format!("MI target async support: {}", self.snapshot.async_supported),
        );
        self.snapshot.stop_reason = "connected".into();
        self.restore_breakpoints();
        self.refresh_pending = false;
        self.sync_target_state()?;
        Ok(
            json!({"connected":true,"async_supported":self.snapshot.async_supported,"gdb":portable_path(&self.project.gdb.executable),"state":self.snapshot.state}),
        )
    }
    // An interrupt acknowledgement is not a stop notification. GDB can already
    // have stopped at a breakpoint by the time it handles the interrupt, in
    // which case another *stopped notification may never arrive.
    fn reconcile_stop(&mut self, timeout: Duration) -> Result<bool, String> {
        let response = match self.request("-thread-info", timeout) {
            Ok(response) => response,
            // Some remote GDBs advertise async MI but reject thread queries while
            // automatically continuing past ignored/conditional breakpoints.
            // This is a running state, not a failed wait or a reason to interrupt.
            Err(error)
                if error.contains("Cannot execute this command while the target is running") =>
            {
                return Ok(false);
            }
            Err(error) => return Err(error),
        };
        if self.snapshot.state == state::STOPPED {
            return Ok(true); // A concurrent async notification is authoritative.
        }
        let threads = response
            .data
            .field("threads")
            .ok_or("GDB omitted thread states")?
            .items();
        if threads.is_empty() {
            self.snapshot.generation += 1;
            self.snapshot.frame = Frame::default();
            self.snapshot.stop_reason = "no-inferior".into();
            self.refresh_pending = false;
            self.state(state::READY);
            return Ok(true);
        }
        if !threads
            .iter()
            .all(|thread| thread.string("state") == "stopped")
        {
            return Ok(false);
        }
        self.snapshot.generation += 1;
        self.snapshot.assembly.clear();
        self.snapshot.memory.clear();
        self.snapshot.stop_reason = "state-synchronized".into();
        self.refresh_pending = true;
        self.log(
            "session",
            "GDB confirms stopped threads; synchronizing the session without reconnecting.",
        );
        self.state(state::STOPPED);
        Ok(true)
    }
    fn interrupt_target(&mut self) -> Result<(), String> {
        if let Err(error) = self.mi("-exec-interrupt --all") {
            if self.snapshot.state == state::FAULT {
                return Err(error);
            }
            // Includes the valid race where the target stopped just before
            // interrupt and GDB replies "Inferior not executing".
            if self.snapshot.state != state::STOPPED
                && !self.reconcile_stop(Duration::from_millis(self.project.session.timeout_ms))?
            {
                return Err(error);
            }
        }
        Ok(())
    }
    fn pause(&mut self) -> Result<Json, String> {
        if self.snapshot.state == state::STOPPED {
            return Ok(json!({"stopped":true}));
        }
        self.log(
            "session",
            format!("Pause requested; cached state={}", self.snapshot.state),
        );
        self.interrupt_target()?;
        self.wait_for_stop(Duration::from_secs(5), true)
    }
    fn wait_stopped(&mut self, timeout: Duration) -> Result<Json, String> {
        self.wait_for_stop(timeout, false)
    }
    fn wait_for_stop(&mut self, timeout: Duration, retry_interrupt: bool) -> Result<Json, String> {
        let started = Instant::now();
        let deadline = Instant::now() + timeout;
        let mut check_at = started + Duration::from_millis(250);
        let mut retried = false;
        loop {
            if self.snapshot.state == state::READY
                && (self.snapshot.stop_reason.starts_with("exited")
                    || self.snapshot.stop_reason == "no-inferior")
            {
                return Ok(
                    json!({"reason":self.snapshot.stop_reason,"state":state::READY,"frame":self.snapshot.frame}),
                );
            }
            if self.snapshot.state == state::STOPPED {
                self.refresh_pending = false;
                self.refresh()?;
                return Ok(json!({"reason":self.snapshot.stop_reason,"frame":self.snapshot.frame}));
            }
            if self.snapshot.state == state::FAULT {
                return Err("Target connection lost".into());
            }
            if Instant::now() >= deadline {
                return Err("Timed out waiting for target to stop; it may still be running".into());
            }
            if Instant::now() >= check_at {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining >= Duration::from_millis(100) {
                    if self.reconcile_stop(
                        remaining.min(Duration::from_millis(self.project.session.timeout_ms)),
                    )? {
                        continue;
                    }
                    if retry_interrupt && !retried && started.elapsed() >= Duration::from_secs(1) {
                        self.log(
                            "session",
                            "GDB still reports running threads; retrying interrupt once.",
                        );
                        self.interrupt_target()?;
                        retried = true;
                    }
                }
                check_at = Instant::now() + Duration::from_millis(250);
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
            let frame = Frame::from_mi(f);
            if frame.address != self.snapshot.frame.address
                || frame.level != self.snapshot.frame.level
            {
                self.snapshot.assembly.clear();
                self.snapshot.memory.clear();
            }
            self.snapshot.frame = frame;
        }
        self.invalidate_register_samples();
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
                            tree: Some(WatchTree {
                                type_name: x.string("type"),
                                ..Default::default()
                            }),
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
        for index in 0..self.snapshot.locals.len() {
            let name = self.snapshot.locals[index].name.clone();
            if self
                .watch_expansions
                .contains_key(&format!("locals:{name}"))
            {
                self.snapshot.locals[index] = self.read_variable_tree(&name, true);
            }
        }
        self.refresh_watches();
        // Catalogue mode reads only what the UI/headless client explicitly asks
        // for. Preserve the original dynamic GDB list for projects without one.
        if matches!(self.register_catalogue, Ok(None)) {
            if let Ok(names) = self.mi("-data-list-register-names") {
                self.reg_names = names
                    .data
                    .field("register-names")
                    .map(|v| v.items().iter().map(|x| x.text().to_owned()).collect())
                    .unwrap_or_default();
            }
            let indices = register_indices(&self.reg_names, &self.project.gdb.registers)?;
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
                                        .get(
                                            r.string("number")
                                                .parse::<usize>()
                                                .unwrap_or(usize::MAX),
                                        )
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
                                        ..Default::default()
                                    }
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                }
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
                    .map(|v| {
                        let mut b = Breakpoint::from_mi(v);
                        b.group = self.breakpoint_groups.get(&b.id).cloned();
                        b
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.breakpoint_groups
            .retain(|id, _| self.snapshot.breakpoints.iter().any(|b| &b.id == id));
        self.snapshot
            .breakpoints
            .extend(self.unresolved_breakpoints.clone());
        Ok(())
    }
    fn disconnect(&mut self) -> Result<Json, String> {
        self.watch_bindings.clear();
        self.connected_gdb_endpoint = None;
        self.memory_connections.clear();
        self.rpc_echo = Default::default();
        // Remote all-stop GDB refuses detach after continue. Release that
        // connection with disconnect instead. Native detach itself resumes the
        // inferior, so it must be issued while the inferior is still stopped.
        let resume_remote =
            self.project.session.on_exit == "resume" && self.project.target.mode != "local";
        let mut failure = None;
        if self.gdb.is_some() {
            if self.snapshot.state == state::RUNNING
                && let Err(e) = self.pause()
                && self.snapshot.state != state::READY
            {
                failure = Some(e);
            }
            if matches!(self.snapshot.state.as_str(), state::READY | state::STOPPED) {
                // Query GDB: Console edits in READY do not trigger stopped refreshes.
                if let Err(e) = self.refresh_breakpoints() {
                    failure = Some(e);
                } else {
                    self.remember_breakpoints();
                }
            }
            if self.snapshot.state == state::STOPPED {
                if let Err(e) = self.console("delete breakpoints") {
                    failure = Some(e);
                }
                if let Err(e) = self.commands(
                    &self.project.actions.before_disconnect.clone(),
                    Duration::from_millis(self.project.session.timeout_ms),
                ) {
                    failure = Some(e);
                }
                if resume_remote && let Err(e) = self.mi("-exec-continue") {
                    failure = Some(e);
                }
            }
            let attached = matches!(
                self.snapshot.state.as_str(),
                state::STOPPED | state::RUNNING
            ) || self.project.target.mode != "local";
            self.state(state::DISCONNECTING);
            let command = if self.project.session.on_exit == "disconnect" || resume_remote {
                "-target-disconnect"
            } else {
                "-target-detach"
            };
            if attached && let Err(e) = self.mi(command) {
                failure = Some(e);
            }
            if let Err(e) = self.mi("-gdb-exit") {
                failure = Some(e);
            } else if let Some(gdb) = self.gdb.as_mut()
                && let Err(e) = gdb.wait_for_exit()
            {
                failure = Some(e);
            }
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
        if (self.watch_names != self.project.watch
            || self.saved_breakpoints != self.project.breakpoints)
            && let Err(e) = self.persist_preferences()
        {
            self.log("error", format!("Save preferences: {e}"));
        }
        self.state(if failure.is_some() {
            state::FAULT
        } else {
            state::DISCONNECTED
        });
        if let Some(e) = failure {
            Err(format!(
                "Disconnected with errors; target state must be checked: {e}"
            ))
        } else {
            Ok(json!({"disconnected":true}))
        }
    }
    fn persist_preferences(&mut self) -> Result<(), String> {
        self.project
            .save_preferences(self.watch_names.clone(), self.saved_breakpoints.clone())?;
        // A breakpoint edit also saves Watch. Compare later edits with that
        // successful save, so returning to the startup list still gets written.
        self.project.watch.clone_from(&self.watch_names);
        self.project.breakpoints.clone_from(&self.saved_breakpoints);
        Ok(())
    }
    fn execute(&mut self, method: &str, p: &Json) -> Result<Json, String> {
        if matches!(
            method,
            method::REGISTERS_READ
                | method::REGISTERS_PROBE
                | method::REGISTERS_SELECT
                | method::REGISTERS_MPU
                | method::REGISTERS_CACHE
        ) {
            self.check_register_read_cancelled()?;
        }
        let text = |key: &str| {
            p.get(key)
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        match method {
            method::UI_PREFERENCES => {
                let mut ui: crate::config::Ui =
                    serde_json::from_value(p.clone()).map_err(|e| e.to_string())?;
                if !ui.register_views.is_empty() {
                    return Err("Use register_preferences to update one register view".into());
                }
                ui.register_views = self.project.ui.register_views.clone();
                if ui
                    .refresh
                    .values()
                    .any(|p| p.interval_ms != 0 && !(50..=60000).contains(&p.interval_ms))
                {
                    return Err("Refresh interval must be 0 or 50..60000 ms".into());
                }
                for range in ui.memory.values() {
                    range.validate()?;
                }
                crate::config::validate_register_views(&ui.register_views)?;
                let saved = self.project.save_ui(&ui)?;
                self.project.ui = ui;
                Ok(json!({"saved":saved}))
            }
            method::REGISTER_PREFERENCES => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct ViewRequest {
                    scope: String,
                    preferences: crate::registers::display::Preferences,
                }
                let view: ViewRequest =
                    serde_json::from_value(p.clone()).map_err(|e| e.to_string())?;
                let saved = self
                    .project
                    .save_register_view(&view.scope, &view.preferences)?;
                self.project
                    .ui
                    .register_views
                    .insert(view.scope.clone(), view.preferences);
                Ok(json!({"saved":saved,"scope":view.scope}))
            }
            method::CONNECT => self.connect(),
            method::RECONNECT => {
                self.disconnect()?;
                self.connect()
            }
            method::DISCONNECT => self.disconnect(),
            method::QUIT => {
                let result = self.disconnect();
                self.exiting = true;
                result
            }
            method::STATUS => Ok(serde_json::to_value(&self.snapshot).unwrap()),
            method::REGISTERS_LIST => self.registers_list(),
            method::REGISTERS_MATRIX => self.register_matrix(),
            method::REGISTERS_PROBE => self.probe_register_capabilities(p),
            method::REGISTERS_SELECT => self.read_selected_registers(p),
            method::REGISTERS_MPU => self.mpu_regions(p),
            method::REGISTERS_CACHE => self.m_cache_view(p),
            method::REGISTERS_READ => self.read_registers(p),
            method::REGISTER_BOUNDARY => {
                self.invalidate_register_boundary();
                Ok(json!({"context":self.register_context()}))
            }
            method::REGISTER_SHARED_INVALIDATE => {
                let owners = p["owners"].as_array().ok_or("Shared owners are required")?;
                for sample in &mut self.snapshot.register_samples {
                    if sample
                        .owner
                        .as_ref()
                        .is_some_and(|owner| owners.iter().any(|item| item.as_str() == Some(owner)))
                    {
                        sample.stale();
                    }
                }
                if let Some(probe) = &mut self.snapshot.register_probe {
                    for sample in &mut probe.samples {
                        if sample.owner.as_ref().is_some_and(|owner| {
                            owners.iter().any(|item| item.as_str() == Some(owner))
                        }) {
                            sample.stale();
                        }
                    }
                    probe.decode();
                }
                self.publish();
                Ok(json!({"invalidated":true}))
            }
            method::MEMORY_CHANNELS => self.memory_channels(),
            method::COMPLETE => {
                self.inactive()?;
                let input = text("text");
                if input.is_empty() || input.len() > 512 || input.chars().any(char::is_control) {
                    return Ok(json!({"matches":[]}));
                }
                let expression = p.get("expression").and_then(Json::as_bool).unwrap_or(false);
                // Symbol queries never evaluate expressions or read target memory.
                // Bare watch names use variables only, excluding function names.
                if expression
                    && input.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    && let Ok(record) = self.mi(&format!(
                        "-symbol-info-variables --name {} --max-results 64",
                        mi::quote(&format!("^{input}"))
                    ))
                {
                    let mut names = Vec::new();
                    if let Some(debug) = record.data.field("symbols").and_then(|v| v.field("debug"))
                    {
                        for file in debug.items() {
                            if let Some(symbols) = file.field("symbols") {
                                names.extend(symbols.items().iter().map(|s| s.string("name")));
                            }
                        }
                    }
                    names.retain(|s| !s.is_empty() && !s.chars().any(char::is_control));
                    names.sort();
                    names.dedup();
                    names.truncate(64);
                    return Ok(json!({"matches":names}));
                }
                let query = if expression {
                    format!("print {input}")
                } else {
                    input
                };
                let record = self.mi(&format!("-complete {}", mi::quote(&query)))?;
                let mut names: Vec<String> = record
                    .data
                    .field("matches")
                    .into_iter()
                    .flat_map(|v| v.items())
                    .filter_map(|v| {
                        let value = if expression {
                            v.text().strip_prefix("print ")?
                        } else {
                            v.text()
                        };
                        (!value.is_empty() && !value.chars().any(char::is_control))
                            .then(|| value.to_owned())
                    })
                    .take(64)
                    .collect();
                names.sort();
                names.dedup();
                Ok(json!({"matches":names}))
            }
            method::CONTINUE => {
                if self.snapshot.state == state::READY {
                    return self.execute(method::RUN, p);
                }
                self.stopped()?;
                let generation = self.snapshot.generation;
                self.mi("-exec-continue")?;
                if self.snapshot.generation == generation {
                    self.state(state::RUNNING);
                }
                Ok(json!({"running":self.snapshot.state==state::RUNNING}))
            }
            method::RUN => {
                if !matches!(self.snapshot.state.as_str(), state::READY | state::STOPPED) {
                    return Err("Run requires a connected, inactive or stopped target".into());
                }
                let generation = self.snapshot.generation;
                let commands = if self.project.actions.run.is_empty() {
                    vec!["-exec-run".into()]
                } else {
                    self.project.actions.run.clone()
                };
                self.commands(
                    &commands,
                    Duration::from_millis(self.project.session.timeout_ms),
                )?;
                if self.snapshot.generation == generation {
                    self.state(state::RUNNING);
                }
                Ok(json!({"running":self.snapshot.state==state::RUNNING}))
            }
            method::PAUSE => self.pause(),
            method::WAIT_STOPPED => self.wait_stopped(Duration::from_millis(
                p.get("timeout_ms")
                    .and_then(Json::as_u64)
                    .unwrap_or(10000)
                    .min(60000),
            )),
            method::STEP | method::NEXT | method::STEPI | method::FINISH => {
                self.stopped()?;
                let generation = self.snapshot.generation;
                let cmd = match method {
                    method::STEP => "-exec-step",
                    method::NEXT => "-exec-next",
                    method::STEPI => "-exec-step-instruction",
                    _ => "-exec-finish",
                };
                self.mi(cmd).map_err(|error| {
                    format!(
                        "{method} at {} ({}): {error}",
                        self.snapshot.frame.address, self.snapshot.frame.function
                    )
                })?;
                if self.snapshot.generation == generation {
                    self.state(state::RUNNING);
                }
                Ok(json!({"running":self.snapshot.state==state::RUNNING}))
            }
            method::SYNCHRONIZE => {
                self.inactive()?;
                // This fixed internal command only invalidates GDB's value cache.
                // Arbitrary user Console commands still clear endpoint evidence.
                let endpoint = self.connected_gdb_endpoint.clone();
                self.console("maintenance flush register-cache")?;
                self.connected_gdb_endpoint = endpoint;
                self.sync_target_state()?;
                Ok(json!({"state":self.snapshot.state}))
            }
            method::RESTART_SHARED => {
                self.stopped()?;
                if self.project.multicore.restart.is_empty() {
                    return Err("Shared reset is not configured".into());
                }
                self.invalidate_register_boundary();
                self.commands(
                    &self.project.multicore.restart.clone(),
                    Duration::from_millis(self.project.session.timeout_ms),
                )?;
                // The coordinator refreshes every connection after this command.
                Ok(json!({"restarted":true}))
            }
            method::RESTART => {
                if self.project.actions.restart.is_empty() {
                    return Err("Restart is not configured by this environment".into());
                }
                self.stopped()?;
                self.invalidate_register_boundary();
                self.commands(
                    &self.project.actions.restart.clone(),
                    Duration::from_millis(self.project.session.timeout_ms),
                )?;
                self.sync_target_state()?;
                Ok(json!({"restarted":true,"state":self.snapshot.state}))
            }
            method::EVALUATE => {
                self.stopped()?;
                let r = self.mi(&format!(
                    "-data-evaluate-expression {}",
                    mi::quote(&text("expression"))
                ))?;
                Ok(json!({"value":r.data.string("value")}))
            }
            method::WATCH => {
                let expr = text("expression");
                if expr.is_empty() {
                    return Err("Expression is required".into());
                }
                if self.watch_names.len() >= 64 && !self.watch_names.contains(&expr) {
                    return Err("Watch limit is 64 expressions".into());
                }
                if !self.watch_names.contains(&expr) {
                    self.watch_names.push(expr.clone());
                    if self.snapshot.state != state::STOPPED {
                        self.snapshot.watches.push(Variable {
                            name: expr,
                            value: "<available after next stop>".into(),
                            ..Default::default()
                        });
                        self.publish();
                    }
                }
                if self.snapshot.state == state::STOPPED {
                    self.refresh()?;
                }
                Ok(json!({"watches":self.watch_names}))
            }
            method::UNWATCH => {
                self.watch_names.retain(|s| s != &text("expression"));
                self.watch_expansions.remove(&text("expression"));
                self.snapshot
                    .watches
                    .retain(|v| self.watch_names.contains(&v.name));
                self.publish();
                Ok(json!({"watches":self.watch_names}))
            }
            method::WATCH_EXPAND => self.expand_watch(p),
            method::WATCH_RESOLVE => self.resolve_watch(p),
            method::LOCAL_EXPAND => self.expand_local(p),
            method::MEMORY_READ => self.read_memory_channel(p),
            method::MEMORY_DUMP => self.read_memory_dump(p),
            method::WRITE_PREVIEW => self.preview_write(p),
            method::WRITE_APPLY => self.apply_write(p),
            method::WRITE_INVALIDATE => {
                self.invalidate_written_views();
                Ok(json!({"invalidated":true}))
            }
            method::WRITE_CANCEL => self.cancel_write(p),
            method::WRITE_DISCARD => {
                self.write_drafts.clear();
                Ok(json!({"discarded":true}))
            }
            method::BREAK
            | method::DATA_BREAK
            | method::DELETE_BREAK
            | method::ENABLE_BREAK
            | method::UPDATE_BREAK
            | method::BREAK_APPLY => self.breakpoint_command(method, p),
            method::FRAME => {
                self.write_drafts.clear();
                self.stopped()?;
                let index = p.get("level").and_then(Json::as_u64).unwrap_or(0);
                self.snapshot.memory_selection_epoch =
                    self.snapshot.memory_selection_epoch.wrapping_add(1);
                self.watch_bindings.clear();
                self.publish();
                self.mi(&format!("-stack-select-frame {index}"))?;
                self.refresh()?;
                Ok(json!({"frame":self.snapshot.frame}))
            }
            method::REFRESH => {
                self.refresh()?;
                Ok(json!({"refreshed":true}))
            }
            method::PERIPHERAL_READ => {
                if p["channel"]
                    .as_str()
                    .is_some_and(|channel| !channel.is_empty())
                {
                    return Err("Use memory_read to select an explicit bus channel".into());
                }
                self.read_memory_channel(p)
            }
            method::MEMORY => {
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
            method::DISASSEMBLE => {
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
                            .map(|i| {
                                format!(
                                    "{}  {}",
                                    i.string("address"),
                                    i.string("inst").replace('\t', "    ")
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                self.publish();
                Ok(json!({"assembly":self.snapshot.assembly}))
            }
            method::SYMBOLS => self.symbols(&text("query")),
            method::FILES => {
                let r = self.mi("-file-list-exec-source-files")?;
                let mut files: Vec<String> = r
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
                files.sort();
                files.dedup();
                // Shared, not copied, by every later Snapshot publish.
                self.snapshot.files = files.into();
                self.publish();
                Ok(json!({"files":&*self.snapshot.files}))
            }
            method::DOWNLOAD => {
                if !self.project.tasks.download.trim().is_empty() {
                    return self.project_task("download");
                }
                if self.project.actions.download.is_empty() {
                    return Err("Download is not configured by this environment".into());
                }
                self.stopped()?;
                self.state(state::DOWNLOADING);
                let result = self.commands(
                    &self.project.actions.download.clone(),
                    Duration::from_secs(90),
                );
                if self.snapshot.state != state::FAULT {
                    self.state(state::STOPPED);
                }
                result?;
                self.sync_target_state()?;
                Ok(json!({"downloaded":true}))
            }
            method::CONSOLE => {
                self.write_drafts.clear();
                let command = text("command");
                let trimmed = command.trim();
                match trimmed {
                    "c" | "continue" => {
                        return self.execute(method::CONTINUE, &Json::Null);
                    }
                    "s" | "step" => return self.execute(method::STEP, &Json::Null),
                    "n" | "next" => return self.execute(method::NEXT, &Json::Null),
                    "si" | "stepi" => return self.execute(method::STEPI, &Json::Null),
                    "interrupt" => return self.execute(method::PAUSE, &Json::Null),
                    "q" | "quit" => return self.execute(method::QUIT, &Json::Null),
                    "run" | "r" => return self.execute(method::RUN, &Json::Null),
                    _ => {}
                }
                if !matches!(self.snapshot.state.as_str(), state::READY | state::STOPPED) {
                    return Err("Console requires a connected inactive or stopped target".into());
                }
                // Console may change registers, select a frame, replace symbols or reset a chip;
                // even an error can follow a partial command. Never retain valid pre-command data.
                self.invalidate_register_boundary();
                let r = self.console(&command)?;
                self.refresh_pending = true;
                Ok(json!({"result":r.data}))
            }
            method::SET_ELF => {
                self.write_drafts.clear();
                if self.gdb.is_some() {
                    return Err("Disconnect before changing ELF".into());
                }
                self.invalidate_register_boundary();
                self.project.program.elf = text("path").into();
                Ok(json!({"elf":self.project.program.elf}))
            }
            method::BUILD => self.project_task("build"),
            _ => Err(format!("Unknown method: {method}")),
        }
    }
}
