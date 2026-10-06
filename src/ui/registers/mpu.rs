//! Explicit MPU overview. Opening, scrolling and switching banks only display cached evidence.
use super::*;
use crate::registers::mpu::{Bank, View};

pub(super) struct Popup {
    bank: Bank,
    m_profile: bool,
    cache: bool,
    count: Option<(Context, u8)>,
    scroll: usize,
    max_scroll: usize,
    button: usize,
    hits: Vec<(Rect, usize)>,
    pending: Option<u64>,
    error: Option<String>,
}
impl App {
    pub(in crate::ui) fn open_mpu_view(&mut self, arg: &str) {
        let m_profile = self
            .register_view
            .catalogue
            .as_ref()
            .is_some_and(|c| crate::registers::m_profile::adapted_cpu(&c.cpu));
        if self
            .register_view
            .catalogue
            .as_ref()
            .is_none_or(|c| !m_profile && c.architecture != "armv8-r-aarch32")
        {
            self.notice = "MPU overview requires an adapted M3/M4/M7 or R52 catalogue.".into();
            return;
        }
        let bank = match arg.trim() {
            "" | "m" if m_profile => Bank::El1,
            _ if m_profile => {
                self.notice = "Cortex-M uses :mpu or :mpu m (no EL1/EL2 banks)".into();
                return;
            }
            "el1" => Bank::El1,
            "el2" => Bank::El2,
            "" => {
                if self
                    .register_view
                    .register_index(self.selected(3))
                    .is_some_and(|i| {
                        self.register_view.catalogue.as_ref().unwrap().registers[i].group
                            == "mpu_el2"
                    })
                {
                    Bank::El2
                } else {
                    Bank::El1
                }
            }
            _ => {
                self.notice = "Use :mpu, :mpu el1 or :mpu el2".into();
                return;
            }
        };
        self.register_view.mpu_popup = Some(Popup {
            bank,
            m_profile,
            cache: false,
            count: None,
            scroll: 0,
            max_scroll: 0,
            button: 0,
            hits: vec![],
            pending: None,
            error: None,
        });
    }
    pub(in crate::ui) fn open_cache_view(&mut self, arg: &str) {
        if !arg.trim().is_empty()
            || self
                .register_view
                .catalogue
                .as_ref()
                .is_none_or(|c| c.cpu != "cortex-m7")
        {
            self.notice = "Use :cache with the Cortex-M7 catalogue".into();
            return;
        }
        self.open_mpu_view("");
        self.register_view.mpu_popup.as_mut().unwrap().cache = true;
    }
    fn request_mpu_regions(&mut self, engine: Option<&EngineHandle>) {
        if self.demo
            || engine.is_none()
            || self.snapshot.state != "STOPPED"
            || self.register_view.pending.is_some()
            || self.register_view.probe_pending.is_some()
        {
            return;
        }
        let Some(popup) = self.register_view.mpu_popup.as_ref() else {
            return;
        };
        let bank = popup.bank;
        let m_profile = popup.m_profile;
        let cache = popup.cache;
        let context = self.register_context();
        if context.frame != 0
            || self.snapshot.register_probe.as_ref().is_none_or(|p| {
                p.context != context
                    || p.identity.as_ref().is_none_or(|i| {
                        if m_profile {
                            i.model.as_ref().is_none_or(|model| {
                                self.register_view
                                    .catalogue
                                    .as_ref()
                                    .is_none_or(|c| model.to_ascii_lowercase() != c.cpu)
                            })
                        } else {
                            i.model.as_deref() != Some("Cortex-R52")
                        }
                    })
                    || !p.facts.contains_key(if cache {
                        "mcache.clidr"
                    } else if m_profile {
                        "mpu.regions"
                    } else {
                        bank.count_fact()
                    })
            })
        {
            self.register_view.mpu_popup.as_mut().unwrap().error =
                Some("Probe this physical core at frame 0 before reading banks".into());
            return;
        }
        let id = self.next_id;
        self.register_view.pending = Some((id, context.clone()));
        let popup = self.register_view.mpu_popup.as_mut().unwrap();
        popup.pending = Some(id);
        popup.error = None;
        self.submit(
            engine,
            if cache { "registers_cache" } else { "registers_mpu" },
            if cache { json!({"context":context,"read":true}) } else { json!({"context":context,"bank":if m_profile {json!("m")} else {json!(bank)},"read":true}) },
        );
        if !self.pending_commands.contains(&id) {
            self.register_view.pending = None;
            self.register_view.mpu_popup.as_mut().unwrap().pending = None;
        }
    }
    pub(super) fn mpu_read_response(&mut self, id: u64, error: Option<&str>) {
        if let Some(popup) = self
            .register_view
            .mpu_popup
            .as_mut()
            .filter(|p| p.pending == Some(id))
        {
            popup.pending = None;
            popup.error = error.map(String::from);
        }
    }
    fn activate_mpu_button(&mut self, engine: Option<&EngineHandle>) {
        let Some(popup) = self.register_view.mpu_popup.as_mut() else {
            return;
        };
        match popup.button {
            0 => {
                if popup.m_profile {
                    return;
                }
                popup.bank = if popup.bank == Bank::El1 {
                    Bank::El2
                } else {
                    Bank::El1
                };
                popup.count = None;
                popup.scroll = 0;
                popup.error = None;
                popup.pending = None;
                self.cancel_register_read();
            }
            1 => self.probe_registers(engine),
            2 => self.request_mpu_regions(engine),
            _ => {
                self.cancel_register_read();
                self.register_view.mpu_popup = None;
            }
        }
    }
    pub(in crate::ui) fn mpu_key(&mut self, key: KeyEvent, engine: Option<&EngineHandle>) -> bool {
        let Some(popup) = self.register_view.mpu_popup.as_mut() else {
            return false;
        };
        match key.code {
            KeyCode::Esc => {
                self.cancel_register_read();
                self.register_view.mpu_popup = None;
            }
            KeyCode::Tab | KeyCode::Right => popup.button = (popup.button + 1) % 4,
            KeyCode::BackTab | KeyCode::Left => popup.button = (popup.button + 3) % 4,
            KeyCode::Up => popup.scroll = popup.scroll.saturating_sub(1),
            KeyCode::Down => popup.scroll = (popup.scroll + 1).min(popup.max_scroll),
            KeyCode::PageUp => popup.scroll = popup.scroll.saturating_sub(8),
            KeyCode::PageDown => popup.scroll = (popup.scroll + 8).min(popup.max_scroll),
            KeyCode::Home => popup.scroll = 0,
            KeyCode::End => popup.scroll = popup.max_scroll,
            KeyCode::Enter => self.activate_mpu_button(engine),
            KeyCode::Char('r') => self.request_mpu_regions(engine),
            KeyCode::Char('p') => self.probe_registers(engine),
            KeyCode::Char('b') => {
                popup.button = 0;
                self.activate_mpu_button(engine);
            }
            _ => {}
        }
        true
    }
    pub(in crate::ui) fn mpu_mouse(
        &mut self,
        mouse: MouseEvent,
        engine: Option<&EngineHandle>,
    ) -> bool {
        let Some(popup) = self.register_view.mpu_popup.as_mut() else {
            return false;
        };
        match mouse.kind {
            MouseEventKind::ScrollDown => popup.scroll = (popup.scroll + 3).min(popup.max_scroll),
            MouseEventKind::ScrollUp => popup.scroll = popup.scroll.saturating_sub(3),
            MouseEventKind::Down(event::MouseButton::Left) => {
                if let Some((_, button)) = popup
                    .hits
                    .iter()
                    .find(|(r, _)| r.contains((mouse.column, mouse.row).into()))
                {
                    popup.button = *button;
                    self.activate_mpu_button(engine);
                }
            }
            _ => {}
        }
        true
    }
}
fn evidence(sample: Option<&Sample>, context: &Context) -> String {
    match sample {
        None => "Not read".into(),
        Some(s) => format!(
            "{} · {:?} · {} {}",
            s.value.as_ref().map(|v| v.hex.as_str()).unwrap_or("—"),
            if crate::registers::mpu::current(Some(s), context).is_some() {
                State::Valid
            } else if s.state == State::Valid {
                State::Stale
            } else {
                s.state
            },
            s.source,
            s.detail
        ),
    }
}
fn content(view: &View) -> Vec<String> {
    let flag = |v: Option<bool>| v.map(|v| if v { "on" } else { "off" }).unwrap_or("unknown");
    let mut lines = vec![
        format!(
            "{} {:?} · {} regions · M={} BR={}",
            view.owner,
            view.bank,
            view.count,
            flag(view.global_enabled),
            flag(view.background_enabled)
        ),
        format!(
            "Control: {}",
            evidence(view.control.as_ref(), &view.context)
        ),
    ];
    for (index, sample) in view.mair.iter().enumerate() {
        lines.push(format!(
            "MAIR{index}: {}",
            evidence(sample.as_ref(), &view.context)
        ));
    }
    if view.bank == Bank::El2 {
        lines.push(format!(
            "HCR: {}",
            evidence(view.hcr.as_ref(), &view.context)
        ));
        lines.push(format!(
            "HPRENR: {}",
            evidence(view.hprenr.as_ref(), &view.context)
        ));
    }
    for region in &view.regions {
        if let Some(r) = &region.decoded {
            lines.push(format!(
                "#{:02} {}–{} · {} · {} · Attr{}",
                region.index,
                r.base,
                r.limit_inclusive,
                if r.enabled { "enabled" } else { "disabled" },
                if r.execute_never { "XN" } else { "executable" },
                r.attribute_index
            ));
            lines.push(format!("  {} · SH (Normal): {}", r.access, r.shareability));
            lines.push(format!(
                "  {}",
                region
                    .attribute
                    .as_ref()
                    .map(|a| format!("{} {}", a.raw, a.label()))
                    .unwrap_or("Memory type unknown".into())
            ));
        } else {
            lines.push(format!("#{:02} pair unavailable/stale", region.index));
        }
        lines.push(format!(
            "  Base: {}",
            evidence(region.base.as_ref(), &view.context)
        ));
        lines.push(format!(
            "  Limit: {}",
            evidence(region.limit.as_ref(), &view.context)
        ));
        for issue in &region.issues {
            lines.push(format!("  ! {issue}"));
        }
    }
    lines.extend(view.notes.iter().cloned());
    lines
}
fn m_content(
    view: &crate::registers::mpu::m_profile::View,
    catalogue: &Catalogue,
    current: bool,
) -> Vec<String> {
    let mut lines = vec![
        format!(
            "{} {} · {} regions · {:?}",
            view.owner,
            view.cpu,
            view.count,
            if current { State::Valid } else { State::Stale }
        ),
        format!(
            "RNR saved={} restored={}",
            view.original_selector
                .as_ref()
                .map(|v| v.hex.as_str())
                .unwrap_or("not read"),
            view.restored_selector
                .as_ref()
                .map(|v| v.hex.as_str())
                .unwrap_or("not read")
        ),
    ];
    let mut show = |label: &str, sample: &Sample| {
        let mut sample = sample.clone();
        if !current {
            sample.stale();
        }
        lines.push(format!(
            "{label}: {}",
            evidence(Some(&sample), &view.context)
        ));
        if current
            && let Some(raw) = &sample.value
            && let Some(register) = catalogue.register(&sample.id)
        {
            let fields: Vec<_> = register
                .fields
                .iter()
                .filter_map(|f| {
                    let value = f.extract(raw).ok()?;
                    Some(format!(
                        "{}={}{}",
                        f.name,
                        value.hex,
                        f.enum_name(&value)
                            .map(|n| format!(" ({n})"))
                            .unwrap_or_default()
                    ))
                })
                .collect();
            lines.push(format!("  {}", fields.join(" · ")));
        }
    };
    show("CPUID", &view.identity);
    show("TYPE", &view.mpu_type);
    if let Some(control) = &view.control {
        show("CTRL", control);
    }
    for region in &view.regions {
        show(&format!("#{} RBAR", region.index), &region.base);
        show(&format!("#{} RASR", region.index), &region.attributes);
    }
    lines.push("Configuration samples; region fields use the selected core catalogue. No effective-address permission claim.".into());
    lines
}
fn cache_content(
    view: &crate::registers::m_cache::View,
    catalogue: &Catalogue,
    current: bool,
) -> Vec<String> {
    let mut lines = vec![
        format!(
            "{} Cortex-M7 · {:?}",
            view.owner,
            if current { State::Valid } else { State::Stale }
        ),
        format!(
            "CSSELR saved={} restored={}",
            view.original_selector
                .as_ref()
                .map(|v| v.hex.as_str())
                .unwrap_or("not read"),
            view.restored_selector
                .as_ref()
                .map(|v| v.hex.as_str())
                .unwrap_or("not read")
        ),
    ];
    let mut samples = vec![
        ("CPUID".to_string(), &view.identity),
        ("CLIDR".into(), &view.clidr),
        ("CTR".into(), &view.ctr),
    ];
    for cache in &view.caches {
        samples.push((
            format!(
                "L1 {} CCSIDR · size={}",
                cache.kind,
                if current {
                    cache
                        .size_bytes()
                        .map(|n| format!("{} KiB", n / 1024))
                        .unwrap_or("unknown encoding".into())
                } else {
                    "stale".into()
                }
            ),
            &cache.size_id,
        ));
    }
    for (label, sample) in samples {
        let mut sample = sample.clone();
        if !current {
            sample.stale();
        }
        lines.push(format!(
            "{label}: {}",
            evidence(Some(&sample), &view.context)
        ));
        if current
            && let Some(raw) = &sample.value
            && let Some(register) = catalogue.register(&sample.id)
        {
            let fields: Vec<_> = register
                .fields
                .iter()
                .filter_map(|f| {
                    let value = f.extract(raw).ok()?;
                    Some(format!(
                        "{}={}{}",
                        f.name,
                        value.hex,
                        f.enum_name(&value)
                            .map(|n| format!(" ({n})"))
                            .unwrap_or_default()
                    ))
                })
                .collect();
            lines.push(format!("  {}", fields.join(" · ")));
        }
    }
    if view.caches.is_empty() {
        lines.push("No implemented cache observed; CSSELR/CCSIDR were not accessed".into());
    }
    lines.push("Cache capacity is separate from enable state. No cache maintenance or configuration writes.".into());
    lines
}
pub(super) fn wrap(lines: Vec<String>, width: usize) -> Vec<Line<'static>> {
    use unicode_width::UnicodeWidthChar;
    let mut result = vec![];
    for line in lines {
        let mut text = String::new();
        let mut used = 0;
        for c in line.chars() {
            let n = c.width().unwrap_or(0);
            if used + n > width && !text.is_empty() {
                result.push(Line::from(std::mem::take(&mut text)));
                used = 0;
            }
            text.push(c);
            used += n;
        }
        result.push(Line::from(text));
    }
    result
}
pub(in crate::ui) fn draw(f: &mut UiFrame, app: &mut App) {
    let context = app.register_context();
    let Some(popup) = app.register_view.mpu_popup.as_ref() else {
        return;
    };
    let bank = popup.bank;
    let m_profile = popup.m_profile;
    let cache = popup.cache;
    let fresh = app
        .snapshot
        .register_probe
        .as_ref()
        .filter(|p| {
            p.context == context
                && app.snapshot.state == "STOPPED"
                && p.identity.as_ref().is_some_and(|i| {
                    if m_profile {
                        i.model.as_ref().is_some_and(|model| {
                            app.register_view
                                .catalogue
                                .as_ref()
                                .is_some_and(|c| model.to_ascii_lowercase() == c.cpu)
                        })
                    } else {
                        i.model.as_deref() == Some("Cortex-R52")
                    }
                })
        })
        .and_then(|p| {
            p.facts.get(if cache {
                "mcache.clidr"
            } else if m_profile {
                "mpu.regions"
            } else {
                bank.count_fact()
            })
        })
        .and_then(|f| {
            if cache {
                crate::registers::m_cache::valid_clidr(f.value)
                    .then(|| (f.value & 3).count_ones() as u8)
            } else if m_profile {
                u8::try_from(f.value)
                    .ok()
                    .filter(|n| matches!(n, 0 | 8 | 16))
            } else {
                bank.validate_count(f.value).ok()
            }
        });
    if let Some(n) = fresh {
        app.register_view.mpu_popup.as_mut().unwrap().count = Some((context.clone(), n));
    } else if m_profile
        && !cache
        && let Some(view) = &app.snapshot.register_mpu
        && view.context.core == context.core
        && app
            .register_view
            .catalogue
            .as_ref()
            .is_some_and(|c| c.cpu == view.cpu)
    {
        app.register_view.mpu_popup.as_mut().unwrap().count =
            Some((view.context.clone(), view.count));
    }
    let popup = app.register_view.mpu_popup.as_ref().unwrap();
    let mut lines = vec![];
    if cache {
        if let Some(view) = &app.snapshot.register_cache
            && view.context.core == context.core
            && let Some(catalogue) = &app.register_view.catalogue
        {
            let current = fresh.is_some()
                && view.valid_for(&context)
                && app.snapshot.register_probe.as_ref().is_some_and(|p| {
                    p.facts.get("mcache.clidr").map(|f| u128::from(f.value))
                        == view.clidr.value.as_ref().and_then(|v| v.integer().ok())
                        && p.facts.get("mcache.ctr").map(|f| u128::from(f.value))
                            == view.ctr.value.as_ref().and_then(|v| v.integer().ok())
                });
            lines.extend(cache_content(view, catalogue, current));
        } else {
            lines.push(
                "Probe this physical M7 core, then Read I/D cache IDs and restore CSSELR".into(),
            );
            lines.push("Opening this view reads nothing. TCM/configuration fields are in the register tree.".into());
        }
    } else if let Some((observed, count)) = &popup.count {
        if *observed != context || fresh.is_none() {
            lines.push("Region count is last-known; probe current stop before reading".into());
        }
        if m_profile {
            if let Some(view) = &app.snapshot.register_mpu
                && let Some(catalogue) = &app.register_view.catalogue
            {
                if view.context.core == context.core && view.cpu == catalogue.cpu {
                    lines.extend(m_content(
                        view,
                        catalogue,
                        fresh == Some(view.count) && view.valid_for(&context),
                    ));
                } else {
                    lines.push("No MPU bank samples for this physical core".into());
                }
            } else {
                lines.push("Press Read to sample regions and restore RNR".into());
            }
        } else {
            let samples: Vec<_> = app
                .register_view
                .values
                .values()
                .cloned()
                .map(|mut s| {
                    if app.snapshot.state != "STOPPED" {
                        s.stale();
                    }
                    s
                })
                .collect();
            match View::from_samples(bank, u64::from(*count), &context, &samples) {
                Ok(view) => lines.extend(content(&view)),
                Err(e) => lines.push(e),
            }
        }
    } else {
        lines.push(format!(
            "{} {:?}: implementation count unknown",
            context.core, bank
        ));
        lines.push(
            "Probe caps explicitly at physical frame 0; opening this view reads nothing".into(),
        );
    }
    if popup.pending.is_some() {
        lines.insert(
            0,
            if cache {
                "Reading current-core cache IDs…"
            } else {
                "Reading current-core MPU registers…"
            }
            .into(),
        );
    }
    if let Some(error) = &popup.error {
        lines.insert(0, format!("Error: {error}"));
    }
    let area = f.area();
    let w = area.width.min(112);
    let h = area.height.min(30);
    let rect = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    );
    theme::overlay(f, rect);
    let card = theme::card(
        if cache {
            " M7 cache · ↑↓ scroll · Tab actions · Esc close "
        } else {
            " MPU regions · ↑↓ scroll · Tab actions · Esc close "
        },
        true,
    );
    let inner = card.inner(rect);
    f.render_widget(card, rect);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let lines = wrap(lines, usize::from(inner.width));
    let visible = inner.height.saturating_sub(1);
    let popup = app.register_view.mpu_popup.as_mut().unwrap();
    popup.max_scroll = lines.len().saturating_sub(usize::from(visible));
    popup.scroll = popup.scroll.min(popup.max_scroll);
    popup.hits.clear();
    f.render_widget(
        Paragraph::new(
            lines
                .into_iter()
                .skip(popup.scroll)
                .take(usize::from(visible))
                .collect::<Vec<_>>(),
        ),
        Rect::new(inner.x, inner.y, inner.width, visible),
    );
    let labels = [
        if cache {
            "L1 I / D"
        } else if m_profile {
            "M regions"
        } else if bank == Bank::El1 {
            "EL1 / EL2"
        } else {
            "EL2 / EL1"
        },
        "Probe",
        "Read",
        "Close",
    ];
    for (button, label) in labels.iter().enumerate() {
        let start = inner.width * button as u16 / 4;
        let end = inner.width * (button as u16 + 1) / 4;
        let r = Rect::new(inner.x + start, inner.bottom() - 1, end - start, 1);
        f.render_widget(
            Paragraph::new(*label).style(theme::selected(button == popup.button)),
            r,
        );
        popup.hits.push((r, button));
    }
}

#[cfg(test)]
mod tests;
