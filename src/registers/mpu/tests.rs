use super::*;
use crate::registers::{Implementation, Reason};
fn context() -> Context {
    Context {
        session: 11,
        generation: 7,
        core: "core0".into(),
        frame: 0,
    }
}
fn sample(id: &str, n: &str) -> Sample {
    Sample {
        id: id.into(),
        state: State::Valid,
        implementation: Implementation::Yes,
        reason: Reason::Unknown,
        detail: String::new(),
        value: Some(RawValue::parse(n, 32).unwrap()),
        owner: Some("core:core0".into()),
        context: context(),
        view: crate::registers::SampleView::PhysicalCore,
        owner_generation: None,
        provenance: None,
        last_value_provenance: None,
        eligibility: None,
        last_value_eligibility: None,
        timestamp_ms: 10,
        source: "openocd:cp15".into(),
    }
}
#[test]
fn mair_all_256_encodings_preserve_device_reserved_and_normal_policies() {
    for byte in 0..=255 {
        let raw = RawValue::parse(&format!("0x{byte:02x}"), 32).unwrap();
        let attr = Attribute::decode(0, &raw).unwrap();
        assert_eq!(attr.raw, format!("0x{byte:02x}"));
        match attr.memory {
            MemoryType::Device { name } => {
                assert!(matches!(byte, 0 | 4 | 8 | 12));
                assert!(name.starts_with("Device-"));
            }
            MemoryType::Unpredictable { .. } => assert!(
                (byte < 16 && !matches!(byte, 0 | 4 | 8 | 12)) || (byte >= 16 && byte & 15 == 0)
            ),
            MemoryType::Normal { outer, inner } => {
                assert!(byte >= 16 && byte & 15 != 0);
                for (n, p) in [(byte >> 4, outer), (byte & 15, inner)] {
                    if n == 4 {
                        assert_eq!(p.policy, "Non-cacheable");
                        assert_eq!(p.read_allocate, None);
                    } else {
                        assert_eq!(p.transient, Some(n < 8));
                        assert_eq!(p.read_allocate, Some(n & 2 != 0));
                        assert_eq!(p.write_allocate, Some(n & 1 != 0));
                    }
                }
            }
        }
    }
    let raw = RawValue::parse("0xff440400", 32).unwrap();
    for (index, label) in [
        (0, "Device-nGnRnE"),
        (1, "Device-nGnRE"),
        (2, "Normal"),
        (3, "Normal"),
        (4, "Device-nGnRnE"),
        (7, "Normal"),
    ] {
        assert!(
            Attribute::decode(index, &raw)
                .unwrap()
                .label()
                .starts_with(label)
        );
    }
    assert!(Attribute::decode(8, &raw).is_err());
    assert!(Attribute::decode(0, &RawValue::parse("0xff", 64).unwrap()).is_err());
}
#[test]
fn mpu_count_plans_cover_each_implemented_region_once_and_do_not_use_selectors() {
    for bank in [Bank::El1, Bank::El2] {
        for count in [16, 20, 24] {
            let ids = bank.read_ids(count).unwrap();
            assert_eq!(
                ids.len(),
                usize::from(count) * 2 + if bank == Bank::El1 { 3 } else { 5 }
            );
            for index in 0..count {
                for id in bank.kind().ids(index) {
                    assert_eq!(ids.iter().filter(|x| **x == id).count(), 1);
                }
            }
            assert!(!ids.iter().any(|id| id.ends_with("selr")));
        }
        for count in [1, 15, 17, 25, 255] {
            assert!(bank.read_ids(count).is_err());
        }
    }
    assert!(Bank::El1.read_ids(0).is_err());
    assert!(Bank::El2.read_ids(0).unwrap().is_empty());
    assert_eq!(
        Bank::El1
            .count(&RawValue::parse("0x1800", 32).unwrap())
            .unwrap(),
        24
    );
    assert_eq!(
        Bank::El2
            .count(&RawValue::parse("0x14", 32).unwrap())
            .unwrap(),
        20
    );
}
#[test]
fn mpu_regions_decode_only_matching_core_stop_and_mair_half_with_raw_errors_retained() {
    let mut samples = vec![
        sample("sctlr", "0x20001"),
        sample("mair0", "0x000000ff"),
        sample("prbar0", "0x2000001f"),
        sample("prlar0", "0x2000ffc1"),
        sample("prbar1", "0xffffffff"),
        sample("prlar1", "0xffffffcf"),
    ];
    let view = View::from_samples(Bank::El1, 16, &context(), &samples).unwrap();
    assert_eq!(view.regions.len(), 16);
    assert_eq!(view.global_enabled, Some(true));
    assert_eq!(view.background_enabled, Some(true));
    let r = &view.regions[0];
    assert!(r.issues.is_empty());
    assert_eq!(r.decoded.as_ref().unwrap().limit_inclusive, "0x2000ffff");
    assert_eq!(r.attribute.as_ref().unwrap().index, 0);
    assert_eq!(
        view.regions[1].decoded.as_ref().unwrap().limit_inclusive,
        "0xffffffff"
    );
    assert!(view.regions[1].attribute.is_none());
    assert!(view.regions[1].issues[0].contains("MAIR"));
    for mutation in 0..5 {
        let mut changed = samples.clone();
        match mutation {
            0 => changed[2].context.generation += 1,
            1 => changed[2].context.session += 1,
            2 => changed[2].context.frame = 1,
            3 => changed[2].owner = Some("core:core1".into()),
            _ => {
                changed[2].state = State::Unavailable;
                changed[2].detail = "access denied".into();
            }
        }
        let view = View::from_samples(Bank::El1, 16, &context(), &changed).unwrap();
        assert!(view.regions[0].decoded.is_none());
    }
    samples[1].state = State::Stale;
    assert!(
        View::from_samples(Bank::El1, 16, &context(), &samples)
            .unwrap()
            .regions[0]
            .attribute
            .is_none()
    );
    let el2 = View::from_samples(Bank::El2, 0, &context(), &samples).unwrap();
    assert!(el2.regions.is_empty());
    assert!(el2.global_enabled.is_none());
}
#[test]
fn el2_control_permissions_and_enable_alias_do_not_borrow_el1_evidence() {
    let samples = vec![
        sample("sctlr", "1"),
        sample("hsctlr", "0"),
        sample("hmair0", "0x44"),
        sample("hprenr", "0"),
        sample("hprbar0", "0x20000002"),
        sample("hprlar0", "0x2000ffc1"),
    ];
    let view = View::from_samples(Bank::El2, 16, &context(), &samples).unwrap();
    assert_eq!(view.global_enabled, Some(false));
    assert_eq!(
        view.regions[0].decoded.as_ref().unwrap().access,
        "EL2/EL1/EL0 RW"
    );
    assert!(view.regions[0].issues.iter().any(|s| s.contains("HPRENR")));
    assert!(matches!(
        view.regions[0].attribute.as_ref().unwrap().memory,
        MemoryType::Normal { .. }
    ));
    assert!(
        View::from_samples(Bank::El1, 16, &context(), &samples)
            .unwrap()
            .regions[0]
            .decoded
            .is_none()
    );
}
