use super::*;
use crate::registers::{
    Catalogue, Implementation, RawValue, Reader, Reason, Sample, SampleView, State,
    provenance::{Access, Acquisition, ByteOrder, Phase, Provenance, Route},
};

// Independent IHI0054B/DDI0528B offsets and values, not Descriptor/generator data.
const WORDS: &[(&str, &str, u64, u64)] = &[
    ("stm_cidr0", "stm", 0xff0, 13),
    ("stm_cidr1", "stm", 0xff4, 0x90),
    ("stm_cidr2", "stm", 0xff8, 5),
    ("stm_cidr3", "stm", 0xffc, 0xb1),
    ("stm_devarch", "stm", 0xfbc, 0x47710a63),
    ("stm_devtype", "stm", 0xfcc, 0x63),
    ("stm_pidr0", "stm", 0xfe0, 0x63),
    ("stm_pidr1", "stm", 0xfe4, 0xb9),
    ("stm_pidr2", "stm", 0xfe8, 0x0b),
    ("stm_pidr3", "stm", 0xfec, 0),
    ("stm_pidr4", "stm", 0xfd0, 4),
    ("stm_devid", "stm", 0xfc8, 65536),
    ("stm_feat1r", "stm", 0xea0, 0x8240),
    ("stm_feat2r", "stm", 0xea4, 0x42),
    ("stm_feat3r", "stm", 0xea8, 127),
    ("stm_heidr", "stm_hwe", 0xdfc, 0x11),
    ("stm_hefeat1r", "stm_hwe", 0xdf8, 0x20200001),
    ("stm_dmaidr", "stm_dma", 0xcfc, 2),
];
fn fixture() -> Probe {
    let context = Context {
        session: 31,
        generation: 9,
        core: "cpu.3".into(),
        frame: 0,
    };
    let mut p = Probe {
        context: context.clone(),
        thread: "1".into(),
        identity: None,
        facts: BTreeMap::new(),
        samples: vec![],
        gdb_names: vec![],
        notes: vec![],
    };
    let mut clock = 10;
    for (name, after) in [
        ("stm", false),
        ("stm_hwe", false),
        ("stm_hwe", true),
        ("stm_dma", false),
        ("stm_dma", true),
        ("stm", true),
    ] {
        for &(id, c, off, value) in WORDS.iter().filter(|w| w.1 == name) {
            let reader = Reader::Mmio {
                component: c.into(),
                offset: off,
                require_owner_mapping: true,
            };
            let access = Access {
                timer: None,
                pmu: None,
                gic: None,
                banked: None,
                vfp: None,
                vfp_pair: None,
                route: Route::GdbMemory {
                    endpoint: Some("localhost:6433".into()),
                    configured_endpoint: "localhost:6433".into(),
                    address: format!("0x{:x}", 0x51000000 + off),
                    bits: 32,
                    byte_order: ByteOrder::Little,
                },
                phase: Phase::Responded,
                command: "independent memory request".into(),
                context: context.clone(),
                timestamp_ms: clock,
                completed_ms: Some(clock + 1),
            };
            p.samples.push(Sample {
                id: format!("mmio_probe.{id}{}", if after { ".after" } else { "" }),
                state: State::Valid,
                implementation: Implementation::Unknown,
                reason: Reason::Unknown,
                detail: String::new(),
                value: Some(RawValue::from_integer(value.into(), 32).unwrap()),
                owner: Some("chip:board".into()),
                context: context.clone(),
                view: SampleView::PhysicalCore,
                owner_generation: Some(7),
                provenance: Some(Provenance {
                    acquisition: Acquisition::CapabilityProbe,
                    catalogue_reader: reader,
                    access: Some(access),
                    aliases: vec![],
                }),
                last_value_provenance: None,
                eligibility: None,
                last_value_eligibility: None,
                timestamp_ms: clock,
                source: format!("mmio:{c}"),
            });
            clock += 2;
        }
    }
    p.decode();
    p
}
fn set(p: &mut Probe, id: &str, n: u64) {
    for s in &mut p.samples {
        if s.id == format!("mmio_probe.{id}") || s.id == format!("mmio_probe.{id}.after") {
            s.value = Some(RawValue::from_integer(n.into(), 32).unwrap());
        }
    }
    p.decode();
}
fn first(p: &mut Probe) -> &mut Sample {
    p.samples
        .iter_mut()
        .find(|s| s.id == "mmio_probe.stm_devarch")
        .unwrap()
}

