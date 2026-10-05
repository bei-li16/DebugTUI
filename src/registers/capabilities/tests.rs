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
fn gic_virtual_priority_evidence_never_decides_physical_ap_capacity() {
    let p = probe(&[
        ("cpsr", 0x1a),
        ("icc_ctlr", 4 << 8),
        ("ich_vtr", (6 << 29) | (5 << 26) | 3),
    ]);
    assert_eq!(p.facts["icc.physical.pribits"].value, 5);
    assert_eq!(p.facts["icc.physical.prebits"].value, 5);
    assert_eq!(p.facts["icv.virtual.pribits"].value, 7);
    assert_eq!(p.facts["icv.virtual.prebits"].value, 6);
    assert_eq!(p.facts["ich.list_registers"].value, 4);
    for mode in [0x10, 0x13, 0x1f] {
        let p = probe(&[
            ("cpsr", mode),
            ("icc_ctlr", 6 << 8),
            ("ich_vtr", 0x90180003),
        ]);
        assert!(!p.facts.contains_key("icc.physical.prebits"));
        assert!(!p.facts.contains_key("icv.virtual.prebits"));
    }
    let p = probe(&[("cpsr", 0x1a), ("ich_vtr", 0x90180003)]);
    assert!(!p.facts.contains_key("icc.physical.prebits"));
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
        ("pmcr", 0x41003000),
    ]);
    for key in [
        "el2.present",
        "timer.present",
        "gic.system_interface",
        "pmu.present",
    ] {
        assert_eq!(p.facts[key].value, 1);
    }
    assert_eq!(p.facts["pmu.counters"].value, 6);
    let p = probe(&[("id_pfr1", 0), ("id_dfr0", 0)]);
    assert_eq!(p.facts["el2.present"].value, 0);
    assert_eq!(p.facts["pmu.present"].value, 0);
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
    let mut p = probe(&[("cpsr", 0x1a), ("icc_ctlr", 0x400)]);
    let declared = BTreeMap::from([
        ("icc.physical.prebits".into(), 7),
        ("vfp.present".into(), 1),
    ]);
    let effective = p.effective(&declared);
    assert_eq!(effective["icc.physical.prebits"], 5);
    assert_eq!(effective["vfp.present"], 1);
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
        timestamp_ms: 30,
        source: "gdb:midr".into(),
    });
    let report = p.report_lines().join("\n");
    assert!(report.contains("core=core1 session=11 generation=7 frame=0"));
    assert!(report.contains("raw=0x00000400 source=gdb:icc_ctlr"));
    assert!(report.contains("midr: Unavailable raw=Unknown"));
    assert!(!report.contains("midr: Unavailable raw=0x"));
}
