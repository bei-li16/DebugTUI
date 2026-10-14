//! Independent source-derived checks and explicit defect reproducers. No target IO.
use debugtui::registers::{
    Access, Catalogue, DescriptionLocation, Implementation, RawValue, Reader,
};
use std::{collections::BTreeMap, path::PathBuf};
fn catalogue() -> Catalogue {
    Catalogue::load(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("reference/stm32f429-system-registers.toml"),
    )
    .unwrap()
}

#[test]
fn primary_r0p1_actlr_positions_and_permission_views() {
    // 100166_0001_04_en PDF53 Table4-2, verified directly from the PDF.
    let c = catalogue();
    let r = c.register("scb.actlr").unwrap();
    assert_eq!(r.access, Access::Rw);
    for (name, bit) in [
        ("DISOOFP", 9),
        ("DISFPCA", 8),
        ("DISFOLD", 2),
        ("DISDEFWBUF", 1),
        ("DISMCYCINT", 0),
    ] {
        let f = r.fields.iter().find(|f| f.name == name).unwrap();
        assert_eq!((f.segments[0].offset, f.segments[0].width), (bit, 1));
        for raw in [0u128, 1u128 << bit, 0xffffffff] {
            assert_eq!(
                f.extract(&RawValue::from_integer(raw, 32).unwrap())
                    .unwrap()
                    .integer()
                    .unwrap(),
                (raw >> bit) & 1
            );
        }
    }
    let latency = r.fields.iter().find(|f| f.name == "DISMCYCINT").unwrap();
    assert!(latency.description.contains("增加中断延迟"));
    let sbzp = r.fields.iter().find(|f| f.name == "DISFPCA").unwrap();
    assert!(sbzp.description.contains("写零或保持"));
}

#[test]
fn primary_stir_and_dcrsr_layouts_keep_commands_unreadable() {
    // DDI0403E.e PDF619,703-705: IRQ selector and core transfer selector are distinct.
    let c = catalogue();
    for (id, address) in [("nvic.stir", 0xe000ef00), ("dcb.dcrsr", 0xe000edf4)] {
        let r = c.register(id).unwrap();
        assert_eq!(r.access, Access::Wo);
        assert!(
            matches!(r.reader,Reader::DescriptionOnly{location:DescriptionLocation::CorePrivate{address:a}} if a==address)
        );
        assert!(!c.automatic_read(r, &BTreeMap::new()));
        assert!(r.writer.is_none() && r.write.is_none());
    }
    let stim = c.register("nvic.stir").unwrap();
    let irq = stim.fields.iter().find(|f| f.name == "INTID").unwrap();
    assert_eq!((irq.segments[0].offset, irq.segments[0].width), (0, 9));
    let r = c.register("dcb.dcrsr").unwrap();
    let select = r.fields.iter().find(|f| f.name == "REGSEL").unwrap();
    let mut expected: Vec<u128> = (0..19).collect();
    expected.extend([20, 33]);
    expected.extend(64..96);
    let values: Vec<_> = select
        .enums
        .iter()
        .map(|e| e.value.parse::<u128>().unwrap())
        .collect();
    assert_eq!(values, expected);
    let r = c.register("dcb.dcrdr").unwrap();
    assert_eq!(r.access, Access::Rw);
    assert!(r.reset.is_none());
    assert_eq!(
        (
            r.fields[0].segments[0].offset,
            r.fields[0].segments[0].width
        ),
        (0, 32)
    );
    assert!(r.writer.is_none());
}

