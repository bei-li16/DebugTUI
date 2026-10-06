use super::*;
use serde_json::json;

#[test]
fn matrix_stale_probe_never_overrides_configuration_or_claims_current_evidence() {
    let mut inventory = report("cortex-r52");
    inventory
        .environment
        .registers
        .facts
        .insert("timer.present".into(), 0);
    inventory.probe = Some(capabilities::Probe {
        context: inventory.context.clone(),
        thread: "1".into(),
        identity: None,
        facts: BTreeMap::from([(
            "timer.present".into(),
            capabilities::Fact {
                value: 1,
                register: "id_pfr1".into(),
                source: "fixture".into(),
                detail: "".into(),
            },
        )]),
        samples: vec![],
        nvic: None,
        gdb_names: vec![],
        notes: vec![],
    });
    inventory.refresh();
    assert!(inventory.probe_current);
    assert_eq!(inventory.effective_facts["timer.present"], 1);
    assert_eq!(row(&inventory, "cntpct").support, Support::Unobserved);
    inventory.context.generation += 1;
    inventory.refresh();
    assert!(!inventory.probe_current);
    assert_eq!(inventory.effective_facts["timer.present"], 0);
    assert_eq!(
        row(&inventory, "cntpct").support,
        Support::ConditionsExcluded
    );
    assert!(
        inventory.probe.is_some(),
        "Retain the old probe as history, explicitly marked non-current"
    );
}

fn report(cpu: &str) -> Report {
    let mut report = Report {
        schema_version: 1,
        source: format!("builtin:{cpu}"),
        context: Context {
            session: 7,
            generation: 3,
            core: "core1".into(),
            frame: 0,
        },
        catalogue: Some(Catalogue::builtin(cpu).unwrap()),
        environment: Environment {
            registers: Config::default(),
            selected_core: Some("core1".into()),
            gdb_endpoint: "localhost:3331".into(),
            observed_gdb_endpoint: None,
            gdb_core_endpoints: BTreeMap::new(),
            channels: vec![],
            channels_source: "configuration".into(),
            target_state: "STOPPED".into(),
            access_fault: None,
        },
        probe: None,
        probe_current: false,
        effective_facts: BTreeMap::new(),
        fact_source: String::new(),
        observations: vec![],
        owner_generations: BTreeMap::new(),
        rows: vec![],
        categories: vec![],
        planned_classes: vec![],
        meaning: String::new(),
    };
    report.refresh();
    report
}
fn row<'a>(report: &'a Report, id: &str) -> &'a Row {
    report.rows.iter().find(|r| r.id == id).unwrap()
}
fn sample(report: &Report, id: &str) -> Sample {
    let reg = report.catalogue.as_ref().unwrap().register(id).unwrap();
    let mut sample: Sample = serde_json::from_value(json!({
        "id":id,"state":"valid","implementation":"unknown","reason":"unknown","detail":"",
        "value":{"bits":reg.bits,"hex":format!("0x{:0width$x}",u128::MAX >> (128-reg.bits),width=usize::from(reg.bits/4))},
        "owner":row(report,id).owner,"context":report.context,"timestamp_ms":22,"source":"fixture","view":"selected_frame",
        "provenance":{"acquisition":"catalogue","catalogue_reader":reg.reader,
        "access":{"route":{"kind":"gdb_register","endpoint":"localhost:3331","configured_endpoint":"localhost:3331","name":id,"index":0},
          "phase":"responded","command":"fixture","context":report.context,"timestamp_ms":20,"completed_ms":22}}
    })).unwrap();
    sample.value = Some(RawValue::from_integer(u128::MAX >> (128 - reg.bits), reg.bits).unwrap());
    sample
}

#[test]
fn matrix_declared_categories_widths_and_conditions_never_imply_observed_support() {
    for cpu in ["cortex-r52", "cortex-r52+"] {
        let mut inventory = report(cpu);
        let catalogue = inventory.catalogue.as_ref().unwrap();
        assert_eq!(inventory.rows.len(), catalogue.registers.len());
        assert_eq!(inventory.categories.len(), catalogue.groups.len());
        assert_eq!(inventory.planned_classes.len(), 12);
        let stm = inventory
            .planned_classes
            .iter()
            .find(|c| c.id == "stm")
            .unwrap();
        assert_eq!(stm.entries, 46);
        assert_eq!(stm.hardware_support, "unverified");
        assert!(
            inventory
                .planned_classes
                .iter()
                .all(|c| c.hardware_support == "unverified")
        );
        assert!(
            inventory
                .rows
                .iter()
                .all(|r| r.support != Support::ObservedValue)
        );
        for (id, bits) in [("r0", 32), ("d0", 64), ("q0", 128), ("cntpct", 64)] {
            assert_eq!(row(&inventory, id).bits, bits);
            assert!(!row(&inventory, id).state_conditions.is_empty());
        }
        assert_eq!(
            row(&inventory, "d0").implementation,
            Implementation::Unknown
        );
        inventory
            .environment
            .registers
            .facts
            .insert("vfp.d_registers".into(), 16);
        inventory.refresh();
        assert_eq!(row(&inventory, "d31").support, Support::ConditionsExcluded);
        assert_eq!(inventory.fact_source, "configuration");
        assert!(!inventory.probe_current);
        assert!(inventory.categories.iter().all(|c| c.observed_values == 0));
        let encoded = serde_json::to_value(&inventory).unwrap();
        let mut restored: Report = serde_json::from_value(encoded.clone()).unwrap();
        restored.refresh();
        assert_eq!(serde_json::to_value(restored).unwrap(), encoded);
    }
}

