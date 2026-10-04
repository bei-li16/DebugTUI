use super::*;

#[test]
fn mrrc_and_genuine_isb_configuration_is_explicit_and_strict() {
    let old: Config = toml::from_str("cp15_command='arm mrc'").unwrap();
    assert!(old.cp15_64_command.is_empty() && old.isb_command.is_empty());
    for command in ["arm mrrc", "aarch64 mrc", "aarch64 mrrc; resume"] {
        assert!(
            Config {
                cp15_64_command: command.into(),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
    let valid = Config {
        cp15_command: "aarch64 mrc".into(),
        selector_command: "aarch64 mcr".into(),
        cp15_64_command: "aarch64 mrrc".into(),
        isb_command: "aarch64 isb".into(),
        ..Default::default()
    };
    assert!(valid.validate().is_ok());
    let copy: Config = toml::from_str(&toml::to_string(&valid).unwrap()).unwrap();
    assert_eq!(copy.cp15_64_command, valid.cp15_64_command);
    assert_eq!(copy.isb_command, valid.isb_command);
    for isb in ["arm isb", "aarch64 isb; halt"] {
        assert!(
            Config {
                isb_command: isb.into(),
                ..valid.clone()
            }
            .validate()
            .is_err()
        );
    }
    assert!(
        Config {
            cp15_command: "arm mrc".into(),
            selector_command: "arm mcr".into(),
            ..valid
        }
        .validate()
        .is_err()
    );
}

#[test]
fn selector_command_requires_an_explicit_matching_mrc_family() {
    for (mrc, mcr, valid) in [
        ("", "", true),
        ("arm mrc", "", true),
        ("arm mrc", "arm mcr", true),
        ("aarch64 mrc", "aarch64 mcr", true),
        ("", "arm mcr", false),
        ("arm mrc", "aarch64 mcr", false),
        ("aarch64 mrc", "arm mcr", false),
        ("arm mrc", "arm mcr; resume", false),
    ] {
        let config = Config {
            cp15_command: mrc.into(),
            selector_command: mcr.into(),
            ..Default::default()
        };
        assert_eq!(config.validate().is_ok(), valid, "{mrc}/{mcr}");
        let copy: Config = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
        assert_eq!(copy.selector_command, mcr);
    }
    assert!(
        toml::from_str::<Config>("cp15_command='arm mrc'")
            .unwrap()
            .selector_command
            .is_empty()
    );
}

#[test]
fn a_user_cpu_preset_overrides_builtins_and_invalid_overrides_do_not_fall_back() {
    let directory =
        std::env::temp_dir().join(format!("debugtui-user-registers-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let mut catalogue = Catalogue::builtin("cortex-r52").unwrap();
    catalogue.description = "customer-owned register catalogue".into();
    let path = directory.join("cortex-r52.toml");
    fs::write(&path, toml::to_string(&catalogue).unwrap()).unwrap();
    let config = Config {
        cpu: "cortex-r52".into(),
        ..Default::default()
    };
    let (loaded, source) = config.load_cpu_directory(&directory).unwrap().unwrap();
    assert_eq!(loaded.description, catalogue.description);
    assert!(source.starts_with("user:"));
    fs::write(&path, "version=999").unwrap();
    assert!(config.load_cpu_directory(&directory).is_err());
    fs::remove_file(&path).unwrap();
    assert_eq!(
        config.load_cpu_directory(&directory).unwrap().unwrap().1,
        "builtin:cortex-r52"
    );
    fs::remove_dir(directory).unwrap();
}

#[test]
fn shipped_catalogues_have_valid_routes_and_cpu_specific_models() {
    for cpu in ["cortex-r52", "cortex-r52+", "cortex-m4"] {
        let catalogue = Catalogue::builtin(cpu).unwrap();
        assert_eq!(catalogue.cpu, cpu);
        if cpu == "cortex-m4" {
            assert!(catalogue.register("xpsr").is_some());
            assert!(catalogue.register("sctlr").is_none());
        } else {
            assert_eq!(catalogue.register("cntpct").unwrap().bits, 64);
            assert!(matches!(
                catalogue.register("sctlr").unwrap().reader,
                Reader::Cp15 {
                    cp: 15,
                    op1: 0,
                    crn: 1,
                    crm: 0,
                    op2: 0
                }
            ));
            assert!(catalogue.register("hprbar23").is_some());
            assert!(matches!(
                catalogue.register("prbar23").unwrap().reader,
                Reader::Cp15 {
                    cp: 15,
                    op1: 1,
                    crn: 6,
                    crm: 11,
                    op2: 4
                }
            ));
            assert!(matches!(
                catalogue.register("prlar23").unwrap().reader,
                Reader::Cp15 {
                    cp: 15,
                    op1: 1,
                    crn: 6,
                    crm: 11,
                    op2: 5
                }
            ));
            assert!(matches!(
                catalogue.register("ich_vtr").unwrap().reader,
                Reader::Cp15 {
                    cp: 15,
                    op1: 4,
                    crn: 12,
                    crm: 11,
                    op2: 1
                }
            ));
            assert_eq!(catalogue.register("ich_vtr").unwrap().bits, 32);
            assert!(catalogue.register("elr_hyp").is_some());
        }
    }
}

#[test]
fn explicit_catalogue_precedes_cpu_and_missing_file_is_an_error() {
    let config = Config {
        cpu: "unknown-cpu".into(),
        catalogue: Path::new(env!("CARGO_MANIFEST_DIR")).join("profiles/registers/cortex-m4.toml"),
        ..Default::default()
    };
    let (catalogue, source) = config.load().unwrap().unwrap();
    assert_eq!(catalogue.cpu, "cortex-m4");
    assert!(source.starts_with("file:"));
    let missing = Config {
        catalogue: PathBuf::from("missing-register-catalogue.toml"),
        ..Default::default()
    };
    assert!(missing.load().is_err());
    assert!(Config::default().load().unwrap().is_none());
}

fn catalogue() -> Catalogue {
    Catalogue::parse(
        r#"
version = 1
cpu = "cortex-r52"
architecture = "armv8-r-aarch32"
[[groups]]
id = "core"
name = "Core"
[[registers]]
id = "cpsr"
name = "CPSR"
group = "core"
bits = 32
access = "rw"
reader = { kind = "gdb", name = "cpsr" }
"#,
    )
    .unwrap()
}

#[test]
fn strict_catalogue_validation_and_bom() {
    let valid = toml::to_string(&catalogue()).unwrap();
    assert!(Catalogue::parse(&format!("\u{feff}{valid}")).is_ok());
    assert!(Catalogue::parse(&valid.replace("version = 1", "version = 2")).is_err());
    assert!(Catalogue::parse(&format!("unexpected = true\n{valid}")).is_err());
    let mut c = catalogue();
    c.registers.push(c.registers[0].clone());
    assert!(c.validate().is_err());
    c.registers.pop();
    c.registers[0].group = "missing".into();
    assert!(c.validate().is_err());
}

#[test]
fn group_and_alias_cycles_and_scope_changes_are_rejected() {
    let mut c = catalogue();
    c.groups[0].parent = Some("core".into());
    assert!(c.validate().is_err());
    c.groups[0].parent = None;
    c.registers[0].reader = Reader::Alias {
        source: "cpsr".into(),
        offset: 0,
    };
    assert!(c.validate().is_err());
    c.registers[0].reader = Reader::Gdb {
        name: "cpsr".into(),
    };
    let mut alias = c.registers[0].clone();
    alias.id = "alias".into();
    alias.reader = Reader::Alias {
        source: "cpsr".into(),
        offset: 1,
    };
    c.registers.push(alias);
    assert!(c.validate().is_err());
    c.registers[1].bits = 8;
    c.registers[1].scope = Scope::Chip;
    assert!(c.validate().is_err());
    c.registers[1].scope = Scope::Core;
    assert!(c.validate().is_ok());
}

#[test]
fn cp15_and_mrrc_encoding_widths_are_distinct() {
    let mut c = catalogue();
    c.registers[0].reader = Reader::Cp15 {
        cp: 15,
        op1: 0,
        crn: 1,
        crm: 0,
        op2: 0,
    };
    assert!(c.validate().is_ok());
    c.registers[0].bits = 64;
    assert!(c.validate().is_err());
    c.registers[0].reader = Reader::Cp15_64 {
        cp: 15,
        op1: 1,
        crm: 14,
    };
    assert!(c.validate().is_ok());
    c.registers[0].reader = Reader::Cp15_64 {
        cp: 15,
        op1: 16,
        crm: 14,
    };
    assert!(c.validate().is_err());
}

#[test]
fn all_raw_widths_and_endianness_roundtrip_without_json_precision_loss() {
    for bits in [8, 16, 32, 64, 128] {
        let value = RawValue::from_integer(mask(bits), bits).unwrap();
        for little in [true, false] {
            assert_eq!(
                RawValue::from_bytes(&value.bytes(little).unwrap(), bits, little).unwrap(),
                value
            );
        }
        let serialized = serde_json::to_string(&value).unwrap();
        assert!(serialized.contains("\"hex\":\"0x"));
        assert_eq!(
            serde_json::from_str::<RawValue>(&serialized).unwrap(),
            value
        );
    }
    assert_eq!(
        RawValue::parse("0x123456789abcdef0fedcba9876543210", 128)
            .unwrap()
            .integer()
            .unwrap(),
        0x123456789abcdef0fedcba9876543210
    );
    assert!(RawValue::parse("0x100000000", 32).is_err());
    assert!(RawValue::parse("-1", 32).is_err());
    assert!(RawValue::from_integer(0, 0).is_err());
    assert!(RawValue::from_integer(0, 129).is_err());
    assert!(RawValue::from_bytes(&[0xff, 0xff], 8, true).is_err());
    let malformed: RawValue = serde_json::from_str(r#"{"bits":129,"hex":"0x0"}"#).unwrap();
    assert!(malformed.integer().is_err());
}

#[test]
fn cpsr_discontiguous_it_and_mode_enums() {
    let it = Field {
        name: "IT".into(),
        description: String::new(),
        segments: vec![
            Segment {
                offset: 25,
                width: 2,
            },
            Segment {
                offset: 10,
                width: 6,
            },
        ],
        access: None,
        enums: vec![],
    };
    let value = RawValue::from_integer((2 << 25) | (0b101011 << 10) | 19, 32).unwrap();
    assert_eq!(it.extract(&value).unwrap().integer().unwrap(), 0b10101110);
    let mode = Field {
        name: "M".into(),
        description: String::new(),
        segments: vec![Segment {
            offset: 0,
            width: 5,
        }],
        access: None,
        enums: vec![EnumValue {
            value: "19".into(),
            name: "AArch32_SVC".into(),
        }],
    };
    assert_eq!(
        mode.enum_name(&mode.extract(&value).unwrap()),
        Some("AArch32_SVC")
    );
    assert_eq!(mode.enum_name(&RawValue::from_integer(0, 5).unwrap()), None);
    let full = Field {
        name: "ALL".into(),
        description: String::new(),
        segments: vec![Segment {
            offset: 0,
            width: 128,
        }],
        access: None,
        enums: vec![],
    };
    let max = RawValue::from_integer(u128::MAX, 128).unwrap();
    assert_eq!(full.extract(&max).unwrap(), max);
}

#[test]
fn overlapping_and_out_of_bounds_fields_are_rejected() {
    let mut field = Field {
        name: "bad".into(),
        description: String::new(),
        segments: vec![Segment {
            offset: 30,
            width: 3,
        }],
        access: None,
        enums: vec![],
    };
    assert!(field.validate(32).is_err());
    field.segments = vec![
        Segment {
            offset: 0,
            width: 4,
        },
        Segment {
            offset: 3,
            width: 2,
        },
    ];
    assert!(field.validate(32).is_err());
}

#[test]
fn overlapping_float_views_and_special_values() {
    let d = RawValue::parse("0x3ff000003f800000", 64).unwrap();
    assert_eq!(d.slice(0, 32).unwrap().float().unwrap(), "1");
    assert_eq!(d.slice(32, 32).unwrap().hex, "0x3ff00000");
    assert_eq!(
        RawValue::parse("0x80000000", 32).unwrap().float().unwrap(),
        "-0"
    );
    assert_eq!(
        RawValue::parse("0x7f800000", 32).unwrap().float().unwrap(),
        "inf"
    );
    assert_eq!(
        RawValue::parse("0x7fc00000", 32).unwrap().float().unwrap(),
        "NaN"
    );
    assert!(d.slice(64, 32).is_err());
}

#[test]
fn gic_physical_priority_conditions_5_6_7_and_unknown() {
    let mut register = catalogue().registers.remove(0);
    for (index, min) in [(1, 6), (2, 7), (3, 7)] {
        register.conditions = vec![Condition {
            fact: "icc.physical.prebits".into(),
            min,
            max: None,
        }];
        for prebits in [5, 6, 7] {
            let facts = BTreeMap::from([("icc.physical.prebits".into(), prebits)]);
            let implementation = register.implementation(&facts).0;
            assert_eq!(
                implementation,
                if prebits >= min {
                    Implementation::Yes
                } else {
                    Implementation::No
                },
                "AP{index} with {prebits} bits"
            );
            if implementation == Implementation::No {
                assert!(!register.auto_read(implementation));
            }
        }
        let virtual_only = BTreeMap::from([("icv.virtual.prebits".into(), 7)]);
        assert_eq!(
            register.implementation(&virtual_only).0,
            Implementation::Unknown
        );
    }
    register.access = Access::Wo;
    assert!(!register.auto_read(Implementation::Yes));
    register.access = Access::Ro;
    register.read_side_effect = true;
    assert!(!register.auto_read(Implementation::Yes));
}

#[test]
fn owner_requires_explicit_cluster_topology_and_isolates_clusters() {
    let topology = Topology {
        chip: "tha6206".into(),
        clusters: BTreeMap::from([
            ("core0".into(), "clusterA".into()),
            ("core1".into(), "clusterB".into()),
        ]),
    };
    assert_eq!(
        topology.owner(Scope::Core, "core0").as_deref(),
        Some("core:core0")
    );
    assert_eq!(
        topology.owner(Scope::Cluster, "core0").as_deref(),
        Some("cluster:clusterA")
    );
    assert_eq!(
        topology.owner(Scope::Cluster, "core1").as_deref(),
        Some("cluster:clusterB")
    );
    assert_eq!(topology.owner(Scope::Cluster, "core2"), None);
    assert_eq!(
        topology.owner(Scope::Chip, "core0"),
        topology.owner(Scope::Chip, "core1")
    );
}

#[test]
fn stale_sessions_generations_cores_frames_and_owner_are_rejected() {
    let context = Context {
        session: 1,
        generation: 4,
        core: "core0".into(),
        frame: 0,
    };
    let mut sample = Sample {
        id: "r0".into(),
        state: State::Valid,
        implementation: Implementation::Unknown,
        reason: Reason::Unknown,
        detail: String::new(),
        value: Some(RawValue::parse("1", 32).unwrap()),
        owner: Some("core:core0".into()),
        context: context.clone(),
        timestamp_ms: 0,
        source: "gdb:r0".into(),
    };
    assert!(sample.applies(&context, Some("core:core0")));
    for change in [
        Context {
            session: 2,
            ..context.clone()
        },
        Context {
            generation: 5,
            ..context.clone()
        },
        Context {
            core: "core1".into(),
            ..context.clone()
        },
        Context {
            frame: 1,
            ..context.clone()
        },
    ] {
        assert!(!sample.applies(&change, Some("core:core0")));
    }
    sample.owner = Some("cluster:clusterA".into());
    sample.source = "mmio:stm".into();
    assert!(sample.applies(
        &Context {
            core: "core1".into(),
            ..context.clone()
        },
        Some("cluster:clusterA")
    ));
    assert!(!sample.applies(&context, Some("cluster:clusterB")));
    sample.stale();
    assert_eq!(sample.state, State::Stale);
    assert!(sample.value.is_some());
}

#[test]
fn writers_are_independent_and_reject_width_scope_permission_and_read_effect_conflicts() {
    let catalogue = Catalogue::builtin("cortex-r52").unwrap();
    let r0 = catalogue.register("r0").unwrap();
    assert!(matches!(&r0.writer, Some(Writer::GdbInteger { name }) if name == "r0"));
    assert!(catalogue.register("sctlr").unwrap().writer.is_none());
    assert!(catalogue.register("d0").unwrap().writer.is_none());
    assert!(catalogue.register("cpsr").unwrap().writer.is_none());
    for kind in 0..5 {
        let mut invalid = catalogue.clone();
        let reg = invalid.registers.iter_mut().find(|r| r.id == "r0").unwrap();
        match kind {
            0 => {
                reg.bits = 128;
                reg.write.as_mut().unwrap().bits = 128;
            }
            1 => reg.scope = Scope::Chip,
            2 => reg.access = Access::Ro,
            3 => reg.write = None,
            _ => reg.read_side_effect = true,
        }
        assert!(invalid.validate().is_err());
    }
    let mut legacy = catalogue;
    for reg in &mut legacy.registers {
        reg.writer = None;
        reg.write = None;
    }
    legacy.validate().unwrap();
    assert!(!toml::to_string(&legacy).unwrap().contains("writer"));
}
