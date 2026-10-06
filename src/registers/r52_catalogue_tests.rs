//! Independent TRM anchors for the bounded R52 model; no target permission claim.
use super::*;

fn field<'a>(register: &'a Register, name: &str) -> &'a Field {
    register.fields.iter().find(|f| f.name == name).unwrap()
}
fn value(register: &Register, name: &str, raw: &str) -> u128 {
    field(register, name)
        .extract(&RawValue::parse(raw, 32).unwrap())
        .unwrap()
        .integer()
        .unwrap()
}

#[test]
fn r52_core_trm_encodings_sources_resets_and_read_only_delivery_are_complete() {
    // MRC anchors independently transcribed from TRM sections 4.2/4.3.
    let expected = [
        ("midr", (0, 0, 0, 0), 169, Access::Ro),
        ("mpidr", (0, 0, 0, 5), 182, Access::Ro),
        ("sctlr", (0, 1, 0, 0), 201, Access::Rw),
        ("hsctlr", (4, 1, 0, 0), 145, Access::Rw),
        ("cpacr", (0, 1, 0, 2), 80, Access::Rw),
        ("hcr", (4, 1, 1, 0), 121, Access::Rw),
        ("mpuir", (0, 0, 0, 4), 181, Access::Ro),
        ("hmpuir", (4, 0, 0, 4), 136, Access::Ro),
        ("prselr", (0, 6, 2, 1), 195, Access::Rw),
        ("hprselr", (4, 6, 2, 1), 140, Access::Rw),
        ("hprenr", (4, 6, 1, 1), 135, Access::Rw),
        ("mair0", (0, 10, 2, 0), 171, Access::Rw),
        ("mair1", (0, 10, 2, 1), 171, Access::Rw),
        ("hmair0", (4, 10, 2, 0), 132, Access::Rw),
        ("hmair1", (4, 10, 2, 1), 132, Access::Rw),
    ];
    for cpu in ["cortex-r52", "cortex-r52+"] {
        let catalogue = Catalogue::builtin(cpu).unwrap();
        let encoded = toml::to_string(&catalogue).unwrap();
        assert!(
            encoded.len() as u64 <= MAX_CATALOGUE_BYTES,
            "{cpu}: serialized preset is {} bytes, exceeding {MAX_CATALOGUE_BYTES}",
            encoded.len()
        );
        let restored = Catalogue::parse(&encoded).unwrap();
        assert_eq!(toml::to_string(&restored).unwrap(), encoded);
        for (id, encoding, page, access) in expected {
            let r = catalogue.register(id).unwrap();
            assert!(
                matches!(r.reader, Reader::Cp15 {cp:15,op1,crn,crm,op2} if (op1,crn,crm,op2)==encoding)
            );
            assert_eq!((r.bits, r.scope, r.access), (32, Scope::Core, access));
            assert!(!r.fields.is_empty() && !r.fields_missing && !r.read_side_effect);
            assert!(r.writer.is_none() && r.write.is_none() && r.verification.is_none());
            let source = r.source.as_ref().unwrap();
            assert_eq!(source.number, "100026_0104_01_en");
            assert_eq!(source.page, Some(page));
            assert!(source.section.starts_with("4.3."));
            assert!(r.access_condition.contains("current Debug EL"));
            assert!(r.access_condition.contains("CPSR/DSPSR"));
            assert_eq!(
                r.reset,
                match id {
                    "cpacr" | "hprenr" => Some(0),
                    "hcr" => Some(2),
                    _ => None,
                }
            );
            if ["midr", "mpidr", "prselr"].contains(&id) {
                assert_eq!(r.confidence, metadata::Confidence::Medium);
                assert!(r.description.contains("Source conflict:"));
            } else {
                assert_eq!(r.confidence, metadata::Confidence::High);
            }
            if cpu == "cortex-r52+" {
                assert!(r.description.contains("R52+ physical identity"));
            }
        }
    }
}

#[test]
fn r52_core_fields_preserve_revision_affinity_and_ignored_cpacr_bits_without_guessing() {
    let catalogue = Catalogue::builtin("cortex-r52").unwrap();
    let midr = catalogue.register("midr").unwrap();
    for (raw, revision) in [("0x411fd134", 4), ("0x411fd135", 5)] {
        assert_eq!(value(midr, "Implementer", raw), 0x41);
        assert_eq!(value(midr, "Variant", raw), 1);
        assert_eq!(value(midr, "Architecture", raw), 15);
        assert_eq!(value(midr, "PartNum", raw), 0xd13);
        assert_eq!(value(midr, "Revision", raw), revision);
    }
    let mpidr = catalogue.register("mpidr").unwrap();
    for (raw, core) in [("0x80ab1202", 2), ("0x80ab1205", 5)] {
        assert_eq!(value(mpidr, "Aff0", raw), core);
        assert_eq!(value(mpidr, "Aff1", raw), 0x12);
        assert_eq!(value(mpidr, "Aff2", raw), 0xab);
        assert_eq!(value(mpidr, "M", raw), 1);
        assert_eq!(value(mpidr, "MT", raw), 0);
    }
    let cpacr = catalogue.register("cpacr").unwrap();
    assert_eq!(value(cpacr, "ASEDIS", "0x80700000"), 1);
    assert_eq!(value(cpacr, "cp10", "0x80700000"), 3);
    assert_eq!(value(cpacr, "cp11", "0x80700000"), 1);
    assert!(field(cpacr, "cp11").description.contains("UNKNOWN"));
    let reserved = field(cpacr, "cp10")
        .extract(&RawValue::parse("0x00200000", 32).unwrap())
        .unwrap();
    assert_eq!(field(cpacr, "cp10").enum_name(&reserved), Some("Reserved"));
}