#[test]
fn matrix_routes_select_native_width_protocol_and_current_target_without_fallback() {
    let mut inventory = report("cortex-r52");
    assert_eq!(row(&inventory, "cntpct").plan.transport, "gdb");
    assert!(
        !row(&inventory, "cntpct")
            .state_conditions
            .iter()
            .any(|s| s.contains("frame 0"))
    );
    let config = &mut inventory.environment.registers;
    config.tcl_endpoint = "localhost:6666".into();
    config.targets.insert("core1".into(), "cpu1".into());
    config.cp15_command = "arm mrc".into();
    config.cp15_64_command = "aarch64 mrrc".into();
    config.timer_command = "aarch64 timer".into();
    config.pmu_command = "aarch64 pmu".into();
    config.gic_command = "aarch64 gic".into();
    config.vfp_command = "aarch64 vfp".into();
    config.banked_command = "aarch64 banked".into();
    inventory.refresh();
    for (id, command, protocol) in [
        ("cntpct", "aarch64 timer cntpct", timer::PROTOCOL),
        ("pmcr", "aarch64 pmu pmcr", pmu::PROTOCOL),
        ("icc_ctlr", "aarch64 gic icc_ctlr", gic::PROTOCOL),
        ("d0", "aarch64 vfp d0", vfp::PROTOCOL),
    ] {
        let plan = &row(&inventory, id).plan;
        assert_eq!(plan.operation, command);
        assert_eq!(plan.protocol.as_deref(), Some(protocol));
        assert_eq!(plan.target.as_deref(), Some("cpu1"));
        assert!(plan.available);
    }
    inventory.environment.access_fault = Some("Unknown target restoration".into());
    inventory.refresh();
    assert!(!row(&inventory, "cntpct").plan.available);
    assert_eq!(row(&inventory, "cntpct").plan.transport, "tcl");
    assert_eq!(row(&inventory, "r0").plan.transport, "gdb");
    inventory.environment.access_fault = None;
    inventory.environment.registers.targets.remove("core1");
    inventory.refresh();
    assert!(!row(&inventory, "d0").plan.available);
    assert!(row(&inventory, "d0").plan.target.is_none());
}

#[test]
fn matrix_current_receipts_and_retained_values_have_distinct_support_states() {
    let mut inventory = report("cortex-r52");
    let original = sample(&inventory, "r0");
    inventory.observations = vec![original.clone()];
    inventory.refresh();
    assert_eq!(row(&inventory, "r0").support, Support::ObservedValue);
    for fault in 0..5 {
        let mut changed = original.clone();
        match fault {
            0 => changed.provenance = None,
            1 => {
                changed
                    .provenance
                    .as_mut()
                    .unwrap()
                    .access
                    .as_mut()
                    .unwrap()
                    .phase = Phase::Started
            }
            2 => {
                changed
                    .provenance
                    .as_mut()
                    .unwrap()
                    .access
                    .as_mut()
                    .unwrap()
                    .completed_ms = Some(19)
            }
            3 => {
                changed
                    .provenance
                    .as_mut()
                    .unwrap()
                    .access
                    .as_mut()
                    .unwrap()
                    .context
                    .generation += 1
            }
            _ => changed.value.as_mut().unwrap().bits = 64,
        }
        inventory.observations = vec![changed];
        inventory.refresh();
        assert_eq!(row(&inventory, "r0").support, Support::UnprovenValue);
    }
    let mut failed = original.clone();
    failed.state = State::Error;
    failed.detail = "backend error".into();
    failed.inherit_value_origin(&original);
    inventory.observations = vec![failed];
    inventory.refresh();
    assert_eq!(row(&inventory, "r0").support, Support::Error);
    assert_eq!(inventory.observations[0].value, original.value);
    assert!(inventory.observations[0].last_value_provenance.is_some());
    inventory.environment.target_state = "RUNNING".into();
    inventory.refresh();
    assert_eq!(row(&inventory, "r0").support, Support::Stale);
    assert!(!row(&inventory, "r0").observation_context_current);
    inventory.environment.target_state = "STOPPED".into();
    inventory.context.frame = 1;
    inventory.refresh();
    assert_eq!(row(&inventory, "r0").support, Support::Stale);
}

