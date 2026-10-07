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
    if let Some(proof) = &access.r52_core {
        match proof.current_el() {
            Some(2) => {
                text.push(format!(
                    "R52 current Debug EL: 2; external MIDR: {} / EDSCR: {}",
                    proof.midr.hex, proof.dscr.hex
                ));
                text.push(format!(
                    "R52 stopped DSPSR: {} / DLR: {}",
                    proof.dspsr.hex, proof.dlr.hex
                ));
                if proof.bank == crate::registers::r52_core::CapacityBank::None {
                    text.push("R52 MPU capacity: not requested for this scalar".into());
                } else {
                    text.push(format!(
                        "R52 fresh MPU capacity: {:?} / {}",
                        proof.bank, proof.capacity.hex
                    ));
                }
                text.push("Saved DSPSR is stopped program state, not access authorization. Proof and value belong to this request only.".into());
            }
            _ => text.push(
                "R52 recorded Debug state is invalid or is not EL2; no permission inferred.".into(),
            ),
        }
    }
    if let Some(proof) = &access.vfp {
        match proof.validate() {
            Ok(()) => {
                text.push(format!(
                    "FP current Debug EL: 2 (Hyp); external MIDR: {} / EDSCR: {}",
                    proof.midr.hex, proof.dscr.hex
                ));
                text.push(format!(
                    "FP stopped DSPSR: {} / DLR: {} / HCPTR: {}",
                    proof.dspsr.hex, proof.dlr.hex, proof.hcptr.hex
                ));
                text.push("Saved DSPSR is stopped program state; it does not authorize current Debug instructions. No mode or FPU enable change.".into());
            }
            Err(error) => text.push(format!("FP current access evidence invalid: {error}")),
        }
    }
    if let Some(pair) = &access.vfp_pair {
        match pair.features() {
            Ok(features) => {
                text.push(format!(
                    "FP storage pair: D{} / D{}; {} D registers; Q views: {}",
                    pair.first_d,
                    pair.first_d + 1,
                    features.d_registers,
                    if features.neon {
                        "supported"
                    } else {
                        "not implemented"
                    }
                ));
                text.push(format!("FP pair raw: {}", pair.raw.hex));
                text.push(format!(
                    "FP capacity MVFR0: {} / MVFR1: {} / FPEXC: {}",
                    pair.mvfr0.hex, pair.mvfr1.hex, pair.fpexc.hex
                ));
                text.push("S0-S31 overlap D0-D15; Qn overlaps D(2n)/D(2n+1). Lane 0 uses low bits; display interpretation does not grant execution permission. Different pairs are separate samples.".into());
            }
            Err(error) => text.push(format!("FP storage evidence invalid: {error}")),
        }
    }
    if let Some(banked) = &access.banked {
        text.push(format!(
            "Banked transfer: {:?}; current Debug mode: {:?}",
            banked.read_method,
            banked
                .current_mode()
                .ok()
                .flatten()
                .map(|mode| format!("0x{mode:02x}"))
        ));
        text.push(format!(
            "Banked physical MIDR: {} / current Debug EDSCR: {}",
            banked.midr.hex, banked.dscr.hex
        ));
        text.push(format!(
            "Stopped DSPSR: {} / DLR: {}",
            banked.dspsr.hex, banked.dlr.hex
        ));
        text.push(
            "Current mode is derived only for EL0/User or EL2/Hyp. Banks are separate samples."
                .into(),
        );
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
    if let Some(pmu) = &access.pmu {
        text.push(format!(
            "PMU transfer: {:?}; physical event count: {:?}",
            pmu.read_method,
            pmu.physical_count()
        ));
        text.push(format!(
            "PMU physical MIDR: {} / current Debug EDSCR: {}",
            pmu.midr.hex, pmu.dscr.hex
        ));
        text.push(format!(
            "Stopped DSPSR: {} / DLR: {}",
            pmu.dspsr.hex, pmu.dlr.hex
        ));
        text.push(format!(
            "ID_DFR0: {} / PMCR: {} / HDCR: {} / preserved PMSELR: {}",
            pmu.id_dfr0.hex, pmu.pmcr.hex, pmu.hdcr.hex, pmu.pmselr.hex
        ));
        text.push("PMU observation does not enable/reset counters or write PMSELR; items are separate samples.".into());
    }
    if let Some(gic) = &access.gic {
        text.push(format!(
            "GIC transfer: {:?}; interface: {:?}",
            gic.read_method, gic.view
        ));
        text.push(format!(
            "GIC physical MIDR: {} / current Debug EDSCR: {}",
            gic.midr.hex, gic.dscr.hex
        ));
        text.push(format!(
            "Stopped DSPSR: {} / DLR: {}",
            gic.dspsr.hex, gic.dlr.hex
        ));
        text.push(format!("Physical ICC priority bits: {:?}; virtual ICV priority/preemption bits: {:?}/{:?}; ICH list entries: {:?}",gic.physical_priority_bits(),gic.virtual_priority_bits(),gic.virtual_preemption_bits(),gic.list_count()));
        text.push(format!(
            "ID_PFR1: {} / ICC_HSRE: {} / ICC_SRE: {} / ICC_CTLR: {} / ICH_VTR: {}",
            gic.id_pfr1.hex, gic.icc_hsre.hex, gic.icc_sre.hex, gic.icc_ctlr.hex, gic.ich_vtr.hex
        ));
        text.push(format!(
            "Preserved HCR: {} / ICH_HCR: {} / HSTR: {}",
            gic.hcr.hex, gic.ich_hcr.hex, gic.hstr.hex
        ));
        text.push("GIC observation does not acknowledge, enable or deactivate interrupts; LR/LRC and different cores are separate samples.".into());
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
    #[test]
    fn r52_alias_details_keep_saved_user_state_separate_from_current_debug_evidence() {
        use crate::registers::{
            RawValue, Reader,
            provenance::Access,
            r52_core::{Request, Response},
        };
        let reader = Reader::Cp15 {
            cp: 15,
            op1: 0,
            crn: 1,
            crm: 0,
            op2: 0,
        };
        let request = Request::from_reader(&reader).unwrap();
        let response = Response::parse("midr 0x411fd134 dscr 0x01050213 dspsr 0xa2000410 dlr 0x81234568 bank none capacity 0x00000000 value 0x00c50078", &request).unwrap();
        let mut origin = Provenance::declared(&Reader::Alias {
            source: "sctlr".into(),
            offset: 0,
        });
        origin.access = Some(serde_json::from_value::<Access>(serde_json::json!({
            "r52_core":response.evidence,
            "route":{"kind":"tcl_register","endpoint":"localhost:6666","target":"soc.r52.1","operation":"R52 read sctlr"},
            "phase":"responded","command":"bounded R52 transaction","context":{"session":7,"generation":3,"core":"core1","frame":0},"timestamp_ms":10,"completed_ms":11
        })).unwrap());
        let text = details("Current value", &origin).join("\n");
        for expected in [
            "R52 current Debug EL: 2",
            "0xa2000410",
            "soc.r52.1",
            "not access authorization",
            "this request only",
            "not requested for this scalar",
        ] {
            assert!(text.contains(expected), "{expected}: {text}");
        }
        origin
            .access
            .as_mut()
            .unwrap()
            .r52_core
            .as_mut()
            .unwrap()
            .dscr = RawValue::parse("0x01050113", 32).unwrap();
        let text = details("Old value", &origin).join("\n");
        assert!(text.contains("no permission inferred"));
        assert!(!text.contains("R52 current Debug EL: 2"));
    }
    #[test]
    fn floating_pair_details_explain_capacity_raw_bits_and_mapping_without_permissions() {
        use crate::registers::{
            Context, RawValue, Reader,
            provenance::Access,
            vfp::{Kind, Response},
        };
        let response = Response::parse("midr 0x411fd134 dscr 0x01000200 dspsr 0xa2000410 dlr 0x81234568 hcptr 0x00000000 mvfr0 0x10110222 mvfr1 0x12111111 fpexc 0x40000700 value 0x7ff8000000000042800000003f800000", Kind::Quad(0)).unwrap();
        let mut origin = Provenance::declared(&Reader::Vfp { name: "q0".into() });
        origin.access = Some(Access {
            banked: None,
            vfp: Some(response.evidence.clone()),
            vfp_pair: response.pair_evidence(Kind::Quad(0)),
            r52_core: None,
            timer: None,
            pmu: None,
            gic: None,
            route: Route::TclRegister {
                endpoint: "localhost:6666".into(),
                target: "cpu1".into(),
                operation: "VFP read q0".into(),
            },
            phase: Phase::Responded,
            command: "physical transaction".into(),
            context: Context {
                session: 1,
                generation: 2,
                core: "core1".into(),
                frame: 0,
            },
            timestamp_ms: 20,
            completed_ms: Some(24),
        });
        let text = details("Current value", &origin).join("\n");
        for expected in [
            "FP current Debug EL: 2 (Hyp)",
            "FP stopped DSPSR: 0xa2000410 / DLR: 0x81234568",
            "does not authorize current Debug instructions",
            "D0 / D1; 32 D registers; Q views: supported",
            "0x7ff8000000000042800000003f800000",
            "0x10110222",
            "0x12111111",
            "0x40000700",
            "S0-S31 overlap D0-D15",
            "Lane 0 uses low bits",
            "does not grant execution permission",
            "Different pairs are separate samples",
        ] {
            assert!(text.contains(expected), "{expected}: {text}");
        }
        origin
            .access
            .as_mut()
            .unwrap()
            .vfp_pair
            .as_mut()
            .unwrap()
            .first_d = 1;
        let malformed = details("Current value", &origin).join("\n");
        assert!(malformed.contains("FP storage evidence invalid"));
        assert!(!malformed.contains("Q views: supported"));
        let mut invalid = origin.clone();
        invalid.access.as_mut().unwrap().vfp.as_mut().unwrap().dscr =
            RawValue::parse("0x01000100", 32).unwrap();
        let text = details("Current value", &invalid).join("\n");
        assert!(text.contains("FP current access evidence invalid"));
        assert!(!text.contains("FP current Debug EL: 2"));
    }
    use crate::registers::{Reader, Scope, State};
    use crate::session::state;
    use crate::ui::registers::{
        Row, draw_status,
        framework_tests::key,
        tests::{app, engine, sample},
    };
    use crossterm::event::KeyCode;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn gic_details_keep_physical_and_virtual_interfaces_and_old_evidence_distinct() {
        use crate::registers::{Catalogue, Context, gic::Response, provenance::Access};
        for (id, view, value) in [
            ("icc_ctlr", "physical_icc", "0x00000403"),
            ("ich_vtr", "hypervisor_ich", "0x90180003"),
        ] {
            let wire = format!(
                "view {view} midr 0x411fd134 dscr 0x01000200 dspsr 0xa2000410 dlr 0x81234568 id_pfr1 0x10111011 icc_hsre 0x0000000f icc_sre 0x00000007 icc_ctlr 0x00000403 ich_vtr 0x90180003 hcr 0x00000038 ich_hcr 0x00007c01 hstr 0x00001000 value {value}"
            );
            let mut provenance = Provenance::declared(
                &Catalogue::builtin("cortex-r52")
                    .unwrap()
                    .register(id)
                    .unwrap()
                    .reader,
            );
            provenance.access = Some(Access {
                banked: None,
                vfp: None,
                vfp_pair: None,
                gic: Some(Response::parse(&wire, id, 32).unwrap().evidence),
                r52_core: None,
                timer: None,
                pmu: None,
                route: Route::TclRegister {
                    endpoint: "localhost:6666".into(),
                    target: "cpu1".into(),
                    operation: format!("GIC read {id}"),
                },
                phase: Phase::Responded,
                command: "INTERNAL_SCRIPT".into(),
                context: Context {
                    session: 1,
                    generation: 2,
                    core: "core1".into(),
                    frame: 0,
                },
                timestamp_ms: 10,
                completed_ms: Some(12),
            });
            let text = details("Retained origin", &provenance).join("\n");
            for expected in [
                "Host request interval: 10..12",
                "Physical ICC priority bits: Some(5)",
                "virtual ICV priority/preemption bits: Some(5)/Some(5)",
                "ICH list entries: Some(4)",
                "0x00000038",
                "0x00001000",
                "does not acknowledge",
                "cpu1",
            ] {
                assert!(text.contains(expected), "{expected}: {text}");
            }
            assert!(text.contains(if id == "icc_ctlr" {
                "PhysicalIcc"
            } else {
                "HypervisorIch"
            }));
            assert!(!text.contains("INTERNAL_SCRIPT"));
            let mut old = serde_json::to_value(&provenance).unwrap();
            old["access"].as_object_mut().unwrap().remove("gic");
            let old: Provenance = serde_json::from_value(old).unwrap();
            assert!(old.access.as_ref().unwrap().gic.is_none());
            assert!(
                !details("Old origin", &old)
                    .join("\n")
                    .contains("GIC transfer")
            );
        }
    }
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
                require_owner_mapping: false,
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
                r52_core: None,
                timer: None,
                pmu: None,
                gic: None,
                banked: None,
                vfp: None,
                vfp_pair: None,
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
                loop {
                    terminal.draw(|f| draw_status(f, &mut app)).unwrap();
                    let buffer = terminal.backend().buffer();
                    for y in 0..height {
                        for x in 0..width {
                            evidence.push_str(buffer[(x, y)].symbol());
                        }
                    }
                    if app.register_view.status_popup.as_ref().unwrap().at_end() {
                        break;
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
                nvic: None,
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
                app.snapshot.state = if stopped {
                    state::STOPPED
                } else {
                    state::RUNNING
                }
                .into();
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
