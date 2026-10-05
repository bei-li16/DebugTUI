use super::*;

fn probe(values: &[(&str, u64)]) -> Probe {
    let mut values = values.to_vec();
    if !values.iter().any(|(id, _)| *id == "midr") {
        values.push(("midr", 0x411fd134));
    }
    let context = Context {
        session: 11,
        generation: 7,
        core: "core1".into(),
        frame: 0,
    };
    let mut probe = Probe {
        context: context.clone(),
        thread: "3".into(),
        identity: None,
        facts: BTreeMap::new(),
        samples: vec![],
        gdb_names: vec![],
        notes: vec![],
    };
    for &(id, value) in &values {
        probe.samples.push(Sample {
            id: id.into(),
            state: State::Valid,
            implementation: Implementation::Unknown,
            reason: Reason::Unknown,
            detail: String::new(),
            value: Some(RawValue::parse(&format!("0x{value:08x}"), 32).unwrap()),
            owner: Some("core:core1".into()),
            context: context.clone(),
            view: crate::registers::SampleView::PhysicalCore,
            owner_generation: None,
            provenance: None,
            last_value_provenance: None,
            eligibility: None,
            last_value_eligibility: None,
            timestamp_ms: 29,
            source: format!("gdb:{id}"),
        });
    }
    probe.decode();
    probe
}

#[test]
fn identity_decodes_actual_midr_and_keeps_other_parts_unknown() {
    let p = probe(&[("midr", 0x411fd134)]);
    let identity = p.identity.unwrap();
    assert_eq!(identity.model.as_deref(), Some("Cortex-R52"));
    assert_eq!(identity.revision_name, "r1p4");
    assert_eq!(
        (identity.implementer, identity.architecture, identity.part),
        (0x41, 15, 0xd13)
    );
    for raw in [0x411fd164, 0x421fd134, 0x411ed134] {
        assert!(probe(&[("midr", raw)]).identity.unwrap().model.is_none());
    }
}

#[test]
fn unadapted_or_unreadable_cpu_identity_cannot_enable_optional_capabilities() {
    for raw in [0x411fd164, 0x421fd134, 0x411ed134] {
        let p = probe(&[
            ("midr", raw),
            ("cpsr", 0x1a),
            ("mpuir", 0x1800),
            ("icc_ctlr", 0x400),
            ("id_pfr1", 0x10111001),
        ]);
        assert!(p.identity.as_ref().unwrap().model.is_none());
        assert_eq!(p.facts.len(), 1);
        assert_eq!(p.facts["cpu.mode"].value, 0x1a);
        assert!(p.notes.iter().any(|n| n.contains("not an adapted")));
    }
    let mut p = probe(&[("mpuir", 0x1800)]);
    p.samples.iter_mut().find(|s| s.id == "midr").unwrap().state = State::Unavailable;
    p.decode();
    assert!(p.identity.is_none());
    assert!(p.facts.is_empty());
}

#[test]
fn capability_evidence_rejects_other_owners_frames_generations_and_failed_samples() {
    for mismatch in 0..8 {
        let mut p = probe(&[("mpuir", 0x1800)]);
        match mismatch {
            0 => p.samples[0].owner = Some("core:core0".into()),
            1 => p.samples[0].owner = Some("chip:tha6".into()),
            2 => p.samples[0].context.frame = 1,
            3 => p.samples[0].context.generation += 1,
            4 => p.samples[0].context.session += 1,
            5 => p.samples[0].state = State::Error,
            6 => p.samples[0].value = Some(RawValue::parse("0x1800", 64).unwrap()),
            _ => p.samples[0].value = None,
        }
        p.decode();
        assert!(p.facts.is_empty(), "mismatch {mismatch}");
        assert_eq!(p.raw("mpuir"), None);
    }
}

