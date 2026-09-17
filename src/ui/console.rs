use super::*;

pub(super) const HISTORY_LIMIT: usize = 2000;

pub(super) struct ConsoleView {
    pub rect: Rect,
    pub bar: Rect,
    pub latest: Rect,
    pub top: usize,
    pub follow: bool,
    pub focused: bool,
    pub unread: usize,
    drag: Option<u16>,
}
impl Default for ConsoleView {
    fn default() -> Self {
        Self {
            rect: Rect::default(),
            bar: Rect::default(),
            latest: Rect::default(),
            top: 0,
            follow: true,
            focused: false,
            unread: 0,
            drag: None,
        }
    }
}
impl ConsoleView {
    pub fn clear_hits(&mut self) {
        self.rect = Rect::default();
        self.bar = Rect::default();
        self.latest = Rect::default();
    }
    pub fn max(&self, len: usize) -> usize {
        len.saturating_sub(self.rect.height as usize)
    }
    pub fn layout(&mut self, area: Rect, len: usize) {
        self.rect = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
        self.bar = Rect::new(
            area.right().saturating_sub(1),
            area.y,
            area.width.min(1),
            area.height,
        );
        self.top = if self.follow {
            self.max(len)
        } else {
            self.top.min(self.max(len))
        };
    }
    pub fn appended(&mut self, evicted: bool) {
        if evicted {
            self.top = self.top.saturating_sub(1);
        }
        if !self.follow {
            self.unread = self.unread.saturating_add(1);
        }
    }
    fn seek(&mut self, top: usize, len: usize) {
        self.top = top.min(self.max(len));
        self.follow = self.top == self.max(len);
        if self.follow {
            self.unread = 0;
        }
    }
    pub fn thumb(&self, len: usize) -> (usize, usize, usize) {
        let height = self.bar.height as usize;
        if height == 0 {
            return (0, 0, 0);
        }
        let max = self.max(len);
        let size = (height * self.rect.height as usize / len.max(1)).clamp(1, height);
        let position = self
            .top
            .min(max)
            .saturating_mul(height - size)
            .checked_div(max)
            .unwrap_or(0);
        (position, size, max)
    }
    fn drag_to(&mut self, row: u16, offset: u16, len: usize) {
        let (_, size, max) = self.thumb(len);
        let travel = (self.bar.height as usize).saturating_sub(size);
        let pos = row.saturating_sub(self.bar.y).saturating_sub(offset) as usize;
        self.seek(
            pos.min(travel)
                .saturating_mul(max)
                .checked_div(travel)
                .unwrap_or(0),
            len,
        );
    }
}

