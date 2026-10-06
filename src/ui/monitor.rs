//! Visible-item, bounded refresh scheduling. No target writes or implicit halt/resume.
use super::*;
use crate::config::RefreshPolicy;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Clone, Debug, Default, Deserialize)]
pub(super) struct Binding {
    pub address: u64,
    pub bits: u32,
    pub little_endian: bool,
    #[serde(default)]
    pub signed: bool,
    #[serde(default)]
    pub float: bool,
    #[serde(default)]
    pub binding_id: String,
}
impl Binding {
    fn display(&self, value: u64) -> String {
        if self.float && self.bits == 32 {
            return f32::from_bits(value as u32).to_string();
        }
        if self.float && self.bits == 64 {
            return f64::from_bits(value).to_string();
        }
        if self.signed {
            let shift = 64 - self.bits;
            return (((value << shift) as i64) >> shift).to_string();
        }
        value.to_string()
    }
}
#[derive(Clone)]
pub(super) struct Item {
    pub key: String,
    pub name: String,
    pub watch: Option<(String, Vec<usize>)>,
    pub memory: Option<Binding>,
    pub peripheral: Option<(usize, usize)>,
    pub safe_auto: bool,
}
struct Sample {
    legacy: bool,
    binding: Option<Binding>,
    generation: u64,
    session: u64,
    frame: u32,
    channel: String,
    request_context: crate::registers::Context,
    epoch: u64,
    route_key: String,
    receipt: Option<memory_access::Receipt>,
    pub value: Option<u64>,
    pub text: String,
    pub error: Option<String>,
    pub changed: bool,
    due: Instant,
    sampled: Option<Instant>,
}
struct Pending {
    id: u64,
    context: Option<usize>,
    generation: u64,
    session: u64,
    frame: u32,
    item: Item,
    key: String,
    resolve: bool,
    policy: RefreshPolicy,
    request_context: crate::registers::Context,
    epoch: u64,
    route_key: String,
}
struct Popup {
    item: Item,
    policy: RefreshPolicy,
    interval: String,
    enabled: bool,
    field: usize,
}
#[derive(Default)]
pub(super) struct Monitor {
    samples: HashMap<String, Sample>,
    pending: Option<Pending>,
    popup: Option<Popup>,
    pub hits: Vec<(Rect, usize)>,
    next_read: Option<(Item, RefreshPolicy)>,
    fair_index: usize,
}
impl Monitor {
    pub fn invalidate(&mut self) {
        self.samples.clear();
        self.popup = None;
        self.next_read = None;
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn invalidate_bindings(&mut self) {
        for sample in self.samples.values_mut() {
            if sample
                .binding
                .as_ref()
                .is_some_and(|binding| !binding.binding_id.is_empty())
            {
                sample.binding = None;
            }
        }
        self.popup = None;
        self.next_read = None;
    }
    pub fn modal(&self) -> bool {
        self.popup.is_some()
    }
}
impl App {
    pub(super) fn monitor_key(&self, key: &str) -> String {
        let key = format!(
            "{}|{key}",
            self.snapshot
                .core
                .as_ref()
                .map(|c| c.name.as_str())
                .unwrap_or("single")
        );
        if self.project.debug.chip.is_empty() {
            key
        } else {
            format!("chip:{}|{key}", self.project.debug.chip)
        }
    }
    pub(super) fn monitor_policy(&self, item: &Item) -> RefreshPolicy {
        self.monitor_policy_override(item)
            .cloned()
            .unwrap_or_default()
    }
    fn monitor_policy_override(&self, item: &Item) -> Option<&RefreshPolicy> {
        let core = self
            .snapshot
            .core
            .as_ref()
            .map(|c| c.name.as_str())
            .unwrap_or("single");
        let root = item.watch.as_ref().map(|(root, _)| format!("watch:{root}"));
        self.project
            .refresh_policy(core, &item.key, root.as_deref())
    }
    pub(super) fn watch_monitor_item(&self, row: usize) -> Option<Item> {
        let nodes = watch::rows(&self.snapshot.watches);
        let node = nodes.get(row / 2).filter(|n| !n.more)?;
        Some(Item {
            key: node.key(),
            name: node.value.name.clone(),
            watch: Some((node.root.into(), node.path.to_vec())),
            memory: None,
            peripheral: None,
            safe_auto: true,
        })
    }
    pub(super) fn open_monitor(&mut self, pane: usize, row: usize) {
        let item = match pane {
            1 => self.watch_monitor_item(row),
            10 => self.peripheral_monitor_item(row),
            _ => None,
        };
        let Some(item) = item else {
            self.notice = "Select an addressable Watch member or readable peripheral register to configure access.".into();
            return;
        };
        let policy = self.monitor_policy(&item);
        self.monitor.popup = Some(Popup {
            item,
            interval: if policy.interval_ms == 0 {
                500
            } else {
                policy.interval_ms
            }
            .to_string(),
            enabled: policy.interval_ms > 0,
            policy,
            field: 0,
        });
        self.formats.popup = None;
        self.editing = false;
        self.watch_editing = false;
        self.completion.invalidate();
    }
    pub(super) fn memory_access_description(&self, policy: &RefreshPolicy) -> String {
        if policy.channel.is_empty() {
            let core = self
                .snapshot
                .core
                .as_ref()
                .map(|core| core.name.as_str())
                .unwrap_or("selected core");
            let endpoint = self
                .snapshot
                .core
                .as_ref()
                .map(|core| core.endpoint.as_str())
                .unwrap_or(&self.project.target.endpoint);
            format!("configured GDB {core} @ {endpoint} · stopped only")
        } else if let Some(channel) = self
            .project
            .memory_access
            .iter()
            .find(|channel| channel.id == policy.channel)
        {
            format!(
                "configured {} → {} @ {} · source={} · {}",
                channel.id,
                channel.target,
                channel.tcl_endpoint,
                self.project.memory_access_source,
                if channel.while_running {
                    "running + stopped"
                } else {
                    "stopped only"
                }
            )
        } else {
            format!("Unavailable channel: {}", policy.channel)
        }
    }
    pub(super) fn access_caption(&self, pane: usize, row: usize) -> String {
        let item = match pane {
            1 => self.watch_monitor_item(row),
            10 => self.peripheral_monitor_item(row),
            _ => None,
        };
        let configured = self.memory_access_description(
            &item
                .as_ref()
                .map(|item| self.monitor_policy(item))
                .unwrap_or_default(),
        );
        if let Some(item) = item
            && let Some(sample) = self.monitor.samples.get(&self.monitor_key(&item.key))
        {
            if let Some(receipt) = &sample.receipt {
                let fresh = self.monitor_fresh(&item.key)
                    && sample.route_key
                        == self.memory_route_fingerprint(&self.monitor_policy(&item).channel);
                return format!(
                    "{}: {}{}",
                    if fresh { "sampled" } else { "retained / stale" },
                    receipt.caption(),
                    sample
                        .error
                        .as_ref()
                        .map(|e| format!(" · error: {e}"))
                        .unwrap_or_default()
                );
            }
            if sample.legacy {
                return format!("legacy raw · origin receipt unavailable · {configured}");
            }
            if let Some(error) = &sample.error {
                return format!("{configured} · error: {error}");
            }
        }
        format!("{configured} · not sampled")
    }