#[test]
fn repeated_decoding_replaces_observation_notes_and_uses_matching_source_evidence() {
    let mut p = probe(&[("mpuir", 17 << 8)]);
    p.notes.push("operator note".into());
    assert!(p.notes.iter().any(|n| n.contains("outside the adapted")));
    p.samples[0].value = Some(RawValue::parse("0x1800", 32).unwrap());
    let mut rejected = p.samples[0].clone();
    rejected.context.core = "core0".into();
    rejected.source = "wrong-core-channel".into();
    p.samples.insert(0, rejected);
    for _ in 0..3 {
        p.decode();
    }
    assert_eq!(p.facts["mpu.el1.regions"].value, 24);
    assert_eq!(p.facts["mpu.el1.regions"].source, "gdb:mpuir");
    assert!(!p.notes.iter().any(|n| n.contains("outside the adapted")));
    assert_eq!(p.notes, ["operator note"]);
}

#[test]
fn r52_el1_and_el2_region_counts_use_different_fields_and_known_ranges() {
    for count in [16, 20, 24] {
        let p = probe(&[("mpuir", (count << 8) | 3), ("hmpuir", count | 0x5a00)]);
        assert_eq!(p.facts["mpu.el1.regions"].value, count);
        assert_eq!(p.facts["mpu.el2.regions"].value, count);
        assert_eq!(p.facts["mpu.el1.regions"].source, "gdb:mpuir");
    }
    let p = probe(&[("mpuir", 0), ("hmpuir", 0)]);
    assert!(!p.facts.contains_key("mpu.el1.regions"));
    assert_eq!(p.facts["mpu.el2.regions"].value, 0);
    for count in [1, 8, 17, 32, 255] {
        let p = probe(&[("mpuir", count << 8), ("hmpuir", count)]);
        assert!(!p.facts.contains_key("mpu.el1.regions"));
        assert!(!p.facts.contains_key("mpu.el2.regions"));
        assert!(p.notes.iter().any(|n| n.contains("outside the adapted")));
    }
}