#[test]
fn stm_component_is_independent_of_cpu_and_retains_exact_capacity_and_owner_evidence() {
    let p = fixture();
    assert!(p.identity.is_none());
    assert_eq!(p.samples.len(), 36);
    for (name, value) in [
        ("stm.present", 1),
        ("stm.part", 0x963),
        ("stm.stimulus_ports", 65536),
        ("stm.masters", 128),
        ("stm.sptrigger", 1),
        ("stm.sync", 1),
        ("stm_hwe.events", 64),
        ("stm_hwe.mux", 2),
        ("stm_dma.present", 1),
    ] {
        assert_eq!(p.facts[name].value, value, "{name}");
    }
    let c = Catalogue::builtin("cortex-r52").unwrap();
    let e = c.eligibility(
        c.register("stm_tcsr").unwrap(),
        &[("stm.part".into(), 0)].into(),
        Some(&p),
        &p.context,
    );
    assert_eq!(e.implementation, Implementation::Yes);
    assert!(
        e.probe
            .unwrap()
            .observations
            .iter()
            .any(|s| s.id == "mmio_probe.stm_devarch.after" && s.owner_generation == Some(7))
    );
}
#[test]
fn stm_identity_rejects_other_architectures_designers_classes_and_capacities() {
    for (id, n, key) in [
        ("stm_cidr1", 0xf0, "stm.present"),
        ("stm_devarch", 0x47700a63, "stm.present"),
        ("stm_devtype", 0x34, "stm.present"),
        ("stm_pidr1", 0x49, "stm.present"),
        ("stm_pidr2", 3, "stm.present"),
        ("stm_pidr4", 0x14, "stm.present"),
        ("stm_pidr0", 0x163, "stm.present"),
        ("stm_devid", 0, "stm.present"),
        ("stm_devid", 65537, "stm.present"),
        ("stm_heidr", 0x21, "stm_hwe.present"),
        ("stm_hefeat1r", 257 << 15, "stm_hwe.present"),
        ("stm_hefeat1r", 6 << 28, "stm_hwe.present"),
        ("stm_dmaidr", 0, "stm_dma.present"),
    ] {
        let mut p = fixture();
        set(&mut p, id, n);
        assert!(!p.facts.contains_key(key), "{id}={n:x}");
        assert!(
            p.notes
                .iter()
                .any(|s| s.contains("capabilities remain unknown"))
        );
    }
    let mut p = fixture();
    set(&mut p, "stm_heidr", 1);
    assert!(!p.facts.contains_key("stm_hwe.present"));
    set(&mut p, "stm_hefeat1r", 1);
    assert_eq!(p.facts["stm_hwe.present"].value, 1);
}
#[test]
fn stm_proof_rejects_stale_mixed_or_forged_observations() {
    for kind in 0..14 {
        let mut p = fixture();
        let s = first(&mut p);
        match kind {
            0 => s.stale(),
            1 => s.context.generation += 1,
            2 => s.owner = Some("core:cpu.3".into()),
            3 => s.owner_generation = Some(8),
            4 => s.view = SampleView::SelectedFrame,
            5 => s.source = "mmio:stm_hwe".into(),
            6 => s.value.as_mut().unwrap().bits = 64,
            7 => s.provenance.as_mut().unwrap().acquisition = Acquisition::Catalogue,
            8 => {
                s.provenance.as_mut().unwrap().catalogue_reader = Reader::Mmio {
                    component: "stm".into(),
                    offset: 0,
                    require_owner_mapping: true,
                }
            }
            9 => {
                s.provenance
                    .as_mut()
                    .unwrap()
                    .access
                    .as_mut()
                    .unwrap()
                    .phase = Phase::Started
            }
            10 => {
                s.provenance
                    .as_mut()
                    .unwrap()
                    .access
                    .as_mut()
                    .unwrap()
                    .completed_ms = None
            }
            11 => {
                if let Route::GdbMemory { endpoint, .. } = &mut s
                    .provenance
                    .as_mut()
                    .unwrap()
                    .access
                    .as_mut()
                    .unwrap()
                    .route
                {
                    *endpoint = Some("other:6433".into())
                }
            }
            12 => s.value = Some(RawValue::from_integer(0x47720a63, 32).unwrap()),
            13 => {
                let duplicate = s.clone();
                p.samples.push(duplicate);
            }
            _ => unreachable!(),
        }
        p.decode();
        assert!(!p.facts.contains_key("stm.present"), "mutation {kind}");
        assert!(!data_allowed(&p, "stm", 0xe80, 32));
    }
}
#[test]
fn stm_optional_proof_requires_same_aperture_owner_epoch_and_enclosing_requests() {
    for kind in 0..4 {
        let mut p = fixture();
        for s in p.samples.iter_mut().filter(|s| s.source == "mmio:stm_hwe") {
            match kind {
                0 => s.owner = Some("chip:other".into()),
                1 => s.owner_generation = Some(8),
                2 => {
                    if let Route::GdbMemory { address, .. } = &mut s
                        .provenance
                        .as_mut()
                        .unwrap()
                        .access
                        .as_mut()
                        .unwrap()
                        .route
                    {
                        let n = u64::from_str_radix(&address[2..], 16).unwrap();
                        *address = format!("0x{:x}", n + 0x1000);
                    }
                }
                3 => {
                    let a = s.provenance.as_mut().unwrap().access.as_mut().unwrap();
                    a.timestamp_ms += 1000;
                    a.completed_ms = a.completed_ms.map(|n| n + 1000);
                }
                _ => unreachable!(),
            }
        }
        p.decode();
        assert!(p.facts.contains_key("stm.present"));
        assert!(!p.facts.contains_key("stm_hwe.present"));
        assert!(p.facts.contains_key("stm_dma.present"));
    }
}
#[test]
fn stm_unknown_reserved_and_absent_features_cannot_enable_optional_reads() {
    for code in [0, 3] {
        let mut p = fixture();
        set(&mut p, "stm_feat2r", 0x40 | code);
        assert!(!p.facts.contains_key("stm.sptrigger"));
        assert!(!data_allowed(&p, "stm", 0xe20, 32));
    }
    let mut p = fixture();
    set(&mut p, "stm_feat1r", 0);
    assert!(!p.facts.contains_key("stm.sync"));
    assert!(!p.facts.contains_key("stm.trigger_control"));
    assert!(!data_allowed(&p, "stm", 0xe90, 32));
    set(&mut p, "stm_feat1r", 0xc100);
    assert_eq!(p.facts["stm.sync"].value, 0);
    assert!(!p.facts.contains_key("stm.trigger_control"));
    set(&mut p, "stm_feat2r", 5);
    assert_eq!(p.facts["stm.sper"].value, 0);
    assert_eq!(p.facts["stm.sptrigger"].value, 0);
    assert!(!data_allowed(&p, "stm", 0xe00, 32));
    assert!(!data_allowed(&p, "stm", 0xe68, 32));
    for events in [0, 1, 32, 33, 256] {
        let mut p = fixture();
        set(&mut p, "stm_hefeat1r", events << 15);
        assert_eq!(p.facts["stm_hwe.events"].value, events);
        assert_eq!(data_allowed(&p, "stm_hwe", 0xd60, 32), events > 32);
        assert!(!data_allowed(&p, "stm_hwe", 0xd20, 32));
        assert!(!data_allowed(&p, "stm_hwe", 0xd68, 32));
    }
}
#[test]
fn stm_stimulus_write_only_reserved_wide_and_unadapted_part_slots_are_rejected() {
    let mut p = fixture();
    for (name, offset) in [
        ("stm", 0),
        ("stm", 0xe84),
        ("stm", 0xfb0),
        ("stm", 0xef0),
        ("stm_dma", 0xc04),
        ("stm_dma", 0xc08),
        ("stm_hwe", 0xd04),
    ] {
        assert!(!data_allowed(&p, name, offset, 32));
    }
    assert!(!data_allowed(&p, "stm", 0xe80, 64));
    assert!(data_allowed(&p, "stm", 0xe80, 32));
    set(&mut p, "stm_pidr0", 0x64);
    assert_eq!(p.facts["stm.present"].value, 1);
    assert!(!data_allowed(&p, "stm", 0xe80, 32));
    assert!(data_allowed(&p, "stm", 0xea0, 32));
    assert!(data_allowed(&p, "stm", 0xfbc, 32));
    p.samples.retain(|s| s.source == "mmio:stm");
    p.decode();
    assert!(!p.facts.contains_key("stm_hwe.present"));
    assert!(!data_allowed(&p, "stm_dma", 0xc10, 32));
}
#[test]
fn stm_ap_proof_keeps_endianness_bus_geometry_and_current_binding() {
    let mut p = fixture();
    for s in &mut p.samples {
        let a = s.provenance.as_mut().unwrap().access.as_mut().unwrap();
        let Route::GdbMemory { address, .. } = &a.route else {
            panic!()
        };
        a.route = Route::TclMemory {
            endpoint: "localhost:6666".into(),
            target: "soc.ap".into(),
            channel: "ap".into(),
            configuration_source: "memory_access".into(),
            address: address.clone(),
            bits: 32,
            bus_width: 32,
            count: 1,
            byte_order: ByteOrder::Big,
            atomic: false,
        };
    }
    p.decode();
    let binding = Component {
        base: 0x51000000,
        channel: "ap".into(),
        little_endian: false,
    };
    assert!(applicable(
        &p,
        &p.context,
        "stm",
        Some("chip:board"),
        &binding
    ));
    let mut other = binding.clone();
    other.little_endian = true;
    assert!(!applicable(
        &p,
        &p.context,
        "stm",
        Some("chip:board"),
        &other
    ));
    other = binding.clone();
    other.base += 0x1000;
    assert!(!applicable(
        &p,
        &p.context,
        "stm",
        Some("chip:board"),
        &other
    ));
    if let Route::TclMemory { count, .. } = &mut first(&mut p)
        .provenance
        .as_mut()
        .unwrap()
        .access
        .as_mut()
        .unwrap()
        .route
    {
        *count = 2;
    }
    p.decode();
    assert!(!p.facts.contains_key("stm.present"));
}
#[test]
fn stm_builtins_declare_chip_scope_and_never_supply_control_writers() {
    for cpu in ["cortex-r52", "cortex-r52+"] {
        let c = Catalogue::builtin(cpu).unwrap();
        let entries: Vec<_> = c
            .registers
            .iter()
            .filter(|r| matches!(&r.reader,Reader::Mmio{component:c,..} if component(c)))
            .collect();
        assert_eq!(entries.len(), 46);
        for r in entries {
            assert_eq!(r.bits, 32);
            assert_eq!(r.scope, super::super::Scope::Chip);
            assert!(r.writer.is_none());
            if r.access == super::super::Access::Wo {
                let Reader::Mmio {
                    component, offset, ..
                } = &r.reader
                else {
                    panic!()
                };
                assert!(!readable(component, *offset, 32));
            }
        }
    }
}
