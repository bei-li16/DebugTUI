use super::*;
use crate::registers::{
    Implementation, RawValue,
    policy::field_key,
    provenance::{Access, Acquisition, ByteOrder, Provenance},
};

fn context() -> Context {
    Context {
        session: 19,
        generation: 3,
        core: "m4".into(),
        frame: 0,
    }
}
fn sample(catalogue: &Catalogue, id: &str, value: u64) -> Sample {
    let register = catalogue.register(id).unwrap();
    let Reader::CorePrivate { address } = register.reader else {
        panic!("PPB ID expected")
    };
    Sample {
        id: id.into(),
        state: State::Valid,
        implementation: Implementation::Unknown,
        reason: Reason::Unknown,
        detail: String::new(),
        value: Some(RawValue::from_integer(value.into(), 32).unwrap()),
        owner: Some("core:m4".into()),
        context: context(),
        view: SampleView::PhysicalCore,
        owner_generation: None,
        timestamp_ms: 10,
        source: "mmio:ppb".into(),
        provenance: Some(Provenance {
            acquisition: Acquisition::CapabilityProbe,
            catalogue_reader: register.reader.clone(),
            aliases: vec![],
            access: Some(Access {
                banked: None,
                vfp: None,
                vfp_pair: None,
                timer: None,
                pmu: None,
                gic: None,
                route: Route::GdbMemory {
                    endpoint: Some("localhost:4901".into()),
                    configured_endpoint: "localhost:4901".into(),
                    address: format!("0x{address:x}"),
                    bits: 32,
                    byte_order: ByteOrder::Little,
                },
                phase: Phase::Responded,
                command: format!("-data-read-memory-bytes 0x{address:x} 4"),
                context: context(),
                timestamp_ms: 10,
                completed_ms: Some(12),
            }),
        }),
        last_value_provenance: None,
        eligibility: None,
        last_value_eligibility: None,
    }
}
fn probe(catalogue: &Catalogue, cpuid: u64) -> Probe {
    Probe {
        context: context(),
        thread: "1".into(),
        identity: None,
        facts: BTreeMap::new(),
        samples: vec![sample(catalogue, "scb.cpuid", cpuid)],
        nvic: None,
        gdb_names: vec![],
        notes: vec![],
    }
}

#[test]
fn m_cpacr_permission_fields_preserve_reserved_values_and_have_exact_manual_sources() {
    for (cpu, page) in [("cortex-m4", 264), ("cortex-m7", 287)] {
        let c = Catalogue::builtin(cpu).unwrap();
        let register = c.register("scb.cpacr").unwrap();
        assert!(register.writer.is_none());
        assert_eq!(register.source.as_ref().unwrap().page, Some(page));
        for cp10 in 0..4_u128 {
            for cp11 in 0..4_u128 {
                let raw = RawValue::from_integer((cp10 << 20) | (cp11 << 22), 32).unwrap();
                let fields = &register.fields;
                assert_eq!(
                    fields
                        .iter()
                        .find(|f| f.name == "CP10")
                        .unwrap()
                        .extract(&raw)
                        .unwrap()
                        .integer()
                        .unwrap(),
                    cp10
                );
                let f = fields.iter().find(|f| f.name == "CP11").unwrap();
                assert_eq!(f.extract(&raw).unwrap().integer().unwrap(), cp11);
                assert_eq!(f.enums.len(), 4);
                assert_eq!(f.enums[2].name, "ReservedUnpredictable");
            }
        }
        // GDB's external regfile read does not execute an FP instruction through CPACR.
        assert!(c.register("d0").unwrap().access_rule.need_enable.is_none());
    }
}