#[test]
fn raw_gic_counts_and_stopped_hyp_never_prove_physical_or_virtual_ap_capacity() {
    for mode in [0x10, 0x13, 0x1a, 0x1f] {
        for bits in 1..=8 {
            let p = probe(&[
                ("cpsr", mode),
                ("icc_ctlr", (bits - 1) << 8),
                ("ich_vtr", (6 << 29) | (5 << 26) | 3),
            ]);
            assert_eq!(p.facts["icc.ctlr_pribits"].value, bits);
            assert_eq!(p.facts["ich.vtr_pribits"].value, 7);
            assert_eq!(p.facts["ich.vtr_prebits"].value, 6);
            assert_eq!(p.facts["ich.vtr_listregs"].value, 4);
            for key in [
                "icc.physical.pribits",
                "icc.physical.prebits",
                "icv.virtual.pribits",
                "icv.virtual.prebits",
                "ich.list_registers",
            ] {
                assert!(!p.facts.contains_key(key), "{mode:#x} {bits}: {key}");
            }
        }
    }
}
#[test]
fn gic_capacities_require_independent_current_native_interface_evidence() {
    use crate::registers::{
        gic::{Response, View},
        provenance::{Access, Phase, Provenance, Route},
        timer::ReadMethod,
    };
    for mode in [0x10, 0x13, 0x1a] {
        let mut p = probe(&[("cpsr", mode), ("icc_ctlr", 0x403), ("ich_vtr", 0x90180003)]);
        for (id, view, value) in [
            ("icc_ctlr", "physical_icc", "0x00000403"),
            ("ich_vtr", "hypervisor_ich", "0x90180003"),
        ] {
            let wire = format!(
                "view {view} midr 0x411fd134 dscr 0x01000200 dspsr 0xa2000410 dlr 0x81234568 id_pfr1 0x10111011 icc_hsre 0x0000000f icc_sre 0x00000007 icc_ctlr 0x00000403 ich_vtr 0x90180003 hcr 0x00000038 ich_hcr 0x00007c01 hstr 0x00001000 value {value}"
            );
            let sample = p.samples.iter_mut().find(|s| s.id == id).unwrap();
            let mut provenance = Provenance::declared(
                &Catalogue::builtin("cortex-r52")
                    .unwrap()
                    .register(id)
                    .unwrap()
                    .reader,
            );
            provenance.access = Some(Access {
                timer: None,
                pmu: None,
                gic: Some(Response::parse(&wire, id, 32).unwrap().evidence),
                route: Route::TclRegister {
                    endpoint: "localhost:1".into(),
                    target: "cpu1".into(),
                    operation: format!("GIC read {id}"),
                },
                phase: Phase::Responded,
                command: format!("aarch64 gic {id}"),
                context: p.context.clone(),
                timestamp_ms: 29,
                completed_ms: Some(30),
            });
            sample.source = "openocd:aarch64 gic".into();
            sample.provenance = Some(provenance);
        }
        p.decode();
        for key in [
            "icc.physical.pribits",
            "icc.physical.prebits",
            "icv.virtual.pribits",
            "icv.virtual.prebits",
        ] {
            assert_eq!(p.facts[key].value, 5);
        }
        assert_eq!(p.facts["ich.list_registers"].value, 4);
        for (id, key, other) in [
            ("icc_ctlr", "icc.physical.prebits", "icv.virtual.prebits"),
            ("ich_vtr", "icv.virtual.prebits", "icc.physical.prebits"),
        ] {
            for mismatch in 0..12 {
                let mut bad = p.clone();
                let sample = bad.samples.iter_mut().find(|s| s.id == id).unwrap();
                let a = sample.provenance.as_mut().unwrap().access.as_mut().unwrap();
                match mismatch {
                    0 => sample.source = format!("gdb:{id}"),
                    1 => sample.view = SampleView::SelectedFrame,
                    2 => a.context.generation += 1,
                    3 => a.phase = Phase::Started,
                    4 => a.completed_ms = None,
                    5 => a.completed_ms = Some(28),
                    6 => {
                        a.route = Route::TclRegister {
                            endpoint: "x".into(),
                            target: "cpu1".into(),
                            operation: "GIC read icc_pmr".into(),
                        }
                    }
                    7 => a.gic.as_mut().unwrap().read_method = ReadMethod::Mrrc64,
                    8 => {
                        a.gic.as_mut().unwrap().view = if id == "icc_ctlr" {
                            View::HypervisorIch
                        } else {
                            View::PhysicalIcc
                        }
                    }
                    9 => a.gic.as_mut().unwrap().midr = RawValue::parse("0x411fd130", 32).unwrap(),
                    10 => a.gic.as_mut().unwrap().dscr = RawValue::parse("0x01000100", 32).unwrap(),
                    _ => sample.value = Some(RawValue::parse("0x00000000", 32).unwrap()),
                }
                bad.decode();
                assert!(!bad.facts.contains_key(key), "{id} mismatch {mismatch}");
                assert_eq!(bad.facts[other].value, 5);
            }
        }
    }
}

#[test]
fn optional_unknown_encodings_and_cpacr_do_not_invent_absence_or_fpu_enablement() {
    let p = probe(&[
        ("id_pfr1", 0x20222002),
        ("id_dfr0", 0x0f000000),
        ("cpacr", 0x00f00000),
    ]);
    for key in [
        "el2.present",
        "timer.present",
        "gic.system_interface",
        "pmu.present",
        "vfp.present",
        "vfp.enabled",
    ] {
        assert!(!p.facts.contains_key(key), "{key}");
    }
    assert_eq!(p.facts["vfp.cpacr_permission"].value, 15);
    assert_eq!(p.facts["pmu.version"].value, 15);
    let p = probe(&[
        ("id_pfr1", 0x10111001),
        ("id_dfr0", 0x03010066),
        ("cpsr", 0x1a),
        ("pmcr", 0x41132000),
    ]);
    for key in [
        "el2.present",
        "timer.present",
        "gic.system_interface",
        "pmu.present",
    ] {
        assert_eq!(p.facts[key].value, 1);
    }
    assert!(!p.facts.contains_key("pmu.counters"));
    let p = probe(&[("id_pfr1", 0), ("id_dfr0", 0)]);
    assert_eq!(p.facts["el2.present"].value, 0);
    assert_eq!(p.facts["pmu.present"].value, 0);
}

