use super::*;
use serde_json::json;

#[test]
fn bounded_el2_preflight_delegates_only_el_and_preserves_alias_owner_enable_and_halt_rules() {
    let mut c = catalogue();
    c.registers[1].reader = Reader::Cp15 {
        cp: 15,
        op1: 4,
        crn: 1,
        crm: 1,
        op2: 0,
    };
    c.registers[1].scope = Scope::Core;
    c.registers[1].access_rule.min_el = Some(2);
    let facts = BTreeMap::from([(field_key("identity", "ENABLE"), 1)]);
    let alias = c.register("alias").unwrap();
    assert_eq!(
        c.access_denial(alias, &facts, true, None).unwrap().0,
        Reason::Unknown
    );
    assert!(c.checked_el2_backend_denial(alias, &facts, true).is_none());
    assert_eq!(
        c.checked_el2_backend_denial(alias, &BTreeMap::new(), true)
            .unwrap()
            .0,
        Reason::Unknown
    );
    assert_eq!(
        c.checked_el2_backend_denial(
            alias,
            &BTreeMap::from([(field_key("identity", "ENABLE"), 0)]),
            true
        )
        .unwrap()
        .0,
        Reason::FeatureDisabled
    );
    c.registers[1].scope = Scope::Unknown;
    assert!(
        c.checked_el2_backend_denial(&c.registers[2], &facts, true)
            .unwrap()
            .1
            .contains("owner")
    );
    c.registers[1].scope = Scope::Core;
    c.registers[1].access_rule.need_halt = Some(true);
    assert!(
        c.checked_el2_backend_denial(&c.registers[2], &facts, false)
            .unwrap()
            .1
            .contains("NeedHalt")
    );
    c.registers[1].access_rule.min_el = Some(3);
    assert_eq!(
        c.checked_el2_backend_denial(&c.registers[2], &facts, true)
            .unwrap()
            .0,
        Reason::AccessRestricted
    );
}

#[test]
fn dependency_depth_is_checked_even_when_parents_were_already_visited() {
    let mut c = catalogue();
    let source = c.registers[0].clone();
    c.registers = (0..66)
        .map(|index| {
            let mut register = source.clone();
            register.id = format!("word{index}");
            if index > 0 {
                register.present_if = Some(FieldCondition {
                    reg: format!("word{}", index - 1),
                    field: "ENABLE".into(),
                    op: Compare::Eq,
                    value: 1,
                });
            }
            register
        })
        .collect();
    assert!(c.validate().unwrap_err().contains("cycle/depth"));
    c.registers.truncate(64);
    c.validate().unwrap();
}

#[test]
fn core_private_gdb_requires_known_dedicated_worker_endpoint() {
    let mut cores = BTreeMap::from([
        ("core.0".into(), "localhost:3333".into()),
        ("core.2".into(), "localhost:3334".into()),
    ]);
    assert!(
        core_private::gdb_endpoint("localhost:3334", Some("localhost:3334"), &cores, "core.2")
            .is_ok()
    );
    assert!(core_private::gdb_endpoint("localhost:3334", None, &cores, "core.2").is_err());
    assert!(
        core_private::gdb_endpoint("localhost:3333", Some("localhost:3333"), &cores, "core.2")
            .is_err()
    );
    cores.insert("core.0".into(), "localhost:3334".into());
    assert!(
        core_private::gdb_endpoint("localhost:3334", Some("localhost:3334"), &cores, "core.2")
            .is_err()
    );
}

