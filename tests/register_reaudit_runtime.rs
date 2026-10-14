//! Independent source/formula and negative-path tests. Pure memory; no device IO.
use debugtui::registers::{
    self, Access, Catalogue, Component, Config, Context, Implementation, RawValue, Reader, Scope,
};
use registers::matrix::{Environment, Report, Support};
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
fn r52() -> Catalogue {
    Catalogue::load(&root().join("reference/cortex-r52-system-registers.toml")).unwrap()
}
fn facts(values: &[(&str, u64)]) -> BTreeMap<String, u64> {
    values.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}
fn normalized(s: &str) -> String {
    s.replace("\\\\?\\", "").replace('\\', "/").to_lowercase()
}
fn inventory(c: Catalogue, config: Config, core: &str) -> Report {
    Report {
        schema_version: 1,
        source: "independent offline fixture".into(),
        context: Context {
            session: 7,
            generation: 3,
            core: core.into(),
            frame: 0,
        },
        catalogue: Some(c),
        environment: Environment {
            registers: config,
            selected_core: Some(core.into()),
            gdb_endpoint: "invalid://no-device".into(),
            observed_gdb_endpoint: None,
            gdb_core_endpoints: BTreeMap::new(),
            channels: vec![],
            channels_source: "fixture".into(),
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
    }
}

#[test]
fn all16_bcr_split_bas_and_ro_mirrors_all_raw_patterns() {
    // R52 TRM physical640: BAS[1]/[3] mirror [0]/[2]; full bits [8:5].
    let c = r52();
    let noise = 0xffff_ffffu128 & !(15u128 << 5);
    let mut vectors = 0;
    for n in 0..8 {
        for suffix in ["", "_mmio"] {
            let id = format!("dbgbcr{n}{suffix}");
            let r = c.register(&id).unwrap();
            let bas = r.fields.iter().find(|f| f.name == "BAS").unwrap();
            let mirror = r.fields.iter().find(|f| f.name == "BAS_MIRROR").unwrap();
            assert_eq!(bas.access, Some(Access::Rw));
            assert_eq!(mirror.access, Some(Access::Ro));
            assert_eq!(
                bas.segments
                    .iter()
                    .map(|s| (s.offset, s.width))
                    .collect::<Vec<_>>(),
                vec![(5, 1), (7, 1)]
            );
            assert_eq!(
                mirror
                    .segments
                    .iter()
                    .map(|s| (s.offset, s.width))
                    .collect::<Vec<_>>(),
                vec![(6, 1), (8, 1)]
            );
            assert!(r.writer.is_none() && r.write.is_none());
            for pattern in 0..16u128 {
                for unrelated in [0, noise] {
                    let value = RawValue::from_integer((pattern << 5) | unrelated, 32).unwrap();
                    // Deliberately independent low/high bit formula, not copied segment iteration.
                    assert_eq!(
                        bas.extract(&value).unwrap().integer().unwrap(),
                        (pattern & 1) | ((pattern >> 1) & 2),
                        "{id} pattern={pattern}"
                    );
                    assert_eq!(
                        mirror.extract(&value).unwrap().integer().unwrap(),
                        ((pattern >> 1) & 1) | ((pattern >> 2) & 2),
                        "{id} pattern={pattern}"
                    );
                    vectors += 1;
                }
            }
        }
    }
    assert_eq!(vectors, 512); // Arbitrary raw vectors test decoding, not legal hardware programming.
}

#[test]
fn gic_all1519_source_formula_fields_conditions_and_origins_in_real_loader() {
    // Independently authored primary-table inventory; no repair generator is used.
    let evidence: Value = serde_json::from_slice(
        &fs::read(root().join(
            "artifacts/system-registers-reaudit-r52-f429/gic-mmio-all-structural-and-semantic.json",
        ))
        .unwrap(),
    )
    .unwrap();
    let c = r52();
    let checks = evidence["checks"].as_array().unwrap();
    assert_eq!(checks.len(), 1519);
    let mut fields = 0;
    for case in checks {
        let id = case["register"].as_str().unwrap();
        let r = c.register(id).unwrap();
        let e = &case["expected"];
        assert_eq!(u64::from(r.bits), e["bits"].as_u64().unwrap(), "{id}");
        assert_eq!(serde_json::to_value(r.access).unwrap(), e["access"], "{id}");
        let component = id.split('.').next().unwrap();
        assert_eq!(
            r.scope,
            if component == "gicr" {
                Scope::Core
            } else {
                Scope::Cluster
            },
            "{id}"
        );
        assert!(
            matches!(&r.reader,Reader::Mmio{component:c,offset,require_owner_mapping:true} if c==component && *offset==e["offset"].as_u64().unwrap()),
            "{id}"
        );
        assert!(r.writer.is_none() && r.write.is_none() && r.write_fields.is_empty());
        assert_eq!(r.reset, e["reset"].as_u64());
        let expected_fields = e["fields"].as_array().unwrap();
        assert_eq!(r.fields.len(), expected_fields.len(), "{id}");
        for (f, exp) in r.fields.iter().zip(expected_fields) {
            assert_eq!(f.name, exp["name"].as_str().unwrap(), "{id}");
            assert_eq!(f.segments.len(), 1);
            assert_eq!(
                u64::from(f.segments[0].offset),
                exp["offset"].as_u64().unwrap()
            );
            assert_eq!(
                u64::from(f.segments[0].width),
                exp["width"].as_u64().unwrap()
            );
            assert_eq!(
                serde_json::to_value(f.access.unwrap()).unwrap(),
                exp["access"],
                "{id}:{}",
                f.name
            );
            fields += 1;
        }
        let origin = r.definition_origin.as_ref().unwrap();
        assert_eq!(
            normalized(&origin.declared_in),
            normalized(&format!(
                "file:{}",
                case["current_origin"]["file"].as_str().unwrap()
            )),
            "{id}"
        );
        assert_eq!(origin.inheritance.len(), 2, "{id}");
        assert!(origin.overrides.is_none());
        assert!(origin.inheritance[0].ends_with("cortex-r52-system-registers.toml"));
        assert!(r.source.as_ref().unwrap().number == "100026_0104_01_en");
    }
    assert_eq!(fields, 17752);
}

#[test]
fn gic_capacity_all_values_missing_and_invalid_never_default_to_maximum() {
    // TRM289 ITLinesNumber1..30, tables285/304 and primary field formulas.
    let c = r52();
    let distributor: Vec<_> = c
        .registers
        .iter()
        .filter(|r| r.id.starts_with("gicd."))
        .collect();
    assert_eq!(distributor.len(), 1485);
    for n in (0..=31u64).chain([u64::MAX]) {
        let f = facts(&[("gicd.present", 1), ("gicd.itlinesnumber", n)]);
        let yes = distributor
            .iter()
            .filter(|r| c.implementation(r, &f).0 == Implementation::Yes)
            .count();
        assert_eq!(
            yes,
            if (1..=30).contains(&n) {
                15 + 49 * n as usize
            } else {
                15
            },
            "n={n}"
        );
        for r in &distributor {
            let mut expected = true;
            if let Some(index) = r.id.strip_prefix("gicd.irouter") {
                expected = (1..=30).contains(&n) && index.parse::<u64>().unwrap() < 32 * (n + 1);
            }
            if let Some(index) = r.id.strip_prefix("gicd.ipriorityr") {
                expected = (1..=30).contains(&n) && index.parse::<u64>().unwrap() < 8 * (n + 1);
            }
            if let Some(index) = r.id.strip_prefix("gicd.icfgr") {
                expected = (1..=30).contains(&n) && index.parse::<u64>().unwrap() < 2 * (n + 1);
            }
            for name in [
                "igroupr",
                "isenabler",
                "icenabler",
                "ispendr",
                "icpendr",
                "isactiver",
                "icactiver",
            ] {
                if let Some(index) = r.id.strip_prefix(&format!("gicd.{name}")) {
                    expected = (1..=30).contains(&n) && index.parse::<u64>().unwrap() <= n;
                }
            }
            assert_eq!(
                c.implementation(r, &f).0,
                if expected {
                    Implementation::Yes
                } else {
                    Implementation::No
                },
                "{} n={n}",
                r.id
            );
            assert_eq!(c.automatic_read(r, &f), expected, "{} n={n}", r.id);
        }
    }
    for r in distributor {
        assert_eq!(
            c.implementation(r, &BTreeMap::new()).0,
            Implementation::Unknown
        );
        assert!(!c.automatic_read(r, &BTreeMap::new()));
        for invalid_present in [0, 2, u64::MAX] {
            let f = facts(&[
                ("gicd.present", invalid_present),
                ("gicd.itlinesnumber", 30),
            ]);
            assert_eq!(c.implementation(r, &f).0, Implementation::No);
            assert!(!c.automatic_read(r, &f));
        }
        let f = facts(&[("gicd.present", 1)]);
        let fixed = r.conditions.len() == 1;
        assert_eq!(
            c.implementation(r, &f).0,
            if fixed {
                Implementation::Yes
            } else {
                Implementation::Unknown
            }
        );
        if !fixed {
            assert!(!c.automatic_read(r, &f));
        }
    }
    for cores in [0, 1, 2, 3, 4, 5, u64::MAX] {
        let f = facts(&[("gicr.core_target.present", 1), ("r52.gic.cores", cores)]);
        for r in c.registers.iter().filter(|r| r.id.starts_with("gicr.")) {
            assert_eq!(
                c.implementation(r, &f).0,
                if (1..=4).contains(&cores) {
                    Implementation::Yes
                } else {
                    Implementation::No
                }
            );
            assert_eq!(c.automatic_read(r, &f), (1..=4).contains(&cores));
        }
        for export in [0, 1, 2] {
            let f = facts(&[("r52.gic.export_port", export), ("r52.gic.cores", cores)]);
            let r = c.register("gicr_export.typer").unwrap();
            let yes = export == 1 && (1..=4).contains(&cores);
            assert_eq!(
                c.implementation(r, &f).0,
                if yes {
                    Implementation::Yes
                } else {
                    Implementation::No
                }
            );
            assert_eq!(c.automatic_read(r, &f), yes);
        }
    }
}

#[test]
fn all28_rom_and_gic_views_use_exact_owner_without_legacy_fallback() {
    // TRM416-420 shared cluster ROM; TRM281-282 GICD cluster/GICR physical core.
    let c = r52();
    let rom: Vec<_> = c
        .registers
        .iter()
        .filter(|r| matches!(&r.reader,Reader::Mmio{component,..} if component=="rom"))
        .collect();
    assert_eq!(rom.len(), 28);
    let mut config = Config::default();
    config.topology.clusters = BTreeMap::from([
        ("a0".into(), "alpha".into()),
        ("a1".into(), "alpha".into()),
        ("b0".into(), "beta".into()),
    ]);
    // Deliberately conflicting generic and wrong-owner routes must not be borrowed.
    for component in ["rom", "gicd", "gicr", "gicr_export"] {
        config.components.insert(
            component.into(),
            Component {
                base: 0xdead_0000,
                channel: String::new(),
                little_endian: true,
            },
        );
        config.component_owners.insert(
            component.into(),
            BTreeMap::from([(
                "core:other".into(),
                Component {
                    base: 0xbeef_0000,
                    channel: String::new(),
                    little_endian: true,
                },
            )]),
        );
    }
    for r in c.registers.iter().filter(|r| {
        r.id.starts_with("gicd.")
            || r.id.starts_with("gicr.")
            || r.id.starts_with("gicr_export.")
            || rom.iter().any(|x| x.id == r.id)
    }) {
        let Reader::Mmio {
            component,
            require_owner_mapping,
            ..
        } = &r.reader
        else {
            panic!("{}", r.id)
        };
        assert!(*require_owner_mapping);
        let owner = config.topology.owner(r.scope, "a0");
        assert!(
            config.component(component, owner.as_deref(), true).is_err(),
            "{} borrowed a route",
            r.id
        );
    }
    for r in rom {
        assert_eq!(r.scope, Scope::Cluster);
        assert_eq!(
            config.topology.owner(r.scope, "a0"),
            Some("cluster:alpha".into())
        );
        assert_eq!(
            config.topology.owner(r.scope, "a1"),
            Some("cluster:alpha".into())
        );
        assert_eq!(
            config.topology.owner(r.scope, "b0"),
            Some("cluster:beta".into())
        );
        assert_eq!(config.topology.owner(r.scope, "missing"), None);
    }
    for core in ["a0", "a1", "b0", "missing"] {
        let mut report = inventory(c.clone(), config.clone(), core);
        report.refresh();
        for row in report.rows.iter().filter(|r| {
            r.id.starts_with("romentry")
                || r.id.starts_with("rompidr")
                || r.id.starts_with("romcidr")
        }) {
            assert!(!row.plan.available);
            assert_eq!(
                row.support,
                if core == "missing" {
                    Support::UnknownOwner
                } else {
                    Support::RouteUnavailable
                }
            );
        }
    }
    let periph = 0x8000_0000; // Synthetic aligned fixture, never a board-address claim.
    for (component, owner, base) in [
        ("rom", "cluster:alpha", 0x7000_0000),
        ("gicd", "cluster:alpha", periph),
        ("gicr", "core:a0", periph + 0x100000),
        ("gicr", "core:a1", periph + 0x120000),
        ("gicr_export", "cluster:alpha", periph + 0x140000),
    ] {
        config.component_owners.get_mut(component).unwrap().insert(
            owner.into(),
            Component {
                base,
                channel: String::new(),
                little_endian: true,
            },
        );
    }
    config.facts = facts(&[
        ("gicd.present", 1),
        ("gicd.itlinesnumber", 30),
        ("gicr.core_target.present", 1),
        ("r52.gic.cores", 2),
        ("r52.gic.export_port", 1),
    ]);
    for (core, rbase) in [("a0", periph + 0x100000), ("a1", periph + 0x120000)] {
        let mut report = inventory(c.clone(), config.clone(), core);
        report.refresh();
        for row in report.rows.iter().filter(|r| {
            r.id.starts_with("gicd.")
                || r.id.starts_with("gicr.")
                || r.id.starts_with("gicr_export.")
                || r.id.starts_with("romentry")
                || r.id.starts_with("rompidr")
                || r.id.starts_with("romcidr")
        }) {
            let reg = c.register(&row.id).unwrap();
            let Reader::Mmio {
                component, offset, ..
            } = &reg.reader
            else {
                unreachable!()
            };
            let base = match component.as_str() {
                "gicd" => periph,
                "gicr" => rbase,
                "gicr_export" => periph + 0x140000,
                "rom" => 0x7000_0000,
                _ => unreachable!(),
            };
            assert!(row.plan.available, "{}", row.id);
            assert_eq!(row.plan.address, Some(format!("0x{:x}", base + offset)));
            assert_eq!(row.support, Support::Unobserved);
            assert!(row.plan.memory.as_ref().is_some_and(|m| !m.atomic));
            assert!(row.observation.is_none());
        }
    }
}

#[test]
fn independent_write_payload_validation_and_read_decode_separation() {
    let text = r#"version=1
cpu="fixture"
architecture="fixture"
[[groups]]
id="trace"
name="Trace"
[[registers]]
id="stim"
name="Stimulus"
group="trace"
bits=32
access="rw"
scope="core"
reader={kind="core_private",address=0xE0000000}
fields=[{name="READY",segments=[{offset=0,width=1}],access="ro"}]
write_fields=[{name="PAYLOAD",segments=[{offset=0,width=32}],access="wo"}]
"#;
    let c = Catalogue::parse(text).unwrap();
    let r = c.register("stim").unwrap();
    assert!(r.writer.is_none() && r.write.is_none());
    assert_eq!(
        r.fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        vec!["READY"]
    );
    assert_eq!(r.write_fields[0].name, "PAYLOAD");
    assert_eq!(
        r.fields[0]
            .extract(&RawValue::from_integer(0x8000_0001, 32).unwrap())
            .unwrap()
            .integer()
            .unwrap(),
        1
    );
    for replacement in ["access=\"ro\"", "access=\"rw\""] {
        let invalid = text.replace("access=\"wo\"", replacement);
        assert!(Catalogue::parse(&invalid).is_err());
    }
    assert!(Catalogue::parse(&text.replace("offset=0,width=32", "offset=1,width=32")).is_err());
    let mut invalid = c.clone();
    let overlap = invalid.registers[0].write_fields[0].clone();
    invalid.registers[0].write_fields.push(overlap);
    assert!(invalid.validate().is_err());
    let mut invalid = c.clone();
    invalid.registers[0].access = Access::Ro;
    assert!(invalid.validate().is_err());
    let mut invalid = c.clone();
    invalid.registers[0].write_fields[0]
        .segments
        .push(registers::Segment {
            offset: 4,
            width: 3,
        });
    assert!(invalid.validate().is_err());
    let f429 = Catalogue::load(&root().join("reference/stm32f429-system-registers.toml")).unwrap();
    let executable = f429.register("r0").unwrap();
    let mut invalid = c.clone();
    invalid.registers[0].writer = executable.writer.clone();
    invalid.registers[0].write = executable.write.clone();
    assert!(invalid.validate().is_err());
}

#[test]
fn descriptive_reader_illegal_permissions_positions_widths_and_writer_are_rejected() {
    let text = r#"version=1
cpu="fixture"
architecture="fixture"
[[groups]]
id="debug"
name="Debug"
[[registers]]
id="command"
name="Command"
group="debug"
bits=32
access="wo"
scope="core"
reader={kind="description_only",location={kind="core_private",address=0xE000EDF4}}
"#;
    let c = Catalogue::parse(text).unwrap();
    assert!(!c.registers[0].auto_read(Implementation::Yes));
    assert!(!c.automatic_read(&c.registers[0], &BTreeMap::new()));
    for (from, to) in [
        ("access=\"wo\"", "access=\"ro\""),
        ("scope=\"core\"", "scope=\"cluster\""),
        ("bits=32", "bits=64"),
        ("0xE000EDF4", "0xE000EDF5"),
        ("0xE000EDF4", "0x1000"),
    ] {
        assert!(
            Catalogue::parse(&text.replace(from, to)).is_err(),
            "{from}->{to}"
        );
    }
    let unsupported = text
        .replace("description_only", "description_unsupported")
        .replace("access=\"wo\"", "access=\"rw\"");
    let c = Catalogue::parse(&unsupported).unwrap();
    assert!(!c.registers[0].auto_read(Implementation::Yes));
    assert!(c.registers[0].writer.is_none());
    assert!(Catalogue::parse(&unsupported.replace("access=\"rw\"", "access=\"wo\"")).is_err());
    assert!(Catalogue::parse(&format!("{unsupported}\nread_side_effect=true\n")).is_err());
    let f429 = Catalogue::load(&root().join("reference/stm32f429-system-registers.toml")).unwrap();
    let executable = f429.register("r0").unwrap();
    let mut invalid = c.clone();
    invalid.registers[0].writer = executable.writer.clone();
    invalid.registers[0].write = executable.write.clone();
    assert!(invalid.validate().is_err());
}

#[test]
fn affected_builtin_profiles_retain_old_readers_and_no_payload_or_descriptive_route() {
    for cpu in [
        "cortex-m3",
        "cortex-m4",
        "cortex-m7",
        "cortex-r52",
        "cortex-r52+",
    ] {
        let c = Catalogue::builtin(cpu).unwrap();
        c.validate().unwrap();
        assert!(!c.registers.is_empty());
        assert!(
            c.registers.iter().all(|r| r.write_fields.is_empty()
                && !matches!(
                    r.reader,
                    Reader::DescriptionOnly { .. } | Reader::DescriptionUnsupported { .. }
                )),
            "{cpu}"
        );
        for r in &c.registers {
            if let Some(origin) = &r.definition_origin {
                assert!(!origin.declared_in.is_empty() && !origin.inheritance.is_empty());
            }
        }
    }
}
