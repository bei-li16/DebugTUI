use super::*;
use crate::registers::{
    Catalogue, Config, Implementation, RawValue, Reason,
    provenance::{Access, Provenance},
};

// Independent TRM addresses/values. Do not consume generator or Descriptor arrays.
const WORDS: &[(&str, &str, u64, u16, u64)] = &[
    ("ed_midr", "debug_external", 0xd00, 32, 0x411fd134),
    ("edcidr0", "debug_external", 0xff0, 32, 0x0d),
    ("edcidr1", "debug_external", 0xff4, 32, 0x90),
    ("edcidr2", "debug_external", 0xff8, 32, 5),
    ("edcidr3", "debug_external", 0xffc, 32, 0xb1),
    ("eddevaff0", "debug_external", 0xfa8, 32, 0x80000102),
    ("eddevaff1", "debug_external", 0xfac, 32, 0),
    ("eddfr_word0", "debug_external", 0xd28, 32, 0),
    ("eddfr_word1", "debug_external", 0xd2c, 32, 0x1070710f),
    ("gicd_iidr", "gicd", 8, 32, 0x0101443b),
    ("gicd_cidr0", "gicd", 0xfff0, 32, 0x0d),
    ("gicd_cidr1", "gicd", 0xfff4, 32, 0xf0),
    ("gicd_cidr2", "gicd", 0xfff8, 32, 5),
    ("gicd_cidr3", "gicd", 0xfffc, 32, 0xb1),
    ("gicd_typer", "gicd", 4, 32, 0x02480001),
    ("gicr_iidr", "gicr", 4, 32, 0x0101443b),
    ("gicr_cidr0", "gicr", 0xfff0, 32, 0x0d),
    ("gicr_cidr1", "gicr", 0xfff4, 32, 0xf0),
    ("gicr_cidr2", "gicr", 0xfff8, 32, 5),
    ("gicr_cidr3", "gicr", 0xfffc, 32, 0xb1),
    ("gicr_typer", "gicr", 8, 64, 0x0000000200000210),
];
fn fixture() -> Probe {
    let context = Context {
        session: 31,
        generation: 9,
        core: "logical.77".into(),
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
    for (component, after) in [
        ("debug_external", false),
        ("gicd", false),
        ("gicd", true),
        ("gicr", false),
        ("gicr", true),
        ("debug_external", true),
    ] {
        for &(id, c, offset, bits, raw) in WORDS.iter().filter(|w| w.1 == component) {
            let base = match c {
                "debug_external" => 0x40000000,
                "gicd" => 0x30000000,
                _ => 0x70000000,
            };
            let reader = Reader::Mmio {
                component: c.into(),
                offset,
                require_owner_mapping: true,
            };
            let access = Access {
                timer: None,
                pmu: None,
                gic: None,
                banked: None,
                route: Route::GdbMemory {
                    endpoint: Some("localhost:6330".into()),
                    configured_endpoint: "localhost:6330".into(),
                    address: format!("0x{:x}", base + offset),
                    bits,
                    byte_order: ByteOrder::Little,
                },
                phase: Phase::Responded,
                command: format!("-data-read-memory-bytes 0x{:x} {}", base + offset, bits / 8),
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
                value: Some(RawValue::from_integer(raw.into(), bits).unwrap()),
                owner: Some(if c == "gicd" {
                    "cluster:A".into()
                } else {
                    "core:logical.77".into()
                }),
                context: context.clone(),
                view: SampleView::PhysicalCore,
                owner_generation: (c == "gicd").then_some(5),
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
fn sample<'a>(p: &'a mut Probe, id: &str) -> &'a mut Sample {
    p.samples
        .iter_mut()
        .find(|s| s.id == format!("mmio_probe.{id}"))
        .unwrap()
}
fn set(p: &mut Probe, id: &str, n: u64) {
    for s in &mut p.samples {
        if s.id == format!("mmio_probe.{id}") || s.id == format!("mmio_probe.{id}.after") {
            s.value =
                Some(RawValue::from_integer(n.into(), s.value.as_ref().unwrap().bits).unwrap());
        }
    }
    p.decode();
}

#[test]
fn fresh_mmio_proof_decodes_independent_identity_capacity_and_shared_eligibility() {
    let p = fixture();
    assert_eq!(p.samples.len(), 42);
    assert_eq!(p.facts["debug_external.breakpoints"].value, 8);
    assert_eq!(p.facts["debug_external.watchpoints"].value, 8);
    assert_eq!(p.facts["debug_external.context_breakpoints"].value, 2);
    assert_eq!(p.facts["gicd.interrupts"].value, 64);
    assert_eq!(p.facts["gicr.target_id"].value, 2);
    assert!(p.identity.is_none());
    let c = Catalogue::builtin("cortex-r52").unwrap();
    let declared = [("gicd.interrupts".into(), 992)].into();
    let e = c.eligibility(
        c.register("gicd_isenabler1").unwrap(),
        &declared,
        Some(&p),
        &p.context,
    );
    assert_eq!(e.implementation, Implementation::Yes);
    assert!(
        e.conditions
            .iter()
            .any(|v| v.configured_value == Some(992) && v.value == Some(64))
    );
    let observations = &e.probe.as_ref().unwrap().observations;
    assert!(
        observations
            .iter()
            .any(|s| s.id == "mmio_probe.gicd_typer.after"
                && s.owner.as_deref() == Some("cluster:A")
                && s.owner_generation == Some(5))
    );
    assert!(
        e.lines("Current")
            .iter()
            .any(|l| l.contains("cluster:A") && l.contains("generation=5"))
    );
    let old: Config = toml::from_str("cpu='cortex-r52'").unwrap();
    assert!(!old.mmio_probe);
    assert!(
        serde_json::to_value(old)
            .unwrap()
            .get("mmio_probe")
            .is_none()
    );
}
#[test]
fn fresh_mmio_proof_checks_exact_r52_fields_without_absence_inference() {
    for lines in [1, 30] {
        let mut p = fixture();
        set(&mut p, "gicd_typer", 0x02480000 | lines);
        assert_eq!(p.facts["gicd.interrupts"].value, 32 * (lines + 1));
    }
    for (id, value, key) in [
        ("gicd_typer", 0x02480000, "gicd.present"),
        ("gicd_typer", 0x0248001f, "gicd.present"),
        ("gicd_typer", 0x00480001, "gicd.present"),
        ("gicd_iidr", 0x0100043b, "gicd.present"),
        ("gicd_cidr1", 0x90, "gicd.present"),
        ("gicr_typer", 0x0000000100000110, "gicr.present"),
        ("gicr_typer", 0x0000000200000110, "gicr.present"),
        ("gicr_typer", 0x0000000200000211, "gicr.present"),
        ("eddfr_word0", 1, "debug_external.present"),
        ("eddfr_word1", 0x10607100, "debug_external.present"),
        ("eddevaff0", 2, "gicd.present"),
        ("eddevaff1", 1, "gicr.present"),
        ("ed_midr", 0x411fd164, "gicd.present"),
    ] {
        let mut p = fixture();
        set(&mut p, id, value);
        assert!(!p.facts.contains_key(key), "{id}={value:x}");
        assert!(
            p.notes
                .iter()
                .any(|n| n.contains("physical capabilities remain unknown"))
        );
    }
    for low in [0, 1, 15] {
        let mut p = fixture();
        set(&mut p, "eddfr_word1", 0x10707100 | low);
        assert_eq!(p.facts["debug_external.breakpoints"].value, 8);
    }
}
#[test]
fn fresh_mmio_proof_rejects_stale_mixed_routes_owners_requests_and_duplicate_samples() {
    for mutation in 0..15 {
        let mut p = fixture();
        let s = sample(&mut p, "gicd_typer");
        match mutation {
            0 => s.stale(),
            1 => s.context.generation += 1,
            2 => s.owner = Some("core:logical.77".into()),
            3 => s.owner_generation = Some(6),
            4 => s.view = SampleView::SelectedFrame,
            5 => s.source = "gdb:gicd_typer".into(),
            6 => s.value.as_mut().unwrap().bits = 64,
            7 => s.provenance.as_mut().unwrap().acquisition = Acquisition::Catalogue,
            8 => {
                s.provenance.as_mut().unwrap().catalogue_reader = Reader::Mmio {
                    component: "gicd".into(),
                    offset: 8,
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
                s.provenance
                    .as_mut()
                    .unwrap()
                    .access
                    .as_mut()
                    .unwrap()
                    .context
                    .frame = 1
            }
            12 => {
                if let Route::GdbMemory { endpoint, .. } = &mut s
                    .provenance
                    .as_mut()
                    .unwrap()
                    .access
                    .as_mut()
                    .unwrap()
                    .route
                {
                    *endpoint = Some("other:6330".into());
                }
            }
            13 => {
                if let Route::GdbMemory { address, .. } = &mut s
                    .provenance
                    .as_mut()
                    .unwrap()
                    .access
                    .as_mut()
                    .unwrap()
                    .route
                {
                    *address = "0x31000004".into();
                }
            }
            _ => {
                s.provenance
                    .as_mut()
                    .unwrap()
                    .access
                    .as_mut()
                    .unwrap()
                    .timestamp_ms = 0
            }
        }
        p.decode();
        assert!(!p.facts.contains_key("gicd.present"), "mutation {mutation}");
        assert_eq!(p.facts["gicr.present"].value, 1);
    }
    let mut p = fixture();
    p.samples.push(p.samples[0].clone());
    p.decode();
    assert!(p.facts.is_empty());
    let mut p = fixture();
    sample(&mut p, "ed_midr").stale();
    p.decode();
    assert!(p.facts.is_empty());
}
#[test]
fn fresh_mmio_proof_checks_actual_ap_transport_and_cpu_identity_conflicts() {
    let mut p = fixture();
    for s in &mut p.samples {
        let a = s.provenance.as_mut().unwrap().access.as_mut().unwrap();
        if let Route::GdbMemory { address, bits, .. } = &a.route {
            a.route = Route::TclMemory {
                endpoint: "127.0.0.1:5000".into(),
                target: "ap.actual".into(),
                channel: "ap".into(),
                configuration_source: "project".into(),
                address: address.clone(),
                bits: *bits,
                bus_width: 32,
                count: *bits / 32,
                byte_order: ByteOrder::Big,
                atomic: false,
            };
        }
    }
    p.decode();
    assert_eq!(p.facts["gicr.target_id"].value, 2);
    for mutation in 0..4 {
        let mut p = p.clone();
        let a = sample(&mut p, "gicr_typer")
            .provenance
            .as_mut()
            .unwrap()
            .access
            .as_mut()
            .unwrap();
        if let Route::TclMemory {
            target,
            count,
            atomic,
            configuration_source,
            ..
        } = &mut a.route
        {
            match mutation {
                0 => target.clear(),
                1 => *count = 1,
                2 => *atomic = true,
                _ => configuration_source.clear(),
            }
        }
        p.decode();
        assert!(!p.facts.contains_key("gicr.present"));
        assert_eq!(p.facts["gicd.present"].value, 1);
    }
    let mut conflicting = p.samples[0].clone();
    conflicting.id = "midr".into();
    conflicting.value = Some(RawValue::from_integer(0x411fd135, 32).unwrap());
    p.samples.push(conflicting);
    p.decode();
    assert!(p.facts.is_empty());
}
