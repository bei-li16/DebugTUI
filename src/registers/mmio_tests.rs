use super::*;

#[test]
fn owner_mappings_are_explicit_strict_and_never_reuse_a_default_or_other_owner() {
    let mut config: Config = toml::from_str(
        r#"
[components.debug_external]
base=3735928556
channel='legacy'
little_endian=true
[component_owners.debug_external.'core:core.0']
base=1073741824
channel='ap0'
little_endian=true
[component_owners.debug_external.'core:core.2']
base=1342177280
channel='ap2'
little_endian=false
[component_owners.gicd.'cluster:A']
base=1610612736
channel='cluster_ap'
little_endian=true
[component_owners.soc.'chip:board']
base=1879048192
channel=''
little_endian=true
"#,
    )
    .unwrap();
    config.validate().unwrap();
    assert_eq!(
        config
            .component("debug_external", Some("core:core.2"), true)
            .unwrap()
            .base,
        0x50000000
    );
    assert!(
        !config
            .component("debug_external", Some("core:core.2"), true)
            .unwrap()
            .little_endian
    );
    for owner in [
        None,
        Some("core:core.1"),
        Some("cluster:A"),
        Some("chip:board"),
    ] {
        for required in [true, false] {
            assert!(config.component("debug_external", owner, required).is_err());
        }
    }
    assert_eq!(
        config
            .component("gicd", Some("cluster:A"), true)
            .unwrap()
            .channel,
        "cluster_ap"
    );
    assert!(config.component("gicd", Some("cluster:B"), true).is_err());
    assert_eq!(
        config
            .component("soc", Some("chip:board"), true)
            .unwrap()
            .base,
        0x70000000
    );
    let saved = toml::to_string(&config).unwrap();
    let restored: Config = toml::from_str(&saved).unwrap();
    assert_eq!(toml::to_string(&restored).unwrap(), saved);
    config.component_owners.clear();
    assert!(
        config
            .component("debug_external", Some("core:core.0"), true)
            .is_err()
    );
    assert_eq!(
        config
            .component("debug_external", Some("core:core.0"), false)
            .unwrap()
            .channel,
        "legacy"
    );
    let old: Reader =
        serde_json::from_str(r#"{"kind":"mmio","component":"legacy","offset":0}"#).unwrap();
    assert_eq!(
        serde_json::to_value(old).unwrap(),
        serde_json::json!({"kind":"mmio","component":"legacy","offset":0})
    );
    for owner in [
        "core:",
        "core: x",
        "core:x ",
        "unknown:x",
        "cluster:\n",
        "chip:",
    ] {
        config.component_owners.insert(
            "debug_external".into(),
            BTreeMap::from([(
                owner.into(),
                Component {
                    base: 0,
                    channel: "ap0".into(),
                    little_endian: true,
                },
            )]),
        );
        assert!(config.validate().is_err(), "{owner:?}");
    }
    for text in [
        "component_owners=[]",
        "[component_owners.bus.'core:c']\nbase=0\nchannel='ap'\nlittle_endian=true\nunknown=true",
        "[component_owners.bus.'core:c']\nbase=0\nchannel='ap'",
    ] {
        assert!(toml::from_str::<Config>(text).is_err());
    }
}

#[test]
fn r52_mmio_catalogues_cover_exact_gic_and_debug_apertures_with_scoped_owners() {
    for cpu in ["cortex-r52", "cortex-r52+"] {
        let catalogue = Catalogue::builtin(cpu).unwrap();
        let count = |component: &str| {
            catalogue
                .registers
                .iter()
                .filter(|r| matches!(&r.reader,Reader::Mmio{component:name,..} if name==component))
                .count()
        };
        assert_eq!(count("gicd"), 1485);
        assert_eq!(count("gicr"), 33);
        assert_eq!(count("debug_external"), 80);
        for (id, component, offset, bits, scope) in [
            ("gicd_ctlr", "gicd", 0, 32, Scope::Cluster),
            ("gicd_icactiver30", "gicd", 0x3f8, 32, Scope::Cluster),
            ("gicd_ipriorityr247", "gicd", 0x7dc, 32, Scope::Cluster),
            ("gicd_icfgr61", "gicd", 0xcf4, 32, Scope::Cluster),
            ("gicd_irouter32", "gicd", 0x6100, 64, Scope::Cluster),
            ("gicd_irouter991", "gicd", 0x7ef8, 64, Scope::Cluster),
            ("gicr_typer", "gicr", 8, 64, Scope::Core),
            ("gicr_ispendr0", "gicr", 0x10200, 32, Scope::Core),
            ("gicr_icfgr1", "gicr", 0x10c04, 32, Scope::Core),
            ("edprsr", "debug_external", 0x314, 32, Scope::Core),
            ("dbgbxvr6", "debug_external", 0x464, 32, Scope::Core),
            ("dbgbxvr7", "debug_external", 0x474, 32, Scope::Core),
            ("dbgwcr7", "debug_external", 0x878, 32, Scope::Core),
            ("eddfr_word0", "debug_external", 0xd28, 32, Scope::Core),
            ("eddfr_word1", "debug_external", 0xd2c, 32, Scope::Core),
            ("ed_midr", "debug_external", 0xd00, 32, Scope::Core),
        ] {
            let r = catalogue.register(id).unwrap();
            assert_eq!(r.bits, bits, "{id}");
            assert_eq!(r.scope, scope, "{id}");
            assert_eq!(
                r.reader,
                Reader::Mmio {
                    component: component.into(),
                    offset,
                    require_owner_mapping: true
                },
                "{id}"
            );
            assert!(r.writer.is_none() && r.write.is_none());
        }
        for name in ["gicd", "gicr"] {
            for (n, offset) in [(0, 0xffe0), (3, 0xffec), (4, 0xffd0), (7, 0xffdc)] {
                assert!(
                    matches!(catalogue.register(&format!("{name}_pidr{n}")).unwrap().reader,Reader::Mmio{offset:o,..} if o==offset)
                );
            }
        }
        assert_eq!(catalogue.register("gicr_ctlr").unwrap().access, Access::Ro);
        assert_eq!(
            catalogue.register("gicr_icfgr0").unwrap().access,
            Access::Ro
        );
        assert!(catalogue.register("gicd_ispendr0").is_none());
        assert!(catalogue.register("gicd_irouter992").is_none());
    }
    assert!(
        Catalogue::builtin("cortex-m4")
            .unwrap()
            .register("edscr")
            .is_none()
    );
}

#[test]
fn mmio_capacities_side_effects_and_write_only_registers_filter_without_implicit_access() {
    let catalogue = Catalogue::builtin("cortex-r52").unwrap();
    let facts = BTreeMap::from([
        ("gicd.present".into(), 1),
        ("gicd.interrupts".into(), 64),
        ("debug_external.present".into(), 1),
        ("debug_external.breakpoints".into(), 8),
        ("debug_external.watchpoints".into(), 8),
    ]);
    for id in [
        "gicd_igroupr1",
        "gicd_ipriorityr15",
        "gicd_icfgr3",
        "gicd_irouter63",
        "dbgbxvr7",
        "dbgwcr7",
    ] {
        let r = catalogue.register(id).unwrap();
        assert_eq!(
            catalogue.implementation(r, &facts).0,
            Implementation::Yes,
            "{id}"
        );
        assert!(catalogue.automatic_read(r, &facts));
        assert!(!catalogue.automatic_read(r, &BTreeMap::new()));
    }
    for id in [
        "gicd_igroupr2",
        "gicd_ipriorityr16",
        "gicd_icfgr4",
        "gicd_irouter64",
        "gicd_irouter991",
    ] {
        let r = catalogue.register(id).unwrap();
        assert_eq!(
            catalogue.implementation(r, &facts).0,
            Implementation::No,
            "{id}"
        );
        assert!(!catalogue.automatic_read(r, &facts));
    }
    for id in ["edprsr", "edpcsr_lo", "edpcsr_hi", "dbgdtrtx_el0"] {
        let r = catalogue.register(id).unwrap();
        assert_eq!(catalogue.read_policy(r), (true, true));
        assert!(!catalogue.automatic_read(r, &facts), "{id}");
    }
    let rx = catalogue.register("dbgdtrrx_el0").unwrap();
    assert_eq!(catalogue.read_policy(rx), (true, false));
    assert!(catalogue.automatic_read(rx, &facts));
    for id in ["editr", "edrcr", "dbgoslar", "edlar"] {
        let r = catalogue.register(id).unwrap();
        assert_eq!(catalogue.read_policy(r), (false, false));
        assert!(!catalogue.automatic_read(r, &facts), "{id}");
    }
    let typer = RawValue::parse("0x0248001e", 32).unwrap();
    assert_eq!(
        catalogue
            .register("gicd_typer")
            .unwrap()
            .fields
            .iter()
            .find(|f| f.name == "ITLinesNumber")
            .unwrap()
            .extract(&typer)
            .unwrap()
            .integer()
            .unwrap(),
        30
    );
}
