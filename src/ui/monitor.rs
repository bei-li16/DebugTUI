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
    channel: String,
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
    item: Item,
    key: String,
    resolve: bool,
    policy: RefreshPolicy,
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
    pub fn modal(&self) -> bool {
        self.popup.is_some()
    }
}
impl App {
    fn monitor_key(&self, key: &str) -> String {
        format!(
            "{}|{key}",
            self.snapshot
                .core
                .as_ref()
                .map(|c| c.name.as_str())
                .unwrap_or("single")
        )
    }
    fn monitor_policy(&self, item: &Item) -> RefreshPolicy {
        self.project
            .ui
            .refresh
            .get(&self.monitor_key(&item.key))
            .cloned()
            .or_else(|| {
                item.watch.as_ref().and_then(|(root, _)| {
                    self.project
                        .ui
                        .refresh
                        .get(&self.monitor_key(&format!("watch:{root}")))
                        .cloned()
                })
            })
            .unwrap_or_default()
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
    fn monitor_channels(&self) -> Vec<(String, String)> {
        let core = self.snapshot.core.as_ref().map(|c| &c.name);
        std::iter::once((String::new(), "GDB · selected core · stopped only".into()))
            .chain(
                self.project
                    .memory_access
                    .iter()
                    .filter(|a| a.cores.is_empty() || core.is_some_and(|c| a.cores.contains(c)))
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
            && !self
                .project
                .memory_access
                .iter()
                .any(|a| a.id == policy.channel && a.while_running)
        {
            return false;
        }
        let key = self.monitor_key(&item.key);
        let generation = self.snapshot.generation;
        let sample = self
            .monitor
            .samples
            .entry(key.clone())
            .or_insert_with(|| Sample {
                legacy: false,
                binding: item.memory.clone(),
                generation,
                channel: policy.channel.clone(),
                value: None,
                text: String::new(),
                error: None,
                changed: false,
                due: Instant::now(),
                sampled: None,
            });
        if sample.generation != generation && self.snapshot.state == "STOPPED" {
            sample.binding = item.memory.clone();
            sample.generation = generation;
        }
        if sample.channel != policy.channel {
            sample.channel = policy.channel.clone();
            sample.sampled = None;
            sample.value = None;
        }
        let (method, params, resolve) = if let Some(binding) = &sample.binding {
            (
                "memory_read",
                json!({"address":binding.address,"bits":binding.bits,"little_endian":binding.little_endian,"channel":policy.channel}),
                false,
            )
        } else if self.snapshot.state == "STOPPED"
            && let Some((expression, path)) = &item.watch
        {
            (
                "watch_resolve",
                json!({"expression":expression,"path":path}),
                true,
            )
        } else {
            sample.error = Some("Pause once to resolve this expression's address".into());
            sample.due = Instant::now() + Duration::from_secs(1);
            return false;
        };
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
            item,
            key,
            resolve,
            policy,
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
        if pending.context != self.snapshot.core.as_ref().map(|c| c.index)
            || pending.generation != self.snapshot.generation
        {
            return true;
        }
        let Some(sample) = self.monitor.samples.get_mut(&pending.key) else {
            return true;
        };
        let mut error = error.map(str::to_owned);
        if pending.resolve && error.is_none() {
            match serde_json::from_value::<Binding>(result.clone()) {
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
            } else {
                error = Some("Memory response has no value".into());
            }
        }
        sample.error = error.clone();
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
        if let Some(register) = pending.item.peripheral {
            self.apply_peripheral_monitor(register, result["value"].as_u64(), error.as_deref());
        }
        true
    }
    pub(super) fn ensure_monitors(&mut self, engine: Option<&EngineHandle>) -> bool {
        if self.demo
            || self.setup.is_some()
            || self.quitting
            || self.pending_task.is_some()
            || self.monitor.busy()
            || !self.pending_commands.is_empty()
            || self.pending_view.is_some()
            || self.completion.busy()
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
        if self.project.ui.refresh.contains_key(&key) {
            return;
        }
        let previous = self.monitor.samples.get(&key).and_then(|s| s.value);
        self.monitor.samples.insert(
            key,
            Sample {
                legacy: true,
                binding: None,
                generation: live.generation,
                channel: String::new(),
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
            || (sample.legacy && self.snapshot.state != "RUNNING")
        {
            return None;
        }
        if let Some(error) = &sample.error {
            return Some((format!("! {error}"), false, true));
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
                && (!s.legacy || self.snapshot.state == "RUNNING")
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
            && !self
                .project
                .memory_access
                .iter()
                .any(|a| a.id == policy.channel && a.while_running)
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
    let rect = super::center(f.area(), 85, 16);
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
    if inner.height > 10 {
        f.render_widget(Paragraph::new("Tab / arrows select · Enter apply / toggle\nInterval: Delete clears, type 50..60000 ms\nOnly visible expanded values are polled.\nWatch addresses resolve while stopped; no implicit halt.").style(Style::default().fg(theme::MUTED)),Rect::new(inner.x+1,inner.y+9,inner.width.saturating_sub(2),inner.height-9));
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
        assert!(a.monitor_response(id,&json!({"address":536870912,"bits":32,"little_endian":true,"signed":false,"float":false}),None));
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
        a.monitor_response(request.id, &json!({"value":1}), None);
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
        a.monitor_response(request.id, &json!({"value":7}), None);
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
}