#[test]
fn pmcr_guest_counts_and_unadapted_hyp_values_never_prove_physical_counter_absence() {
    for mode in [0x10, 0x13, 0x1a] {
        for count in [0, 1, 4, 6, 31] {
            for enabled in [0, 1] {
                let p = probe(&[
                    ("cpsr", mode),
                    ("id_dfr0", 0x03010066),
                    ("pmcr", 0x41130000 | (count << 11) | enabled),
                ]);
                assert_eq!(p.facts["pmu.present"].value, 1);
                assert_eq!(p.facts["pmu.pmcr_n"].value, count);
                assert!(!p.facts.contains_key("pmu.counters"));
                assert!(
                    p.notes
                        .iter()
                        .any(|note| note.contains("Physical PMU count remains unknown"))
                );
            }
        }
    }
    let p = probe(&[("pmcr", 0x41132000)]);
    assert!(
        !p.facts.contains_key("pmu.counters"),
        "Unknown actual mode cannot prove physical capacity"
    );
}

#[test]
fn pmu_physical_capacity_requires_matching_current_native_read_evidence_not_stopped_mode() {
    use crate::registers::{
        Reader,
        provenance::{Access, Phase, Provenance, Route},
        timer::ReadMethod,
    };
    for mode in [0x10, 0x13, 0x1a] {
        let mut p = probe(&[
            ("cpsr", mode),
            ("id_dfr0", 0x03010066),
            ("pmcr", 0x41132048),
        ]);
        let evidence=super::super::pmu::Response::parse("midr 0x411fd134 dscr 0x01000200 dspsr 0xa2000410 dlr 0x81234568 id_dfr0 0x03010066 pmcr 0x41132048 hdcr 0x00400e02 pmselr 0x00000003 value 0x41132048","pmcr",32).unwrap().evidence;
        let sample = p.samples.iter_mut().find(|s| s.id == "pmcr").unwrap();
        sample.source = "openocd:aarch64 pmu".into();
        let mut provenance = Provenance::declared(&Reader::Cp15 {
            cp: 15,
            op1: 0,
            crn: 9,
            crm: 12,
            op2: 0,
        });
        provenance.access = Some(Access {
            timer: None,
            pmu: Some(evidence),
            gic: None,
            route: Route::TclRegister {
                endpoint: "localhost:1".into(),
                target: "cpu1".into(),
                operation: "PMU read pmcr".into(),
            },
            phase: Phase::Responded,
            command: "aarch64 pmu pmcr".into(),
            context: p.context.clone(),
            timestamp_ms: 29,
            completed_ms: Some(30),
        });
        sample.provenance = Some(provenance);
        p.decode();
        assert_eq!(p.facts["pmu.counters"].value, 4);
        assert_eq!(p.facts["pmu.guest_partition"].value, 2);
        for mismatch in 0..9 {
            let mut bad = p.clone();
            let sample = bad.samples.iter_mut().find(|s| s.id == "pmcr").unwrap();
            let access = sample.provenance.as_mut().unwrap().access.as_mut().unwrap();
            match mismatch {
                0 => sample.source = "gdb:pmcr".into(),
                1 => access.context.generation += 1,
                2 => access.phase = Phase::Started,
                3 => access.pmu.as_mut().unwrap().dscr = RawValue::parse("0x01000100", 32).unwrap(),
                4 => access.pmu.as_mut().unwrap().midr = RawValue::parse("0x411fd130", 32).unwrap(),
                5 => access.pmu.as_mut().unwrap().pmcr = RawValue::parse("0x41132049", 32).unwrap(),
                6 => access.pmu.as_mut().unwrap().read_method = ReadMethod::Unknown,
                7 => sample.view = crate::registers::SampleView::SelectedFrame,
                _ => access.pmu = None,
            }
            bad.decode();
            assert!(
                !bad.facts.contains_key("pmu.counters"),
                "mode {mode}, mismatch {mismatch}"
            );
        }
    }
}