#[test]
fn primary_mpu_alias_addresses_and_mvfr2_unknown_exclusion() {
    // M4 PDF59 and DDI0403E.e PDF635-636/642: +8,+16,+24 word-pair aliases.
    let c = catalogue();
    for i in 1..4 {
        for (name, base) in [("rbar", 0xe000ed9c), ("rasr", 0xe000eda0)] {
            let r = c.register(&format!("mpu.{name}_a{i}")).unwrap();
            assert!(matches!(r.reader,Reader::CorePrivate{address:a} if a==base+8*i));
            assert_eq!(r.access, Access::Rw);
            assert!(r.writer.is_none());
        }
    }
    let r = c.register("fpu.mvfr2").unwrap();
    assert_eq!(
        c.implementation(r, &BTreeMap::new()).0,
        Implementation::Unknown
    );
    assert!(!c.automatic_read(r, &BTreeMap::new()));
    let facts = [("stm32f429.mvfr2.present".to_owned(), 0)]
        .into_iter()
        .collect();
    assert_eq!(c.implementation(r, &facts).0, Implementation::No);
    let f = &r.fields[0];
    assert_eq!((f.segments[0].offset, f.segments[0].width), (4, 4));
    assert_eq!(
        f.enums.iter().map(|e| e.value.as_str()).collect::<Vec<_>>(),
        vec!["0", "4"]
    );
}

#[test]
fn wo_etm_reserved_fields_must_not_be_advertised_as_readable() {
    // DDI0440C PDF32 Table3-1 marks both interfaces WO, PDF43/44 merely Reserved[31:1].
    // This is an independent semantic expectation, intentionally retained if it fails.
    let c = catalogue();
    for id in ["etm.ittrigout", "etm.itatbctr0"] {
        let r = c.register(id).unwrap();
        let f = r.fields.iter().find(|f| f.name == "Reserved_31_1").unwrap();
        assert!(
            !f.access.unwrap_or(r.access).readable(),
            "{id} reserved field is displayed as readable despite the WO interface"
        );
    }
}

#[test]
fn description_unsupported_must_not_be_automatically_eligible() {
    // Source route restriction is independent of the configured component presence declaration.
    let c = catalogue();
    let r = c.register("etm.lar").unwrap();
    let facts = [("stm32f429.etm_m4.present".to_owned(), 1)]
        .into_iter()
        .collect();
    assert_eq!(c.implementation(r, &facts).0, Implementation::Yes);
    assert!(!r.auto_read(Implementation::Yes));
    assert!(
        !c.automatic_read(r, &facts),
        "Catalogue automatic_read ignores the DescriptionUnsupported exclusion used by Register auto_read"
    );
}

#[test]
fn primary_r52_numproc_all_five_bit_patterns_with_unrelated_bit_noise() {
    // IHI0064H.b PDF385: [13:12] are the top two bits; [30:28] the low three.
    // R52 TRM PDF527 fixes the product value to zero (one traced PE).
    // Nonzero vectors exercise extraction only; they do not claim product support.
    let c = Catalogue::load(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("reference/cortex-r52-system-registers.toml"),
    )
    .unwrap();
    let r = c.register("trcidr3").unwrap();
    let f = r.fields.iter().find(|f| f.name == "NUMPROC").unwrap();
    assert_eq!(r.access, Access::Ro);
    assert_eq!(
        f.segments
            .iter()
            .map(|s| (s.offset, s.width))
            .collect::<Vec<_>>(),
        vec![(28, 3), (12, 2)]
    );
    assert_eq!(
        f.enums.iter().map(|e| e.value.as_str()).collect::<Vec<_>>(),
        vec!["0x0"]
    );
    assert!(
        matches!(&r.reader,Reader::Mmio{component,offset,require_owner_mapping:true} if component=="etm" && *offset==0x1ec)
    );
    assert!(r.writer.is_none() && r.write.is_none());
    let unrelated = 0xffff_ffffu128 & !((7u128 << 28) | (3u128 << 12));
    for top in 0..4u128 {
        for low in 0..8u128 {
            let expected = 8 * top + low;
            let encoded = (low << 28) | (top << 12);
            for noise in [0, unrelated] {
                let raw = RawValue::from_integer(encoded | noise, 32).unwrap();
                assert_eq!(
                    f.extract(&raw).unwrap().integer().unwrap(),
                    expected,
                    "top={top}, low={low}, noise={noise:x}"
                );
            }
        }
    }
}
