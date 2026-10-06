use super::*;
use serde_json::json;

fn context() -> Context {
    Context {
        session: 9,
        generation: 4,
        core: "core.2".into(),
        frame: 0,
    }
}
fn sample(context: &Context, id: &str, raw: &str) -> Sample {
    serde_json::from_value(json!({
        "id":id,"state":"valid","implementation":"unknown","reason":"unknown","detail":"",
        "value":RawValue::parse(raw,32).unwrap(),"owner":format!("core:{}",context.core),"context":context,
        "timestamp_ms":31,"source":format!("gdb:{id}"),"view":"physical_core"
    })).unwrap()
}
fn probe() -> capabilities::Probe {
    let context = context();
    let mut probe = capabilities::Probe {
        context: context.clone(),
        thread: "2".into(),
        identity: None,
        facts: BTreeMap::new(),
        samples: [
            ("midr", "0x411fd134"),
            ("cpsr", "0x1a"),
            ("icc_ctlr", "0x400"),
        ]
        .into_iter()
        .map(|(id, raw)| sample(&context, id, raw))
        .collect(),
        nvic: None,
        gdb_names: vec![],
        notes: vec![],
    };
    let entry = probe
        .samples
        .iter_mut()
        .find(|s| s.id == "icc_ctlr")
        .unwrap();
    let wire = "view physical_icc midr 0x411fd134 dscr 0x01000200 dspsr 0xa2000410 dlr 0x81234568 id_pfr1 0x10111011 icc_hsre 0x0000000f icc_sre 0x00000007 icc_ctlr 0x00000400 ich_vtr 0x90180003 hcr 0x00000038 ich_hcr 0x00007c01 hstr 0x00001000 value 0x00000400";
    let mut provenance = super::super::provenance::Provenance::declared(
        &Catalogue::builtin("cortex-r52")
            .unwrap()
            .register("icc_ctlr")
            .unwrap()
            .reader,
    );
    provenance.access = Some(super::super::provenance::Access {
        banked: None,
        vfp: None,
        vfp_pair: None,
        gic: Some(
            super::super::gic::Response::parse(wire, "icc_ctlr", 32)
                .unwrap()
                .evidence,
        ),
        timer: None,
        pmu: None,
        route: super::super::provenance::Route::TclRegister {
            endpoint: "localhost:1".into(),
            target: "cpu2".into(),
            operation: "GIC read icc_ctlr".into(),
        },
        phase: super::super::provenance::Phase::Responded,
        command: "aarch64 gic icc_ctlr".into(),
        context: context.clone(),
        timestamp_ms: 31,
        completed_ms: Some(32),
    });
    entry.source = "openocd:aarch64 gic".into();
    entry.provenance = Some(provenance);
    probe.decode();
    probe
}

#[test]
fn all_builtin_optional_conditions_preserve_unknown_and_exact_count_boundaries() {
    for cpu in [
        "cortex-m3",
        "cortex-m4",
        "cortex-m7",
        "cortex-r52",
        "cortex-r52+",
    ] {
        let catalogue = Catalogue::builtin(cpu).unwrap();
        for register in &catalogue.registers {
            let dependencies = catalogue.read_dependencies(register).unwrap();
            let conditions: Vec<_> = dependencies.iter().flat_map(|r| &r.conditions).collect();
            if conditions.is_empty() {
                continue;
            }
            let mut facts: BTreeMap<_, _> =
                conditions.iter().map(|c| (c.fact.clone(), c.min)).collect();
            if cpu == "cortex-m4" || cpu == "cortex-m7" {
                // Effective observation domain, not Config::facts: declared capacities
                // alone must no longer prove the presence of the M floating-point bank.
                assert_eq!(
                    catalogue.implementation(register, &facts).0,
                    Implementation::Unknown
                );
                assert!(!catalogue.automatic_read(register, &facts));
                facts.insert(
                    crate::registers::policy::field_key("fpu.mvfr0", "SIMDReg"),
                    1,
                );
            }
            assert_eq!(
                catalogue.implementation(register, &facts).0,
                Implementation::Yes,
                "{cpu}: {}",
                register.id
            );
            assert_eq!(
                catalogue.implementation(register, &BTreeMap::new()).0,
                Implementation::Unknown
            );
            assert!(!catalogue.automatic_read(register, &BTreeMap::new()));
            for condition in conditions {
                let mut excluded = facts.clone();
                if let Some(value) = condition.min.checked_sub(1) {
                    excluded.insert(condition.fact.clone(), value);
                    assert_eq!(
                        catalogue.implementation(register, &excluded).0,
                        Implementation::No
                    );
                    assert!(!catalogue.automatic_read(register, &excluded));
                }
                if let Some(value) = condition.max.and_then(|n| n.checked_add(1)) {
                    excluded.insert(condition.fact.clone(), value);
                    assert_eq!(
                        catalogue.implementation(register, &excluded).0,
                        Implementation::No
                    );
                }
            }
        }
    }
}