#[test]
fn enabled_dwt_zero_count_is_known_and_disabling_revokes_the_capacity() {
    let c = Catalogue::builtin("cortex-m4").unwrap();
    let mut p = probe(&c, 0x410fc241);
    p.samples
        .extend([sample(&c, "dcb.demcr", 1 << 24), sample(&c, "dwt.ctrl", 0)]);
    decode(&mut p, &c);
    assert_eq!(p.facts["dwt.comparators"].value, 0);
    let mut disabled = sample(&c, "dcb.demcr", 0);
    disabled.timestamp_ms += 1;
    p.samples.push(disabled);
    decode(&mut p, &c);
    assert!(!p.facts.contains_key("dwt.comparators"));
    assert_eq!(p.facts["dwt.enabled"].value, 0);
}

#[test]
fn m_identity_and_id_ranges_supply_actual_dynamic_capacities() {
    for (cpu, cpuid, regions) in [
        ("cortex-m3", 0x412fc231, 0),
        ("cortex-m4", 0x410fc241, 8),
        ("cortex-m7", 0x411fc271, 16),
    ] {
        let c = Catalogue::builtin(cpu).unwrap();
        let mut p = probe(&c, cpuid);
        p.samples.extend([
            sample(&c, "scs.ictr", 7),
            sample(&c, "mpu.type", regions << 8),
            sample(&c, "dcb.demcr", 1 << 24),
            sample(&c, "dwt.ctrl", 4 << 28),
            sample(&c, "fpb.ctrl", 0x100052a1),
        ]);
        decode(&mut p, &c);
        assert_eq!(
            p.identity.as_ref().unwrap().part,
            ((cpuid >> 4) & 0xfff) as u16
        );
        assert_eq!(p.facts["nvic.banks"].value, 8);
        assert_eq!(p.facts["nvic.lines_upper_bound"].value, 256);
        assert_eq!(p.facts["mpu.regions"].value, regions);
        assert_eq!(p.facts["mpu.present"].value, u64::from(regions > 0));
        assert_eq!(p.facts["dwt.comparators"].value, 4);
        assert_eq!(p.facts["fpb.code_comparators"].value, 90);
        assert_eq!(p.facts["fpb.literal_comparators"].value, 2);
        assert!(
            p.report_lines()
                .iter()
                .any(|s| s.contains("decoded from CPUID"))
        );
    }
}

#[test]
fn unadapted_m_identity_invalid_capacity_and_unenabled_modules_stay_unknown() {
    let c = Catalogue::builtin("cortex-m4").unwrap();
    for cpuid in [0, u32::MAX.into(), 0x410fc271, 0x410ec241, 0x420fc241] {
        let mut p = probe(&c, cpuid);
        p.samples.push(sample(&c, "mpu.type", 0x800));
        decode(&mut p, &c);
        assert!(p.facts.is_empty());
        assert!(probe_denial(&p, &c, "mpu.type").is_some());
    }
    let mut p = probe(&c, 0x410fc241);
    p.samples.extend([
        sample(&c, "scs.ictr", 15),
        sample(&c, "mpu.type", 0x1000),
        sample(&c, "dcb.demcr", 0),
        sample(&c, "dwt.ctrl", 0xf0000000),
        sample(&c, "fpb.ctrl", 0xf0000000),
        sample(&c, "fpu.mvfr0", 0),
    ]);
    decode(&mut p, &c);
    assert_eq!(p.facts.len(), 1);
    assert_eq!(p.facts["dwt.enabled"].value, 0);
    assert_eq!(
        probe_denial(&p, &c, "dwt.ctrl").unwrap().0,
        Reason::FeatureDisabled
    );
    let facts = c.observation_facts(&BTreeMap::new(), Some(&p), &[], &context());
    assert!(!facts.contains_key(&field_key("mpu.type", "DREGION")));
    assert!(!facts.contains_key(&field_key("scs.ictr", "INTLINESNUM")));
    assert!(!facts.contains_key(&field_key("fpu.mvfr0", "SIMDReg")));
    assert_eq!(
        c.implementation(c.register("mpu.ctrl").unwrap(), &facts).0,
        Implementation::Unknown
    );
    assert_eq!(
        c.implementation(c.register("nvic.iser0").unwrap(), &facts)
            .0,
        Implementation::Unknown
    );
}

