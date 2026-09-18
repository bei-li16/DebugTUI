//! Multi-core session coordinator.
//!
//! Wraps one or more [`session::EngineHandle`] instances behind the same
//! `EngineHandle` interface, so the rest of the application (UI, headless,
//! JSONL) does not change.  When the project has no `[[cores]]` array the
//! coordinator degenerates to a transparent single-engine pass-through,
//! preserving the original single-core behaviour exactly.

use crate::{
    config::{Core, Project, Service, SyncConfig},
    session::{self, Event, Request, EngineHandle},
};
use serde_json::{json, Value as Json};
use std::{
    io::Read,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

pub fn spawn(project: Project) -> EngineHandle {
    let (commands, requests) = mpsc::channel();
    let (events_tx, events_rx) = mpsc::sync_channel(512);
    let cancellation = Arc::new(AtomicBool::new(false));
    let cancel = cancellation.clone();
    thread::spawn(move || {
        let mut coord = Coordinator::new(project, events_tx, cancel);
        coord.run(requests);
    });
    EngineHandle {
        commands,
        events: events_rx,
        cancellation,
    }
}

struct EngineEntry {
    name: String,
    handle: EngineHandle,
}

struct PendingConnect {
    request_id: u64,
    responded: Vec<bool>,
    first_error: Option<String>,
}

struct Coordinator {
    engines: Vec<EngineEntry>,
    live_watch: Option<crate::live_watch::LiveWatchHandle>,
    active: usize,
    events: SyncSender<Event>,
    exiting: bool,
    server: Option<Child>,
    service: Option<Service>,
    sync: Option<SyncConfig>,
    tcl_endpoint: Option<String>,
    job: Option<crate::process::Job>,
    cancellation: Arc<AtomicBool>,
    pending_connect: Option<PendingConnect>,
    session_started: Instant,
}

impl Coordinator {
    fn new(project: Project, events: SyncSender<Event>, cancellation: Arc<AtomicBool>) -> Self {
        let multi = !project.cores.is_empty();
        let service = if multi { project.service.clone() } else { None };
        let sync = if multi { project.sync.clone() } else { None };
        let tcl_endpoint = project.live_watch.as_ref().map(|lw| lw.tcl_endpoint.clone());
        let mut engines = Vec::new();
        if project.cores.is_empty() {
            let handle = session::spawn(project.clone());
            engines.push(EngineEntry { name: "default".into(), handle });
        } else {
            let mut sorted: Vec<&Core> = project.cores.iter().collect();
            sorted.sort_by_key(|c| c.startup_order);
            for core in &sorted {
                let derived = derive_project(&project, core);
                let handle = session::spawn(derived);
                engines.push(EngineEntry { name: core.name.clone(), handle });
            }
        }
        // Default to the core with the highest startup_order (the "main" core
        // that starts last). For THA6206 this is core.0; core.1 boots first
        // only to unblock the MULTICORE_SYNCVAR spin.
        let active = engines.len().saturating_sub(1);
        let live_watch = project.live_watch.as_ref().and_then(|lw| {
            match crate::live_watch::spawn(lw, project.watch.clone()) {
                Ok(h) => {
                    let _ = events.send(Event::Log {
                        channel: "live".into(),
                        text: format!("Live watch started: {} @ {}ms", lw.bus_target, lw.interval_ms),
                        timestamp: String::new(),
                        elapsed_ms: 0,
                    });
                    Some(h)
                }
                Err(e) => {
                    let _ = events.send(Event::Log {
                        channel: "live".into(),
                        text: format!("Live watch disabled: {e}"),
                        timestamp: String::new(),
                        elapsed_ms: 0,
                    });
                    None
                }
            }
        });
        Self {
            engines,
            live_watch,
            active,
            events,
            exiting: false,
            server: None,
            service,
            sync,
            tcl_endpoint,
            job: None,
            cancellation,
            pending_connect: None,
            session_started: Instant::now(),
        }
    }

    fn log(&self, channel: &str, text: String) {
        let stamp = crate::logging::Stamp::now();
        let elapsed = stamp.elapsed_ms(self.session_started);
        let _ = self.events.send(Event::Log {
            channel: channel.into(),
            text,
            timestamp: stamp.wall,
            elapsed_ms: elapsed,
        });
    }

    // -- Service management (multi-core only) --

    fn start_service(&mut self) -> Result<(), String> {
        let Some(service) = self.service.clone() else { return Ok(()) };
        if !service.enabled || self.server.is_some() {
            return Ok(());
        }
        self.log("coordinator", format!("Starting service: {}", service.command.display()));
        if self.job.is_none() {
            self.job = Some(crate::process::Job::new()?);
        }
        let mut cmd = Command::new(&service.command);
        cmd.args(&service.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(cwd) = &service.cwd {
            cmd.current_dir(cwd);
        }
        session::hidden(&mut cmd);
        session::tool_environment(&mut cmd, &service.env, &service.unset_env);
        let mut child = cmd.spawn()
            .map_err(|e| format!("Start service {}: {e}", service.command.display()))?;
        self.job.as_ref().unwrap().attach(&mut child)?;

        let (ready_tx, ready_rx) = mpsc::sync_channel::<()>(1);
        if service.ready.is_empty() {
            let _ = ready_tx.try_send(());
        }
        let streams: [(Box<dyn Read + Send>, &str); 2] = [
            (Box::new(child.stdout.take().unwrap()), "server"),
            (Box::new(child.stderr.take().unwrap()), "server-error"),
        ];
        for (mut stream, channel) in streams {
            let markers = service.ready.clone();
            let tx = ready_tx.clone();
            let events = self.events.clone();
            thread::spawn(move || {
                let mut buf = [0u8; 4096];
                let mut pending = String::new();
                loop {
                    let n = match stream.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    pending.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if markers.iter().any(|m| pending.contains(m.as_str())) {
                        let _ = tx.try_send(());
                    }
                    while let Some(end) = pending.find('\n') {
                        let line = pending[..end].trim_end_matches('\r').to_owned();
                        pending.drain(..=end);
                        let _ = events.send(Event::Log {
                            channel: channel.into(),
                            text: line,
                            timestamp: String::new(),
                            elapsed_ms: 0,
                        });
                    }
                }
            });
        }
        self.server = Some(child);
        let deadline = Instant::now() + Duration::from_millis(service.timeout_ms);
        loop {
            if self.cancellation.load(Ordering::Relaxed) {
                return Err("Connection cancelled".into());
            }
            if ready_rx.try_recv().is_ok() {
                self.log("coordinator", "Service ready".into());
                return Ok(());
            }
            if let Some(status) = self.server.as_mut().unwrap().try_wait().map_err(|e| e.to_string())? {
                return Err(format!("Service exited ({status})"));
            }
            if Instant::now() > deadline {
                return Err("Service startup timed out".into());
            }
            thread::sleep(Duration::from_millis(25));
        }
    }

    fn stop_service(&mut self) {
        if let Some(mut server) = self.server.take() {
            let _ = server.kill();
            let _ = server.wait();
        }
    }

    fn run_sync_open(&mut self) -> Result<(), String> {
        let Some(sync) = &self.sync else { return Ok(()) };
        if sync.open.is_empty() {
            return Ok(());
        }
        let Some(endpoint) = &self.tcl_endpoint else {
            self.log("coordinator", "Sync open skipped: no live_watch.tcl_endpoint".into());
            return Ok(());
        };
        self.log("coordinator", format!("Sync open: {} TCL commands via {}", sync.open.len(), endpoint));
        crate::live_watch::send_tcl_commands(endpoint, &sync.open)
    }

    // -- Main loop --

    fn run(&mut self, requests: Receiver<Request>) {
        let total = self.engines.len();
        let mut exited = 0usize;
        while exited < total {
            // Phase 1: drain events (no self borrows)
            let mut drained: Vec<(usize, Event)> = Vec::new();
            for i in 0..total {
                while let Ok(event) = self.engines[i].handle.events.try_recv() {
                    drained.push((i, event));
                }
            }
            // Drain live-watch events (directly forwarded)
            if let Some(lw) = &self.live_watch {
                while let Ok(event) = lw.events.try_recv() {
                    let _ = self.events.send(event);
                }
            }
            // Phase 2: process events
            for (i, event) in drained {
                if matches!(event, Event::Exit) {
                    exited += 1;
                    continue;
                }
                // Connect response aggregation for multi-core
                if self.pending_connect.is_some() {
                    if let Event::Response { id, ok, error, .. } = &event {
                        let pc_id = self.pending_connect.as_ref().unwrap().request_id;
                        if *id == pc_id && i < total {
                            let mut pc = self.pending_connect.take().unwrap();
                            pc.responded[i] = true;
                            if !ok && pc.first_error.is_none() {
                                pc.first_error = error.clone();
                            }
                            if pc.responded.iter().all(|r| *r) {
                                let ok = pc.first_error.is_none();
                                let err = pc.first_error.clone();
                                let _ = self.events.send(Event::Response {
                                    id: pc.request_id,
                                    ok,
                                    result: json!({
                                        "connected": ok,
                                        "core_count": total,
                                        "active_core": self.active,
                                        "core_names": self.engines.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
                                    }),
                                    error: err,
                                });
                            } else {
                                self.pending_connect = Some(pc);
                            }
                            continue;
                        }
                    }
                }
                // Forward events: single-core = all; multi-core = active engine only
                if total == 1 || i == self.active {
                    let _ = self.events.send(event);
                }
            }
            // Process UI requests
            match requests.recv_timeout(Duration::from_millis(20)) {
                Ok(req) => self.route(req),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    for e in self.engines.iter() {
                        let _ = e.handle.send(Request::new(u64::MAX, "quit", json!({})));
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
        if let Some(lw) = self.live_watch.take() {
            lw.cancellation.store(true, Ordering::Relaxed);
        }
        self.stop_service();
        let _ = self.events.send(Event::Exit);
    }

    fn route(&mut self, request: Request) {
        match request.method.as_str() {
            "select_core" => {
                if self.engines.len() > 1 {
                    let idx = request.params.get("index").and_then(Json::as_u64).unwrap_or(0) as usize;
                    if idx < self.engines.len() {
                        self.active = idx;
                        self.log("coordinator", format!("Active core: {}", self.engines[idx].name));
                    }
                }
                let _ = self.events.send(Event::Response {
                    id: request.id,
                    ok: true,
                    result: json!({
                        "active_core": self.active,
                        "core_count": self.engines.len(),
                        "core_name": self.engines.get(self.active).map(|e| e.name.as_str()).unwrap_or(""),
                    }),
                    error: None,
                });
            }
            "quit" => {
                self.exiting = true;
                for engine in self.engines.iter() {
                    let _ = engine.handle.send(Request::new(request.id, "quit", json!({})));
                }
            }
            "connect" if self.engines.len() > 1 => {
                // 1. Start the shared service
                if let Err(e) = self.start_service() {
                    let _ = self.events.send(Event::Response {
                        id: request.id,
                        ok: false,
                        result: Json::Null,
                        error: Some(e),
                    });
                    return;
                }
                // 2. Run sync.open TCL commands
                if let Err(e) = self.run_sync_open() {
                    self.log("error", format!("Sync open: {e}"));
                }
                // 3. Send connect to all engines, aggregate responses
                self.pending_connect = Some(PendingConnect {
                    request_id: request.id,
                    responded: vec![false; self.engines.len()],
                    first_error: None,
                });
                for engine in self.engines.iter() {
                    let _ = engine.handle.send(Request {
                        id: request.id,
                        method: "connect".into(),
                        params: json!({}),
                    });
                }
            }
            "run" if self.engines.len() > 1 => {
                // Multi-core: broadcast run to every engine.  Each engine's
                // run action is specific to its core (core.1 = "continue",
                // core.0 = "tbreak _main; continue").  This avoids the
                // monitor-resume path which hangs on DSCR sticky errors.
                for engine in self.engines.iter() {
                    let _ = engine.handle.send(Request {
                        id: request.id,
                        method: "run".into(),
                        params: json!({}),
                    });
                }
            }
            _ => {
                if let Some(engine) = self.engines.get(self.active) {
                    let _ = engine.handle.send(request);
                }
            }
        }
    }
}

/// Derive a per-core Project from the base.  The core's endpoint, actions,
/// and init replace or extend the base.  Service is removed because the
/// Coordinator manages the shared service for multi-core mode.
fn derive_project(base: &Project, core: &Core) -> Project {
    let mut p = base.clone();
    p.target.endpoint = core.endpoint.clone();
    if !core.after_connect.is_empty() {
        p.target.after_connect = core.after_connect.clone();
    }
    if !core.run.is_empty() {
        p.actions.run = core.run.clone();
    }
    if !core.init.is_empty() {
        let mut init = base.gdb.init.clone();
        init.extend(core.init.iter().cloned());
        p.gdb.init = init;
    }
    p.service = None;
    p.path = None;
    p
}
