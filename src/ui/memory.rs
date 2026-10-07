//! Memory ranges use an explicit route and keep responses scoped to their request.
use super::*;
use crate::{config::MemoryRange, registers::Context};

struct Popup {
    range: MemoryRange,
    count: String,
    channel: String,
    field: usize,
}
struct Pending {
    id: u64,
    key: String,
    context: Context,
    channel: String,
    range: MemoryRange,
    epoch: u64,
    route_key: String,
}
struct Sample {
    key: String,
    context: Context,
    channel: String,
    range: MemoryRange,
    state: String,
    lines: Vec<String>,
    sampled: Instant,
    receipt: memory_access::Receipt,
    epoch: u64,
    route_key: String,
}
#[derive(Default)]
pub(super) struct MemoryView {
    popup: Option<Popup>,
    pending: Option<Pending>,
    sample: Option<Sample>,
    attempted: Option<(String, Context, String, u64)>,
    error: Option<String>,
    hits: Vec<(Rect, usize)>,
}
impl MemoryView {
    pub(super) fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub(super) fn modal(&self) -> bool {
        self.popup.is_some()
    }
}

impl App {
    pub(super) fn memory_edit_candidate(&self) -> writes::Candidate {
        let range = self.memory_range();
        writes::Candidate {
            target: json!({"kind":"memory","address":range.address,"bits":8,"channel":self.memory_channel()}),
            selection: crate::writes::Selection::Register,
            title: "RAM · literal address and byte count".into(),
            bits: 8,
            value: String::new(),
            reason: None,
        }
    }
    pub(super) fn memory_snapshot(&mut self, next: &Snapshot) {
        let owner_changed = next
            .core
            .as_ref()
            .map(|core| (&core.name, core.index, &core.endpoint))
            != self
                .snapshot
                .core
                .as_ref()
                .map(|core| (&core.name, core.index, &core.endpoint))
            || next.register_session != self.snapshot.register_session
            || next.memory_selection_epoch != self.snapshot.memory_selection_epoch;
        if owner_changed {
            self.memory_panel.sample = None;
            self.memory_panel.attempted = None;
        }
        if owner_changed
            || next.generation != self.snapshot.generation
            || next.frame.level != self.snapshot.frame.level
        {
            self.memory_panel.error = None;
        }
        if owner_changed
            || matches!(
                next.state.as_str(),
                state::DISCONNECTED | state::STARTING_GDB | state::FAULT
            )
        {
            self.memory_panel.popup = None;
        }
    }
    fn memory_key(&self) -> String {
        format!(
            "chip:{}|{}|memory:range",
            self.project.debug.chip,
            self.snapshot
                .core
                .as_ref()
                .map(|core| core.name.as_str())
                .unwrap_or("single")
        )
    }
    fn memory_range(&self) -> MemoryRange {
        let core = self
            .snapshot
            .core
            .as_ref()
            .map(|core| core.name.as_str())
            .unwrap_or("single");
        self.project
            .ui
            .memory
            .get(&self.memory_key())
            .or_else(|| self.project.ui.memory.get(&format!("{core}|memory:range")))
            .or_else(|| self.project.ui.memory.get("memory:range"))
            .cloned()
            .unwrap_or_default()
    }
    fn memory_channel(&self) -> String {
        self.project
            .ui
            .refresh
            .get(&self.memory_key())
            .or_else(|| {
                self.project.refresh_policy(
                    self.snapshot
                        .core
                        .as_ref()
                        .map(|core| core.name.as_str())
                        .unwrap_or("single"),
                    "memory:range",
                    None,
                )
            })
            .map(|p| p.channel.clone())
            .unwrap_or_default()
    }
    fn memory_routes(&self) -> Vec<String> {
        let core = self
            .snapshot
            .core
            .as_ref()
            .map(|core| core.name.as_str())
            .unwrap_or("default");
        std::iter::once(String::new())
            .chain(
                self.project
                    .memory_access
                    .iter()
                    .filter(|channel| {
                        channel.cores.is_empty() || channel.cores.iter().any(|name| name == core)
                    })
                    .map(|channel| channel.id.clone()),
            )
            .collect()
    }
    pub(super) fn open_memory_access(&mut self) {
        let range = self.memory_range();
        self.memory_panel.popup = Some(Popup {
            count: range.count.to_string(),
            range,
            channel: self.memory_channel(),
            field: 0,
        });
        self.formats.popup = None;
        self.editing = false;
        self.watch_editing = false;
        self.completion.invalidate();
    }
    fn set_memory_range(
        &mut self,
        range: MemoryRange,
        channel: String,
        engine: Option<&EngineHandle>,
    ) -> bool {
        if !(1..=4096).contains(&range.count)
            || range.address.is_empty()
            || range.address.len() > 256
            || range.address.chars().any(char::is_control)
        {
            self.notice = "Enter an address and a byte count of 1..4096.".into();
            return false;
        }
        if !channel.is_empty() {
            if !self.memory_routes().contains(&channel) {
                self.notice = "Memory channel is not available for this core.".into();
                return false;
            }
            let value = if let Some(hex) = range
                .address
                .strip_prefix("0x")
                .or_else(|| range.address.strip_prefix("0X"))
            {
                u64::from_str_radix(hex, 16)
            } else {
                range.address.parse()
            };
            if value
                .ok()
                .and_then(|value| value.checked_add(range.count))
                .is_none()
            {
                self.notice = "Bus access needs a literal hexadecimal or decimal address and a range without overflow.".into();
                return false;
            }
        }
        let key = self.memory_key();
        self.project.ui.memory.insert(key.clone(), range);
        self.project.ui.refresh.insert(
            key,
            crate::config::RefreshPolicy {
                channel,
                interval_ms: 0,
            },
        );
        self.memory_panel.sample = None;
        self.memory_panel.error = None;
        self.memory_panel.attempted = None;
        self.save_ui(engine);
        true
    }
    pub(super) fn memory_command(&mut self, engine: Option<&EngineHandle>, arg: &str) {
        let mut parts = arg.split_whitespace();
        let range = MemoryRange {
            address: parts.next().unwrap_or("$sp").into(),
            count: parts.next().unwrap_or("256").parse().unwrap_or(0),
        };
        if parts.next().is_some() {
            self.notice = "Usage: :memory ADDRESS [COUNT]".into();
            return;
        }
        if self.set_memory_range(range, self.memory_channel(), engine) {
            self.select_pane(pane::MEMORY);
            self.request_memory_dump(engine, true);
        }
    }
    pub(super) fn memory_key_event(
        &mut self,
        key: KeyEvent,
        engine: Option<&EngineHandle>,
    ) -> bool {
        let routes = self.memory_routes();
        let Some(popup) = &mut self.memory_panel.popup else {
            return false;
        };
        match key.code {
            KeyCode::Esc => self.memory_panel.popup = None,
            KeyCode::Tab | KeyCode::Down => popup.field = (popup.field + 1) % 5,
            KeyCode::BackTab | KeyCode::Up => popup.field = (popup.field + 4) % 5,
            KeyCode::Left | KeyCode::Right | KeyCode::Enter if popup.field == 2 => {
                let index = routes
                    .iter()
                    .position(|channel| channel == &popup.channel)
                    .unwrap_or(0);
                let delta = if key.code == KeyCode::Left {
                    routes.len() - 1
                } else {
                    1
                };
                popup.channel = routes[(index + delta) % routes.len()].clone();
            }
            KeyCode::Enter if popup.field == 3 => {
                let range = MemoryRange {
                    address: popup.range.address.clone(),
                    count: popup.count.parse().unwrap_or(0),
                };
                let channel = popup.channel.clone();
                if self.set_memory_range(range, channel, engine) {
                    self.memory_panel.popup = None;
                    self.request_memory_dump(engine, true);
                }
            }
            KeyCode::Enter if popup.field == 4 => self.memory_panel.popup = None,
            KeyCode::Char('u')
                if key.modifiers.contains(KeyModifiers::CONTROL) && popup.field < 2 =>
            {
                if popup.field == 0 {
                    popup.range.address.clear();
                } else {
                    popup.count.clear();
                }
            }
            KeyCode::Backspace if popup.field < 2 => {
                if popup.field == 0 {
                    popup.range.address.pop();
                } else {
                    popup.count.pop();
                }
            }
            KeyCode::Delete if popup.field < 2 => {
                if popup.field == 0 {
                    popup.range.address.clear();
                } else {
                    popup.count.clear();
                }
            }
            KeyCode::Char(c)
                if popup.field == 0 && !c.is_control() && popup.range.address.len() < 256 =>
            {
                popup.range.address.push(c)
            }
            KeyCode::Char(c) if popup.field == 1 && c.is_ascii_digit() && popup.count.len() < 4 => {
                popup.count.push(c)
            }
            _ => {}
        }
        true
    }
    pub(super) fn memory_paste(&mut self, text: &str) -> bool {
        let Some(popup) = &mut self.memory_panel.popup else {
            return false;
        };
        if popup.field == 0 {
            let mut bytes = popup.range.address.len();
            for c in text.chars().filter(|c| !c.is_control()) {
                if bytes + c.len_utf8() > 256 {
                    break;
                }
                popup.range.address.push(c);
                bytes += c.len_utf8();
            }
        } else if popup.field == 1 {
            for c in text.chars().filter(char::is_ascii_digit) {
                if popup.count.len() >= 4 {
                    break;
                }
                popup.count.push(c);
            }
        }
        true
    }
    pub(super) fn memory_mouse(
        &mut self,
        mouse: MouseEvent,
        engine: Option<&EngineHandle>,
    ) -> bool {
        if !self.memory_panel.modal() {
            return false;
        }
        if mouse.kind == MouseEventKind::Down(event::MouseButton::Left)
            && let Some((_, field)) = self
                .memory_panel
                .hits
                .iter()
                .find(|(rect, _)| rect.contains((mouse.column, mouse.row).into()))
                .copied()
        {
            self.memory_panel.popup.as_mut().unwrap().field = field;
            if field >= 2 {
                self.memory_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), engine);
            }
        }
        true
    }
    pub(super) fn ensure_memory_dump(&mut self, engine: Option<&EngineHandle>) -> bool {
        if self.side_pane != pane::MEMORY
            || self.view_rects[pane::MEMORY].height == 0
            || self.demo
            || self.setup.is_some()
            || self.quitting
            || self.memory_panel.modal()
            || self.pending_view.is_some()
            || self.monitor.busy()
            || self.completion.busy()
            || self.symbol_search.busy()
            || !self.pending_commands.is_empty()
        {
            return false;
        }
        self.request_memory_dump(engine, false)
    }
    pub(super) fn request_memory_dump(
        &mut self,
        engine: Option<&EngineHandle>,
        manual: bool,
    ) -> bool {
        let Some(engine) = engine else {
            return false;
        };
        if self.demo || self.memory_panel.busy() || self.setup.is_some() || self.quitting {
            return false;
        }
        if !matches!(
            self.snapshot.state.as_str(),
            state::STOPPED | state::RUNNING
        ) {
            return false;
        }
        let context = self.register_context();
        let key = self.memory_key();
        let channel = self.memory_channel();
        if self.snapshot.state == state::RUNNING
            && !self.project.memory_access.iter().any(|access| {
                access.id == channel
                    && access.while_running
                    && (access.cores.is_empty() || access.cores.contains(&context.core))
            })
        {
            if manual {
                self.notice = "This memory channel requires a stopped core.".into();
            }
            return false;
        }
        // Arbitrary ranges are manual while running. Do not poll unknown MMIO.
        if !manual && self.snapshot.state != state::STOPPED {
            return false;
        }
        let epoch = self.snapshot.memory_selection_epoch;
        let route_key = self.memory_route_fingerprint(&channel);
        let attempt = (key.clone(), context.clone(), route_key.clone(), epoch);
        if !manual && self.memory_panel.attempted.as_ref() == Some(&attempt) {
            return false;
        }
        let range = self.memory_range();
        let id = self.next_id;
        self.memory_panel.attempted = Some(attempt);
        self.memory_panel.error = None;
        self.memory_panel.pending = Some(Pending {
            id,
            key,
            context: context.clone(),
            channel: channel.clone(),
            range: range.clone(),
            epoch,
            route_key,
        });
        self.view_tops[pane::MEMORY] = 0;
        self.submit(Some(engine), method::MEMORY_DUMP, json!({"address":range.address,"count":range.count,"channel":channel,"context":context,"selection_epoch":epoch}));
        true
    }
    pub(super) fn memory_response(&mut self, id: u64, result: &Value, error: Option<&str>) -> bool {
        if !self
            .memory_panel
            .pending
            .as_ref()
            .is_some_and(|pending| pending.id == id)
        {
            return false;
        }
        let pending = self.memory_panel.pending.take().unwrap();
        self.pending_commands.remove(&id);
        self.fx.response(id, error.is_none());
        if pending.key != self.memory_key()
            || pending.context != self.register_context()
            || pending.channel != self.memory_channel()
            || pending.range != self.memory_range()
            || pending.epoch != self.snapshot.memory_selection_epoch
            || pending.route_key != self.memory_route_fingerprint(&pending.channel)
        {
            return true;
        }
        let decoded = (|| -> Result<Sample, String> {
            if let Some(error) = error {
                return Err(error.into());
            }
            if result["channel"].as_str() != Some(pending.channel.as_str())
                || serde_json::from_value::<Context>(result["context"].clone()).ok()
                    != Some(pending.context.clone())
                || (pending.channel.is_empty() && self.snapshot.state != state::STOPPED)
            {
                return Err("Memory response belongs to an expired context or route".into());
            }
            let address = result["address"]
                .as_str()
                .ok_or("Memory response has no address")?;
            let base = u64::from_str_radix(address.trim_start_matches("0x"), 16)
                .map_err(|_| "Invalid memory response address")?;
            if let Ok(requested) = crate::session::literal_address(&pending.range.address)
                && requested != base
            {
                return Err("Memory response belongs to a different requested address".into());
            }
            let route_address = if pending.channel.is_empty() {
                pending.range.address.clone()
            } else {
                format!("0x{base:x}")
            };
            let receipt = self.memory_receipt(
                result,
                &memory_access::Expected {
                    context: &pending.context,
                    epoch: pending.epoch,
                    channel: &pending.channel,
                    address: &route_address,
                    bits: (pending.range.count * 8) as u16,
                    little: true,
                    dump: true,
                },
            )?;
            let bytes = result["bytes"]
                .as_array()
                .ok_or("Memory response has no bytes")?;
            if bytes.len() != self.memory_range().count as usize
                || base.checked_add(bytes.len() as u64).is_none()
            {
                return Err("Invalid memory response byte count".into());
            }
            let bytes = bytes
                .iter()
                .map(|value| {
                    value
                        .as_u64()
                        .and_then(|v| u8::try_from(v).ok())
                        .ok_or("Invalid memory response byte")
                })
                .collect::<Result<Vec<_>, _>>()?;
            let lines = bytes
                .chunks(16)
                .enumerate()
                .map(|(i, row)| {
                    format!(
                        "{:08x}  {}",
                        base + i as u64 * 16,
                        row.iter()
                            .map(|byte| format!("{byte:02x}"))
                            .collect::<Vec<_>>()
                            .join(" ")
                    )
                })
                .collect();
            Ok(Sample {
                key: pending.key,
                context: pending.context,
                channel: pending.channel,
                range: pending.range,
                state: result["state"].as_str().unwrap_or("").into(),
                lines,
                sampled: Instant::now(),
                receipt,
                epoch: pending.epoch,
                route_key: pending.route_key,
            })
        })();
        match decoded {
            Ok(sample) => {
                self.memory_panel.sample = Some(sample);
                self.notice = "Memory range read completed.".into();
            }
            Err(error) => {
                self.memory_panel.error = Some(error.clone());
                self.notice = format!("Memory: {error}");
            }
        }
        true
    }
    pub(super) fn memory_lines(&self) -> &[String] {
        if let Some(sample) = &self.memory_panel.sample {
            if sample.key == self.memory_key()
                && sample.context.session == self.snapshot.register_session
                && sample.epoch == self.snapshot.memory_selection_epoch
                && sample.channel == self.memory_channel()
                && sample.range == self.memory_range()
                && sample.route_key == self.memory_route_fingerprint(&sample.channel)
            {
                return &sample.lines;
            }
            return &[];
        }
        if self.demo {
            &self.snapshot.memory
        } else {
            &[]
        }
    }
    pub(super) fn memory_caption(&self) -> String {
        let range = self.memory_range();
        let route = self.memory_access_description(&crate::config::RefreshPolicy {
            channel: self.memory_channel(),
            interval_ms: 0,
        });
        let status = if self.memory_panel.busy() {
            "reading".into()
        } else if let Some(sample) = &self.memory_panel.sample {
            let fresh = sample.key == self.memory_key()
                && sample.context == self.register_context()
                && sample.channel == self.memory_channel()
                && sample.range == self.memory_range()
                && sample.state == self.snapshot.state
                && sample.epoch == self.snapshot.memory_selection_epoch
                && sample.route_key == self.memory_route_fingerprint(&sample.channel)
                && self.memory_panel.error.is_none()
                && (self.snapshot.state == state::STOPPED || !sample.channel.is_empty());
            format!(
                "{} · {} · {}s ago{}",
                if fresh { "sampled" } else { "retained / stale" },
                sample.receipt.caption(),
                sample.sampled.elapsed().as_secs(),
                self.memory_panel
                    .error
                    .as_ref()
                    .map(|error| format!(" · error: {error}"))
                    .unwrap_or_default()
            )
        } else if let Some(error) = &self.memory_panel.error {
            format!("error: {error}")
        } else {
            "not read".into()
        };
        format!(
            "{} · {} bytes · {route}\n{status}",
            range.address, range.count
        )
    }
}