#[test]
fn m_id_requires_physical_exact_completed_source_and_latest_failure_revokes_facts() {
    let c = Catalogue::builtin("cortex-m4").unwrap();
    for kind in 0..10 {
        let mut p = probe(&c, 0x410fc241);
        let s = &mut p.samples[0];
        match kind {
            0 => s.owner = Some("core:other".into()),
            1 => s.context.generation += 1,
            2 => s.view = SampleView::SelectedFrame,
            3 => s.value = Some(RawValue::from_integer(0x410fc241, 64).unwrap()),
            4 => {
                s.provenance
                    .as_mut()
                    .unwrap()
                    .access
                    .as_mut()
                    .unwrap()
                    .phase = Phase::Started
            }
            5 => {
                s.provenance
                    .as_mut()
                    .unwrap()
                    .access
                    .as_mut()
                    .unwrap()
                    .completed_ms = None
            }
            6 => {
                s.provenance.as_mut().unwrap().catalogue_reader = Reader::CorePrivate {
                    address: 0xe000ed90,
                }
            }
            7 => {
                if let Route::GdbMemory { endpoint, .. } = &mut s
                    .provenance
                    .as_mut()
                    .unwrap()
                    .access
                    .as_mut()
                    .unwrap()
                    .route
                {
                    *endpoint = None
                }
            }
            8 => {
                if let Route::GdbMemory { address, .. } = &mut s
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
            9 => {
                if let Route::GdbMemory {
                    configured_endpoint,
                    ..
                } = &mut s
                    .provenance
                    .as_mut()
                    .unwrap()
                    .access
                    .as_mut()
                    .unwrap()
                    .route
                {
                    *configured_endpoint = "localhost:wrong".into();
                }
            }
            _ => unreachable!(),
        }
        decode(&mut p, &c);
        assert!(p.identity.is_none(), "kind={kind}");
    }
    let mut p = probe(&c, 0x410fc241);
    p.samples.push(sample(&c, "mpu.type", 0x800));
    decode(&mut p, &c);
    let configured = BTreeMap::from([("mpu.regions".into(), 255), ("vfp.present".into(), 1)]);
    let facts = c.observation_facts(&configured, Some(&p), &[], &context());
    assert_eq!(facts["mpu.regions"], 8);
    assert!(!facts.contains_key("vfp.present"));
    let mut failed = p.samples[1].clone();
    failed.timestamp_ms = 20;
    failed.state = State::Error;
    failed.value = None;
    let facts = c.observation_facts(&configured, Some(&p), &[failed], &context());
    assert!(!facts.contains_key("mpu.regions"));
    assert!(!facts.contains_key(&field_key("mpu.type", "DREGION")));
    let mut other = context();
    other.generation += 1;
    assert!(
        c.observation_facts(&configured, Some(&p), &[], &other)
            .is_empty()
    );
}

#[test]
fn m_id_accepts_explicit_completed_tcl_memory_source_and_rejects_partial_words() {
    let c = Catalogue::builtin("cortex-m4").unwrap();
    let mut p = probe(&c, 0x410fc241);
    p.samples.push(sample(&c, "mpu.type", 0x800));
    for s in &mut p.samples {
        let Reader::CorePrivate { address } = s.provenance.as_ref().unwrap().catalogue_reader
        else {
            unreachable!()
        };
        s.provenance
            .as_mut()
            .unwrap()
            .access
            .as_mut()
            .unwrap()
            .route = Route::TclMemory {
            endpoint: "127.0.0.1:6601".into(),
            target: "soc.m4".into(),
            channel: "m4_ppb".into(),
            configuration_source: "environment".into(),
            address: format!("0x{address:x}"),
            bits: 32,
            bus_width: 32,
            count: 1,
            byte_order: ByteOrder::Little,
            atomic: true,
        };
    }
    // Generic redecoding needs the selected M profile retained by the M decoder.
    decode(&mut p, &c);
    p.decode();
    assert_eq!(p.facts["mpu.regions"].value, 8);
    if let Route::TclMemory { count, .. } = &mut p.samples[1]
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
    assert!(!p.facts.contains_key("mpu.regions"));
}

#[test]
fn m_fp_capacity_accepts_adapted_single_double_and_rejects_reserved_encoding() {
    for (cpu, cpuid, mvfr, expected) in [
        ("cortex-m4", 0x410fc241, 0x10110021, Some(0)),
        ("cortex-m7", 0x411fc271, 0x10110221, Some(1)),
        ("cortex-m4", 0x410fc241, 0x10110221, None),
        ("cortex-m7", 0x411fc271, 0x10110121, None),
    ] {
        let c = Catalogue::builtin(cpu).unwrap();
        let mut p = probe(&c, cpuid);
        p.samples.push(sample(&c, "fpu.mvfr0", mvfr));
        decode(&mut p, &c);
        assert_eq!(p.facts.get("vfp.double").map(|f| f.value), expected);
        assert_eq!(
            p.facts.get("vfp.d_registers").map(|f| f.value),
            expected.map(|_| 16)
        );
    }
}

fn svd(bits: u8, cpu: &str) -> crate::svd::Device {
    crate::svd::Device::parse(&format!(r#"<device schemaVersion="1.3"><name>test</name><version>1</version><description>test</description>
        <cpu><name>{cpu}</name><revision>r0p0</revision><endian>little</endian><mpuPresent>true</mpuPresent><fpuPresent>true</fpuPresent><nvicPrioBits>{bits}</nvicPrioBits><vendorSystickConfig>false</vendorSystickConfig></cpu>
        <addressUnitBits>8</addressUnitBits><width>32</width><size>32</size><peripherals><peripheral><name>UART</name><baseAddress>0x40000000</baseAddress>
        <interrupt><name>UART_RX</name><description>RX</description><value>31</value></interrupt><interrupt><name>UART_ERROR</name><value>239</value></interrupt>
        <registers><register><name>DATA</name><addressOffset>0</addressOffset></register></registers></peripheral></peripherals></device>"#)).unwrap()
}

#[test]
fn nvic_metadata_keeps_declarations_source_conflicts_and_real_interrupt_list() {
    let d = svd(4, "CM4");
    assert_eq!(d.cpu_name.as_deref(), Some("CM4"));
    assert_eq!(d.interrupts[0].value, 31);
    let n = nvic_metadata(Some(&d), "G:\\test.svd", Some(5), "cortex-m4", Some(64));
    assert_eq!(n.priority_bits.as_ref().unwrap().value, 4);
    assert!(
        n.priority_bits
            .as_ref()
            .unwrap()
            .source
            .contains("/device/cpu/nvicPrioBits")
    );
    assert_eq!(n.interrupts.as_ref().unwrap().len(), 2);
    assert!(n.notes.iter().any(|s| s.contains("priority conflict")));
    assert!(n.notes.iter().any(|s| s.contains("239 exceeds")));
    let n = nvic_metadata(Some(&d), "test.svd", Some(5), "cortex-m7", None);
    assert_eq!(n.priority_bits.unwrap().value, 5);
    assert!(n.interrupts.is_none());
    assert!(
        nvic_metadata(None, "", None, "cortex-m4", None)
            .priority_bits
            .is_none()
    );
    assert_eq!(
        nvic_metadata(None, "", Some(3), "cortex-m3", None)
            .priority_bits
            .unwrap()
            .value,
        3
    );
    assert!(
        nvic_metadata(Some(&svd(9, "CM4")), "invalid.svd", None, "cortex-m4", None)
            .priority_bits
            .is_none()
    );
    for bits in [0, 2, 9, 255] {
        let config = super::super::Config {
            facts: BTreeMap::from([("nvic.priority_bits".into(), bits)]),
            ..Default::default()
        };
        assert!(config.validate().unwrap_err().contains("3..8"));
    }
}
