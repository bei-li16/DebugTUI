use super::*;
use crate::svd::Device;
use std::collections::{HashMap, HashSet};

pub(super) const PANE: usize = 10;
type RegisterKey = (usize, usize);
#[derive(Clone, Copy)]
enum Row {
    Peripheral(usize),
    Register(usize, usize),
    Field(usize, usize, usize),
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> App {
        let mut a = App::new(Project::default(), false);
        a.snapshot.state = "STOPPED".into();
        a.peripherals.device =
            Some(Device::parse(include_str!("../../tests/fixtures/peripherals.svd")).unwrap());
        a.peripherals.rebuild();
        a.select_pane(PANE);
        a.view_rects[PANE] = Rect::new(0, 0, 70, 5);
        a.side_rect = a.view_rects[PANE];
        a
    }
    fn response(a: &mut App, request: &Request, value: Option<u64>) {
        a.update(Event::Response {
            id: request.id,
            ok: value.is_some(),
            result: json!({"value":value}),
            error: value.is_none().then(|| "Unreadable memory".into()),
        });
    }
    #[test]
    fn register_and_field_formats_are_independent_and_changes_are_per_field() {
        let mut a = app();
        a.toggle_peripheral(None);
        a.selection = 1;
        a.toggle_peripheral(None);
        a.peripherals.values.insert(
            (0, 0),
            Reading {
                value: Some(43),
                previous: Some(42),
                changed: true,
                stamp: a.view_stamp(),
                ..Default::default()
            },
        );
        let mut t = Terminal::new(TestBackend::new(160, 42)).unwrap();
        t.draw(|f| draw(f, &mut a)).unwrap();
        let field = a
            .formats
            .hits
            .iter()
            .find(|i| i.pane == PANE && i.row == 2)
            .unwrap()
            .clone();
        let register = a
            .formats
            .hits
            .iter()
            .find(|i| i.pane == PANE && i.row == 1)
            .unwrap()
            .clone();
        a.open_format(Some(field.clone()));
        a.formats.index = 0;
        a.apply_format(None);
        assert_eq!(
            a.base_for(&field.key, crate::config::Radix::Hex),
            crate::config::Radix::Binary
        );
        assert_eq!(
            a.base_for(&register.key, crate::config::Radix::Hex),
            crate::config::Radix::Hex
        );
        t.draw(|f| draw(f, &mut a)).unwrap();
        let text = t
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(text.contains("0b") && text.contains("0x2b"));
        let reg = &a.peripherals.device.as_ref().unwrap().peripherals[0].registers[0];
        assert!(reg.fields.iter().any(|f| f.value(43) != f.value(42)));
    }
    #[test]
    fn collapsed_hidden_and_running_views_never_read_and_manual_refresh_is_one_register() {
        let mut a = app();
        let (engine, rx) = session::test_channel();
        assert!(!a.ensure_visible_data(Some(&engine))); // all collapsed
        a.toggle_peripheral(None);
        assert!(a.ensure_visible_data(Some(&engine)));
        let req = rx.try_recv().unwrap();
        assert_eq!(req.method, "peripheral_read");
        assert_eq!(req.params["address"], 0x40000000u64);
        response(&mut a, &req, Some(42));
        // Other visible rows are write-only or have read side effects.
        assert!(!a.ensure_visible_data(Some(&engine)));
        a.snapshot.generation += 1;
        a.select_pane(3);
        assert!(!a.ensure_visible_data(Some(&engine)));
        a.select_pane(PANE);
        a.snapshot.state = "RUNNING".into();
        assert!(!a.ensure_visible_data(Some(&engine)));
        a.snapshot.state = "STOPPED".into();
        assert!(a.ensure_visible_data(Some(&engine)));
        let req = rx.try_recv().unwrap();
        response(&mut a, &req, Some(43));
        assert!(a.peripherals.values[&(0, 0)].changed);
        a.selection = 1;
        a.refresh_peripheral(Some(&engine));
        let req = rx.try_recv().unwrap();
        assert_eq!(req.params["address"], 0x40000000u64);
        response(&mut a, &req, None);
        assert!(!a.ensure_visible_data(Some(&engine))); // failed reads do not loop
        a.refresh_peripheral(Some(&engine));
        let req = rx.try_recv().unwrap();
        response(&mut a, &req, Some(45));
        assert!(a.peripherals.values[&(0, 0)].error.is_none());
        a.selection = 2; // write only: reject even explicit refresh
        a.refresh_peripheral(Some(&engine));
        assert!(rx.try_recv().is_err());
        a.selection = 3; // readAction: explicit refresh only
        a.refresh_peripheral(Some(&engine));
        let req = rx.try_recv().unwrap();
        assert_eq!(req.params["address"], 0x40000008u64);
        response(&mut a, &req, Some(0));
        a.selection = 0;
        a.toggle_peripheral(Some(false));
        a.snapshot.generation += 1;
        assert!(!a.ensure_visible_data(Some(&engine)));
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn peripheral_tree_fields_mouse_button_and_scrollbar_have_real_hit_regions() {
        let mut a = app();
        let (engine, rx) = session::test_channel();
        a.toggle_peripheral(None);
        a.selection = 1;
        a.key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            Some(&engine),
        );
        assert!(matches!(a.peripherals.rows[2], Row::Field(0, 0, 0)));
        for (w, h) in [(180, 44), (80, 24), (45, 12)] {
            let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
            t.draw(|f| draw(f, &mut a)).unwrap();
            assert!(a.scrollbars[PANE].height > 0);
            let button = a
                .action_hits
                .iter()
                .find(|(_, action)| *action == "peripheral-refresh")
                .unwrap()
                .0;
            a.mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(event::MouseButton::Left),
                    column: button.x,
                    row: button.y,
                    modifiers: KeyModifiers::NONE,
                },
                Some(&engine),
            );
            let req = rx.try_recv().unwrap();
            assert_eq!(req.method, "peripheral_read");
            response(&mut a, &req, Some(0x1234));
        }
        a.editing = true;
        a.key(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
            Some(&engine),
        );
        assert_eq!(a.input, "r");
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn render_svd_previews_when_requested() {
        let Ok(root) = std::env::var("DEBUGTUI_RENDER_DIR") else {
            return;
        };
        fs::create_dir_all(&root).unwrap();
        let mut a = App::new(Project::default(), true);
        a.peripherals = Peripherals::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/svd/stm32/STM32F429.svd"),
        );
        a.select_pane(PANE);
        let p = a
            .peripherals
            .device
            .as_ref()
            .unwrap()
            .peripherals
            .iter()
            .position(|p| p.name == "GPIOB")
            .unwrap();
        a.selection = p;
        a.toggle_peripheral(Some(true));
        a.view_tops[PANE] = p;
        a.selection = p + 1;
        a.toggle_peripheral(Some(true));
        a.peripherals.values.insert(
            (p, 0),
            Reading {
                stamp: a.view_stamp(),
                value: Some(0x280),
                ..Default::default()
            },
        );
        for (w, h) in [(180, 44), (80, 24), (45, 12)] {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|f| draw(f, &mut a)).unwrap();
            let cells:Vec<_>=terminal.backend().buffer().content.iter().map(|c|{
                let color=|color|match color {Color::Rgb(r,g,b)=>format!("#{r:02x}{g:02x}{b:02x}"),_=>"inherit".into()};
                json!({"s":c.symbol(),"fg":color(c.fg),"bg":color(c.bg),"bold":c.modifier.contains(Modifier::BOLD)})
            }).collect();
            fs::write(
                Path::new(&root).join(format!("svd-{w}.json")),
                serde_json::to_vec(&json!({"width":w,"height":h,"cells":cells})).unwrap(),
            )
            .unwrap();
        }
    }
}
#[derive(Default)]
struct Reading {
    stamp: String,
    value: Option<u64>,
    error: Option<String>,
    changed: bool,
    previous: Option<u64>,
}
#[derive(Default)]
pub(super) struct Peripherals {
    device: Option<Device>,
    error: Option<String>,
    open: HashSet<usize>,
    fields: HashSet<RegisterKey>,
    rows: Vec<Row>,
    values: HashMap<RegisterKey, Reading>,
    pending: Option<(u64, RegisterKey, String)>,
}
impl Peripherals {
    pub fn load(path: &Path) -> Self {
        let mut view = Self::default();
        if !path.as_os_str().is_empty() {
            match Device::load(path) {
                Ok(device) => view.device = Some(device),
                Err(error) => view.error = Some(error),
            }
        }
        view.rebuild();
        view
    }
    fn rebuild(&mut self) {
        self.rows.clear();
        if let Some(device) = &self.device {
            for (p, peripheral) in device.peripherals.iter().enumerate() {
                self.rows.push(Row::Peripheral(p));
                if !self.open.contains(&p) {
                    continue;
                }
                for (r, register) in peripheral.registers.iter().enumerate() {
                    self.rows.push(Row::Register(p, r));
                    if self.fields.contains(&(p, r)) {
                        for i in 0..register.fields.len() {
                            self.rows.push(Row::Field(p, r, i));
                        }
                    }
                }
            }
        }
    }
    pub fn len(&self) -> usize {
        self.rows.len()
    }
    pub fn invalidate(&mut self) {
        for value in self.values.values_mut() {
            value.stamp.clear();
        }
    }
    fn selected_register(&self, index: usize) -> Option<RegisterKey> {
        match self.rows.get(index)? {
            Row::Register(p, r) | Row::Field(p, r, _) => Some((*p, *r)),
            _ => None,
        }
    }
    pub fn response(&mut self, id: u64, result: &Value, error: Option<&str>) {
        if let Some((pending, key, stamp)) = &self.pending
            && *pending == id
        {
            let value = result.get("value").and_then(Value::as_u64);
            let old = self.values.get(key).and_then(|v| v.value);
            self.values.insert(
                *key,
                Reading {
                    stamp: stamp.clone(),
                    value,
                    error: error
                        .map(str::to_owned)
                        .or_else(|| value.is_none().then(|| "No register value returned".into())),
                    previous: old,
                    changed: old.zip(value).is_some_and(|(a, b)| a != b),
                },
            );
            self.pending = None;
        }
    }
}
impl App {
    pub(super) fn peripheral_format_item(&self, row: usize) -> Option<formats::Item> {
        let device = self.peripherals.device.as_ref()?;
        let (p, r, field) = match self.peripherals.rows.get(row)? {
            Row::Register(p, r) => (*p, *r, None),
            Row::Field(p, r, i) => (*p, *r, Some(*i)),
            _ => return None,
        };
        let peripheral = &device.peripherals[p];
        let register = &peripheral.registers[r];
        let mut name = format!("{}.{}", peripheral.name, register.name);
        let mut value = self.peripherals.values.get(&(p, r)).and_then(|v| v.value);
        if let Some(i) = field {
            let f = &register.fields[i];
            name.push('.');
            name.push_str(&f.name);
            value = value.map(|v| f.value(v));
        }
        Some(formats::Item {
            rect: Rect::default(),
            pane: PANE,
            row,
            key: format!("svd:{}:{name}", device.name),
            name,
            raw: value
                .map(|v| format!("0x{v:x}"))
                .unwrap_or_else(|| "—".into()),
            default: crate::config::Radix::Hex,
        })
    }
    pub(super) fn toggle_peripheral(&mut self, expand: Option<bool>) {
        let selected = self.selected(PANE);
        let Some(row) = self.peripherals.rows.get(selected).copied() else {
            return;
        };
        match row {
            Row::Peripheral(p) => {
                let open = expand.unwrap_or(!self.peripherals.open.contains(&p));
                if open {
                    self.peripherals.open.insert(p);
                } else {
                    self.peripherals.open.remove(&p);
                }
            }
            Row::Register(p, r) => {
                let open = expand.unwrap_or(!self.peripherals.fields.contains(&(p, r)));
                if open {
                    self.peripherals.fields.insert((p, r));
                } else {
                    self.peripherals.fields.remove(&(p, r));
                }
            }
            Row::Field(_, _, _) => return,
        }
        self.peripherals.rebuild();
        self.selection = selected.min(self.peripherals.len().saturating_sub(1));
    }
    pub(super) fn refresh_peripheral(&mut self, engine: Option<&EngineHandle>) {
        if self.snapshot.state != "STOPPED" || !self.pending_commands.is_empty() {
            self.notice =
                "Stop the target and wait for the current command before refreshing.".into();
            return;
        }
        let Some(key) = self.peripherals.selected_register(self.selected(PANE)) else {
            self.notice = "Select a peripheral register, then click Refresh or press r.".into();
            return;
        };
        self.read_peripheral(engine, key);
    }
    fn read_peripheral(&mut self, engine: Option<&EngineHandle>, key: RegisterKey) -> bool {
        if engine.is_none() || self.demo {
            return false;
        }
        let Some(device) = &self.peripherals.device else {
            return false;
        };
        let register = &device.peripherals[key.0].registers[key.1];
        if !register.readable_width() || device.little_endian.is_none() {
            self.notice = "Cannot read: write-only, unsupported width/alignment, or unspecified SVD byte order.".into();
            return false;
        }
        let params = json!({"address":register.address,"bits":register.bits,"little_endian":device.little_endian});
        self.peripherals.pending = Some((self.next_id, key, self.view_stamp()));
        self.pending_view = Some((self.next_id, PANE));
        self.submit(engine, "peripheral_read", params);
        true
    }
    pub(super) fn ensure_peripherals(&mut self, engine: Option<&EngineHandle>) -> bool {
        if self.side_pane != PANE || self.view_rects[PANE].height == 0 {
            return false;
        }
        let stamp = self.view_stamp();
        let Some(device) = &self.peripherals.device else {
            return false;
        };
        if device.little_endian.is_none() {
            return false;
        }
        let key = self
            .peripherals
            .rows
            .iter()
            .skip(self.view_tops[PANE])
            .take(self.view_rects[PANE].height as usize)
            .filter_map(|row| match row {
                Row::Register(p, r) | Row::Field(p, r, _) => Some((*p, *r)),
                _ => None,
            })
            .find(|&(p, r)| {
                device.peripherals[p].registers[r].auto_read()
                    && self
                        .peripherals
                        .values
                        .get(&(p, r))
                        .is_none_or(|value| value.stamp != stamp)
            });
        key.is_some_and(|key| self.read_peripheral(engine, key))
    }
    pub(super) fn draw_peripherals(&mut self, f: &mut UiFrame, rect: Rect) {
        if let Some(error) = &self.peripherals.error {
            theme::empty(f, rect, "SVD could not be loaded", error);
            return;
        }
        let Some(device) = &self.peripherals.device else {
            theme::empty(
                f,
                rect,
                "No SVD configured",
                "F2 Setup → SVD file selects a chip description.",
            );
            return;
        };
        let selected = self.selected(PANE);
        let stamp = self.view_stamp();
        let rows: Vec<_> = self
            .peripherals
            .rows
            .iter()
            .enumerate()
            .skip(self.view_tops[PANE])
            .take(rect.height as usize)
            .map(|(index, row)| {
                let (label, value, changed, bad, stale) = match *row {
                    Row::Peripheral(p) => {
                        let peripheral = &device.peripherals[p];
                        (
                            format!(
                                "{} {}",
                                if self.peripherals.open.contains(&p) {
                                    "▾"
                                } else {
                                    "▸"
                                },
                                peripheral.name
                            ),
                            format!("0x{:08X}", peripheral.address),
                            false,
                            false,
                            false,
                        )
                    }
                    Row::Register(p, r) => {
                        let register = &device.peripherals[p].registers[r];
                        let reading = self.peripherals.values.get(&(p, r));
                        let text = if let Some(error) = reading.and_then(|v| v.error.as_ref()) {
                            format!("! {error}")
                        } else if let Some(value) = reading.and_then(|v| v.value) {
                            format!("0x{value:0width$X}", width = register.bits as usize / 4)
                        } else if !register.readable {
                            "write-only".into()
                        } else if !register.readable_width() {
                            "unsupported width".into()
                        } else if device.little_endian.is_none() {
                            "unknown byte order".into()
                        } else if register.side_effect {
                            "manual · read side effect".into()
                        } else {
                            "—".into()
                        };
                        (
                            format!(
                                "  {} {}",
                                if register.fields.is_empty() {
                                    "·"
                                } else if self.peripherals.fields.contains(&(p, r)) {
                                    "▾"
                                } else {
                                    "▸"
                                },
                                register.name
                            ),
                            text,
                            reading.is_some_and(|v| v.changed),
                            reading.is_some_and(|v| v.error.is_some()),
                            reading.is_some_and(|v| v.stamp != stamp)
                                || self.snapshot.state != "STOPPED",
                        )
                    }
                    Row::Field(p, r, i) => {
                        let field = &device.peripherals[p].registers[r].fields[i];
                        let reading = self.peripherals.values.get(&(p, r));
                        let value = reading
                            .and_then(|v| v.value)
                            .map(|value| format!("0x{:X}", field.value(value)))
                            .unwrap_or_else(|| "—".into());
                        (
                            format!(
                                "      {} [{}:{}]",
                                field.name,
                                field.offset + field.width - 1,
                                field.offset
                            ),
                            value,
                            reading
                                .and_then(|v| v.previous.zip(v.value))
                                .is_some_and(|(old, new)| field.value(old) != field.value(new)),
                            false,
                            reading.is_some_and(|v| v.stamp != stamp)
                                || self.snapshot.state != "STOPPED",
                        )
                    }
                };
                let key = match *row {
                    Row::Peripheral(_) => None,
                    Row::Register(p, r) => Some(format!(
                        "svd:{}:{}.{}",
                        device.name,
                        device.peripherals[p].name,
                        device.peripherals[p].registers[r].name
                    )),
                    Row::Field(p, r, i) => Some(format!(
                        "svd:{}:{}.{}.{}",
                        device.name,
                        device.peripherals[p].name,
                        device.peripherals[p].registers[r].name,
                        device.peripherals[p].registers[r].fields[i].name
                    )),
                };
                (index, label, value, changed, bad, stale, key)
            })
            .collect();
        for (offset, (index, label, value, changed, bad, stale, key)) in
            rows.into_iter().enumerate()
        {
            let hit = Rect::new(rect.x, rect.y + offset as u16, rect.width, 1);
            let mut spans = vec![Span::styled(
                format!("{label}  "),
                Style::default().fg(if index == selected {
                    theme::ACCENT
                } else {
                    theme::MUTED
                }),
            )];
            if let Some(key) = key {
                let item = formats::Item {
                    rect: hit,
                    pane: PANE,
                    row: index,
                    key,
                    name: label.trim().into(),
                    raw: value,
                    default: crate::config::Radix::Hex,
                };
                spans.extend(self.numeric_spans(&item, changed, bad));
                self.formats.hits.push(item);
            } else {
                spans.push(Span::styled(value, Style::default().fg(theme::MUTED)));
            }
            if stale {
                spans.push(Span::styled("  cached", Style::default().fg(theme::DIM)));
            }
            let line = Line::from(spans).style(Style::default().bg(if index == selected {
                theme::SELECTED
            } else {
                theme::PANEL
            }));
            theme::lines(f, vec![line], hit);
        }
    }
    pub(super) fn peripheral_title(&self) -> String {
        match &self.peripherals.device {
            Some(device) => format!(" {} · Enter expand · r refresh ", device.name),
            None => " Peripheral registers · SVD ".into(),
        }
    }
}