impl App {
    fn focus_console_history(&mut self) {
        self.console_view.focused = true;
        self.editing = false;
        self.watch_editing = false;
        self.completion.invalidate();
    }
    pub(super) fn console_mouse(&mut self, mouse: MouseEvent) -> bool {
        let point = (mouse.column, mouse.row).into();
        let len = self.console.len();
        if let Some(offset) = self.console_view.drag {
            match mouse.kind {
                MouseEventKind::Drag(event::MouseButton::Left) => {
                    self.console_view.drag_to(mouse.row, offset, len);
                    return true;
                }
                MouseEventKind::Up(event::MouseButton::Left) => {
                    self.console_view.drag = None;
                    return true;
                }
                _ => {}
            }
        }
        if mouse.kind == MouseEventKind::Down(event::MouseButton::Left)
            && self.console_view.latest.contains(point)
        {
            self.focus_console_history();
            self.console_view.seek(usize::MAX, len);
            return true;
        }
        if !self.console_view.rect.contains(point) && !self.console_view.bar.contains(point) {
            return false;
        }
        match mouse.kind {
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                self.focus_console_history();
                let delta = if mouse.kind == MouseEventKind::ScrollDown {
                    3
                } else {
                    -3
                };
                self.console_view
                    .seek(self.console_view.top.saturating_add_signed(delta), len);
            }
            MouseEventKind::Down(event::MouseButton::Left) => {
                self.focus_console_history();
                if self.console_view.bar.contains(point) {
                    let (top, size, _) = self.console_view.thumb(len);
                    let click = mouse.row.saturating_sub(self.console_view.bar.y) as usize;
                    let offset = if (top..top + size).contains(&click) {
                        click - top
                    } else {
                        size / 2
                    } as u16;
                    self.console_view.drag = Some(offset);
                    self.console_view.drag_to(mouse.row, offset, len);
                }
            }
            _ => {}
        }
        true
    }
    pub(super) fn console_key(&mut self, key: KeyEvent) -> bool {
        // Shift+PageUp enters history from the editor without altering its draft/history.
        if key.code == KeyCode::PageUp && key.modifiers == KeyModifiers::SHIFT {
            self.focus_console_history();
        }
        if !self.console_view.focused
            || self.input_active()
            || key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return false;
        }
        let len = self.console.len();
        let page = self.console_view.rect.height.saturating_sub(1).max(1) as usize;
        let top = match key.code {
            KeyCode::Up => self.console_view.top.saturating_sub(1),
            KeyCode::Down => self.console_view.top.saturating_add(1),
            KeyCode::PageUp => self.console_view.top.saturating_sub(page),
            KeyCode::PageDown => self.console_view.top.saturating_add(page),
            KeyCode::Home => 0,
            KeyCode::End => usize::MAX,
            KeyCode::Esc => {
                self.console_view.focused = false;
                return true;
            }
            KeyCode::Enter => {
                self.focus_input(false);
                return true;
            }
            _ => return false,
        };
        self.console_view.seek(top, len);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn draw(a: &mut App, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| super::super::draw(f, a)).unwrap();
        t.backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }
    fn app() -> App {
        let mut a = App::new(Project::default(), true);
        a.fx.mode = crate::config::Motion::Off;
        a.console = (0..300).map(|i| format!("[gdb] record-{i:04}")).collect();
        draw(&mut a, 120, 36);
        a
    }
    fn mouse(a: &mut App, kind: MouseEventKind, x: u16, y: u16, engine: &EngineHandle) {
        a.mouse(
            MouseEvent {
                kind,
                column: x,
                row: y,
                modifiers: KeyModifiers::NONE,
            },
            Some(engine),
        );
    }
    #[test]
    fn mouse_scroll_drag_history_latest_and_input_are_independent() {
        let (engine, requests) = session::test_channel();
        let mut a = app();
        assert!(draw(&mut a, 120, 36).contains("record-0299"));
        let bar = a.console_view.bar;
        let input = a.console_input_rect;
        assert!(bar.height > 1 && bar.right() == 120 && bar.bottom() <= input.y);
        let top = a.console_view.top;
        mouse(&mut a, MouseEventKind::ScrollUp, bar.x, bar.y, &engine);
        assert_eq!(a.console_view.top, top - 3);
        assert!(!a.console_view.follow);
        mouse(
            &mut a,
            MouseEventKind::Down(event::MouseButton::Left),
            bar.x,
            bar.y,
            &engine,
        );
        assert_eq!(a.console_view.top, 0);
        assert!(draw(&mut a, 120, 36).contains("record-0000"));
        a.log("[gdb] NEW OUTPUT".into());
        let text = draw(&mut a, 120, 36);
        assert!(
            text.contains("record-0000")
                && !text.contains("NEW OUTPUT")
                && text.contains("Latest (+1)")
        );
        mouse(
            &mut a,
            MouseEventKind::Drag(event::MouseButton::Left),
            0,
            100,
            &engine,
        );
        mouse(
            &mut a,
            MouseEventKind::Up(event::MouseButton::Left),
            0,
            100,
            &engine,
        );
        assert!(a.console_view.follow && draw(&mut a, 120, 36).contains("NEW OUTPUT"));
        a.key(
            KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
            Some(&engine),
        );
        draw(&mut a, 120, 36);
        let latest = a.console_view.latest;
        mouse(
            &mut a,
            MouseEventKind::Down(event::MouseButton::Left),
            latest.x,
            latest.y,
            &engine,
        );
        assert!(a.console_view.follow);
        assert!(requests.try_recv().is_err());
        mouse(
            &mut a,
            MouseEventKind::Down(event::MouseButton::Left),
            input.x,
            input.y,
            &engine,
        );
        assert!(a.editing && !a.console_view.focused);
        a.demo = false;
        a.input = "print counter".into();
        a.key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            Some(&engine),
        );
        let request = requests.try_recv().unwrap();
        assert_eq!(request.method, "console");
        assert_eq!(request.params["command"], "print counter");
    }
    #[test]
    fn keyboard_eviction_resize_and_modal_preserve_history_anchor() {
        let (engine, requests) = session::test_channel();
        let mut a = app();
        a.console = (0..HISTORY_LIMIT)
            .map(|i| format!("[gdb] record-{i:04}"))
            .collect();
        draw(&mut a, 120, 36);
        a.focus_input(false);
        a.input = "unsent draft".into();
        a.key(
            KeyEvent::new(KeyCode::PageUp, KeyModifiers::SHIFT),
            Some(&engine),
        );
        assert!(a.console_view.focused && !a.console_view.follow && !a.editing);
        a.key(
            KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
            Some(&engine),
        );
        for _ in 0..20 {
            a.key(
                KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
                Some(&engine),
            );
        }
        let anchor = a.console[a.console_view.top].clone();
        a.log("[gdb] new".into());
        draw(&mut a, 120, 36);
        assert_eq!(a.console.len(), HISTORY_LIMIT);
        assert_eq!(a.console[a.console_view.top], anchor);
        a.palette = true;
        let before = a.console_view.top;
        let r = a.console_view.rect;
        mouse(&mut a, MouseEventKind::ScrollDown, r.x, r.y, &engine);
        assert_eq!(a.console_view.top, before);
        a.palette = false;
        for (w, h) in [(80, 24), (45, 12), (180, 50), (120, 36)] {
            draw(&mut a, w, h);
            assert!(a.console_view.bar.right() <= w && a.console_input_rect.bottom() <= h);
            assert_eq!(a.console[a.console_view.top], anchor);
        }
        a.key(
            KeyEvent::new(KeyCode::End, KeyModifiers::NONE),
            Some(&engine),
        );
        a.log("[gdb] latest record".into());
        assert!(draw(&mut a, 120, 36).contains("latest record") && a.console_view.follow);
        a.key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            Some(&engine),
        );
        assert!(a.editing && a.input == "unsent draft");
        assert!(requests.try_recv().is_err());
    }
    #[test]
    fn backend_capture_time_is_kept_and_channel_filter_still_works() {
        let mut a = app();
        a.console.clear();
        a.update(Event::Log {
            channel: "gdb".into(),
            text: "line one\nline two".into(),
            timestamp: "2026-09-17 10:20:30.456".into(),
            elapsed_ms: 1234,
        });
        assert_eq!(a.console.len(), 2);
        assert!(
            a.console
                .iter()
                .all(|s| s.starts_with("[10:20:30.456] [gdb]"))
        );
        a.update(Event::Log {
            channel: "server".into(),
            text: "server only".into(),
            timestamp: "2026-09-17 10:20:30.457".into(),
            elapsed_ms: 1235,
        });
        assert_eq!(a.console.len(), 2);
        assert!(a.logs.back().unwrap().contains("[server] server only"));
        assert!(draw(&mut a, 120, 36).contains("10:20:30.456"));
    }
    #[test]
    fn export_console_history_previews_when_requested() {
        let Ok(root) = std::env::var("DEBUGTUI_RENDER_DIR") else {
            return;
        };
        let root = Path::new(&root);
        fs::create_dir_all(root).unwrap();
        let mut a = app();
        a.console.clear();
        for i in 0..80 {
            a.log_at(
                format!("[gdb] ${i} = {}", i * 17),
                &format!("2026-09-17 17:10:{:02}.123", i % 60),
            );
        }
        visual_tests::capture(root, "console-live", &mut a, 160, 42);
        a.focus_console_history();
        a.console_view.seek(10, a.console.len());
        a.log("[gdb] Output received while browsing history".into());
        visual_tests::capture(root, "console-history", &mut a, 160, 42);
        visual_tests::capture(root, "console-history-narrow", &mut a, 80, 24);
    }
}