pub(super) fn draw(f: &mut UiFrame, app: &mut App) {
    app.memory_panel.hits.clear();
    let Some(popup) = &app.memory_panel.popup else {
        return;
    };
    let rect = center(f.area(), 98, 17);
    theme::overlay(f, rect);
    let card = theme::card(" Memory access · Tab select · Esc cancel ", true);
    let inner = card.inner(rect);
    f.render_widget(card, rect);
    let labels = [
        format!("Address: {}", popup.range.address),
        format!("Byte count: {} (1..4096)", popup.count),
        format!(
            "Channel: {}",
            if popup.channel.is_empty() {
                "GDB"
            } else {
                &popup.channel
            }
        ),
        "Apply and read".into(),
        "Cancel".into(),
    ];
    let rows = usize::from(inner.height.min(5));
    let first = popup.field.saturating_sub(rows.saturating_sub(1));
    for (field, label) in labels.iter().enumerate().skip(first).take(rows) {
        let hit = Rect::new(inner.x, inner.y + (field - first) as u16, inner.width, 1);
        f.render_widget(
            Paragraph::new(label.clone()).style(theme::selected(field == popup.field)),
            hit,
        );
        app.memory_panel.hits.push((hit, field));
    }
    let details = format!(
        "{}\nSource: {}\nGDB accepts address expressions while stopped. Bus channels use literal addresses. Reads return a sample, not an atomic range. Ctrl+U clears a field.",
        app.memory_access_description(&crate::config::RefreshPolicy {
            channel: popup.channel.clone(),
            interval_ms: 0
        }),
        if popup.channel.is_empty() {
            "selected core GDB"
        } else {
            &app.project.memory_access_source
        }
    );
    if inner.height > rows as u16 {
        f.render_widget(
            Paragraph::new(details)
                .style(Style::default().fg(theme::MUTED))
                .wrap(Wrap { trim: false }),
            Rect::new(
                inner.x,
                inner.y + rows as u16,
                inner.width,
                inner.height - rows as u16,
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new(Project::default(), false);
        app.snapshot.state = state::STOPPED.into();
        app.snapshot.register_session = 91;
        app.project.debug.chip = "chip-a".into();
        app.select_pane(pane::MEMORY);
        app.view_rects[pane::MEMORY] = Rect::new(0, 0, 40, 8);
        app.project.memory_access.push(crate::config::MemoryAccess {
            id: "ap".into(),
            target: "soc.bus".into(),
            tcl_endpoint: "127.0.0.1:6666".into(),
            while_running: true,
            ..Default::default()
        });
        app.project.ui.memory.insert(
            app.memory_key(),
            MemoryRange {
                address: "0x100000008".into(),
                count: 4,
            },
        );
        app
    }

    fn result(app: &App, channel: &str) -> Value {
        let mut result = memory_access::fixture(app, channel, "0x100000008", 32, true, true);
        result["address"] = json!("0x100000008");
        result["bytes"] = json!([0, 127, 128, 255]);
        result
    }

    #[test]
    fn popup_is_offline_cancel_preserves_settings_and_controls_fit_small_windows() {
        let (engine, requests) = session::test_channel();
        let mut app = app();
        let before = serde_json::to_value(&app.project.ui).unwrap();
        app.command(Some(&engine), ":memory-access");
        assert!(!app.ensure_memory_dump(Some(&engine)));
        assert!(COMMANDS.contains(&"memory-access") && COMMANDS.contains(&"memory-refresh"));
        for (width, height) in [(45, 12), (80, 24), (120, 36)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            for field in 0..5 {
                app.memory_panel.popup.as_mut().unwrap().field = field;
                terminal
                    .draw(|frame| super::super::draw(frame, &mut app))
                    .unwrap();
                if height >= 24 {
                    assert!(
                        app.action_hits
                            .iter()
                            .any(|(_, command)| *command == "memory-access"),
                        "{width}x{height}, pane={}, side={}, hits={:?}",
                        app.pane,
                        app.side_pane,
                        app.action_hits
                    );
                    assert!(
                        app.action_hits
                            .iter()
                            .any(|(_, command)| *command == "memory-refresh")
                    );
                }
                assert!(
                    app.memory_panel
                        .hits
                        .iter()
                        .any(|(_, found)| *found == field)
                );
                assert!(
                    app.memory_panel
                        .hits
                        .iter()
                        .all(|(rect, _)| rect.right() <= width && rect.bottom() <= height)
                );
            }
        }
        app.memory_panel.popup.as_mut().unwrap().field = 0;
        app.memory_key_event(
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
            Some(&engine),
        );
        app.memory_paste("0x20000000\r\n");
        app.memory_key_event(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            Some(&engine),
        );
        assert_eq!(before, serde_json::to_value(&app.project.ui).unwrap());
        assert!(requests.try_recv().is_err());
    }

    #[test]
    fn channel_and_range_apply_sends_explicit_route_and_persists_chip_core_settings() {
        let (engine, requests) = session::test_channel();
        let mut app = app();
        app.command(Some(&engine), ":memory-access");
        let popup = app.memory_panel.popup.as_mut().unwrap();
        popup.field = 2;
        app.memory_key_event(
            KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
            Some(&engine),
        );
        app.memory_panel.popup.as_mut().unwrap().field = 3;
        app.memory_key_event(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            Some(&engine),
        );
        assert_eq!(requests.recv().unwrap().method, method::UI_PREFERENCES);
        let read = requests.recv().unwrap();
        assert_eq!(read.method, method::MEMORY_DUMP);
        assert_eq!(read.params["address"], "0x100000008");
        assert_eq!(read.params["count"], 4);
        assert_eq!(read.params["channel"], "ap");
        assert_eq!(read.params["context"]["session"], 91);
        assert!(
            app.project
                .ui
                .memory
                .contains_key("chip:chip-a|single|memory:range")
        );
        app.project.debug.chip = "chip-b".into();
        assert_eq!(app.memory_range(), MemoryRange::default());
        assert_eq!(app.memory_channel(), "");
        assert!(app.memory_response(read.id, &json!(null), Some("late")));
        assert!(app.memory_panel.error.is_none());
        assert!(app.pending_commands.is_empty());
    }

    #[test]
    fn stale_core_session_stop_frame_and_range_responses_are_discarded() {
        for mutation in 0..5 {
            let (engine, requests) = session::test_channel();
            let mut app = app();
            assert!(app.request_memory_dump(Some(&engine), true));
            let read = requests.recv().unwrap();
            let response = result(&app, "");
            match mutation {
                0 => app.snapshot.register_session += 1,
                1 => app.snapshot.generation += 1,
                2 => app.snapshot.frame.level += 1,
                3 => {
                    app.snapshot.core = Some(session::CoreStatus {
                        name: "core1".into(),
                        index: 1,
                        endpoint: "localhost:3334".into(),
                        state: state::STOPPED.into(),
                    })
                }
                _ => {
                    let mut range = app.memory_range();
                    range.address = "0x20000000".into();
                    app.project.ui.memory.insert(app.memory_key(), range);
                }
            }
            app.memory_response(read.id, &response, None);
            assert!(app.memory_panel.sample.is_none(), "mutation {mutation}");
            assert!(!app.memory_panel.busy());
            assert!(app.pending_commands.is_empty());
        }
    }

    #[test]
    fn reads_are_bounded_on_demand_and_running_gdb_results_are_never_applied() {
        let (engine, requests) = session::test_channel();
        let mut app = app();
        assert!(app.ensure_memory_dump(Some(&engine)));
        let read = requests.recv().unwrap();
        let response = result(&app, "");
        assert!(!app.ensure_memory_dump(Some(&engine)));
        app.memory_response(read.id, &response, None);
        assert_eq!(
            app.memory_bytes(),
            vec![
                (0x100000008, "0x00".into()),
                (0x100000009, "0x7f".into()),
                (0x10000000a, "0x80".into()),
                (0x10000000b, "0xff".into())
            ]
        );
        assert!(!app.ensure_memory_dump(Some(&engine)));
        assert!(requests.try_recv().is_err());
        app.snapshot.state = state::RUNNING.into();
        assert!(app.memory_caption().contains("stale"));
        assert!(!app.request_memory_dump(Some(&engine), true));
        assert!(requests.try_recv().is_err());
        app.snapshot.state = state::STOPPED.into();
        assert!(app.request_memory_dump(Some(&engine), true));
        let read = requests.recv().unwrap();
        app.snapshot.state = state::RUNNING.into();
        app.memory_response(read.id, &response, None);
        assert!(app.memory_panel.error.as_ref().unwrap().contains("expired"));
    }

    #[test]
    fn failure_does_not_retry_or_fallback_and_running_bus_reads_are_explicit() {
        let (engine, requests) = session::test_channel();
        let mut app = app();
        app.project.ui.refresh.insert(
            app.memory_key(),
            crate::config::RefreshPolicy {
                channel: "ap".into(),
                interval_ms: 0,
            },
        );
        app.snapshot.state = state::RUNNING.into();
        assert!(!app.ensure_memory_dump(Some(&engine)));
        assert!(app.request_memory_dump(Some(&engine), true));
        let read = requests.recv().unwrap();
        assert_eq!(read.params["channel"], "ap");
        app.memory_response(read.id, &Value::Null, Some("AP failure"));
        assert!(!app.ensure_memory_dump(Some(&engine)));
        assert!(requests.try_recv().is_err());
        assert!(app.memory_caption().contains("AP failure"));
        assert!(app.request_memory_dump(Some(&engine), true));
        let read = requests.recv().unwrap();
        app.memory_response(read.id, &result(&app, "ap"), None);
        assert!(app.memory_caption().contains("soc.bus @ 127.0.0.1:6666"));
    }

    #[test]
    fn changing_core_closes_an_old_draft_and_does_not_display_another_cores_error() {
        let (engine, requests) = session::test_channel();
        let mut app = app();
        app.request_memory_dump(Some(&engine), true);
        let read = requests.recv().unwrap();
        app.memory_response(read.id, &Value::Null, Some("core0 read failed"));
        assert!(app.memory_caption().contains("core0 read failed"));
        app.open_memory_access();
        let mut next = app.snapshot.clone();
        next.core = Some(session::CoreStatus {
            name: "core1".into(),
            index: 1,
            endpoint: "localhost:3334".into(),
            state: state::RUNNING.into(),
        });
        next.state = state::RUNNING.into();
        app.update(Event::Snapshot {
            snapshot: Box::new(next),
        });
        assert!(!app.memory_panel.modal());
        assert!(!app.memory_caption().contains("core0 read failed"));
        assert!(requests.try_recv().is_err());
    }
    #[test]
    fn memory_rejects_incomplete_or_wrong_receipts_and_retains_the_observed_origin() {
        let (engine, requests) = session::test_channel();
        let mut app = app();
        app.project.target.endpoint = "configured:3333".into();
        app.request_memory_dump(Some(&engine), true);
        let read = requests.recv().unwrap();
        app.memory_response(read.id, &result(&app, ""), None);
        let initial = app.memory_bytes();
        assert!(!initial.is_empty());
        let caption = app.memory_caption();
        assert!(
            caption.contains("@ unknown") && caption.contains("configured configured:3333"),
            "{caption}"
        );
        for scenario in [
            "no-access",
            "endpoint",
            "address",
            "route-address",
            "command",
            "time",
            "phase",
            "epoch",
            "width",
            "byte-order",
            "atomic",
            "state",
        ] {
            app.request_memory_dump(Some(&engine), true);
            let read = requests.recv().unwrap();
            let mut bad = result(&app, "");
            match scenario {
                "no-access" => {
                    bad.as_object_mut().unwrap().remove("access");
                }
                "endpoint" => bad["access"]["route"]["endpoint"] = json!("other:3333"),
                "address" => bad["address"] = json!("0x10000000c"),
                "route-address" => bad["access"]["route"]["address"] = json!("0x10000000c"),
                "command" => {
                    bad["access"]["command"] = json!("-data-read-memory-bytes 0x10000000c 4")
                }
                "time" => bad["access"]["completed_ms"] = json!(0),
                "phase" => bad["access"]["phase"] = json!("started"),
                "epoch" => bad["selection_epoch"] = json!(app.snapshot.memory_selection_epoch + 1),
                "width" => bad["access"]["route"]["bits"] = json!(64),
                "byte-order" => bad["access"]["route"]["byte_order"] = json!("big"),
                "atomic" => bad["atomic"] = json!(true),
                "state" => bad["state"] = json!(state::RUNNING),
                _ => unreachable!(),
            }
            assert!(app.memory_response(read.id, &bad, None));
            assert_eq!(app.memory_bytes(), initial, "{scenario}");
            let caption = app.memory_caption();
            assert!(
                caption.contains("retained / stale")
                    && caption.contains("@ unknown")
                    && caption.contains("error:"),
                "{scenario}: {caption}"
            );
            assert!(!app.ensure_memory_dump(Some(&engine)));
            assert!(requests.try_recv().is_err());
        }
        let mut next = app.snapshot.clone();
        next.memory_selection_epoch += 1;
        next.memory = vec!["100000008 ff ff ff ff".into()];
        app.update(Event::Snapshot {
            snapshot: Box::new(next),
        });
        assert!(app.memory_bytes().is_empty());
        assert!(!app.memory_caption().contains("retained"));
    }
    #[test]
    fn memory_legacy_ranges_and_routes_are_overridden_by_chip_core_settings_and_endpoint_changes() {
        let (engine, requests) = session::test_channel();
        let mut app = app();
        app.project.ui.memory.clear();
        let range = MemoryRange {
            address: "0x100000008".into(),
            count: 4,
        };
        app.project
            .ui
            .memory
            .insert("memory:range".into(), range.clone());
        app.project.ui.refresh.insert(
            "memory:range".into(),
            crate::config::RefreshPolicy {
                channel: "ap".into(),
                interval_ms: 0,
            },
        );
        assert_eq!(app.memory_range(), range);
        assert_eq!(app.memory_channel(), "ap");
        app.project
            .ui
            .refresh
            .insert(app.memory_key(), Default::default());
        assert_eq!(app.memory_channel(), "");
        app.project.ui.refresh.remove(&app.memory_key());
        app.request_memory_dump(Some(&engine), true);
        let read = requests.recv().unwrap();
        app.memory_response(read.id, &result(&app, "ap"), None);
        assert_eq!(app.memory_bytes().len(), 4);
        app.request_memory_dump(Some(&engine), true);
        let read = requests.recv().unwrap();
        let old = result(&app, "ap");
        app.project.memory_access[0].tcl_endpoint = "other:6666".into();
        app.memory_response(read.id, &old, None);
        assert!(app.memory_bytes().is_empty());
        assert!(app.memory_panel.error.is_none());
        assert!(app.ensure_memory_dump(Some(&engine)));
        let next = requests.recv().unwrap();
        assert_eq!(next.params["channel"], "ap");
    }
}
