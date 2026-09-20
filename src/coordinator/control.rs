use super::*;

impl Coordinator {
    pub(super) fn control_batch(
        &mut self,
        request: Request,
        steps: VecDeque<Step>,
        internal: bool,
    ) {
        self.batch = Some(Batch {
            request,
            steps,
            waiting: None,
            results: vec![],
            errors: vec![],
            last: Json::Null,
            recovering: false,
            aggregate: true,
            internal,
            current_method: String::new(),
            break_undo: None,
        });
    }

    /// Return true when this request has been handled by the group controller.
    pub(super) fn begin_control(&mut self, req: &mut Request) -> bool {
        if req.method == "console" {
            let command = req
                .params
                .get("command")
                .and_then(Json::as_str)
                .unwrap_or("")
                .trim();
            if let Some(method) = session::execution_alias(command) {
                req.method = match method {
                    "step-instruction" => "stepi",
                    "next-instruction" => return false,
                    method => method,
                }
                .into();
            }
        }
        if req.method == "control_scope" {
            let scope = match req.params.get("scope").and_then(Json::as_str) {
                Some("all") => ControlScope::All,
                Some("core") => ControlScope::Core,
                _ => {
                    self.reply(req.id, Json::Null, Some("scope must be all or core".into()));
                    return true;
                }
            };
            self.project.multicore.scope = scope;
            self.log(
                "control",
                format!("Control scope: {scope:?}; stepping uses the selected core"),
            );
            self.publish();
            self.reply(req.id, self.info(), None);
            return true;
        }
        let scope = match req.params.get("scope").and_then(Json::as_str) {
            None => self.project.multicore.scope,
            Some("all") => ControlScope::All,
            Some("core") => ControlScope::Core,
            _ => {
                self.reply(req.id, Json::Null, Some("scope must be all or core".into()));
                return true;
            }
        };
        let shared_reset = req.method == "restart" && !self.project.multicore.restart.is_empty();
        let stepping = matches!(req.method.as_str(), "step" | "next" | "stepi" | "finish");
        let all = scope == ControlScope::All;
        if all && req.method == "wait_stopped" {
            let timeout = req
                .params
                .get("timeout_ms")
                .and_then(Json::as_u64)
                .unwrap_or(8000)
                .clamp(1, 120_000);
            self.control_batch(
                req.clone(),
                VecDeque::from([Step::WaitGroupStop(
                    Instant::now() + Duration::from_millis(timeout),
                )]),
                false,
            );
            self.batch.as_mut().unwrap().aggregate = false;
            return true;
        }
        if !shared_reset
            && req.method != "run"
            && !(all
                && (stepping || matches!(req.method.as_str(), "continue" | "pause" | "restart")))
        {
            return false;
        }
        if req.method == "restart" && !shared_reset {
            self.reply(req.id, Json::Null, Some("Group Reset requires multicore.restart and multicore.restart_core; per-core reset actions may reset the whole chip".into()));
            return true;
        }
        // Keep legacy Run-all behavior unless the caller explicitly asks for one core.
        let indices =
            if all || shared_reset || (req.method == "run" && req.params.get("scope").is_none()) {
                self.order.clone()
            } else {
                vec![self.active]
            };
        if let Some(&i) = indices.iter().find(|&&i| {
            !matches!(
                self.engines[i].snapshot.state.as_str(),
                "READY" | "STOPPED" | "RUNNING"
            )
        }) {
            self.reply(
                req.id,
                self.info(),
                Some(format!(
                    "[{}] Group control requires every selected core to be connected",
                    self.engines[i].name
                )),
            );
            return true;
        }
        if stepping && self.engines[self.active].snapshot.state != "STOPPED" {
            self.reply(
                req.id,
                self.info(),
                Some("Select a stopped core before stepping".into()),
            );
            return true;
        }
        let mut steps = VecDeque::new();
        if shared_reset || stepping || req.method == "pause" {
            for &i in &indices {
                if self.engines[i].snapshot.state == "RUNNING" {
                    steps.push_back(Step::Core(i, "pause".into()));
                }
            }
        }
        if shared_reset {
            let reset = self
                .engines
                .iter()
                .position(|e| e.name == self.project.multicore.restart_core)
                .unwrap();
            steps.push_back(Step::Core(reset, "restart_shared".into()));
            for &i in &indices {
                self.engines[i].launched = false;
                steps.push_back(Step::Core(i, "synchronize".into()));
            }
        } else if stepping {
            steps.push_back(Step::Core(self.active, req.method.clone()));
        } else if matches!(req.method.as_str(), "run" | "continue") {
            for &i in &indices {
                steps.push_back(Step::Resume(i));
            }
        }
        self.log(
            "control",
            format!(
                "{}: {}",
                req.method,
                indices
                    .iter()
                    .map(|&i| self.engines[i].name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
        self.control_batch(req.clone(), steps, false);
        true
    }

    pub(super) fn observe_group_stop(&mut self, i: usize, snapshot: &Snapshot) {
        if !self.multi()
            || self.project.multicore.scope != ControlScope::All
            || !self.project.multicore.halt_peers
            || self.group_stop.is_some()
            || self.exiting
        {
            return;
        }
        // Do not interpret connect/reset refreshes or intentional stepping/pausing
        // as a new breakpoint. A fast stop may arrive before a RUNNING snapshot.
        if self.batch.as_ref().is_some_and(|b| {
            b.internal
                || matches!(
                    b.request.method.as_str(),
                    "connect"
                        | "reconnect"
                        | "restart"
                        | "disconnect"
                        | "quit"
                        | "build"
                        | "download"
                        | "pause"
                        | "step"
                        | "next"
                        | "stepi"
                        | "finish"
                )
        }) {
            return;
        }
        let old = &self.engines[i].snapshot;
        let stopped = snapshot.state == "STOPPED"
            && snapshot.generation != old.generation
            && (old.state == "RUNNING"
                || matches!(
                    snapshot.stop_reason.as_str(),
                    "breakpoint-hit"
                        | "watchpoint-trigger"
                        | "read-watchpoint-trigger"
                        | "access-watchpoint-trigger"
                ));
        if !stopped {
            return;
        }
        self.group_stop = Some(i);
        self.active = i;
        self.log(
            "control",
            format!(
                "[{}] {}: halting peer cores (software coordination)",
                self.engines[i].name, snapshot.stop_reason
            ),
        );
    }

    pub(super) fn begin_peer_halt(&mut self) {
        let trigger = self.group_stop.unwrap();
        let steps = self
            .order
            .iter()
            .copied()
            .filter(|&i| i != trigger && self.engines[i].snapshot.state == "RUNNING")
            .map(|i| Step::Core(i, "pause".into()))
            .collect();
        // Internal batches never emit a response with a user request id.
        self.control_batch(Request::new(0, "pause", json!({})), steps, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn coordinator() -> (Coordinator, Receiver<Event>) {
        let mut p = Project {
            cores: vec![
                Core {
                    name: "core.0".into(),
                    endpoint: "a".into(),
                    startup_order: 1,
                    ..Default::default()
                },
                Core {
                    name: "core.1".into(),
                    endpoint: "b".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        p.multicore.scope = ControlScope::All;
        let (tx, rx) = mpsc::sync_channel(512);
        let mut c = Coordinator::new(p, tx, Arc::new(AtomicBool::new(false)));
        for e in &mut c.engines {
            e.snapshot.state = "STOPPED".into();
        }
        (c, rx)
    }
    fn methods(c: &Coordinator) -> Vec<(usize, &str)> {
        c.batch
            .as_ref()
            .unwrap()
            .steps
            .iter()
            .filter_map(|s| match s {
                Step::Core(i, m) => Some((*i, m.as_str())),
                Step::Resume(i) => Some((*i, "resume")),
                _ => None,
            })
            .collect()
    }
    #[test]
    fn group_continue_orders_both_and_core_override_is_independent() {
        let (mut c, _rx) = coordinator();
        c.begin(Request::new(1, "continue", json!({})));
        assert_eq!(methods(&c), [(1, "resume"), (0, "resume")]);
        c.batch = None;
        c.begin(Request::new(2, "continue", json!({"scope":"core"})));
        assert_eq!(methods(&c), [(0, "continue")]);
    }
    #[test]
    fn shared_reset_is_once_and_refreshes_every_connection() {
        let (mut c, _rx) = coordinator();
        c.project.multicore.restart_core = "core.0".into();
        c.project.multicore.restart = vec!["monitor chipreset".into()];
        for e in &mut c.engines {
            e.snapshot.state = "RUNNING".into();
            e.launched = true;
        }
        c.begin(Request::new(1, "restart", json!({"scope":"core"})));
        assert_eq!(
            methods(&c),
            [
                (1, "pause"),
                (0, "pause"),
                (0, "restart_shared"),
                (1, "synchronize"),
                (0, "synchronize")
            ]
        );
        assert!(c.engines.iter().all(|e| !e.launched));
    }
    #[test]
    fn breakpoint_focus_and_peer_halt_do_not_steal_focus() {
        let (mut c, _rx) = coordinator();
        c.engines[0].snapshot.state = "RUNNING".into();
        c.engines[1].snapshot.state = "RUNNING".into();
        c.event(
            1,
            Event::Snapshot {
                snapshot: Box::new(Snapshot {
                    state: "STOPPED".into(),
                    stop_reason: "breakpoint-hit".into(),
                    generation: 1,
                    ..Default::default()
                }),
            },
        );
        assert_eq!(c.active, 1);
        c.begin_peer_halt();
        assert_eq!(methods(&c), [(0, "pause")]);
        c.event(
            0,
            Event::Snapshot {
                snapshot: Box::new(Snapshot {
                    state: "STOPPED".into(),
                    stop_reason: "signal-received".into(),
                    generation: 1,
                    ..Default::default()
                }),
            },
        );
        assert_eq!(c.active, 1);
        assert!(c.batch.as_ref().unwrap().internal);
    }
    #[test]
    fn pause_continues_after_failure_and_resume_failure_halts_started_peers() {
        let (mut c, _rx) = coordinator();
        for e in &mut c.engines {
            e.snapshot.state = "RUNNING".into();
        }
        c.begin(Request::new(1, "pause", json!({})));
        let mut b = c.batch.take().unwrap();
        b.steps.pop_front();
        c.fail(&mut b, "first core failed".into());
        assert_eq!(b.steps.len(), 1);
        c.begin(Request::new(2, "continue", json!({})));
        let mut b = c.batch.take().unwrap();
        c.fail(&mut b, "resume failed".into());
        assert!(b.recovering);
        assert!(
            b.steps
                .iter()
                .all(|s| matches!(s,Step::Core(_,m) if m == "pause"))
        );
    }
    #[test]
    fn step_pauses_peers_first_and_fault_prevents_partial_resume() {
        let (mut c, rx) = coordinator();
        c.engines[1].snapshot.state = "RUNNING".into();
        c.begin(Request::new(1, "stepi", json!({})));
        assert_eq!(methods(&c), [(1, "pause"), (0, "stepi")]);
        c.batch = None;
        c.engines[1].snapshot.state = "FAULT".into();
        c.begin(Request::new(2, "continue", json!({})));
        assert!(c.batch.is_none());
        assert!(rx.try_iter().any(|e| matches!(
            e,
            Event::Response {
                id: 2,
                ok: false,
                ..
            }
        )));
    }
    #[test]
    fn group_wait_keeps_peer_breakpoints_responsive() {
        let (mut c, _rx) = coordinator();
        for e in &mut c.engines {
            e.snapshot.state = "RUNNING".into();
        }
        c.begin(Request::new(1, "wait_stopped", json!({"timeout_ms":8000})));
        c.advance();
        assert!(c.batch.as_ref().unwrap().waiting.is_none());
        assert!(matches!(
            c.batch.as_ref().unwrap().steps.front(),
            Some(Step::WaitGroupStop(_))
        ));
        c.event(
            1,
            Event::Snapshot {
                snapshot: Box::new(Snapshot {
                    state: "STOPPED".into(),
                    stop_reason: "breakpoint-hit".into(),
                    generation: 1,
                    ..Default::default()
                }),
            },
        );
        c.advance();
        let b = c.batch.as_ref().unwrap();
        assert_eq!(b.waiting.unwrap().0, 0);
        assert_eq!(b.current_method, "pause");
    }
}