#[test]
fn capability_evidence_keeps_zero_missing_exclusion_and_all_declared_conditions() {
    let mut catalogue = Catalogue::builtin("cortex-m4").unwrap();
    catalogue.registers[0].conditions = vec![
        Condition {
            fact: "count".into(),
            min: 1,
            max: Some(8),
        },
        Condition {
            fact: "optional".into(),
            min: 0,
            max: Some(1),
        },
    ];
    let evidence = catalogue.eligibility(
        &catalogue.registers[0],
        &BTreeMap::from([("count".into(), 0)]),
        None,
        &context(),
    );
    assert_eq!(evidence.implementation, Implementation::No);
    assert_eq!(evidence.conditions.len(), 2);
    assert_eq!(evidence.conditions[0].value, Some(0));
    assert_eq!(evidence.conditions[0].source, Source::Configuration);
    assert_eq!(evidence.conditions[1].value, None);
    assert_eq!(evidence.conditions[1].source, Source::Unknown);
    let missing =
        catalogue.eligibility(&catalogue.registers[0], &BTreeMap::new(), None, &context());
    assert_eq!(missing.implementation, Implementation::Unknown);
    let known = catalogue.eligibility(
        &catalogue.registers[0],
        &BTreeMap::from([("count".into(), 8), ("optional".into(), 0)]),
        None,
        &context(),
    );
    assert_eq!(known.implementation, Implementation::Yes);
    let saved = serde_json::to_value(&known).unwrap();
    assert_eq!(
        serde_json::to_value(serde_json::from_value::<Evidence>(saved.clone()).unwrap()).unwrap(),
        saved
    );
}

#[test]
fn observed_capability_basis_records_raw_sources_and_rejects_other_stop_core_frame_or_session() {
    let mut catalogue = Catalogue::builtin("cortex-m4").unwrap();
    catalogue.registers[0].conditions = vec![Condition {
        fact: "icc.physical.prebits".into(),
        min: 5,
        max: Some(5),
    }];
    let declared = BTreeMap::from([("icc.physical.prebits".into(), 7)]);
    let probe = probe();
    let evidence =
        catalogue.eligibility(&catalogue.registers[0], &declared, Some(&probe), &context());
    assert_eq!(evidence.implementation, Implementation::Yes);
    assert_eq!(evidence.conditions[0].configured_value, Some(7));
    assert_eq!(evidence.conditions[0].value, Some(5));
    assert_eq!(evidence.conditions[0].source, Source::Observation);
    let basis = evidence.probe.as_ref().unwrap();
    assert_eq!(
        basis.identity.as_ref().unwrap().model.as_deref(),
        Some("Cortex-R52")
    );
    let raw = basis
        .observations
        .iter()
        .find(|s| s.id == "icc_ctlr")
        .unwrap();
    assert_eq!(raw.raw.as_ref().unwrap().hex, "0x00000400");
    assert_eq!(raw.source, "openocd:aarch64 gic");
    assert_eq!(raw.timestamp_ms, 31);
    let access = raw.provenance.as_ref().unwrap().access.as_ref().unwrap();
    assert_eq!(access.context, context());
    assert_eq!(access.completed_ms, Some(32));
    assert_eq!(
        access.gic.as_ref().unwrap().physical_priority_bits(),
        Some(5)
    );
    let saved = serde_json::to_value(&evidence).unwrap();
    let restored: Evidence = serde_json::from_value(saved.clone()).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), saved);
    assert!(
        evidence
            .lines("Current")
            .iter()
            .any(|s| s.contains("GIC capacity proof") && s.contains("0x01000200"))
    );
    let mut old = sample(&context(), "r0", "0x80000001");
    old.eligibility = Some(evidence.clone());
    let mut failure = old.clone();
    failure.state = State::Unavailable;
    failure.eligibility = None;
    failure.context.generation += 1;
    failure.inherit_value_origin(&old);
    let Some(Retained::Known(original)) = &failure.last_value_eligibility else {
        panic!("Lost native capacity basis")
    };
    assert_eq!(serde_json::to_value(original).unwrap(), saved);
    let mut legacy = saved;
    for observation in legacy["probe"]["observations"].as_array_mut().unwrap() {
        observation.as_object_mut().unwrap().remove("provenance");
    }
    let legacy: Evidence = serde_json::from_value(legacy).unwrap();
    assert!(
        legacy
            .probe
            .unwrap()
            .observations
            .iter()
            .all(|o| o.provenance.is_none())
    );
    for change in ["core", "frame", "stop", "session"] {
        let mut current = context();
        match change {
            "core" => current.core = "core.0".into(),
            "frame" => current.frame = 1,
            "stop" => current.generation += 1,
            _ => current.session += 1,
        }
        let evidence =
            catalogue.eligibility(&catalogue.registers[0], &declared, Some(&probe), &current);
        assert_eq!(evidence.implementation, Implementation::No);
        assert_eq!(evidence.conditions[0].source, Source::Configuration);
        assert!(evidence.probe.is_none());
    }
}