    fn monitor_channels(&self) -> Vec<(String, String)> {
        let core = self.register_context().core;
        std::iter::once((String::new(), "GDB · selected core · stopped only".into()))
            .chain(
                self.project
                    .memory_access
                    .iter()
                    .filter(|a| a.cores.is_empty() || a.cores.contains(&core))
                    .map(|a| {
                        (
                            a.id.clone(),
                            format!(
                                "{} · {}",
                                if a.label.is_empty() { &a.id } else { &a.label },
                                if a.while_running {
                                    "running + stopped"
                                } else {
                                    "stopped only"
                                }
                            ),
                        )
                    }),
            )
            .collect()
    }
    fn monitor_apply(&mut self, engine: Option<&EngineHandle>, once: bool) {
        let Some(popup) = self.monitor.popup.take() else {
            return;
        };
        let interval = popup.interval.parse::<u64>().unwrap_or(0);
        if !(50..=60000).contains(&interval) || (popup.enabled && !popup.item.safe_auto) {
            self.notice = if !popup.item.safe_auto {
                "Read-side-effect/write-only registers cannot be polled automatically.".into()
            } else {
                "Interval must be 50..60000 milliseconds.".into()
            };
            self.monitor.popup = Some(popup);
            return;
        }
        let policy = RefreshPolicy {
            channel: popup.policy.channel,
            interval_ms: if popup.enabled { interval } else { 0 },
        };
        let key = self.monitor_key(&popup.item.key);
        self.project.ui.refresh.insert(key.clone(), policy.clone());
        self.monitor.samples.remove(&key);
        for sample in self.monitor.samples.values_mut() {
            sample.value = None;
            sample.error = None;
            sample.sampled = None;
            sample.due = Instant::now();
        }
        self.save_ui(engine);
        self.notice = format!(
            "{} · {} · {}",
            popup.item.name,
            if policy.channel.is_empty() {
                "GDB"
            } else {
                &policy.channel
            },
            if popup.enabled {
                format!("{} ms", interval)
            } else {
                "manual".into()
            }
        );
        if once {
            self.request_monitor(engine, popup.item, policy);
        }
    }
    pub(super) fn monitor_key_event(
        &mut self,
        key: KeyEvent,
        engine: Option<&EngineHandle>,
    ) -> bool {
        if self.monitor.popup.is_none() {
            return false;
        }
        let channels = self.monitor_channels();
        let popup = self.monitor.popup.as_mut().unwrap();
        match key.code {
            KeyCode::Esc => self.monitor.popup = None,
            KeyCode::Tab | KeyCode::Down => popup.field = (popup.field + 1) % 6,
            KeyCode::BackTab | KeyCode::Up => popup.field = (popup.field + 5) % 6,
            KeyCode::Left | KeyCode::Right | KeyCode::Enter if popup.field == 0 => {
                let index = channels
                    .iter()
                    .position(|(id, _)| *id == popup.policy.channel)
                    .unwrap_or(0);
                let delta = if key.code == KeyCode::Left {
                    channels.len() - 1
                } else {
                    1
                };
                popup.policy.channel = channels[(index + delta) % channels.len()].0.clone();
            }
            KeyCode::Enter | KeyCode::Char(' ') if popup.field == 1 => {
                popup.enabled = !popup.enabled
            }
            KeyCode::Char(c) if popup.field == 2 && c.is_ascii_digit() => {
                if popup.interval.len() < 5 {
                    popup.interval.push(c);
                }
            }
            KeyCode::Backspace if popup.field == 2 => {
                popup.interval.pop();
            }
            KeyCode::Delete if popup.field == 2 => popup.interval.clear(),
            KeyCode::Char('u')
                if key.modifiers.contains(KeyModifiers::CONTROL) && popup.field == 2 =>
            {
                popup.interval.clear()
            }
            KeyCode::Enter if popup.field == 3 => self.monitor_apply(engine, false),
            KeyCode::Enter if popup.field == 4 => self.monitor_apply(engine, true),
            KeyCode::Enter if popup.field == 5 => self.monitor.popup = None,
            _ => {}
        }
        true
    }
    pub(super) fn monitor_mouse(
        &mut self,
        mouse: MouseEvent,
        engine: Option<&EngineHandle>,
    ) -> bool {
        if self.monitor.popup.is_none() {
            return false;
        }
        if mouse.kind == MouseEventKind::Down(event::MouseButton::Left)
            && let Some((_, field)) = self
                .monitor
                .hits
                .iter()
                .find(|(r, _)| r.contains((mouse.column, mouse.row).into()))
                .copied()
        {
            self.monitor.popup.as_mut().unwrap().field = field;
            if field != 2 {
                self.monitor_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), engine);
            }
        }
        true
    }
    fn request_monitor(
        &mut self,
        engine: Option<&EngineHandle>,
        item: Item,
        policy: RefreshPolicy,
    ) -> bool {
        let Some(engine) = engine else {
            return false;
        };
        if self.monitor.pending.is_some() {
            return false;
        }
        if !matches!(self.snapshot.state.as_str(), "STOPPED" | "RUNNING") {
            return false;
        }
        if self.snapshot.state == "RUNNING"
            && !self.project.memory_access.iter().any(|a| {
                a.id == policy.channel
                    && a.while_running
                    && (a.cores.is_empty() || a.cores.contains(&self.register_context().core))
            })
        {
            return false;
        }
        let key = self.monitor_key(&item.key);
        let generation = self.snapshot.generation;
        let session = self.snapshot.register_session;
        let frame = self.snapshot.frame.level;
        let request_context = self.register_context();
        let epoch = self.snapshot.memory_selection_epoch;
        let route_key = self.memory_route_fingerprint(&policy.channel);
        if self.monitor.samples.get(&key).is_some_and(|sample| {
            sample.request_context.session != request_context.session
                || sample.request_context.core != request_context.core
                || sample.request_context.frame != request_context.frame
                || sample.epoch != epoch
                || sample.route_key != route_key
                || sample.channel != policy.channel
        }) && let Some(register) = item.peripheral
        {
            self.peripherals.forget_value(register);
        }
        let sample = self
            .monitor
            .samples
            .entry(key.clone())
            .or_insert_with(|| Sample {
                legacy: false,
                binding: item.memory.clone(),
                generation,
                session,
                frame,
                channel: policy.channel.clone(),
                request_context: request_context.clone(),
                epoch,
                route_key: route_key.clone(),
                receipt: None,
                value: None,
                text: String::new(),
                error: None,
                changed: false,
                due: Instant::now(),
                sampled: None,
            });
        if sample.request_context != request_context
            || sample.epoch != epoch
            || sample.route_key != route_key
            || sample.channel != policy.channel
        {
            sample.binding = item.memory.clone();
            if sample.request_context.session != request_context.session
                || sample.request_context.core != request_context.core
                || sample.request_context.frame != request_context.frame
                || sample.epoch != epoch
                || sample.route_key != route_key
                || sample.channel != policy.channel
            {
                sample.sampled = None;
                sample.value = None;
                sample.receipt = None;
            }
            sample.error = None;
        }
        sample.generation = generation;
        sample.session = session;
        sample.frame = frame;
        sample.request_context = request_context.clone();
        sample.epoch = epoch;
        sample.route_key = route_key.clone();
        sample.channel = policy.channel.clone();
        let (method, mut params, resolve) = if let Some(binding) = &sample.binding {
            (
                "memory_read",
                json!({"address":binding.address,"bits":binding.bits,"little_endian":binding.little_endian,"channel":policy.channel,"context":request_context,"selection_epoch":epoch,
                    "watch_binding":if binding.binding_id.is_empty(){None}else{Some(&binding.binding_id)}}),
                false,
            )
        } else if self.snapshot.state == "STOPPED"
            && let Some((expression, path)) = &item.watch
        {
            (
                "watch_resolve",
                json!({"expression":expression,"path":path,"context":request_context,"selection_epoch":epoch}),
                true,
            )
        } else {
            sample.error = Some("Pause once to resolve this expression's address".into());
            sample.due = Instant::now() + Duration::from_secs(1);
            return false;
        };
        if params.get("watch_binding").is_some_and(Value::is_null) {
            params.as_object_mut().unwrap().remove("watch_binding");
        }
        let id = self.next_id;
        self.next_id += 1;
        if let Err(error) = engine.send(Request::new(id, method, params)) {
            self.notice = error;
            return false;
        }
        self.monitor.pending = Some(Pending {
            id,
            context: self.snapshot.core.as_ref().map(|c| c.index),
            generation,
            session,
            frame,
            item,
            key,
            resolve,
            policy,
            request_context,
            epoch,
            route_key,
        });
        true
    }
    pub(super) fn monitor_response(
        &mut self,
        id: u64,
        result: &Value,
        error: Option<&str>,
    ) -> bool {
        if !self.monitor.pending.as_ref().is_some_and(|p| p.id == id) {
            return false;
        }
        let pending = self.monitor.pending.take().unwrap();
        self.pending_commands.remove(&id);
        self.fx.response(id, error.is_none());
        if pending.context != self.snapshot.core.as_ref().map(|c| c.index)
            || pending.generation != self.snapshot.generation
            || pending.session != self.snapshot.register_session
            || pending.frame != self.snapshot.frame.level
            || pending.request_context != self.register_context()
            || pending.epoch != self.snapshot.memory_selection_epoch
            || pending.route_key != self.memory_route_fingerprint(&pending.policy.channel)
            || pending.policy != self.monitor_policy(&pending.item)
        {
            return true;
        }
        let expected = self.register_context();
        let binding = self
            .monitor
            .samples
            .get(&pending.key)
            .and_then(|sample| sample.binding.clone());
        let receipt = if !pending.resolve && error.is_none() {
            binding
                .as_ref()
                .map(|b| {
                    self.memory_receipt(
                        result,
                        &memory_access::Expected {
                            context: &pending.request_context,
                            epoch: pending.epoch,
                            channel: &pending.policy.channel,
                            address: &format!("0x{:x}", b.address),
                            bits: b.bits as u16,
                            little: b.little_endian,
                            dump: false,
                        },
                    )
                })
                .unwrap_or_else(|| Err("Memory response has no matching binding".into()))
                .map(Some)
        } else {
            Ok(None)
        };
        let Some(sample) = self.monitor.samples.get_mut(&pending.key) else {
            return true;
        };
        let mut error = error.map(str::to_owned);
        if let Err(reason) = &receipt {
            error = Some(reason.clone());
        }
        if pending.resolve && error.is_none() {
            let resolved_context =
                serde_json::from_value::<crate::registers::Context>(result["context"].clone());
            let binding = serde_json::from_value::<Binding>(result.clone()).and_then(|binding| {
                if resolved_context.as_ref().is_ok_and(|c| *c == expected)
                    && self.snapshot.state == "STOPPED"
                    && result["selection_epoch"].as_u64() == Some(pending.epoch)
                    && !binding.binding_id.is_empty()
                    && result["source"] == "gdb_typed_address"
                    && result["state"] == "STOPPED"
                    && result["thread"].as_str().is_some_and(|t| !t.is_empty())
                    && result["frame_address"]
                        .as_str()
                        .is_some_and(|pc| !pc.is_empty())
                    && result["little_endian"].is_boolean()
                    && matches!(binding.bits, 8 | 16 | 32 | 64)
                    && binding.address.is_multiple_of(u64::from(binding.bits / 8))
                {
                    Ok(binding)
                } else {
                    Err(<serde_json::Error as serde::de::Error>::custom(
                        "Watch resolution has no current typed thread/frame proof",
                    ))
                }
            });
            match binding {
                Ok(binding) => sample.binding = Some(binding),
                Err(e) => error = Some(e.to_string()),
            };
        } else if error.is_none() {
            if let Some(value) = result["value"].as_u64() {
                sample.changed = sample.value.is_some_and(|old| old != value);
                sample.value = Some(value);
                sample.text = sample
                    .binding
                    .as_ref()
                    .map(|b| b.display(value))
                    .unwrap_or_default();
                sample.sampled = Some(Instant::now());
                sample.receipt = receipt.unwrap();
            } else {
                error = Some("Memory response has no value".into());
            }
        }
        sample.error = error.clone();
        if pending.item.watch.is_some()
            && error
                .as_ref()
                .is_some_and(|error| error.starts_with("Watch binding"))
        {
            sample.binding = None;
        }
        if pending.resolve && error.is_none() {
            self.monitor.next_read = Some((pending.item.clone(), pending.policy.clone()));
        }
        sample.due = Instant::now()
            + Duration::from_millis(if error.is_some() {
                pending.policy.interval_ms.max(1000)
            } else if pending.resolve {
                0
            } else {
                pending.policy.interval_ms.max(50)
            });
        let retained_receipt = sample.receipt.clone();
        if let Some(register) = pending.item.peripheral {
            self.apply_peripheral_monitor(
                register,
                if error.is_none() {
                    result["value"].as_u64()
                } else {
                    None
                },
                error.as_deref(),
                retained_receipt,
                pending.route_key,
            );
        }
        true
    }
    pub(super) fn ensure_monitors(&mut self, engine: Option<&EngineHandle>) -> bool {
        if self.demo
            || self.setup.is_some()
            || self.quitting
            || self.pending_task.is_some()
            || self.monitor.busy()
            || self.memory_panel.busy()
            || self.memory_panel.modal()
            || !self.pending_commands.is_empty()
            || self.pending_view.is_some()
            || self.completion.busy()
            || self.symbol_search.busy()
        {
            return false;
        }
        let mut items = self.visible_peripheral_monitors();
        if self.variable_pane == 1 && self.view_rects[1].height > 0 {
            let rows = watch::rows(&self.snapshot.watches);
            let start = self.view_tops[1] / 2;
            let end = (self.view_tops[1] + self.view_rects[1].height as usize).div_ceil(2);
            let indices = rows
                .iter()
                .enumerate()
                .skip(start)
                .take(end.saturating_sub(start))
                .filter(|(_, n)| {
                    !n.more
                        && n.value
                            .tree
                            .as_ref()
                            .is_none_or(|t| t.child_count == 0 || t.type_name.contains('*'))
                })
                .map(|(i, _)| i * 2)
                .collect::<Vec<_>>();
            items.extend(
                indices
                    .into_iter()
                    .filter_map(|i| self.watch_monitor_item(i)),
            );
        }
        if let Some((item, _)) = self.monitor.next_read.take() {
            let exists = item.watch.as_ref().is_none_or(|_| {
                watch::rows(&self.snapshot.watches)
                    .iter()
                    .any(|r| r.key() == item.key)
            });
            let policy = self.monitor_policy(&item);
            if exists && (policy.interval_ms == 0 || items.iter().any(|i| i.key == item.key)) {
                return self.request_monitor(engine, item, policy);
            }
        }
        for offset in 0..items.len() {
            let index = (self.monitor.fair_index + offset) % items.len();
            let item = items[index].clone();
            let policy = self.monitor_policy(&item);
            if policy.interval_ms == 0 || !item.safe_auto {
                continue;
            }
            let key = self.monitor_key(&item.key);
            if self
                .monitor
                .samples
                .get(&key)
                .is_some_and(|s| s.due > Instant::now())
            {
                continue;
            }
            if self.request_monitor(engine, item, policy) {
                self.monitor.fair_index = (index + 1) % items.len();
                return true;
            }
        }
        false
    }
    pub(super) fn apply_live_watch(&mut self, live: crate::live_watch::LiveWatchSample) {
        if self.snapshot.state != "RUNNING"
            || live.generation != self.snapshot.generation
            || live.core != self.snapshot.core.as_ref().map(|c| c.index)
            || !self
                .snapshot
                .watches
                .iter()
                .any(|w| w.name == live.expression)
        {
            return;
        }
        let key = self.monitor_key(&format!("watch:{}", live.expression));
        // Explicit per-item memory access/refresh settings take precedence over
        // the legacy raw-global poller; do not overwrite typed samples.
        if self
            .project
            .refresh_policy(
                self.snapshot
                    .core
                    .as_ref()
                    .map(|c| c.name.as_str())
                    .unwrap_or("single"),
                &format!("watch:{}", live.expression),
                None,
            )
            .is_some()
        {
            return;
        }
        let previous = self.monitor.samples.get(&key).and_then(|s| s.value);
        self.monitor.samples.insert(
            key,
            Sample {
                legacy: true,
                binding: None,
                generation: live.generation,
                session: self.snapshot.register_session,
                frame: self.snapshot.frame.level,
                channel: String::new(),
                request_context: self.register_context(),
                epoch: self.snapshot.memory_selection_epoch,
                route_key: String::new(),
                receipt: None,
                value: live.value,
                text: live
                    .value
                    .map(|v| format!("{v} <raw {}-bit>", live.bits))
                    .unwrap_or_default(),
                error: live.error,
                changed: previous.zip(live.value).is_some_and(|(a, b)| a != b),
                due: Instant::now()
                    + Duration::from_millis(
                        self.project
                            .live_watch
                            .as_ref()
                            .map(|c| c.interval_ms)
                            .unwrap_or(200),
                    ),
                sampled: live.value.map(|_| Instant::now()),
            },
        );
    }
    pub(super) fn watch_sample(&self, key: &str) -> Option<(String, bool, bool)> {
        let sample = self.monitor.samples.get(&self.monitor_key(key))?;
        if sample.generation != self.snapshot.generation
            || sample.session != self.snapshot.register_session
            || sample.frame != self.snapshot.frame.level
            || sample.epoch != self.snapshot.memory_selection_epoch
            || (!sample.legacy
                && sample.route_key != self.memory_route_fingerprint(&sample.channel))
            || (sample.legacy && self.snapshot.state != "RUNNING")
        {
            return None;
        }
        if let Some(error) = &sample.error {
            return Some((
                format!(
                    "{}! {error}",
                    sample
                        .value
                        .map(|_| format!("{} (stale) · ", sample.text))
                        .unwrap_or_default()
                ),
                false,
                true,
            ));
        }
        sample
            .value
            .map(|_| (sample.text.clone(), sample.changed, false))
    }
    pub(super) fn monitor_fresh(&self, key: &str) -> bool {
        let exact = self.monitor_key(key);
        self.monitor.samples.iter().any(|(k, s)| {
            (k == &exact
                || exact
                    .strip_prefix(k)
                    .is_some_and(|tail| tail.starts_with('.')))
                && s.generation == self.snapshot.generation
                && s.session == self.snapshot.register_session
                && s.frame == self.snapshot.frame.level
                && (!s.legacy || self.snapshot.state == "RUNNING")
                && s.epoch == self.snapshot.memory_selection_epoch
                && (s.legacy
                    || (s.route_key == self.memory_route_fingerprint(&s.channel)
                        && s.receipt.as_ref().is_some_and(|r| {
                            r.state == self.snapshot.state
                                && r.access.context == self.register_context()
                        })))
                && s.error.is_none()
                && s.sampled.is_some_and(|at| {
                    at.elapsed()
                        < Duration::from_secs(2).max(s.due.saturating_duration_since(at) * 3)
                })
        })
    }
    pub(super) fn manual_monitor(&mut self, engine: Option<&EngineHandle>, item: Item) -> bool {
        let policy = self.monitor_policy(&item);
        if self.snapshot.state == "RUNNING"
            && !self.project.memory_access.iter().any(|a| {
                a.id == policy.channel
                    && a.while_running
                    && (a.cores.is_empty() || a.cores.contains(&self.register_context().core))
            })
        {
            self.notice="Selected memory access requires a stopped core. Pause or select a running-capable channel.".into();
            return false;
        }
        self.request_monitor(engine, item, policy)
    }
}
pub(super) fn draw(f: &mut UiFrame, a: &mut App) {
    a.monitor.hits.clear();
    let Some(p) = &a.monitor.popup else {
        return;
    };
    let channels = a.monitor_channels();
    let route = channels
        .iter()
        .find(|(id, _)| *id == p.policy.channel)
        .map(|(_, label)| label.as_str())
        .unwrap_or("Unavailable channel");
    let rect = super::center(f.area(), 100, 20);
    theme::overlay(f, rect);
    let card = theme::card(" Memory access / refresh · Esc close ", true);
    let inner = card.inner(rect);
    f.render_widget(card, rect);
    let labels = [
        format!("Access  ‹ {route} ›"),
        format!("Live refresh  [{}]", if p.enabled { "on" } else { "off" }),
        format!("Interval (ms)  {}", p.interval),
        "Apply & save".into(),
        "Read once & save".into(),
        "Cancel".into(),
    ];
    f.render_widget(
        Paragraph::new(p.item.name.clone()).style(Style::default().fg(theme::ACCENT)),
        Rect::new(inner.x + 1, inner.y, inner.width.saturating_sub(2), 1),
    );
    for (i, label) in labels.into_iter().enumerate() {
        let y = inner.y + 2 + i as u16;
        if y >= inner.bottom() {
            break;
        }
        let hit = Rect::new(inner.x + 1, y, inner.width.saturating_sub(2), 1);
        f.render_widget(
            Paragraph::new(label).style(theme::selected(p.field == i)),
            hit,
        );
        a.monitor.hits.push((hit, i));
    }
    if inner.height > 8 {
        let description = format!(
            "{}\nSource: {}",
            a.memory_access_description(&p.policy),
            if p.policy.channel.is_empty() {
                "GDB connection"
            } else {
                &a.project.memory_access_source
            }
        );
        f.render_widget(
            Paragraph::new(description)
                .wrap(Wrap { trim: false })
                .style(Style::default().fg(theme::MUTED)),
            Rect::new(
                inner.x + 1,
                inner.y + 8,
                inner.width.saturating_sub(2),
                (inner.height - 8).min(4),
            ),
        );
    }
    if inner.height > 12 {
        f.render_widget(Paragraph::new("Tab / arrows select · Enter apply / toggle\nInterval: Delete clears, type 50..60000 ms\nOnly visible expanded values are polled.\nWatch addresses resolve while stopped; no implicit halt.").style(Style::default().fg(theme::MUTED)),Rect::new(inner.x+1,inner.y+12,inner.width.saturating_sub(2),inner.height-12));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> App {
        let mut a = App::new(Project::default(), false);
        a.snapshot.state = "STOPPED".into();
        a.snapshot.watches = vec![Variable {
            name: "counter".into(),
            value: "1".into(),
            ..Default::default()
        }];
        a.variable_pane = 1;
        a.view_rects[1] = Rect::new(0, 0, 40, 10);
        a.project.memory_access.push(crate::config::MemoryAccess {
            id: "bus".into(),
            label: "AHB".into(),
            target: "soc.bus".into(),
            tcl_endpoint: "127.0.0.1:6666".into(),
            while_running: true,
            cores: vec![],
        });
        a.project.ui.refresh.insert(
            "single|watch:counter".into(),
            RefreshPolicy {
                channel: "bus".into(),
                interval_ms: 100,
            },
        );
        a
    }
    fn resolved(a: &mut App, id: u64) {
        let result = json!({"address":536870912,"bits":32,"little_endian":true,"signed":false,"float":false,
            "binding_id":"fixture-binding","selection_epoch":a.snapshot.memory_selection_epoch,
            "context":a.register_context(),"thread":"1","frame_address":"0x100000008","source":"gdb_typed_address","state":"STOPPED"});
        assert!(a.monitor_response(id, &result, None));
    }
    #[test]
    fn watch_resolution_requires_current_typed_proof_before_scheduling_a_bus_read() {
        for scenario in [
            "missing-context",
            "old-context",
            "no-thread",
            "running",
            "bad-width",
        ] {
            let (mut a, (engine, requests)) = (app(), session::test_channel());
            assert!(a.ensure_monitors(Some(&engine)));
            let request = requests.try_recv().unwrap();
            assert_eq!(request.method, "watch_resolve");
            assert_eq!(request.params["context"], json!(a.register_context()));
            let mut result = json!({"address":536870912,"bits":32,"little_endian":true,
                "binding_id":"fixture-binding","selection_epoch":a.snapshot.memory_selection_epoch,
                "context":a.register_context(),"thread":"1","frame_address":"0x100000008",
                "source":"gdb_typed_address","state":"STOPPED"});
            match scenario {
                "missing-context" => {
                    result.as_object_mut().unwrap().remove("context");
                }
                "old-context" => {
                    result["context"]["session"] = json!(a.snapshot.register_session + 1)
                }
                "no-thread" => result["thread"] = json!(""),
                "running" => a.snapshot.state = "RUNNING".into(),
                "bad-width" => result["bits"] = json!(0),
                _ => unreachable!(),
            }
            assert!(a.monitor_response(request.id, &result, None));
            assert!(a.monitor.next_read.is_none(), "{scenario}");
            let sample = a.monitor.samples.values().next().unwrap();
            assert!(
                sample.binding.is_none() && sample.error.is_some(),
                "{scenario}"
            );
            assert!(!a.ensure_monitors(Some(&engine)));
            assert!(requests.try_recv().is_err());
        }
    }
    #[test]
    fn watch_monitor_requests_use_the_worker_stop_generation_instead_of_the_coordinator_revision() {
        let (mut a, (engine, requests)) = (app(), session::test_channel());
        a.snapshot.generation = 900;
        a.snapshot.register_generation = Some(7);
        assert!(a.ensure_monitors(Some(&engine)));
        let request = requests.try_recv().unwrap();
        assert_eq!(request.method, "watch_resolve");
        assert_eq!(request.params["context"]["generation"], 7);
        resolved(&mut a, request.id);
        assert!(a.ensure_monitors(Some(&engine)));
        let request = requests.try_recv().unwrap();
        assert_eq!(request.method, "memory_read");
        assert_eq!(request.params["context"]["generation"], 7);
        assert_eq!(request.params["context"], json!(a.register_context()));
    }
    #[test]
    fn visible_access_entry_shows_target_endpoint_and_source_without_issuing_a_read() {
        let (mut app, (engine, requests)) = (app(), session::test_channel());
        app.project.memory_access_source = "profile:fixture.toml".into();
        let target = app.project.memory_access[0].target.clone();
        let endpoint = app.project.memory_access[0].tcl_endpoint.clone();
        app.command(Some(&engine), ":watch-access");
        assert!(app.monitor.modal());
        assert!(requests.try_recv().is_err());
        let mut terminal = Terminal::new(TestBackend::new(100, 25)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains(&target));
        assert!(text.contains(&endpoint));
        assert!(text.contains("profile:fixture.toml"));
        app.monitor_key_event(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            Some(&engine),
        );
        assert!(requests.try_recv().is_err());
    }
    #[test]
    fn chip_session_and_frame_boundaries_isolate_policies_and_discard_inflight_bindings() {
        let (mut app, (engine, requests)) = (app(), session::test_channel());
        let old = app
            .project
            .ui
            .refresh
            .remove("single|watch:counter")
            .unwrap();
        app.project.debug.chip = "chip-a".into();
        let key = app.monitor_key("watch:counter");
        app.project.ui.refresh.insert(key.clone(), old);
        assert!(app.ensure_monitors(Some(&engine)));
        let pending = requests.try_recv().unwrap();
        assert_eq!(pending.method, "watch_resolve");
        app.snapshot.register_session += 1;
        resolved(&mut app, pending.id);
        assert!(app.monitor.next_read.is_none());
        assert!(!app.pending_commands.contains(&pending.id));
        let item = app.watch_monitor_item(0).unwrap();
        assert!(app.manual_monitor(Some(&engine), item));
        resolved(&mut app, requests.try_recv().unwrap().id);
        assert!(app.ensure_monitors(Some(&engine)));
        let pending = requests.try_recv().unwrap();
        assert_eq!(pending.method, "memory_read");
        app.snapshot.frame.level += 1;
        app.monitor_response(pending.id, &json!({"value":77}), None);
        assert!(app.watch_sample("watch:counter").is_none());
        assert!(app.pending_commands.is_empty());
        app.project.debug.chip = "chip-b".into();
        assert_ne!(app.monitor_key("watch:counter"), key);
        assert!(
            app.monitor_policy(&app.watch_monitor_item(0).unwrap())
                .channel
                .is_empty()
        );
    }
    #[test]
    fn visible_watch_resolves_once_and_running_poll_never_sends_gdb_or_halt() {
        let (mut a, (engine, rx)) = (app(), session::test_channel());
        assert!(a.ensure_monitors(Some(&engine)));
        let request = rx.try_recv().unwrap();
        assert_eq!(request.method, "watch_resolve");
        resolved(&mut a, request.id);
        assert!(a.ensure_monitors(Some(&engine)));
        let request = rx.try_recv().unwrap();
        assert_eq!(request.method, "memory_read");
        let result = memory_access::scalar_fixture(&a, &request, 1);
        a.monitor_response(request.id, &result, None);
        assert!(!a.ensure_monitors(Some(&engine))); // configured interval, no busy polling
        a.snapshot.state = "RUNNING".into();
        a.monitor
            .samples
            .values_mut()
            .for_each(|s| s.due = Instant::now());
        assert!(a.ensure_monitors(Some(&engine)));
        let request = rx.try_recv().unwrap();
        assert_eq!(request.method, "memory_read");
        assert_eq!(request.params["channel"], "bus");
        let result = memory_access::scalar_fixture(&a, &request, 7);
        a.monitor_response(request.id, &result, None);
        assert_eq!(
            a.watch_sample("watch:counter"),
            Some(("7".into(), true, false))
        );
        assert!(a.monitor_fresh("watch:counter"));
        assert_eq!(a.snapshot.watches[0].value, "1"); // preserve the stopped snapshot
        a.variable_pane = 9;
        a.monitor
            .samples
            .values_mut()
            .for_each(|s| s.due = Instant::now());
        assert!(!a.ensure_monitors(Some(&engine)));
        a.variable_pane = 1;
        a.project
            .ui
            .refresh
            .get_mut("single|watch:counter")
            .unwrap()
            .channel
            .clear();
        assert!(!a.ensure_monitors(Some(&engine)));
        assert!(rx.try_recv().is_err());
        a.snapshot.generation += 1;
        assert!(!a.monitor_fresh("watch:counter"));
        assert!(a.watch_sample("watch:counter").is_none());
    }
    #[test]
    fn manual_resolution_chains_once_error_backoff_and_core_switch_discards_late_reply() {
        let (mut a, (engine, rx)) = (app(), session::test_channel());
        a.project
            .ui
            .refresh
            .get_mut("single|watch:counter")
            .unwrap()
            .interval_ms = 0;
        let item = a.watch_monitor_item(0).unwrap();
        assert!(a.manual_monitor(Some(&engine), item));
        resolved(&mut a, rx.try_recv().unwrap().id);
        assert!(a.ensure_monitors(Some(&engine)));
        let req = rx.try_recv().unwrap();
        a.monitor_response(req.id, &Value::Null, Some("AP unavailable"));
        assert!(a.watch_sample("watch:counter").unwrap().2);
        assert!(!a.ensure_monitors(Some(&engine)));
        a.project
            .ui
            .refresh
            .get_mut("single|watch:counter")
            .unwrap()
            .interval_ms = 50;
        assert!(!a.ensure_monitors(Some(&engine))); // failure backoff overrides 50 ms
        let item = a.watch_monitor_item(0).unwrap();
        assert!(a.manual_monitor(Some(&engine), item));
        let req = rx.try_recv().unwrap();
        let mut snapshot = a.snapshot.clone();
        snapshot.core = Some(session::CoreStatus {
            index: 1,
            name: "core1".into(),
            endpoint: "1".into(),
            state: "STOPPED".into(),
        });
        a.update(Event::Snapshot {
            snapshot: Box::new(snapshot),
        });
        assert!(a.monitor.busy());
        a.monitor_response(req.id, &json!({"value":999}), None);
        assert!(!a.monitor.busy());
        assert!(a.watch_sample("watch:counter").is_none());
        assert!(
            a.monitor_policy(&a.watch_monitor_item(0).unwrap())
                .channel
                .is_empty()
        );
    }
    #[test]
    fn refresh_popup_preserves_interval_validates_and_can_render_at_small_sizes() {
        let mut a = app();
        a.open_monitor(1, 0);
        assert_eq!(a.monitor.popup.as_ref().unwrap().interval, "100");
        for (w, h) in [(45, 12), (80, 24), (180, 50)] {
            let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
            t.draw(|f| super::super::draw(f, &mut a)).unwrap();
            assert!(
                a.monitor
                    .hits
                    .iter()
                    .all(|(r, _)| r.right() <= w && r.bottom() <= h)
            );
        }
        if let Ok(root) = std::env::var("DEBUGTUI_RENDER_DIR") {
            fs::create_dir_all(&root).unwrap();
            super::super::visual_tests::capture(
                Path::new(&root),
                "refresh-settings",
                &mut a,
                160,
                45,
            );
        }
        a.monitor.popup.as_mut().unwrap().interval = "1".into();
        a.monitor_apply(None, false);
        assert!(a.monitor.modal());
        a.monitor.popup.as_mut().unwrap().interval = "75".into();
        a.monitor_apply(None, false);
        assert!(!a.monitor.modal());
        assert_eq!(a.project.ui.refresh["single|watch:counter"].interval_ms, 75);
        a.open_monitor(1, 0);
        a.monitor.popup.as_mut().unwrap().item.safe_auto = false;
        a.monitor_apply(None, false);
        assert!(a.monitor.modal());
        a.monitor_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), None);
        assert!(!a.monitor.modal());
    }
    #[test]
    fn scalar_decoding_matches_signed_float_and_wide_integer_types() {
        let mut b = Binding {
            bits: 8,
            signed: true,
            ..Default::default()
        };
        assert_eq!(b.display(255), "-1");
        b.bits = 32;
        assert_eq!(b.display(0x80000000), "-2147483648");
        b.signed = false;
        assert_eq!(b.display(0xffffffff), "4294967295");
        b.float = true;
        assert_eq!(b.display(1.5f32.to_bits() as u64), "1.5");
        b.bits = 64;
        assert_eq!(b.display((-2.75f64).to_bits()), "-2.75");
    }
    #[test]
    fn per_core_routes_and_policies_are_isolated() {
        let mut a = app();
        a.project.memory_access[0].cores = vec!["core1".into()];
        assert_eq!(a.monitor_channels().len(), 1);
        a.snapshot.core = Some(session::CoreStatus {
            index: 1,
            name: "core1".into(),
            endpoint: "1".into(),
            state: "STOPPED".into(),
        });
        assert_eq!(a.monitor_channels().len(), 2);
        a.project.ui.refresh.insert(
            "core1|watch:counter".into(),
            RefreshPolicy {
                channel: "bus".into(),
                interval_ms: 250,
            },
        );
        assert_eq!(
            a.monitor_policy(&a.watch_monitor_item(0).unwrap())
                .interval_ms,
            250
        );
        a.snapshot.core.as_mut().unwrap().name = "core0".into();
        assert_eq!(a.monitor_channels().len(), 1);
        assert_eq!(
            a.monitor_policy(&a.watch_monitor_item(0).unwrap())
                .interval_ms,
            0
        );
    }
    #[test]
    fn hidden_or_deleted_watch_does_not_chain_a_background_read_after_resolution() {
        for deleted in [false, true] {
            let (mut a, (engine, rx)) = (app(), session::test_channel());
            assert!(a.ensure_monitors(Some(&engine)));
            let request = rx.try_recv().unwrap();
            if deleted {
                a.snapshot.watches.clear();
            } else {
                a.variable_pane = 9;
            }
            resolved(&mut a, request.id);
            assert!(!a.ensure_monitors(Some(&engine)));
            assert!(rx.try_recv().is_err());
        }
    }
    #[test]
    fn watch_binding_survives_continue_revision_but_thread_epoch_and_route_changes_discard_samples()
    {
        let (mut a, (engine, requests)) = (app(), session::test_channel());
        a.snapshot.generation = 100;
        a.snapshot.register_generation = Some(7);
        assert!(a.ensure_monitors(Some(&engine)));
        resolved(&mut a, requests.recv().unwrap().id);
        assert!(a.ensure_monitors(Some(&engine)));
        let read = requests.recv().unwrap();
        let result = memory_access::scalar_fixture(&a, &read, 1);
        a.monitor_response(read.id, &result, None);
        let mut next = a.snapshot.clone();
        next.state = "RUNNING".into();
        next.generation += 1;
        a.update(Event::Snapshot {
            snapshot: Box::new(next),
        });
        a.monitor
            .samples
            .values_mut()
            .for_each(|sample| sample.due = Instant::now());
        assert!(a.ensure_monitors(Some(&engine)));
        let read = requests.recv().unwrap();
        assert_eq!(read.method, "memory_read");
        assert_eq!(read.params["watch_binding"], "fixture-binding");
        assert_eq!(read.params["context"]["generation"], 7);
        let result = memory_access::scalar_fixture(&a, &read, 2);
        a.monitor_response(read.id, &result, None);
        let sampled = a.monitor.samples.values().next().unwrap().sampled;
        let item = a.watch_monitor_item(0).unwrap();
        assert!(a.manual_monitor(Some(&engine), item));
        let read = requests.recv().unwrap();
        a.monitor_response(read.id, &Value::Null, Some("AP failure"));
        assert_eq!(a.monitor.samples.values().next().unwrap().sampled, sampled);
        assert!(
            a.watch_sample("watch:counter")
                .unwrap()
                .0
                .contains("2 (stale)")
        );
        assert!(
            a.access_caption(1, 0)
                .contains("retained / stale: bus → soc.bus")
        );
        a.project.memory_access[0].target = "soc.changed".into();
        assert!(a.watch_sample("watch:counter").is_none());
        let item = a.watch_monitor_item(0).unwrap();
        assert!(!a.manual_monitor(Some(&engine), item));
        assert!(requests.try_recv().is_err());
        let mut next = a.snapshot.clone();
        next.memory_selection_epoch += 1;
        a.update(Event::Snapshot {
            snapshot: Box::new(next),
        });
        assert!(a.monitor.samples.is_empty());
        assert!(!a.ensure_monitors(Some(&engine)));
        assert!(requests.try_recv().is_err());
    }
}