#[test]
fn vfp_capacity_and_enablement_need_independent_current_physical_evidence() {
    for (m0, m1, count, dp, neon) in [
        (0x10110021, 0x11000011, 16, 0, 0),
        (0x10110222, 0x12111111, 32, 1, 1),
    ] {
        let p = probe(&[
            ("mvfr0", m0),
            ("mvfr1", m1),
            ("fpexc", 0x700),
            ("cpacr", 0xf00000),
        ]);
        assert_eq!(p.facts["vfp.present"].value, 1);
        assert_eq!(p.facts["vfp.d_registers"].value, count);
        assert_eq!(p.facts["vfp.double_precision"].value, dp);
        assert_eq!(p.facts["vfp.neon"].value, neon);
        assert_eq!(p.facts["vfp.enabled"].value, 0);
        assert_eq!(p.facts["vfp.enabled"].register, "fpexc");
        assert_eq!(p.facts["vfp.d_registers"].source, "gdb:mvfr0");
        assert_eq!(p.facts["vfp.neon"].source, "gdb:mvfr1");
    }
    for values in [
        vec![("mvfr0", 0x10110222)],
        vec![("mvfr0", 0x10110222), ("mvfr1", 0x12113111)],
        vec![("mvfr0", 0x10110021), ("mvfr1", 0x12111111)],
    ] {
        let p = probe(&values);
        for key in [
            "vfp.present",
            "vfp.d_registers",
            "vfp.double_precision",
            "vfp.neon",
        ] {
            assert!(!p.facts.contains_key(key));
        }
    }
}

#[test]
fn vfp_cross_core_or_stale_features_never_authorize_aliases() {
    for id in ["mvfr0", "mvfr1", "fpexc"] {
        for stale in 0..4 {
            let mut p = probe(&[
                ("mvfr0", 0x10110222),
                ("mvfr1", 0x12111111),
                ("fpexc", 0x40000700),
            ]);
            let sample = p.samples.iter_mut().find(|s| s.id == id).unwrap();
            match stale {
                0 => sample.context.core = "core0".into(),
                1 => sample.context.generation += 1,
                2 => sample.state = State::Unavailable,
                _ => sample.value = Some(RawValue::parse("0x40000700", 64).unwrap()),
            }
            p.decode();
            if id == "fpexc" {
                assert!(!p.facts.contains_key("vfp.enabled"));
            } else {
                assert!(!p.facts.contains_key("vfp.d_registers"));
                assert!(!p.facts.contains_key("vfp.neon"));
            }
        }
    }
}

#[test]
fn observed_context_facts_override_declarations_and_report_retains_raw_sources() {
    let mut p = probe(&[("cpsr", 0x1a), ("icc_ctlr", 0x400), ("mpuir", 0x1800)]);
    let declared = BTreeMap::from([
        ("icc.physical.prebits".into(), 7),
        ("vfp.present".into(), 1),
        ("mpu.el1.regions".into(), 16),
    ]);
    let effective = p.effective(&declared);
    assert_eq!(effective["icc.physical.prebits"], 7);
    assert_eq!(effective["vfp.present"], 1);
    assert_eq!(effective["mpu.el1.regions"], 24);
    assert_eq!(declared["mpu.el1.regions"], 16);
    assert_eq!(declared["icc.physical.prebits"], 7);
    p.samples.push(Sample {
        id: "midr".into(),
        state: State::Unavailable,
        implementation: Implementation::Unknown,
        reason: Reason::AccessRestricted,
        detail: "register inaccessible".into(),
        value: None,
        owner: Some("core:core1".into()),
        context: p.context.clone(),
        view: crate::registers::SampleView::PhysicalCore,
        owner_generation: None,
        provenance: None,
        last_value_provenance: None,
        eligibility: None,
        last_value_eligibility: None,
        timestamp_ms: 30,
        source: "gdb:midr".into(),
    });
    let report = p.report_lines().join("\n");
    assert!(report.contains("core=core1 session=11 generation=7 frame=0"));
    assert!(report.contains("raw=0x00000400 source=gdb:icc_ctlr"));
    assert!(report.contains("midr: Unavailable raw=Unknown"));
    assert!(!report.contains("midr: Unavailable raw=0x"));
}