fn context() -> Context {
    Context {
        session: 3,
        generation: 9,
        core: "core.2".into(),
        frame: 0,
    }
}
fn catalogue() -> Catalogue {
    Catalogue::parse(
        r#"
version=1
cpu='test-m'
architecture='armv7m'
[[groups]]
id='system'
name='System'
[[registers]]
id='identity'
name='Identity'
group='system'
bits=32
access='ro'
reader={kind='core_private',address=0xe000ed00}
fields=[{name='COUNT',segments=[{offset=8,width=8}]},{name='ENABLE',segments=[{offset=4,width=1}]}]
[[registers]]
id='optional'
name='Optional'
group='system'
bits=32
access='ro'
reader={kind='core_private',address=0xe000ed90}
present_if={reg='identity',field='COUNT',op='ge',value=4}
access_rule={need_halt=false,need_enable={reg='identity',field='ENABLE',op='eq',value=1}}
[[registers]]
id='alias'
name='Alias'
group='system'
bits=16
access='ro'
reader={kind='alias',source='optional',offset=0}
access_rule={need_halt=false}
"#,
    )
    .unwrap()
}
fn sample(catalogue: &Catalogue, raw: u32) -> Sample {
    let context = context();
    let reader = &catalogue.register("identity").unwrap().reader;
    serde_json::from_value(json!({"id":"identity","state":"valid","implementation":"unknown","reason":"unknown","detail":"",
        "value":RawValue::from_integer(u128::from(raw),32).unwrap(),"owner":"core:core.2","context":context,"timestamp_ms":10,
        "source":"mmio:ppb","view":"physical_core","provenance":{"acquisition":"catalogue","catalogue_reader":reader,
        "access":{"route":{"kind":"tcl_memory","endpoint":"localhost:6666","target":"soc.m4","channel":"m4","configuration_source":"fixture",
            "address":"0xe000ed00","bits":32,"bus_width":32,"count":1,"byte_order":"little","atomic":false},
            "phase":"responded","command":"soc.m4 read_memory 0xe000ed00 32 1","context":context,"timestamp_ms":9,"completed_ms":10}}})).unwrap()
}

#[test]
fn finite_comparisons_and_widths_preserve_zero_boundaries() {
    for (op, expected) in [
        (Compare::Eq, false),
        (Compare::Ne, true),
        (Compare::Lt, true),
        (Compare::Le, true),
        (Compare::Gt, false),
        (Compare::Ge, false),
    ] {
        assert_eq!(op.matches(0, 1), expected);
    }
    assert!(Compare::Ge.matches(u64::MAX, u64::MAX));
    assert!(!Compare::Gt.matches(u64::MAX, u64::MAX));
    assert_ne!(field_key("a.b", "c"), field_key("a", "b.c"));
    let mut cfg = Config::default();
    cfg.facts.insert(field_key("identity", "ENABLE"), 1);
    assert!(
        cfg.validate().is_err(),
        "configuration must not forge field observations"
    );
}

#[test]
fn presence_enable_and_alias_use_only_current_physical_response_evidence() {
    let c = catalogue();
    let alias = c.register("alias").unwrap();
    let mut configured = BTreeMap::from([
        (field_key("identity", "COUNT"), 8),
        (field_key("identity", "ENABLE"), 1),
    ]);
    let facts = c.observation_facts(&configured, None, &[], &context());
    assert!(facts.is_empty());
    assert_eq!(c.implementation(alias, &facts).0, Implementation::Unknown);
    assert!(!c.automatic_read(alias, &facts));
    let observed = sample(&c, 0x410);
    let facts = c.observation_facts(
        &configured,
        None,
        std::slice::from_ref(&observed),
        &context(),
    );
    assert_eq!(c.implementation(alias, &facts).0, Implementation::Yes);
    assert!(c.access_denial(alias, &facts, false, None).is_none());
    let evidence = c.eligibility_with_samples(
        alias,
        &configured,
        None,
        std::slice::from_ref(&observed),
        &context(),
    );
    assert_eq!(evidence.field_conditions.len(), 2);
    assert!(
        evidence
            .field_conditions
            .iter()
            .all(|e| matches!(e.source, eligibility::Source::Observation))
    );
    let retained = evidence.field_conditions[0].observation.as_ref().unwrap();
    assert_eq!(retained.source, "mmio:ppb");
    assert_eq!(retained.owner.as_deref(), Some("core:core.2"));
    assert_eq!(
        retained
            .provenance
            .as_ref()
            .unwrap()
            .access
            .as_ref()
            .unwrap()
            .command,
        "soc.m4 read_memory 0xe000ed00 32 1"
    );
    assert!(
        evidence
            .lines("Decision")
            .iter()
            .any(|line| line.contains("identity.COUNT Ge 4"))
    );
    let facts = c.observation_facts(&configured, None, &[sample(&c, 0x400)], &context());
    assert_eq!(
        c.access_denial(alias, &facts, true, None).unwrap().0,
        Reason::FeatureDisabled
    );
    let facts = c.observation_facts(&configured, None, &[sample(&c, 0x310)], &context());
    assert_eq!(c.implementation(alias, &facts).0, Implementation::No);
    for damage in 0..9 {
        let mut bad = observed.clone();
        match damage {
            0 => bad.state = State::Stale,
            1 => bad.context.generation += 1,
            2 => bad.owner = Some("core:core.0".into()),
            3 => bad.view = SampleView::SelectedFrame,
            4 => {
                bad.provenance
                    .as_mut()
                    .unwrap()
                    .access
                    .as_mut()
                    .unwrap()
                    .phase = Phase::Planned
            }
            5 => {
                bad.provenance
                    .as_mut()
                    .unwrap()
                    .access
                    .as_mut()
                    .unwrap()
                    .context
                    .session += 1
            }
            6 => {
                bad.provenance
                    .as_mut()
                    .unwrap()
                    .access
                    .as_mut()
                    .unwrap()
                    .completed_ms = None
            }
            7 => {
                if let Route::TclMemory { address, .. } = &mut bad
                    .provenance
                    .as_mut()
                    .unwrap()
                    .access
                    .as_mut()
                    .unwrap()
                    .route
                {
                    *address = "0xe000ed04".into();
                }
            }
            _ => bad.value.as_mut().unwrap().bits = 16,
        }
        configured.insert("declared.count".into(), 12);
        let facts = c.observation_facts(&configured, None, &[bad], &context());
        assert_eq!(facts.len(), 1, "damage {damage}");
        assert_eq!(c.implementation(alias, &facts).0, Implementation::Unknown);
    }
}

