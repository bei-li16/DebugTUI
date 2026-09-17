//! Bounded, event-driven decoration. This module never talks to GDB.
use super::*;
use crate::config::Motion;
use std::collections::HashMap;

struct Pulse {
    at: Instant,
    ms: u64,
}
struct Seen {
    raw: String,
    previous: Option<String>,
}
pub(super) struct Task {
    pub id: u64,
    pub name: String,
    pub at: Instant,
    pub end: Option<Instant>,
    pub ok: Option<bool>,
    pub progress: Option<u64>,
}
pub(super) struct Effects {
    pulses: HashMap<String, Pulse>,
    values: HashMap<String, Seen>,
    requests: HashMap<u64, String>,
    pub task: Option<Task>,
    pub connection: Option<usize>,
    pub focused: bool,
    pub traces: VecDeque<(String, u32, Instant)>,
    pub last_action: String,
    pub tab_file: String,
    pub tab_out: bool,
    pub hover: Option<(Rect, Instant)>,
    pub pressed: Option<Rect>,
    pub phase: u8,
    last_tick: Instant,
    pub mode: Motion,
}
impl Default for Effects {
    fn default() -> Self {
        Self {
            pulses: HashMap::new(),
            values: HashMap::new(),
            requests: HashMap::new(),
            task: None,
            connection: None,
            focused: true,
            traces: VecDeque::new(),
            last_action: String::new(),
            tab_file: String::new(),
            tab_out: false,
            hover: None,
            pressed: None,
            phase: 0,
            last_tick: Instant::now(),
            mode: Motion::Subtle,
        }
    }
}
impl Effects {
    #[cfg(test)]
    pub(super) fn age_for_preview(&mut self, ms: u64) {
        for pulse in self.pulses.values_mut() {
            pulse.at = Instant::now() - Duration::from_millis(ms);
        }
        for (_, _, at) in &mut self.traces {
            *at = Instant::now() - Duration::from_millis(ms);
        }
    }
    pub fn trigger(&mut self, key: impl Into<String>, ms: u64) {
        if !self.focused || self.mode == Motion::Off {
            return;
        }
        if self.pulses.len() >= 256 {
            self.pulses.clear();
        }
        self.pulses.insert(
            key.into(),
            Pulse {
                at: Instant::now(),
                ms,
            },
        );
    }
    pub fn amount(&self, key: &str) -> f64 {
        if !self.focused || self.mode == Motion::Off {
            return 0.0;
        }
        self.pulses.get(key).map_or(0.0, |p| {
            (1.0 - p.at.elapsed().as_secs_f64() * 1000.0 / p.ms as f64).clamp(0.0, 1.0)
        })
    }
    pub fn focus(&mut self, focused: bool) {
        self.focused = focused;
        if !focused {
            self.pulses.clear();
            self.traces.clear();
            self.hover = None;
            self.pressed = None;
        }
    }
    pub fn tick(&mut self, mode: Motion, active: bool) -> bool {
        self.mode = mode;
        if !self.focused {
            return false;
        }
        if mode == Motion::Off {
            let dirty = !self.pulses.is_empty();
            self.pulses.clear();
            self.traces.clear();
            return dirty;
        }
        let interval = if self.pulses.is_empty() { 400 } else { 40 };
        if self.last_tick.elapsed() < Duration::from_millis(interval) {
            return false;
        }
        if self.pulses.is_empty() && !active {
            return false;
        }
        self.pulses
            .retain(|_, p| p.at.elapsed() < Duration::from_millis(p.ms));
        self.traces
            .retain(|(_, _, at)| at.elapsed() < Duration::from_millis(550));
        self.last_tick = Instant::now();
        self.phase = self.phase.wrapping_add(1);
        true
    }
    pub fn observe(&mut self, key: &str, raw: &str) -> Option<String> {
        if self.values.len() >= 2048 && !self.values.contains_key(key) {
            self.values.clear();
        }
        let changed = self.values.get(key).is_some_and(|v| v.raw != raw);
        if changed {
            let old = self.values.get_mut(key).unwrap();
            old.previous = Some(std::mem::replace(&mut old.raw, raw.into()));
            self.trigger(format!("value:{key}"), 750);
        } else if !self.values.contains_key(key) {
            self.values.insert(
                key.into(),
                Seen {
                    raw: raw.into(),
                    previous: None,
                },
            );
        }
        self.values.get(key).and_then(|v| v.previous.clone())
    }
    pub fn request(&mut self, id: u64, method: &str) {
        if self.requests.len() >= 256 {
            self.requests.clear();
        }
        self.requests.insert(id, method.into());
        if matches!(
            method,
            "step" | "next" | "finish" | "stepi" | "continue" | "run"
        ) {
            self.last_action = method.into();
        }
        if matches!(method, "connect" | "reconnect") {
            self.connection = Some(0);
        }
        if matches!(method, "build" | "download") {
            self.task = Some(Task {
                id,
                name: method.into(),
                at: Instant::now(),
                end: None,
                ok: None,
                progress: None,
            });
        }
        if !matches!(
            method,
            "peripheral_read" | "disassemble" | "files" | "complete"
        ) {
            self.trigger("submit", 220);
        }
    }
    pub fn response(&mut self, id: u64, ok: bool) {
        let method = self.requests.remove(&id).unwrap_or_default();
        if matches!(method.as_str(), "connect" | "reconnect") {
            self.connection = if ok { Some(3) } else { None };
            self.trigger(if ok { "connected" } else { "error" }, 1000);
        }
        if let Some(t) = &mut self.task
            && t.id == id
        {
            t.ok = Some(ok);
            t.end = Some(Instant::now());
            self.trigger("task-result", 900);
        }
        if method == "watch" && ok {
            self.trigger("watch-added", 800);
        }
        if !ok {
            self.trigger("error", 500);
        }
    }
    pub fn snapshot(&mut self, old: &Snapshot, new: &Snapshot) {
        if self.connection.is_some() {
            match new.state.as_str() {
                "STARTING SERVER" => self.connection = Some(0),
                "STARTING GDB" => self.connection = Some(1),
                "CONNECTING" => self.connection = Some(2),
                _ => {}
            }
        }
        if new.state == "FAULT" && old.state != "FAULT" {
            self.trigger("error", 600);
            self.connection = None;
        }
        if new.state == "STOPPED" && (old.state != "STOPPED" || old.generation != new.generation) {
            if matches!(
                self.last_action.as_str(),
                "step" | "next" | "finish" | "stepi"
            ) && !old.frame.file.is_empty()
            {
                if self.traces.len() >= 3 {
                    self.traces.pop_front();
                }
                self.traces
                    .push_back((old.frame.file.clone(), old.frame.line, Instant::now()));
            }
            self.trigger("stop", 550);
            if new.stop_reason.contains("breakpoint") {
                self.trigger("break", 320);
            }
            self.tab_file = new.frame.file.clone();
            self.tab_out = self.last_action == "finish";
            self.trigger("tab", 650);
        }
    }
    pub fn glyph(&self, unicode: bool, state: &str, busy: bool) -> &'static str {
        if state == "FAULT" {
            return if unicode { "⊗" } else { "!" };
        }
        if state == "RUNNING" {
            return if unicode {
                ["●", "◉", "●", "○"][self.phase as usize % 4]
            } else {
                "*"
            };
        }
        if busy {
            return if unicode {
                ["◐", "◓", "◑", "◒"][self.phase as usize % 4]
            } else {
                ["|", "/", "-", "\\"][self.phase as usize % 4]
            };
        }
        if state.contains("STOPPED") {
            if unicode { "●" } else { "*" }
        } else if unicode {
            "○"
        } else {
            "o"
        }
    }
    pub fn task_label(&self, state: &str) -> Option<(String, Color)> {
        let t = self.task.as_ref()?;
        let elapsed = t
            .end
            .unwrap_or_else(Instant::now)
            .duration_since(t.at)
            .as_secs_f64();
        let (stage, color) = match t.ok {
            Some(true) => ("completed", theme::GREEN),
            Some(false) => ("failed · see Console", theme::RED),
            None => (state, theme::AMBER),
        };
        Some((
            format!(
                "{} · {stage} · {elapsed:.1}s{}",
                t.name,
                t.progress.map(|p| format!(" · {p}%")).unwrap_or_default()
            ),
            color,
        ))
    }
}
pub(super) fn mix(a: Color, b: Color, t: f64) -> Color {
    match (a, b) {
        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => {
            let n =
                |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t.clamp(0.0, 1.0)).round() as u8;
            Color::Rgb(n(ar, br), n(ag, bg), n(ab, bb))
        }
        _ => b,
    }
}
fn wash(f: &mut UiFrame, rect: Rect, color: Color, amount: f64) {
    if amount <= 0.0 {
        return;
    }
    let rect = rect.intersection(f.area());
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            let cell = &mut f.buffer_mut()[(x, y)];
            cell.bg = mix(cell.bg, color, amount);
        }
    }
}
/// Decorate already laid-out cells. Text, hit boxes, PC and numeric contents never move.
pub(super) fn paint(f: &mut UiFrame, a: &mut App) {
    let task_pulse = a.fx.amount("task-result");
    if task_pulse > 0.0
        && let Some(task) = &a.fx.task
    {
        let y = if f.area().height >= 24 { 2 } else { 1 };
        wash(
            f,
            Rect::new(0, y, f.area().width, 1),
            if task.ok == Some(true) {
                theme::GREEN
            } else {
                theme::RED
            },
            task_pulse * 0.16,
        );
    }
    let running = a.snapshot.state == "RUNNING";
    if running {
        for pane in [1, 3, 4, 9, 10] {
            let r = a.view_rects[pane];
            for y in r.y..r.bottom() {
                for x in r.x..r.right() {
                    f.buffer_mut()[(x, y)].fg = theme::DIM;
                }
            }
        }
    }
    let source = a.source_rect;
    let current =
        a.main_pane == 0 && a.source_key(&a.source_file) == a.source_key(&a.snapshot.frame.file);
    if current {
        let line = a.snapshot.frame.line.saturating_sub(1) as usize;
        if line >= a.source_top && line < a.source_top + source.height as usize {
            let r = Rect::new(
                source.x,
                source.y + (line - a.source_top) as u16,
                source.width,
                1,
            );
            if running {
                for x in r.x..r.right() {
                    f.buffer_mut()[(x, r.y)].bg = theme::PANEL;
                }
                if r.width > 0 {
                    f.buffer_mut()[(r.x, r.y)].fg = theme::DIM;
                }
            } else {
                wash(f, r, theme::GREEN, a.fx.amount("stop") * 0.18);
                let p = a.fx.amount("break");
                if p > 0.0 {
                    if r.width > 1 {
                        f.buffer_mut()[(r.x + 1, r.y)].fg = mix(theme::RED, theme::TEXT, p);
                    }
                    if a.project.ui.animations == Motion::Full {
                        let edge = r.x + ((1.0 - p) * r.width as f64) as u16;
                        wash(
                            f,
                            Rect::new(edge, r.y, 3.min(r.right().saturating_sub(edge)), 1),
                            theme::AMBER,
                            p * 0.25,
                        );
                    }
                }
            }
        }
    }
    if a.main_pane == 0 && a.project.ui.animations == Motion::Full && a.fx.focused {
        for (file, line, at) in &a.fx.traces {
            let index = line.saturating_sub(1) as usize;
            if a.source_key(file) == a.source_key(&a.source_file)
                && index >= a.source_top
                && index < a.source_top + source.height as usize
            {
                let p = (1.0 - at.elapsed().as_secs_f64() / 0.55).clamp(0.0, 1.0);
                wash(
                    f,
                    Rect::new(
                        source.x,
                        source.y + (index - a.source_top) as u16,
                        source.width,
                        1,
                    ),
                    theme::ACCENT,
                    p * 0.09,
                );
            }
        }
    }
    for item in &a.formats.hits {
        wash(
            f,
            item.rect,
            theme::AMBER,
            a.fx.amount(&format!("value:{}", item.key)) * 0.15,
        );
    }
    for (hit, index) in &a.sources.tabs {
        if a.sources.documents[*index].file == a.fx.tab_file {
            wash(
                f,
                *hit,
                if a.fx.tab_out {
                    theme::GREEN
                } else {
                    theme::ACCENT
                },
                a.fx.amount("tab") * 0.24,
            );
        }
    }
    for pane in 0..11 {
        let r = a.scrollbars[pane];
        let p = a.fx.amount(&format!("scroll:{pane}"));
        if p > 0.0 {
            for y in r.y..r.bottom() {
                if r.width > 0 {
                    f.buffer_mut()[(r.x, y)].fg = mix(theme::DIM, theme::ACCENT, p);
                }
            }
        }
    }
    if let Some(rect) = a.fx.pressed {
        wash(f, rect, theme::TEXT, a.fx.amount("press") * 0.2);
    }
    if let Some((rect, at)) = a.fx.hover {
        wash(
            f,
            rect,
            theme::ACCENT,
            (at.elapsed().as_secs_f64() / 0.15).clamp(0.0, 1.0) * 0.10 * a.fx.amount("hover"),
        );
    }
    for (r, focus) in [
        (a.console_input_rect, a.editing),
        (a.watch_input_rect, a.watch_editing),
    ] {
        let p = a.fx.amount("input-focus");
        if focus {
            wash(f, r, theme::ACCENT, 0.08 * (1.0 - p));
        }
        wash(f, r, theme::RED, a.fx.amount("error") * 0.15);
        if r.width > 0 && r.height > 0 {
            f.buffer_mut()[(r.x, r.y)].fg = mix(theme::ACCENT, theme::TEXT, a.fx.amount("submit"));
        }
    }
    let connection = a.fx.amount("connected");
    if connection > 0.0 {
        wash(
            f,
            Rect::new(0, 0, f.area().width, 1),
            theme::GREEN,
            connection * 0.16,
        );
        let input = a.console_input_rect;
        if input.height > 0 && input.width > 0 {
            let x = input.x + ((1.0 - connection) * input.width.saturating_sub(1) as f64) as u16;
            f.buffer_mut()[(x, input.y)].fg = theme::GREEN;
        }
    }
    let p = a.fx.amount("watch-added");
    if p > 0.0
        && let Some(last) = a.formats.hits.iter().rev().find(|i| i.pane == 1)
    {
        wash(f, last.rect, theme::GREEN, p * 0.2);
    }
    // Tiny particles on empty cells only; bounded to three, away from cursor and input text.
    if a.project.ui.animations == Motion::Full && p.max(connection) > 0.0 {
        let r = if p > 0.0 {
            a.watch_input_rect
        } else {
            Rect::new(0, 0, f.area().width, 1)
        };
        if r.width > 12 && r.height > 0 {
            for i in 0..3 {
                let x = r.right() - 3 - i * 3;
                let c = &mut f.buffer_mut()[(x, r.y)];
                if c.symbol() == " " {
                    c.set_symbol(if a.project.ui.unicode {
                        ["·", "✦", "·"][(i + a.fx.phase as u16) as usize % 3]
                    } else {
                        "."
                    });
                    c.fg = mix(theme::DIM, theme::ACCENT, p.max(connection));
                }
            }
        }
    }
}
pub(super) fn completion(f: &mut UiFrame, a: &mut App) {
    let p = a.fx.amount("completion");
    let r = a.completion.area;
    if p > 0.0 {
        for y in r.y..r.bottom() {
            for x in r.x..r.right() {
                let c = &mut f.buffer_mut()[(x, y)];
                c.fg = mix(c.fg, theme::DIM, p * 0.45);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scheduler_sleeps_and_cancels_on_blur_or_off() {
        let mut fx = Effects {
            last_tick: Instant::now() - Duration::from_secs(2),
            ..Default::default()
        };
        assert!(!fx.tick(Motion::Subtle, false));
        fx.trigger("stop", 500);
        assert!(fx.tick(Motion::Subtle, false));
        fx.focus(false);
        assert_eq!(fx.amount("stop"), 0.0);
        assert!(!fx.tick(Motion::Full, true));
        fx.focus(true);
        fx.trigger("stop", 500);
        assert!(fx.tick(Motion::Off, false));
        assert!(fx.pulses.is_empty());
    }
    #[test]
    fn bounded_changes_and_real_stop_feedback() {
        let mut fx = Effects::default();
        assert_eq!(fx.observe("r0", "0xff"), None);
        assert_eq!(fx.observe("r0", "0xfe"), Some("0xff".into()));
        assert!(fx.amount("value:r0") > 0.0);
        fx.request(1, "pause");
        assert_eq!(fx.amount("stop"), 0.0);
        fx.response(1, false);
        assert_eq!(fx.amount("stop"), 0.0);
        assert!(fx.amount("error") > 0.0);
        for i in 0..5000 {
            fx.observe(&format!("k{i}"), "0");
            fx.trigger(format!("p{i}"), 500);
        }
        assert!(fx.values.len() <= 2048 && fx.pulses.len() <= 256);
    }
    #[test]
    fn connection_steps_traces_and_tasks_follow_actual_events() {
        let mut fx = Effects::default();
        fx.request(1, "connect");
        let mut old = Snapshot::default();
        for (state, stage) in [
            ("STARTING SERVER", 0),
            ("STARTING GDB", 1),
            ("CONNECTING", 2),
        ] {
            let new = Snapshot {
                state: state.into(),
                ..Default::default()
            };
            fx.snapshot(&old, &new);
            assert_eq!(fx.connection, Some(stage));
            old = new;
        }
        fx.response(1, true);
        assert_eq!(fx.connection, Some(3));
        old.frame.file = "sample.c".into();
        old.frame.line = 18;
        fx.request(2, "step");
        let new = Snapshot {
            state: "STOPPED".into(),
            generation: 1,
            frame: Frame {
                file: "led.c".into(),
                line: 7,
                ..Default::default()
            },
            ..Default::default()
        };
        fx.snapshot(&old, &new);
        assert_eq!(fx.traces.len(), 1);
        assert_eq!(fx.traces[0].1, 18);
        assert_eq!(fx.tab_file, "led.c");
        fx.request(3, "build");
        assert_eq!(fx.task.as_ref().unwrap().progress, None);
        fx.response(3, false);
        let (label, color) = fx.task_label("DISCONNECTED").unwrap();
        assert!(label.contains("failed"));
        assert_eq!(color, theme::RED);
        fx.age_for_preview(2000);
        fx.last_tick = Instant::now() - Duration::from_secs(1);
        assert!(fx.tick(Motion::Full, false));
        fx.last_tick = Instant::now() - Duration::from_secs(1);
        assert!(!fx.tick(Motion::Full, false));
    }
}
