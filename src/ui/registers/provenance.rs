use crate::registers::provenance::{Acquisition, ByteOrder, Phase, Provenance, Route};

pub(super) fn details(label: &str, provenance: &Provenance) -> Vec<String> {
    let mut text = vec![format!("{label}:")];
    text.push(format!(
        "Read method: {}",
        match provenance.acquisition {
            Acquisition::Catalogue => "catalogue read",
            Acquisition::CapabilityProbe => "capability probe",
            Acquisition::MpuRegions => "MPU regions",
            Acquisition::SelectorBank => "selector bank",
        }
    ));
    text.push(format!(
        "Catalogue reader: {:?}",
        provenance.catalogue_reader
    ));
    let Some(access) = &provenance.access else {
        text.push("No value request entered a transport.".into());
        return text;
    };
    text.push(format!(
        "Value request: {}",
        match access.phase {
            Phase::Planned => "route resolved; dispatch not started",
            Phase::Started => "dispatch started; result unconfirmed",
            Phase::Responded => "transport response received; value validity is shown separately",
        }
    ));
    text.push(format!(
        "Request context: {} / frame {} / stop {} / session {}",
        access.context.core,
        access.context.frame,
        access.context.generation,
        access.context.session
    ));
    text.push(format!(
        "{} at {} ms",
        if access.phase == Phase::Planned {
            "Route resolved"
        } else {
            "Value request"
        },
        access.timestamp_ms
    ));
    if let Some(completed) = access.completed_ms {
        text.push(format!(
            "Host request interval: {}..{} ms; not a hardware timestamp",
            access.timestamp_ms, completed
        ));
    }
    match &access.route {
        Route::GdbRegister {
            endpoint,
            configured_endpoint,
            name,
            index,
        } => {
            text.push(format!("GDB register: {name} / index {index}"));
            gdb_endpoint(&mut text, endpoint.as_deref(), configured_endpoint);
        }
        Route::GdbMemory {
            endpoint,
            configured_endpoint,
            address,
            bits,
            byte_order,
        } => {
            text.push("Memory route: default GDB byte read".into());
            gdb_endpoint(&mut text, endpoint.as_deref(), configured_endpoint);
            text.push(format!("Address: {address} / {bits} bits"));
            text.push(format!(
                "Layout byte order (configured): {}",
                endian(*byte_order)
            ));
            text.push("Atomicity: not guaranteed".into());
        }
        Route::TclRegister {
            endpoint,
            target,
            operation,
        } => {
            text.push(format!("OpenOCD value operation: {operation}"));
            text.push(format!("TCL endpoint: {endpoint}"));
            text.push(format!("Target selected by request: {target}"));
        }
        Route::TclMemory {
            endpoint,
            target,
            channel,
            configuration_source,
            address,
            bits,
            bus_width,
            count,
            byte_order,
            atomic,
        } => {
            text.push(format!("Memory channel: {channel}"));
            text.push(format!(
                "Channel declaration source: {}",
                if configuration_source.is_empty() {
                    "unknown"
                } else {
                    configuration_source
                }
            ));
            text.push(format!("TCL endpoint: {endpoint}"));
            text.push(format!("Target requested: {target}"));
            text.push(format!("Address: {address} / {bits} bits"));
            text.push(format!("Bus request: {count} x {bus_width} bits"));
            text.push(format!(
                "Layout byte order (configured): {}",
                endian(*byte_order)
            ));
            text.push(format!(
                "Atomicity: {}",
                if *atomic {
                    "guaranteed"
                } else {
                    "not guaranteed"
                }
            ));
        }
    }
    if let Some(timer) = &access.timer {
        text.push(format!(
            "Timer transfer: {}",
            match timer.read_method {
                crate::registers::timer::ReadMethod::Unknown => "unknown in older evidence",
                crate::registers::timer::ReadMethod::Mrc32 => "one MRC / full 32-bit register",
                crate::registers::timer::ReadMethod::Mrrc64 => "one MRRC / full 64-bit register",
            }
        ));
        text.push(format!(
            "Timer physical MIDR: {} / current Debug EDSCR: {}",
            timer.midr.hex, timer.dscr.hex
        ));
        text.push(format!(
            "Stopped DSPSR: {} / DLR: {}",
            timer.dspsr.hex, timer.dlr.hex
        ));
        text.push("Timer items are separate samples; no cross-register atomic snapshot.".into());
    }
    for alias in &provenance.aliases {
        text.push(format!(
            "Alias source: {} / offset {} / {} bits",
            alias.source, alias.offset, alias.bits
        ));
    }
    text
}
fn gdb_endpoint(text: &mut Vec<String>, endpoint: Option<&str>, configured: &str) {
    text.push(format!(
        "GDB selected endpoint: {}",
        endpoint.unwrap_or("unknown; local or opaque CLI commands")
    ));
    if endpoint.is_none() || endpoint != Some(configured) {
        text.push(format!(
            "Configured GDB endpoint: {}",
            if configured.is_empty() {
                "none"
            } else {
                configured
            }
        ));
    }
}
fn endian(order: ByteOrder) -> &'static str {
    match order {
        ByteOrder::Little => "little endian",
        ByteOrder::Big => "big endian",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registers::{Reader, Scope, State};
    use crate::ui::registers::{
        Row, draw_status,
        framework_tests::key,
        tests::{app, engine, sample},
    };
    use crossterm::event::KeyCode;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn provenance_popup_distinguishes_catalogue_cpu_scope_latest_attempt_and_retained_origin_without_io()
     {
        for scope in [Scope::Core, Scope::Cluster, Scope::Chip] {
            let mut app = app();
            let (engine, requests) = engine();
            app.project.registers.cpu = "cortex-r52+".into();
            app.project.registers.topology.chip = "board".into();
            app.project
                .registers
                .topology
                .clusters
                .insert("default".into(), "A".into());
            app.snapshot.register_owner_generations =
                [("cluster:A".into(), 4), ("chip:board".into(), 5)].into();
            let catalogue = app.register_view.catalogue.as_mut().unwrap();
            catalogue.registers[0].scope = scope;
            catalogue.registers[0].reader = Reader::Mmio {
                component: "bus".into(),
                offset: 8,
            };
            let reader = catalogue.registers[0].reader.clone();
            app.register_view.rows = vec![Row::Register(0, 1)];
            app.selection = 0;
            let mut original = sample(&app, "r0", "0x12345678");
            original.view = crate::registers::SampleView::PhysicalCore;
            original.owner = app.project.registers.topology.owner(scope, "default");
            original.owner_generation = original
                .owner
                .as_ref()
                .and_then(|o| app.snapshot.register_owner_generations.get(o))
                .copied();
            let mut provenance = Provenance::declared(&reader);
            provenance.access = Some(crate::registers::provenance::Access {
                completed_ms: None,
                timer: None,
                route: Route::TclMemory {
                    endpoint: "127.0.0.1:6666".into(),
                    target: "ap.actual".into(),
                    channel: "ap0".into(),
                    configuration_source: "project".into(),
                    address: "0x20000008".into(),
                    bits: 64,
                    bus_width: 32,
                    count: 2,
                    byte_order: ByteOrder::Little,
                    atomic: false,
                },
                phase: Phase::Responded,
                command: "INTERNAL_WIRE_SCRIPT_MUST_NOT_APPEAR".into(),
                context: original.context.clone(),
                timestamp_ms: 23,
            });
            original.provenance = Some(provenance.clone());
            let mut failed = original.clone();
            failed.state = State::Error;
            failed.inherit_value_origin(&original);
            let access = provenance.access.as_mut().unwrap();
            access.phase = Phase::Started;
            access.timestamp_ms = 29;
            if let Route::TclMemory {
                endpoint, target, ..
            } = &mut access.route
            {
                *endpoint = "127.0.0.1:9999".into();
                *target = "ap.new".into();
            }
            failed.provenance = Some(provenance);
            app.register_view.values.insert(
                (failed.owner.clone().unwrap(), "r0".into(), "default".into()),
                failed,
            );
            for (width, height) in [(100, 24), (35, 12)] {
                key(&mut app, KeyCode::Char('t'), &engine);
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                let mut evidence = String::new();
                for _ in 0..128 {
                    terminal.draw(|f| draw_status(f, &mut app)).unwrap();
                    let buffer = terminal.backend().buffer();
                    for y in 0..height {
                        for x in 0..width {
                            evidence.push_str(buffer[(x, y)].symbol());
                        }
                    }
                    key(&mut app, KeyCode::Down, &engine);
                }
                let compact: String = evidence
                    .chars()
                    .filter(|c| !c.is_whitespace() && *c != '│')
                    .collect();
                let owner = app
                    .project
                    .registers
                    .topology
                    .owner(scope, "default")
                    .unwrap();
                for expected in [
                    format!("Owner:{owner}·{scope:?}"),
                    "CatalogueCPU:cortex-r52".into(),
                    "ConfiguredCPUchoice:cortex-r52+".into(),
                    "ObservedCPU:unknown;nocurrentidentityevidence".into(),
                    "Latestreadattempt:".into(),
                    "Retainedrawvalueorigin:".into(),
                    "dispatchstarted;resultunconfirmed".into(),
                    "transportresponsereceived".into(),
                    "TCLendpoint:127.0.0.1:9999".into(),
                    "TCLendpoint:127.0.0.1:6666".into(),
                    "Targetrequested:ap.new".into(),
                    "Targetrequested:ap.actual".into(),
                    "Memorychannel:ap0".into(),
                    "Address:0x20000008/64bits".into(),
                    "Busrequest:2x32bits".into(),
                    "Atomicity:notguaranteed".into(),
                    "0x12345678".into(),
                ] {
                    assert!(
                        compact.contains(&expected),
                        "{width}x{height}: missing {expected}"
                    );
                }
                assert!(!evidence.contains("INTERNAL_WIRE_SCRIPT"));
                key(&mut app, KeyCode::Esc, &engine);
                assert!(app.register_view.status_popup.is_none());
            }
            assert!(
                requests.try_recv().is_err(),
                "viewing provenance must not access the target"
            );
            let midr = sample(&app, "midr", "0x411fd134");
            let mut probe = crate::registers::capabilities::Probe {
                context: app.register_context(),
                thread: "1".into(),
                identity: None,
                facts: Default::default(),
                samples: vec![midr],
                gdb_names: vec![],
                notes: vec![],
            };
            probe.decode();
            assert_eq!(
                probe.identity.as_ref().unwrap().model.as_deref(),
                Some("Cortex-R52")
            );
            for (current, stopped, expected) in [
                (true, true, "ObservedCPU:Cortex-R52"),
                (false, true, "ObservedCPU:unknown;nocurrentidentityevidence"),
                (true, false, "ObservedCPU:unknown;nocurrentidentityevidence"),
            ] {
                let mut evidence = probe.clone();
                if !current {
                    evidence.context.session += 1;
                }
                app.snapshot.register_probe = Some(evidence);
                app.snapshot.state = if stopped { "STOPPED" } else { "RUNNING" }.into();
                key(&mut app, KeyCode::Char('t'), &engine);
                let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
                terminal.draw(|f| draw_status(f, &mut app)).unwrap();
                key(&mut app, KeyCode::End, &engine);
                terminal.draw(|f| draw_status(f, &mut app)).unwrap();
                let compact: String = terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .flat_map(|c| c.symbol().chars())
                    .filter(|c| !c.is_whitespace() && *c != '│')
                    .collect();
                assert!(compact.contains(expected), "{expected}: {compact}");
                key(&mut app, KeyCode::Esc, &engine);
            }
            assert!(requests.try_recv().is_err());
        }
    }
}