#[test]
fn failure_supersedes_same_context_probe_fields_and_debug_el_is_not_saved_cpsr() {
    let mut c = catalogue();
    let observed = sample(&c, 0x410);
    let probe = capabilities::Probe {
        context: context(),
        thread: "1".into(),
        identity: None,
        facts: BTreeMap::new(),
        samples: vec![observed.clone()],
        nvic: None,
        gdb_names: vec![],
        notes: vec![],
    };
    let mut failed = observed.clone();
    failed.state = State::Error;
    failed.value = None;
    let facts = c.observation_facts(&BTreeMap::new(), Some(&probe), &[failed], &context());
    assert!(facts.is_empty());
    let mut older_failure = observed.clone();
    older_failure.timestamp_ms = 1;
    older_failure.state = State::Error;
    older_failure.value = None;
    assert!(
        !c.observation_facts(&BTreeMap::new(), Some(&probe), &[older_failure], &context())
            .is_empty()
    );
    c.registers[1].access_rule.min_el = Some(2);
    let facts = c.observation_facts(&BTreeMap::new(), None, &[observed], &context());
    let r = c.register("alias").unwrap();
    assert_eq!(
        c.access_denial(r, &facts, true, None).unwrap().0,
        Reason::Unknown
    );
    assert_eq!(
        c.access_denial(r, &facts, true, Some(1)).unwrap().0,
        Reason::AccessRestricted
    );
    assert!(c.access_denial(r, &facts, true, Some(2)).is_none());
    assert!(
        c.access_denial(c.register("identity").unwrap(), &facts, false, Some(2))
            .unwrap()
            .1
            .contains("NeedHalt")
    );
}

#[test]
fn structured_shared_owner_fields_require_explicit_matching_topology() {
    for (scope, owner) in [(Scope::Cluster, "cluster:pair"), (Scope::Chip, "chip:soc")] {
        let mut c = catalogue();
        for register in &mut c.registers {
            register.scope = scope;
            if !matches!(register.reader, Reader::Alias { .. }) {
                register.reader = Reader::Mmio {
                    component: "fixture".into(),
                    offset: 0,
                    require_owner_mapping: true,
                };
            }
        }
        c.validate().unwrap();
        let mut s = sample(&c, 0x410);
        s.owner = Some(owner.into());
        let topology = Topology {
            chip: "soc".into(),
            clusters: BTreeMap::from([("core.2".into(), "pair".into())]),
        };
        assert!(
            c.observation_facts(&BTreeMap::new(), None, &[s.clone()], &context())
                .is_empty()
        );
        let facts = c.observation_facts_for_owners(
            &BTreeMap::new(),
            None,
            &[s.clone()],
            &context(),
            &topology,
        );
        assert_eq!(
            c.implementation(c.register("alias").unwrap(), &facts).0,
            Implementation::Yes
        );
        s.owner = Some("cluster:other".into());
        assert!(
            c.observation_facts_for_owners(&BTreeMap::new(), None, &[s], &context(), &topology)
                .is_empty()
        );
    }
}