#[test]
fn r52_control_and_mair_fields_distinguish_traps_bank_enables_and_attribute_halves() {
    let catalogue = Catalogue::builtin("cortex-r52").unwrap();
    let sctlr = catalogue.register("sctlr").unwrap();
    let hsctlr = catalogue.register("hsctlr").unwrap();
    assert_eq!(field(sctlr, "FI").access, Some(Access::Ro));
    assert_eq!(field(hsctlr, "FI").access, None);
    for r in [sctlr, hsctlr] {
        assert_eq!(value(r, "M", "0x02221005"), 1);
        assert_eq!(value(r, "BR", "0x02221005"), 1);
        assert_eq!(value(r, "I", "0x02221005"), 1);
        assert_eq!(value(r, "EE", "0x02221005"), 1);
        assert_eq!(value(r, "TE", "0x02221005"), 0);
    }
    let hcr = catalogue.register("hcr").unwrap();
    for (raw, read, write) in [("0x40010003", 1, 0), ("0x04010003", 0, 1)] {
        assert_eq!(value(hcr, "TRVM", raw), read);
        assert_eq!(value(hcr, "TVM", raw), write);
        assert_eq!(value(hcr, "TID1", raw), 1);
        assert_eq!(value(hcr, "VM", raw), 1);
    }
    for prefix in ["", "h"] {
        for (bank, raw, bytes) in [
            (0, "0xff440400", [0, 4, 0x44, 0xff]),
            (1, "0x8ba7060c", [0x0c, 6, 0xa7, 0x8b]),
        ] {
            let r = catalogue.register(&format!("{prefix}mair{bank}")).unwrap();
            for (i, byte) in bytes.into_iter().enumerate() {
                assert_eq!(value(r, &format!("Attr{}", bank * 4 + i), raw), byte);
            }
        }
    }
}

#[test]
fn r52_mpu_definitions_have_independent_bank_capacity_direct_encoding_and_permission_sources() {
    let catalogue = Catalogue::builtin("cortex-r52").unwrap();
    for (prefix, fact, level, page) in [
        ("", "mpu.el1.regions", 1, 192),
        ("h", "mpu.el2.regions", 2, 137),
    ] {
        for count in [0, 16, 20, 24] {
            let facts = BTreeMap::from([(fact.into(), count)]);
            for index in 0..24 {
                for limit in [false, true] {
                    let r = catalogue
                        .register(&format!(
                            "{prefix}{}{}",
                            if limit { "prlar" } else { "prbar" },
                            index
                        ))
                        .unwrap();
                    assert_eq!(
                        r.implementation(&facts).0,
                        if index < count {
                            Implementation::Yes
                        } else {
                            Implementation::No
                        }
                    );
                    assert_eq!(
                        r.implementation(&BTreeMap::new()).0,
                        Implementation::Unknown
                    );
                    assert!(
                        matches!(r.reader, Reader::Cp15 {cp:15,op1,crn:6,crm,op2} if u64::from(op1)==(if level==2 {4} else {0})+index/16 && u64::from(crm)==8+(index%16)/2 && u64::from(op2)==4*(index%2)+u64::from(limit))
                    );
                    assert!(r.writer.is_none() && r.reset.is_none());
                    assert_eq!(
                        r.source.as_ref().unwrap().page,
                        Some(if limit {
                            if level == 2 { 139 } else { 194 }
                        } else {
                            page
                        })
                    );
                    if !limit {
                        let ap = field(r, "AP");
                        let raw = ap
                            .extract(&RawValue::parse("0x20000000", 32).unwrap())
                            .unwrap();
                        assert_eq!(
                            ap.enum_name(&raw),
                            Some(if level == 2 {
                                "EL2RW_EL0/EL1None"
                            } else {
                                "EL1RW_EL0None"
                            })
                        );
                    }
                }
            }
        }
    }
    for id in ["prselr", "hprselr"] {
        let r = catalogue.register(id).unwrap();
        assert!(field(r, "REGION").description.contains("bit 4 is RES0"));
        assert!(field(r, "REGION").description.contains("zero-capacity"));
    }
}