#[test]
fn capability_probe_failure_keeps_unknown_and_does_not_publish_retained_raw_as_current_basis() {
    let mut catalogue = Catalogue::builtin("cortex-m4").unwrap();
    catalogue.registers[0].conditions = vec![Condition {
        fact: "icc.physical.prebits".into(),
        min: 5,
        max: None,
    }];
    let mut probe = probe();
    let failed = probe
        .samples
        .iter_mut()
        .find(|s| s.id == "icc_ctlr")
        .unwrap();
    failed.state = State::Unavailable;
    failed.reason = Reason::AccessRestricted;
    failed.detail = "fixture denied; old value retained".into();
    probe.decode();
    let evidence = catalogue.eligibility(
        &catalogue.registers[0],
        &BTreeMap::new(),
        Some(&probe),
        &context(),
    );
    assert_eq!(evidence.implementation, Implementation::Unknown);
    assert_eq!(evidence.conditions[0].source, Source::Unknown);
    let raw = evidence
        .probe
        .as_ref()
        .unwrap()
        .observations
        .iter()
        .find(|s| s.id == "icc_ctlr")
        .unwrap();
    assert_eq!(raw.reason, Reason::AccessRestricted);
    assert!(raw.raw.is_none());
    assert!(raw.provenance.is_none());
    assert!(raw.detail.contains("fixture denied"));
    assert_eq!(
        probe
            .samples
            .iter()
            .find(|s| s.id == "icc_ctlr")
            .unwrap()
            .value
            .as_ref()
            .unwrap()
            .hex,
        "0x00000400"
    );
}

#[test]
fn aliases_inherit_write_only_and_read_effect_policy_without_overriding_manual_intent() {
    let mut catalogue = Catalogue::builtin("cortex-m4").unwrap();
    let mut alias = catalogue.registers[0].clone();
    alias.id = "alias".into();
    alias.reader = Reader::Alias {
        source: catalogue.registers[0].id.clone(),
        offset: 0,
    };
    alias.writer = None;
    alias.write = None;
    catalogue.registers.push(alias);
    catalogue.registers[0].access = Access::Wo;
    assert_eq!(
        catalogue.read_policy(catalogue.register("alias").unwrap()),
        (false, false)
    );
    assert!(!catalogue.automatic_read(catalogue.register("alias").unwrap(), &BTreeMap::new()));
    catalogue.registers[0].access = Access::Ro;
    catalogue.registers[0].read_side_effect = true;
    assert_eq!(
        catalogue.read_policy(catalogue.register("alias").unwrap()),
        (true, true)
    );
    assert!(!catalogue.automatic_read(catalogue.register("alias").unwrap(), &BTreeMap::new()));
    catalogue.registers[0].read_side_effect = false;
    assert!(catalogue.automatic_read(catalogue.register("alias").unwrap(), &BTreeMap::new()));
}

#[test]
fn retained_condition_basis_keeps_original_evidence_across_repeated_failures_and_old_snapshots() {
    let mut catalogue = Catalogue::builtin("cortex-m4").unwrap();
    catalogue.registers[0].conditions = vec![Condition {
        fact: "count".into(),
        min: 1,
        max: None,
    }];
    let mut old = sample(&context(), "r0", "0x80000001");
    assert!(old.eligibility.is_none() && old.last_value_eligibility.is_none());
    let mut failure = old.clone();
    failure.inherit_value_origin(&old);
    assert!(matches!(
        failure.last_value_eligibility,
        Some(Retained::Unknown)
    ));
    old.eligibility = Some(catalogue.eligibility(
        &catalogue.registers[0],
        &BTreeMap::from([("count".into(), 1)]),
        None,
        &context(),
    ));
    failure.eligibility = Some(catalogue.eligibility(
        &catalogue.registers[0],
        &BTreeMap::from([("count".into(), 0)]),
        None,
        &context(),
    ));
    failure.inherit_value_origin(&old);
    let mut again = failure.clone();
    again.eligibility =
        Some(catalogue.eligibility(&catalogue.registers[0], &BTreeMap::new(), None, &context()));
    again.inherit_value_origin(&failure);
    let Some(Retained::Known(retained)) = again.last_value_eligibility else {
        panic!("Original basis lost")
    };
    assert_eq!(retained.conditions[0].value, Some(1));
    assert_eq!(retained.implementation, Implementation::Yes);
}