#[test]
fn matrix_shared_owner_generations_require_acceptance_and_expire_independently() {
    let mut inventory = report("cortex-r52");
    inventory
        .environment
        .registers
        .topology
        .clusters
        .insert("core1".into(), "A".into());
    let catalogue = inventory.catalogue.as_mut().unwrap();
    let reg = catalogue
        .registers
        .iter_mut()
        .find(|r| r.id == "r0")
        .unwrap();
    reg.scope = Scope::Cluster;
    inventory.refresh();
    let mut observed = sample(&inventory, "r0");
    observed.view = SampleView::PhysicalCore;
    inventory.owner_generations.insert("cluster:A".into(), 4);
    inventory.observations = vec![observed.clone()];
    inventory.refresh();
    assert_eq!(row(&inventory, "r0").support, Support::Stale);
    observed.owner_generation = Some(4);
    inventory.observations = vec![observed];
    inventory.refresh();
    assert_eq!(row(&inventory, "r0").support, Support::ObservedValue);
    inventory.owner_generations.insert("cluster:A".into(), 5);
    inventory.refresh();
    assert_eq!(row(&inventory, "r0").support, Support::Stale);
    inventory.environment.registers.topology.clusters.clear();
    inventory.refresh();
    assert_eq!(row(&inventory, "r0").support, Support::UnknownOwner);
}

#[test]
fn matrix_mmio_requires_exact_owner_channel_and_records_non_atomic_full_width() {
    let mut inventory = report("cortex-r52");
    inventory.catalogue = Some(
        Catalogue::parse(
            r#"
version=1
cpu="fixture"
architecture="armv8-r-aarch32"
[[groups]]
id="bus"
name="Bus"
[[registers]]
id="wide"
name="Wide"
group="bus"
bits=64
access="ro"
reader={kind="mmio",component="device",offset=8,require_owner_mapping=true}
[[registers]]
id="slice"
name="Slice"
group="bus"
bits=16
access="ro"
reader={kind="alias",source="wide",offset=32}
"#,
        )
        .unwrap(),
    );
    let config = &mut inventory.environment.registers;
    config.mmio_probe = true;
    config.component_owners.insert(
        "device".into(),
        BTreeMap::from([(
            "core:core1".into(),
            Component {
                base: 0x1_0000_0000,
                channel: "ap".into(),
                little_endian: false,
            },
        )]),
    );
    inventory.environment.channels.push(MemoryAccess {
        id: "ap".into(),
        target: "ap1".into(),
        tcl_endpoint: "localhost:6667".into(),
        cores: vec!["core1".into()],
        ..Default::default()
    });
    inventory.refresh();
    let slice = row(&inventory, "slice");
    assert_eq!(slice.dependencies, vec!["slice", "wide"]);
    assert_eq!(slice.bits, 16);
    assert_eq!(slice.plan.address.as_deref(), Some("0x100000008"));
    let memory = slice.plan.memory.as_ref().unwrap();
    assert_eq!(memory.bus_width, Some(32));
    assert_eq!(memory.count, Some(2));
    assert!(!memory.atomic);
    assert_eq!(memory.byte_order, provenance::ByteOrder::Big);
    assert!(slice.state_conditions.iter().any(|c| c.contains("MMIO")));
    inventory.environment.selected_core = None;
    inventory.refresh();
    assert_eq!(row(&inventory, "wide").support, Support::RouteUnavailable);
    inventory.environment.selected_core = Some("core1".into());
    inventory
        .environment
        .registers
        .component_owners
        .get_mut("device")
        .unwrap()
        .get_mut("core:core1")
        .unwrap()
        .base = u64::MAX - 7;
    inventory.refresh();
    assert!(!row(&inventory, "wide").plan.available);
    inventory.context.core = "core0".into();
    inventory.refresh();
    assert!(!row(&inventory, "wide").plan.available);
}

#[test]
fn matrix_aliases_inherit_write_only_manual_and_missing_category_rules() {
    let mut inventory = report("cortex-r52");
    let catalogue = inventory.catalogue.as_mut().unwrap();
    let mut alias = catalogue.register("r0").unwrap().clone();
    alias.id = "slice".into();
    alias.bits = 16;
    alias.fields.clear();
    alias.writer = None;
    alias.write = None;
    alias.reader = Reader::Alias {
        source: "r0".into(),
        offset: 0,
    };
    catalogue.registers.push(alias);
    let reg = catalogue
        .registers
        .iter_mut()
        .find(|r| r.id == "r0")
        .unwrap();
    reg.read_side_effect = true;
    reg.access_condition = "Privileged only".into();
    inventory.refresh();
    assert!(row(&inventory, "slice").manual_only);
    assert!(!row(&inventory, "slice").automatic_eligible);
    assert!(
        row(&inventory, "slice")
            .state_conditions
            .iter()
            .any(|c| c == "r0: Privileged only")
    );
    inventory
        .catalogue
        .as_mut()
        .unwrap()
        .registers
        .iter_mut()
        .find(|r| r.id == "r0")
        .unwrap()
        .access = Access::Wo;
    inventory.refresh();
    assert_eq!(row(&inventory, "slice").support, Support::WriteOnly);
    inventory.catalogue = None;
    inventory.refresh();
    assert!(inventory.rows.is_empty() && inventory.categories.is_empty());
}
