//! Ordered workspace operations and independent per-core debugger state.
use crate::{
    config::{ControlScope, Core, Project},
    logging::{Stamp, Trace},
    session::{self, CoreStatus, EngineHandle, Event, Request, Snapshot, method, state},
    wake::{Doorbell, RingOnDrop},
};

mod breakpoints;
mod control;
mod registers;
mod server_log;
use serde_json::{Value as Json, json};
use std::{
    collections::{BTreeMap, VecDeque},
    io::Read,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

pub fn spawn(project: Project) -> EngineHandle {
    spawn_with(project, Doorbell::default())
}

/// Like `spawn`, ringing `notify` after each event for the caller.
pub fn spawn_with(project: Project, notify: Doorbell) -> EngineHandle {
    // No extra worker, latency or cancellation indirection for existing projects.
    if project.cores.is_empty() && project.live_watch.is_none() && project.sync.is_none() {
        return session::spawn_with(project, notify);
    }
    let (commands, requests) = mpsc::channel();
    let (events, receiver) = mpsc::sync_channel(512);
    let cancellation = Arc::new(AtomicBool::new(false));
    let cancel = cancellation.clone();
    let inbox = Doorbell::default();
    let worker_inbox = inbox.clone();
    thread::spawn(move || {
        Coordinator::new_with(project, events, cancel, worker_inbox, notify).run(requests)
    });
    EngineHandle {
        commands,
        events: receiver,
        cancellation,
        bell: RingOnDrop(inbox),
    }
}
struct EngineEntry {
    name: String,
    endpoint: String,
    handle: EngineHandle,
    snapshot: Snapshot,
    revision: u64,
    exited: bool,
    unresponsive: bool,
    launched: bool,
}
enum Step {
    StartService,
    StopService,
    Core(usize, String),
    Breakpoint(usize, Json, Option<Json>),
    Resume(usize),
    WaitGroupStop(Instant),
}
struct Batch {
    request: Request,
    steps: VecDeque<Step>,
    waiting: Option<(usize, u64, Instant)>,
    results: Vec<Json>,
    errors: Vec<String>,
    last: Json,
    recovering: bool,
    aggregate: bool,
    internal: bool,
    current_method: String,
    break_undo: Option<VecDeque<Step>>,
    shared_epochs: BTreeMap<String, u64>,
}
struct Coordinator {
    project: Project,
    engines: Vec<EngineEntry>,
    order: Vec<usize>,
    active: usize,
    revision: u64,
    next_id: u64,
    events: SyncSender<Event>,
    cancellation: Arc<AtomicBool>,
    exiting: bool,
    queue: VecDeque<Request>,
    batch: Option<Batch>,
    server: Option<Child>,
    job: Option<crate::process::Job>,
    trace: Option<Trace>,
    started: Instant,
    server_logs: Receiver<(Stamp, String, String)>,
    server_log_tx: SyncSender<(Stamp, String, String)>,
    live_watch: Option<crate::live_watch::LiveWatchHandle>,
    live_key: Option<(usize, u64, Vec<String>)>,
    group_stop: Option<usize>,
    shared_epochs: BTreeMap<String, u64>,
    shared_samples: BTreeMap<(usize, String, String), registers::SharedSample>,
    /// Rung by requests, per-core events, Live Watch and service output.
    inbox: Doorbell,
    /// The consumer's bell, rung after every event.
    notify: Doorbell,
}
impl Coordinator {
    #[cfg(test)]
    fn new(project: Project, events: SyncSender<Event>, cancellation: Arc<AtomicBool>) -> Self {
        Self::new_with(
            project,
            events,
            cancellation,
            Doorbell::default(),
            Doorbell::default(),
        )
    }
    fn new_with(
        project: Project,
        events: SyncSender<Event>,
        cancellation: Arc<AtomicBool>,
        inbox: Doorbell,
        notify: Doorbell,
    ) -> Self {
        let multi = !project.cores.is_empty();
        let cores = if multi {
            project.cores.clone()
        } else {
            vec![Core {
                name: "default".into(),
                endpoint: project.target.endpoint.clone(),
                ..Default::default()
            }]
        };
        let engines = cores
            .iter()
            .enumerate()
            .map(|(i, c)| EngineEntry {
                name: c.name.clone(),
                endpoint: c.endpoint.clone(),
                handle: session::spawn_with(
                    if multi {
                        derive_project(&project, c, i)
                    } else {
                        project.clone()
                    },
                    inbox.clone(),
                ),
                snapshot: Snapshot::default(),
                revision: 0,
                exited: false,
                unresponsive: false,
                launched: false,
            })
            .collect();
        // Stable indices follow configuration order, not startup order.
        let mut order = (0..cores.len()).collect::<Vec<_>>();
        order.sort_by_key(|&i| cores[i].startup_order);
        let active = *order.last().unwrap();
        let (server_log_tx, server_logs) = mpsc::sync_channel(512);
        Self {
            project,
            engines,
            order,
            active,
            revision: 0,
            next_id: 1,
            events,
            cancellation,
            exiting: false,
            queue: VecDeque::new(),
            batch: None,
            server: None,
            job: None,
            trace: None,
            started: Instant::now(),
            server_logs,
            server_log_tx,
            live_watch: None,
            live_key: None,
            group_stop: None,
            shared_epochs: BTreeMap::new(),
            shared_samples: BTreeMap::new(),
            inbox,
            notify,
        }
    }
    fn multi(&self) -> bool {
        !self.project.cores.is_empty()
    }
    fn emit(&self, event: Event) {
        if self.events.send(event).is_err() {
            self.cancellation.store(true, Ordering::Relaxed);
        }
        self.notify.ring();
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
            elapsed_ms: stamp.elapsed_ms(self.started),
            timestamp: stamp.wall,
        });
    }
    fn log(&mut self, channel: &str, text: String) {
        self.log_at(channel, text, Stamp::now());
    }
    fn statuses(&self) -> Vec<CoreStatus> {
        self.engines
            .iter()
            .enumerate()
            .map(|(index, e)| CoreStatus {
                index,
                name: e.name.clone(),
                endpoint: e.endpoint.clone(),
                state: e.snapshot.state.clone(),
            })
            .collect()
    }
    fn snapshot(&self) -> Snapshot {
        let mut s = self.engines[self.active].snapshot.clone();
        if self.multi() {
            s.register_owner_generations = self.shared_epochs.clone();
            s.register_generation = Some(s.generation);
            s.generation = self.engines[self.active].revision;
            s.cores = self.statuses();
            s.core = s.cores.get(self.active).cloned();
            s.control_scope = Some(self.project.multicore.scope);
            self.annotate_breakpoints(&mut s);
        }
        s
    }
    fn publish(&self) {
        self.emit(Event::Snapshot {
            snapshot: Box::new(self.snapshot()),
        });
    }
    fn info(&self) -> Json {
        json!({"active_core":self.active,"core_count":self.engines.len(),"core_name":self.engines[self.active].name,
            "core_names":self.engines.iter().map(|e| &e.name).collect::<Vec<_>>(),"cores":self.statuses(),"control_scope":self.project.multicore.scope})
    }
    fn reply(&self, id: u64, result: Json, error: Option<String>) {
        self.emit(Event::Response {
            id,
            ok: error.is_none(),
            result,
            error,
        });
    }
    fn start_service(&mut self) -> Result<(), String> {
        if !self.multi() {
            return Ok(());
        }
        if let Some(server) = &mut self.server {
            if server.try_wait().map_err(|e| e.to_string())?.is_none() {
                return Ok(());
            }
            self.stop_service();
        }
        let stamp = Stamp::now();
        self.trace = self
            .project
            .session
            .log_dir
            .as_ref()
            .map(|p| Trace::open(&p.join("coordinator"), &stamp))
            .transpose()
            .map_err(|e| e.to_string())?;
        self.started = stamp.at;
        let Some(service) = self.project.service.clone().filter(|s| s.enabled) else {
            return self.sync_open();
        };
        self.job = Some(crate::process::Job::new()?);
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
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Start service {}: {e}", service.command.display()))?;
        self.job.as_ref().unwrap().attach(&mut child)?;
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        if service.ready.is_empty() {
            let _ = ready_tx.try_send(());
        }
        let streams: [(Box<dyn Read + Send>, &str); 2] = [
            (Box::new(child.stdout.take().unwrap()), "server"),
            (Box::new(child.stderr.take().unwrap()), "server-stderr"),
        ];
        for (stream, channel) in streams {
            let markers = service.ready.clone();
            let tx = ready_tx.clone();
            let logs = self.server_log_tx.clone();
            let bell = self.inbox.clone();
            thread::spawn(move || {
                server_log::read_stream(stream, channel, &markers, tx, logs, &bell)
            });
        }
        self.server = Some(child);
        let deadline = Instant::now() + Duration::from_millis(service.timeout_ms);
        loop {
            self.flush_server_logs();
            if self.cancellation.load(Ordering::Relaxed) {
                return Err("Connection cancelled".into());
            }
            if ready_rx.try_recv().is_ok() {
                return self.sync_open();
            }
            if let Some(status) = self
                .server
                .as_mut()
                .unwrap()
                .try_wait()
                .map_err(|e| e.to_string())?
            {
                return Err(format!("Service exited ({status})"));
            }
            if Instant::now() >= deadline {
                return Err("Service startup timed out".into());
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
    /// Returns true when the batch limit stopped the flush early.
    fn flush_server_logs(&mut self) -> bool {
        for _ in 0..128 {
            let Ok((stamp, channel, text)) = self.server_logs.try_recv() else {
                return false;
            };
            self.log_at(&channel, text, stamp);
        }
        true
    }
    /// The earliest moment the current batch must be re-checked without any
    /// new event: a worker reply timeout or a group-stop deadline.
    fn next_deadline(&self) -> Option<Instant> {
        let batch = self.batch.as_ref()?;
        let group = match batch.steps.front() {
            Some(Step::WaitGroupStop(deadline)) => Some(*deadline),
            _ => None,
        };
        batch
            .waiting
            .map(|(_, _, deadline)| deadline)
            .into_iter()
            .chain(group)
            .min()
    }
    fn stop_service(&mut self) {
        self.live_watch = None;
        self.live_key = None;
        if let Some(mut child) = self.server.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.job = None;
        self.flush_server_logs();
    }
    fn sync_open(&mut self) -> Result<(), String> {
        let Some(sync) = &self.project.sync else {
            return Ok(());
        };
        if sync.open.is_empty() {
            return Ok(());
        }
        let endpoint = if sync.tcl_endpoint.is_empty() {
            self.project
                .live_watch
                .as_ref()
                .map(|l| l.tcl_endpoint.as_str())
                .ok_or("sync.open needs a TCL endpoint")?
        } else {
            &sync.tcl_endpoint
        };
        crate::live_watch::send_tcl_commands(endpoint, &sync.open, &self.cancellation)
    }
    fn start_live(&mut self) {
        let desired = (self.project.live_watch.is_some()
            && self.engines[self.active].snapshot.state == state::RUNNING)
            .then(|| {
                (
                    self.active,
                    self.active_generation(),
                    self.engines[self.active]
                        .snapshot
                        .watches
                        .iter()
                        .map(|w| w.name.clone())
                        .collect::<Vec<_>>(),
                )
            });
        if desired == self.live_key {
            return;
        }
        self.live_key = desired.clone();
        self.live_watch = None;
        let Some((_, _, names)) = desired else {
            return;
        };
        if let Some(config) = &self.project.live_watch {
            match crate::live_watch::spawn_with(config, names, self.inbox.clone()) {
                Ok(h) => self.live_watch = Some(h),
                Err(e) => self.log("error", format!("Live Watch unavailable: {e}")),
            }
        }
    }
    fn active_generation(&self) -> u64 {
        let e = &self.engines[self.active];
        if self.multi() {
            e.revision
        } else {
            e.snapshot.generation
        }
    }
    fn live_event(&self, event: Event) {
        let Some((core, generation, names)) = &self.live_key else {
            return;
        };
        if *core != self.active
            || self.engines[*core].snapshot.state != state::RUNNING
            || *generation != self.active_generation()
        {
            return;
        }
        match event {
            Event::LiveWatch { mut sample } => {
                if !names.contains(&sample.expression)
                    || !self.engines[*core]
                        .snapshot
                        .watches
                        .iter()
                        .any(|w| w.name == sample.expression)
                {
                    return;
                }
                sample.core = self.multi().then_some(*core);
                sample.generation = *generation;
                self.emit(Event::LiveWatch { sample });
            }
            event => self.emit(event),
        }
    }
    fn run(&mut self, requests: Receiver<Request>) {
        loop {
            if self.cancellation.load(Ordering::Relaxed) {
                for e in &self.engines {
                    e.handle.cancellation.store(true, Ordering::Relaxed);
                }
            }
            // A source that filled its batch may have more queued: loop again
            // instead of sleeping.
            let mut backlog = self.flush_server_logs();
            for i in 0..self.engines.len() {
                let mut drained = 0;
                loop {
                    if drained == 128 {
                        backlog = true;
                        break;
                    }
                    match self.engines[i].handle.events.try_recv() {
                        Ok(event) => {
                            drained += 1;
                            self.event(i, event);
                        }
                        Err(mpsc::TryRecvError::Disconnected) => {
                            self.worker_exit(i);
                            break;
                        }
                        Err(mpsc::TryRecvError::Empty) => break,
                    }
                }
            }
            if self.batch.is_none() && !self.exiting {
                self.start_live();
            }
            if let Some(live) = &self.live_watch {
                let events: Vec<_> = live.events.try_iter().take(64).collect();
                backlog |= events.len() == 64;
                for event in events {
                    self.live_event(event);
                }
            }
            if let Some((i, token, deadline)) = self.batch.as_ref().and_then(|b| b.waiting)
                && Instant::now() >= deadline
            {
                self.worker_timeout(i, token);
            }
            self.advance();
            if self
                .engines
                .iter()
                .all(|e| e.exited || (self.exiting && e.unresponsive))
                && self.batch.is_none()
            {
                break;
            }
            let idle = match requests.try_recv() {
                Ok(req) if req.is_quit() => {
                    if self.exiting {
                        self.reply(req.id, Json::Null, Some("Session already closing".into()));
                        continue;
                    }
                    // Keep cancellation immediate even for a direct channel caller.
                    self.cancellation.store(true, Ordering::Relaxed);
                    for r in self.queue.drain(..).collect::<Vec<_>>() {
                        self.reply(r.id, Json::Null, Some("Cancelled during exit".into()));
                    }
                    self.queue
                        .push_front(Request::new(req.id, method::QUIT, json!({})));
                    false
                }
                Ok(req) => {
                    if self.queue.len() >= 64 || self.exiting {
                        self.reply(req.id, Json::Null, Some("Session busy or closing".into()));
                    } else {
                        self.queue.push_back(req);
                    }
                    false
                }
                Err(mpsc::TryRecvError::Disconnected)
                    if !self.exiting && !self.queue.iter().any(|r| r.method == method::QUIT) =>
                {
                    self.cancellation.store(true, Ordering::Relaxed);
                    self.queue.clear();
                    self.queue
                        .push_back(Request::new(u64::MAX, method::QUIT, json!({})));
                    false
                }
                Err(_) => true,
            };
            // Every producer rings the inbox, so with nothing queued the only
            // other reason to run is a step deadline.
            if idle && !backlog {
                self.inbox.wait(self.next_deadline());
            }
        }
        self.stop_service();
        for req in self.queue.drain(..).collect::<Vec<_>>() {
            self.reply(req.id, Json::Null, Some("All debug workers exited".into()));
        }
        self.emit(Event::Exit);
    }
    fn worker_exit(&mut self, i: usize) {
        if self.engines[i].exited {
            return;
        }
        self.engines[i].exited = true;
        self.invalidate_shared_owners(i);
        if !self.exiting {
            self.engines[i].snapshot = Snapshot {
                state: state::FAULT.into(),
                ..Default::default()
            };
            self.log(
                "error",
                format!("[{}] Debug worker exited", self.engines[i].name),
            );
            self.publish();
        }
        if let Some((core, id, _)) = self.batch.as_ref().and_then(|b| b.waiting)
            && core == i
        {
            self.event(
                i,
                Event::Response {
                    id,
                    ok: false,
                    result: Json::Null,
                    error: Some("Debug worker exited before replying".into()),
                },
            );
        }
    }
    fn worker_timeout(&mut self, i: usize, token: u64) {
        self.engines[i].unresponsive = true;
        self.invalidate_shared_owners(i);
        self.engines[i]
            .handle
            .cancellation
            .store(true, Ordering::Relaxed);
        self.event(
            i,
            Event::Response {
                id: token,
                ok: false,
                result: Json::Null,
                error: Some("Worker response timed out; reopen this session".into()),
            },
        );
        self.publish();
    }
    fn event(&mut self, i: usize, event: Event) {
        match event {
            // Live samples belong to the coordinator's separately scoped poller.
            Event::LiveWatch { .. } => {}
            Event::Exit => self.worker_exit(i),
            Event::Snapshot { mut snapshot } => {
                self.observe_group_stop(i, &snapshot);
                let changed_state = snapshot.state != self.engines[i].snapshot.state;
                let changed_breaks = snapshot.breakpoints != self.engines[i].snapshot.breakpoints;
                let changed_context = changed_state
                    || snapshot.generation != self.engines[i].snapshot.generation
                    || snapshot.register_session != self.engines[i].snapshot.register_session;
                if changed_context {
                    self.invalidate_shared_owners(i);
                }
                self.filter_shared_snapshot(i, &mut snapshot);
                if snapshot.generation != self.engines[i].snapshot.generation {
                    self.revision += 1;
                    self.engines[i].revision = self.revision;
                }
                if i != self.active && changed_state {
                    self.log(
                        "core",
                        format!(
                            "[{}] {} {}",
                            self.engines[i].name, snapshot.state, snapshot.stop_reason
                        ),
                    );
                }
                self.engines[i].snapshot = *snapshot;
                if i == self.active || changed_context || changed_breaks {
                    self.publish();
                }
            }
            Event::Log {
                channel,
                text,
                timestamp,
                elapsed_ms,
            } => {
                let (channel, text) = if self.multi() && channel != "progress" {
                    (channel, format!("[{}] {text}", self.engines[i].name))
                } else if self.multi() && i != self.active {
                    (
                        "core-progress".into(),
                        format!("[{}] {text}", self.engines[i].name),
                    )
                } else {
                    (channel, text)
                };
                self.emit(Event::Log {
                    channel,
                    text,
                    timestamp,
                    elapsed_ms,
                });
            }
            Event::Response {
                id,
                mut ok,
                mut result,
                mut error,
            } => {
                if !self.batch.as_ref().is_some_and(|b| {
                    b.waiting
                        .is_some_and(|(core, token, _)| core == i && token == id)
                }) {
                    return;
                }
                let mut b = self.batch.take().unwrap();
                b.waiting = None;
                if b.current_method == method::REGISTERS_READ {
                    self.accept_shared_response(i, &b.shared_epochs, &mut result);
                    self.publish();
                }
                if b.current_method == method::REGISTERS_PROBE && ok {
                    self.accept_probe_response(i, &b.shared_epochs, &mut result);
                    self.publish();
                }
                if b.current_method == method::REGISTERS_MATRIX
                    && ok
                    && let Err(failure) = self.filter_register_matrix(i, &mut result)
                {
                    ok = false;
                    error = Some(failure);
                    result = Json::Null;
                }
                if matches!(
                    b.current_method.as_str(),
                    method::REGISTERS_READ | method::REGISTERS_LIST | method::REGISTERS_PROBE
                ) && self.multi()
                    && result.is_object()
                {
                    result["owner_generations"] = json!(self.shared_epochs);
                }
                if ok
                    && b.current_method == method::WRITE_APPLY
                    && matches!(result["scope"].as_str(), Some("chip" | "cluster"))
                    && result["outcome"].as_str().is_some_and(|o| o != "not_sent")
                {
                    // FIFO invalidation precedes any later edit on a peer worker. Conservatively
                    // discard every peer's overlapping caches and drafts after a shared write.
                    for peer in 0..self.engines.len() {
                        if peer != i {
                            let id = self.next_id;
                            self.next_id += 1;
                            let _ = self.engines[peer].handle.send(Request::new(
                                id,
                                method::WRITE_INVALIDATE,
                                json!({}),
                            ));
                        }
                    }
                }
                if ok && matches!(b.current_method.as_str(), method::RUN | method::CONTINUE) {
                    self.engines[i].launched = true;
                }
                if matches!(
                    b.current_method.as_str(),
                    method::CONNECT | method::DISCONNECT
                ) {
                    self.engines[i].launched = false;
                }
                b.results.push(json!({"index":i,"name":self.engines[i].name,"ok":ok,"result":result,"error":error}));
                b.last = result;
                if !ok {
                    self.fail(
                        &mut b,
                        format!(
                            "[{}] {}",
                            self.engines[i].name,
                            error.unwrap_or_else(|| "Command failed".into())
                        ),
                    );
                }
                self.batch = Some(b);
            }
        }
    }
    fn fail(&mut self, b: &mut Batch, error: String) {
        b.errors.push(if b.recovering {
            format!("Rollback: {error}")
        } else {
            error
        });
        if let Some(undo) = b.break_undo.take() {
            b.steps = undo;
            b.recovering = true;
            return;
        }
        if !b.recovering
            && matches!(
                b.request.method.as_str(),
                method::CONNECT | method::RECONNECT | method::BUILD | method::DOWNLOAD
            )
            && b.aggregate
        {
            b.recovering = true;
            b.steps = self
                .order
                .iter()
                .rev()
                .map(|&i| Step::Core(i, method::DISCONNECT.into()))
                .collect();
            b.steps.push_back(Step::StopService);
        } else if !b.recovering
            && !matches!(
                b.request.method.as_str(),
                method::QUIT | method::DISCONNECT | method::PAUSE
            )
        {
            b.steps.clear();
            if b.aggregate
                && matches!(
                    b.request.method.as_str(),
                    method::RUN | method::CONTINUE | method::RESTART
                )
            {
                // A partially resumed/reset group must not silently keep running.
                b.recovering = true;
                for (i, e) in self.engines.iter().enumerate() {
                    if e.snapshot.state == state::RUNNING {
                        b.steps.push_back(Step::Core(i, method::PAUSE.into()));
                    }
                }
                if b.request.method == method::RESTART {
                    for (i, e) in self.engines.iter().enumerate() {
                        if matches!(
                            e.snapshot.state.as_str(),
                            state::READY | state::STOPPED | state::RUNNING
                        ) {
                            b.steps.push_back(Step::Core(i, method::SYNCHRONIZE.into()));
                        }
                    }
                }
            }
        }
    }
    fn advance(&mut self) {
        if self.batch.is_none()
            && self.group_stop.is_some()
            && !self.exiting
            && !self.cancellation.load(Ordering::Relaxed)
        {
            self.begin_peer_halt();
        }
        if self.batch.is_none()
            && let Some(req) = self.queue.pop_front()
        {
            self.begin(req);
        }
        let Some(mut b) = self.batch.take() else {
            return;
        };
        if b.waiting.is_some() {
            self.batch = Some(b);
            return;
        }
        while let Some(mut step) = b.steps.pop_front() {
            let mut explicit_params = None;
            if let Step::Breakpoint(core, params, undo) = step {
                if let (Some(rollback), Some(undo)) = (&mut b.break_undo, undo) {
                    rollback.push_front(Step::Breakpoint(core, undo, None));
                }
                explicit_params = Some(params);
                step = Step::Core(core, method::BREAK_APPLY.into());
            }
            if let Step::Resume(i) = step {
                if self.group_stop.is_some() || self.engines[i].snapshot.state == state::RUNNING {
                    continue;
                }
                step = Step::Core(
                    i,
                    if self.engines[i].launched {
                        method::CONTINUE
                    } else {
                        method::RUN
                    }
                    .into(),
                );
            }
            match step {
                Step::Resume(_) | Step::Breakpoint(..) => unreachable!(),
                Step::WaitGroupStop(deadline) => {
                    if self.cancellation.load(Ordering::Relaxed) {
                        b.errors.push("Group wait cancelled".into());
                        b.steps.clear();
                        continue;
                    }
                    if Instant::now() >= deadline {
                        b.errors.push(
                            "Timed out waiting for the group to stop; cores may still be running"
                                .into(),
                        );
                        continue;
                    }
                    let active = &self.engines[self.active].snapshot;
                    if active.state == state::STOPPED
                        && !self
                            .engines
                            .iter()
                            .any(|e| e.snapshot.state == state::RUNNING)
                    {
                        b.last = json!({"reason":active.stop_reason,"frame":active.frame,"core":self.active,"cores":self.statuses()});
                        continue;
                    }
                    b.steps.push_front(Step::WaitGroupStop(deadline));
                    if self.group_stop.is_some() {
                        for &i in self.order.iter().rev() {
                            if self.engines[i].snapshot.state == state::RUNNING {
                                b.steps.push_front(Step::Core(i, method::PAUSE.into()));
                            }
                        }
                        if !matches!(b.steps.front(), Some(Step::WaitGroupStop(_))) {
                            continue;
                        }
                    }
                    // Process peer events while waiting; never block a worker
                    // whose pending wait would prevent its own group interrupt.
                    self.batch = Some(b);
                    return;
                }
                Step::StopService => self.stop_service(),
                Step::StartService => {
                    if let Err(e) = self.start_service() {
                        self.fail(&mut b, e);
                    }
                }
                Step::Core(i, method) => {
                    if self.engines[i].exited
                        || (self.engines[i].unresponsive && method != method::QUIT)
                    {
                        if method == method::QUIT && self.engines[i].exited {
                            continue;
                        }
                        self.fail(
                            &mut b,
                            format!("[{}] Debug worker unavailable", self.engines[i].name),
                        );
                        continue;
                    }
                    let id = self.next_id;
                    self.next_id += 1;
                    let params = if let Some(params) = explicit_params {
                        params
                    } else if method == b.request.method {
                        b.request.params.clone()
                    } else {
                        json!({})
                    };
                    let timeout = if matches!(method.as_str(), method::BUILD | method::DOWNLOAD) {
                        self.project.tasks.timeout_ms.saturating_add(120_000)
                    } else {
                        self.project
                            .session
                            .timeout_ms
                            .saturating_mul(20)
                            .max(30_000)
                    };
                    let mut worker_request = Request::new(id, &method, params);
                    if worker_request.is_cancellable_read() && method == b.request.method {
                        worker_request.read_cancel = b.request.read_cancel.clone();
                    }
                    if matches!(method.as_str(), method::WRITE_PREVIEW | method::WRITE_APPLY) {
                        worker_request.write_peers = self.statuses();
                    }
                    match self.engines[i].handle.send(worker_request) {
                        Ok(()) => {
                            b.shared_epochs = self.shared_epochs.clone();
                            b.current_method = method;
                            b.waiting =
                                Some((i, id, Instant::now() + Duration::from_millis(timeout)));
                            self.batch = Some(b);
                            return;
                        }
                        Err(e) => {
                            self.worker_exit(i);
                            self.fail(&mut b, e);
                        }
                    }
                }
            }
        }
        let ok = b.errors.is_empty();
        if ok
            && matches!(
                b.request.method.as_str(),
                method::CONNECT | method::RECONNECT | method::BUILD | method::DOWNLOAD
            )
        {
            self.start_live();
        }
        if ok
            && matches!(b.request.method.as_str(), method::WATCH | method::UNWATCH)
            && self.live_watch.is_some()
        {
            self.start_live();
        }
        let result = if b.aggregate {
            let mut info = self.info();
            info["results"] = json!(b.results);
            match b.request.method.as_str() {
                method::CONNECT | method::RECONNECT => info["connected"] = json!(ok),
                method::RUN | method::CONTINUE => {
                    info["running"] = json!(
                        ok && self
                            .engines
                            .iter()
                            .all(|e| e.snapshot.state == state::RUNNING)
                    )
                }
                method::PAUSE => {
                    info["stopped"] = json!(
                        ok && self.engines.iter().all(|e| matches!(
                            e.snapshot.state.as_str(),
                            state::STOPPED | state::READY
                        ))
                    )
                }
                method::RESTART => info["restarted"] = json!(ok),
                method::QUIT | method::DISCONNECT => info["disconnected"] = json!(ok),
                _ => {}
            }
            info
        } else if b.request.method == method::STATUS {
            json!(self.snapshot())
        } else {
            b.last
        };
        if b.break_undo.is_some() || b.request.method == method::BREAK_CORES {
            self.publish();
        }
        if b.internal {
            self.group_stop = None;
            if !ok {
                self.log(
                    "error",
                    format!("Group halt incomplete: {}", b.errors.join("; ")),
                );
            }
            self.publish();
        } else {
            self.reply(b.request.id, result, (!ok).then(|| b.errors.join("; ")));
        }
    }
    fn begin(&mut self, mut req: Request) {
        if self.multi() && self.begin_breakpoint_edit(&req) {
            return;
        }
        if self.multi() && self.begin_control(&mut req) {
            return;
        }
        if self.multi()
            && req.method == method::CONNECT
            && self.engines.iter().any(|e| {
                !matches!(
                    e.snapshot.state.as_str(),
                    state::DISCONNECTED | state::FAULT
                )
            })
        {
            self.reply(
                req.id,
                Json::Null,
                Some("A workspace session already exists. Use Reconnect to replace it.".into()),
            );
            return;
        }
        if self.multi() && req.method == method::SET_ELF {
            self.reply(req.id,Json::Null,Some("Multi-core workspaces share one ELF. Change Program / ELF in F2 Setup and restart the workspace.".into()));
            return;
        }
        if matches!(req.method.as_str(), method::SELECT_CORE | method::CORES) {
            if req.method == method::SELECT_CORE {
                let index = if let Some(name) = req.params.get("name").and_then(Json::as_str) {
                    self.engines.iter().position(|e| e.name == name)
                } else {
                    req.params
                        .get("index")
                        .and_then(Json::as_u64)
                        .and_then(|i| usize::try_from(i).ok())
                };
                let Some(index) = index.filter(|&i| i < self.engines.len()) else {
                    self.reply(
                        req.id,
                        Json::Null,
                        Some("Unknown core name or index".into()),
                    );
                    return;
                };
                // Selection is an edit boundary even if the user later returns to this core.
                // FIFO worker queues discard drafts before a subsequent preview/apply request.
                for engine in &self.engines {
                    let id = self.next_id;
                    self.next_id += 1;
                    let _ = engine
                        .handle
                        .send(Request::new(id, method::WRITE_DISCARD, json!({})));
                }
                self.active = index;
                self.revision += 1;
                self.engines[index].revision = self.revision;
                self.publish();
                if self.live_watch.is_some() {
                    self.start_live();
                }
            }
            self.reply(req.id, self.info(), None);
            return;
        }
        let multi = self.multi();
        let task = multi
            && ((req.method == method::BUILD
                && (!self.project.tasks.build.is_empty() || self.project.build.is_some()))
                || (req.method == method::DOWNLOAD && !self.project.tasks.download.is_empty()));
        let aggregate = multi
            && (task
                || matches!(
                    req.method.as_str(),
                    method::CONNECT
                        | method::RECONNECT
                        | method::RUN
                        | method::DISCONNECT
                        | method::QUIT
                ));
        let mut steps = VecDeque::new();
        if task {
            self.live_watch = None;
            self.live_key = None;
            let connected = self.engines.iter().any(|e| {
                matches!(
                    e.snapshot.state.as_str(),
                    state::READY | state::STOPPED | state::RUNNING
                )
            });
            for &i in self.order.iter().rev() {
                steps.push_back(Step::Core(i, method::DISCONNECT.into()));
            }
            steps.push_back(Step::StopService);
            steps.push_back(Step::Core(self.active, req.method.clone()));
            if connected {
                steps.push_back(Step::StartService);
                for &i in &self.order {
                    steps.push_back(Step::Core(i, method::CONNECT.into()));
                }
            }
            self.batch = Some(Batch {
                request: req,
                steps,
                waiting: None,
                results: vec![],
                errors: vec![],
                last: Json::Null,
                recovering: false,
                aggregate,
                internal: false,
                current_method: String::new(),
                break_undo: None,
                shared_epochs: BTreeMap::new(),
            });
            return;
        }
        if matches!(
            req.method.as_str(),
            method::CONNECT | method::RECONNECT | method::DISCONNECT | method::QUIT
        ) {
            self.live_watch = None;
            self.live_key = None;
        }
        if req.method == method::QUIT {
            self.exiting = true;
            for e in &self.engines {
                e.handle.cancellation.store(true, Ordering::Relaxed);
            }
        }
        if multi && req.method == method::RECONNECT {
            for &i in self.order.iter().rev() {
                steps.push_back(Step::Core(i, method::DISCONNECT.into()));
            }
            steps.push_back(Step::StopService);
        }
        if multi && matches!(req.method.as_str(), method::CONNECT | method::RECONNECT) {
            steps.push_back(Step::StartService);
        }
        for i in if aggregate {
            self.order.clone()
        } else {
            vec![self.active]
        } {
            steps.push_back(Step::Core(
                i,
                if req.method == method::RECONNECT && multi {
                    method::CONNECT.into()
                } else {
                    req.method.clone()
                },
            ));
        }
        if matches!(req.method.as_str(), method::QUIT | method::DISCONNECT) {
            steps.push_back(Step::StopService);
        }
        if multi
            && self.project.target.mode != "local"
            && matches!(req.method.as_str(), method::CONNECT | method::RECONNECT)
        {
            // One core's after_connect may reset the entire chip.
            for &i in &self.order {
                steps.push_back(Step::Core(i, method::SYNCHRONIZE.into()));
            }
        }
        self.batch = Some(Batch {
            request: req,
            steps,
            waiting: None,
            results: vec![],
            errors: vec![],
            last: Json::Null,
            recovering: false,
            aggregate,
            internal: false,
            current_method: String::new(),
            break_undo: None,
            shared_epochs: BTreeMap::new(),
        });
    }
}
impl Drop for Coordinator {
    fn drop(&mut self) {
        self.stop_service();
    }
}
fn derive_project(base: &Project, core: &Core, index: usize) -> Project {
    let mut p = base.clone();
    p.registers = base.registers_for_core(&core.name);
    p.target.endpoint = core.endpoint.clone();
    if !core.after_connect.is_empty() {
        p.target.after_connect = core.after_connect.clone();
    }
    if !core.run.is_empty() {
        p.actions.run = core.run.clone();
    }
    p.gdb.init.extend(core.init.iter().cloned());
    if let Some(watch) = &core.watch {
        p.watch = watch.clone();
    }
    if let Some(breakpoints) = &core.breakpoints {
        p.breakpoints = breakpoints.clone();
    }
    p.preference_core = Some(core.name.clone());
    if let Some(dir) = &mut p.session.log_dir {
        *dir = dir.join(format!("core-{index}"));
    }
    p.service = None;
    p
}

#[cfg(test)]
mod core_register_tests {
    use super::*;
    #[test]
    fn per_core_register_worker_derivation_keeps_independent_effective_routes() {
        let mut project = Project::default();
        project.registers.cpu = "cortex-r52".into();
        project.registers.tcl_endpoint = "localhost:6666".into();
        project.cores = vec![Core {
            name: "core.0".into(), endpoint: "localhost:3333".into(),
            registers: Some(toml::from_str("cpu='cortex-m4'\ntcl_endpoint='localhost:6667'\n[targets]\n'core.0'='soc.m4'\n").unwrap()),
            ..Default::default()
        }, Core {
            name: "core.2".into(), endpoint: "localhost:3334".into(),
            ..Default::default()
        }];
        let first = derive_project(&project, &project.cores[0], 0);
        let second = derive_project(&project, &project.cores[1], 1);
        assert_eq!(first.registers.cpu, "cortex-m4");
        assert_eq!(second.registers.cpu, "cortex-r52");
        assert_eq!(first.registers.tcl_endpoint, "localhost:6667");
        assert_eq!(second.registers.tcl_endpoint, "localhost:6666");
        assert_eq!(first.registers.targets["core.0"], "soc.m4");
        assert!(second.registers.targets.is_empty());
        assert_eq!(first.preference_core.as_deref(), Some("core.0"));
        assert_eq!(second.preference_core.as_deref(), Some("core.2"));
        assert_eq!(project.registers.cpu, "cortex-r52");
    }
}