#[test]
fn malformed_policy_graphs_and_unknown_scope_fail_without_guessing() {
    let original = catalogue();
    for damage in 0..8 {
        let mut c = original.clone();
        match damage {
            0 => c.registers[1].present_if.as_mut().unwrap().field = "MISSING".into(),
            1 => c.registers[1].present_if.as_mut().unwrap().value = 256,
            2 => c.registers[0].read_side_effect = true,
            3 => c.registers[0].scope = Scope::Chip,
            4 => c.registers[1].access_rule.min_el = Some(4),
            5 => {
                c.registers[0].present_if = Some(FieldCondition {
                    reg: "optional".into(),
                    field: "BIT".into(),
                    op: Compare::Eq,
                    value: 1,
                })
            }
            6 => {
                c.registers[1].reader = Reader::Gdb { name: "r1".into() };
            }
            _ => {
                c.registers[0].reader = Reader::CorePrivate {
                    address: 0xe0100000,
                }
            }
        }
        if damage == 5 {
            c.registers[1].fields = vec![Field {
                name: "BIT".into(),
                description: String::new(),
                segments: vec![Segment {
                    offset: 0,
                    width: 1,
                }],
                access: None,
                enums: vec![],
            }];
        }
        assert!(c.validate().is_err(), "damage {damage}");
    }
    let mut c = original;
    c.registers.clear();
    let mut unknown = Catalogue::builtin("cortex-m4").unwrap().registers[0].clone();
    unknown.scope = Scope::Unknown;
    unknown.writer = None;
    unknown.write = None;
    unknown.group = "system".into();
    c.registers.push(unknown);
    c.validate().unwrap();
    assert_eq!(Topology::default().owner(Scope::Unknown, "core.2"), None);
    assert!(
        c.access_denial(&c.registers[0], &BTreeMap::new(), true, Some(2))
            .unwrap()
            .1
            .contains("unknown")
    );
    assert_eq!(
        Catalogue::parse(&toml::to_string(&c).unwrap())
            .unwrap()
            .registers[0]
            .scope,
        Scope::Unknown
    );
}

#[test]
fn core_private_never_uses_shared_or_another_cores_target() {
    use crate::config::MemoryAccess;
    let mut config = Config::default();
    config.components.insert(
        "ppb".into(),
        Component {
            base: 0,
            channel: "m4".into(),
            little_endian: true,
        },
    );
    let mut channels = vec![MemoryAccess {
        id: "m4".into(),
        label: "M4".into(),
        tcl_endpoint: "localhost:6666".into(),
        target: "soc.m4".into(),
        cores: vec!["core.2".into()],
        while_running: false,
    }];
    assert!(core_private::binding(&config, &channels, "core.2").is_err());
    config.component_owners.insert(
        "ppb".into(),
        BTreeMap::from([("core:core.2".into(), config.components["ppb"].clone())]),
    );
    assert!(core_private::binding(&config, &channels, "core.2").is_ok());
    assert!(core_private::binding(&config, &channels, "core.0").is_err());
    for cores in [
        vec![],
        vec!["core.0".into()],
        vec!["core.2".into(), "core.0".into()],
    ] {
        channels[0].cores = cores;
        assert!(core_private::binding(&config, &channels, "core.2").is_err());
    }
    channels[0].cores = vec!["core.2".into()];
    config.targets.insert("core.2".into(), "soc.m7".into());
    assert!(core_private::binding(&config, &channels, "core.2").is_err());
    config.targets.insert("core.2".into(), "soc.m4".into());
    let mut peer = channels[0].clone();
    peer.id = "m7".into();
    peer.cores = vec!["core.0".into()];
    channels.push(peer);
    assert!(core_private::binding(&config, &channels, "core.2").is_err());
}
