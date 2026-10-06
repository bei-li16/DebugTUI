use super::*;

fn field(catalogue: &Catalogue, id: &str, name: &str, raw: u32) -> u128 {
    catalogue
        .register(id)
        .unwrap()
        .fields
        .iter()
        .find(|f| f.name == name)
        .unwrap()
        .extract(&RawValue::from_integer(u128::from(raw), 32).unwrap())
        .unwrap()
        .integer()
        .unwrap()
}

#[test]
fn m_catalogues_share_sourced_ppb_definitions_and_preserve_specific_gdb_views() {
    for cpu in ["cortex-m3", "cortex-m4", "cortex-m7"] {
        let catalogue = Catalogue::builtin(cpu).unwrap();
        assert_eq!(catalogue.cpu, cpu);
        for (id, address) in [
            ("scb.cpuid", 0xe000ed00),
            ("scb.cfsr", 0xe000ed28),
            ("scs.ictr", 0xe000e004),
            ("systick.ctrl", 0xe000e010),
            ("mpu.type", 0xe000ed90),
            ("dcb.dhcsr", 0xe000edf0),
            ("dcb.demcr", 0xe000edfc),
            ("dwt.ctrl", 0xe0001000),
            ("fpb.ctrl", 0xe0002000),
            ("nvic.iser7", 0xe000e11c),
            ("nvic.ipr239", 0xe000e4ef),
        ] {
            let register = catalogue.register(id).unwrap();
            assert!(
                matches!(register.reader, Reader::CorePrivate { address: found } if found == address)
            );
            assert_eq!(register.scope, Scope::Core);
            assert_eq!(register.confidence, metadata::Confidence::High);
            assert!(register.source.is_some());
            assert!(
                register.reset.is_none()
                    && register.verification.is_none()
                    && register.writer.is_none()
            );
        }
        assert!(matches!(
            catalogue.register("r0").unwrap().reader,
            Reader::Gdb { .. }
        ));
        assert_eq!(
            catalogue.register("r0").unwrap().confidence,
            metadata::Confidence::Unknown
        );
        assert!(catalogue.register("sctlr").is_none());
        if cpu == "cortex-m3" {
            assert!(
                catalogue.register("fpu.mvfr0").is_none() && catalogue.register("d0").is_none()
            );
            assert!(catalogue.register("scb.vtor").unwrap().fields_missing);
        } else {
            assert!(matches!(
                catalogue.register("d0").unwrap().reader,
                Reader::Gdb { .. }
            ));
            assert!(matches!(
                catalogue.register("s1").unwrap().reader,
                Reader::Alias { offset: 32, .. }
            ));
            assert!(!catalogue.register("scb.vtor").unwrap().fields_missing);
            assert!(catalogue.register("fpu.fpccr").is_some());
            assert_eq!(
                catalogue
                    .register("d0")
                    .unwrap()
                    .present_if
                    .as_ref()
                    .unwrap()
                    .reg,
                "fpu.mvfr0"
            );
        }
        assert_eq!(
            catalogue.register("scb.itcmcr").is_some(),
            cpu == "cortex-m7"
        );
    }
    let m7 = Catalogue::builtin("cortex-m7").unwrap();
    let command = m7.register("scb.dccimvac").unwrap();
    assert_eq!(command.access, Access::Wo);
    assert_eq!(m7.read_policy(command), (false, false));
    assert!(command.writer.is_none());
}

#[test]
fn m_fault_fields_and_split_fpb_capacity_use_independent_architectural_bits() {
    for cpu in ["cortex-m3", "cortex-m4", "cortex-m7"] {
        let catalogue = Catalogue::builtin(cpu).unwrap();
        for (name, bit) in [
            ("IACCVIOL", 0),
            ("DACCVIOL", 1),
            ("MMARVALID", 7),
            ("IBUSERR", 8),
            ("PRECISERR", 9),
            ("IMPRECISERR", 10),
            ("BFARVALID", 15),
            ("UNDEFINSTR", 16),
            ("INVSTATE", 17),
            ("INVPC", 18),
            ("NOCP", 19),
            ("UNALIGNED", 24),
            ("DIVBYZERO", 25),
        ] {
            assert_eq!(
                field(&catalogue, "scb.cfsr", name, 1 << bit),
                1,
                "{cpu} {name}"
            );
            assert_eq!(
                field(&catalogue, "scb.cfsr", name, !(1 << bit)),
                0,
                "{cpu} {name}"
            );
        }
        assert_eq!(field(&catalogue, "scb.hfsr", "FORCED", 1 << 30), 1);
        assert_eq!(field(&catalogue, "fpb.ctrl", "NUM_CODE", 0x100050a0), 0x5a);
        assert_eq!(field(&catalogue, "fpb.ctrl", "REV", 0x100050a0), 1);
        let dhcsr = catalogue.register("dcb.dhcsr").unwrap();
        assert!(!dhcsr.fields.iter().any(|f| f.name == "DBGKEY"));
        assert!(dhcsr.fields.iter().any(|f| f.name == "S_RESET_ST"));
        for id in ["systick.ctrl", "dcb.dhcsr"] {
            let register = catalogue.register(id).unwrap();
            assert_eq!(catalogue.read_policy(register), (true, true));
            assert!(!catalogue.automatic_read(register, &BTreeMap::new()));
        }
        assert_eq!(
            catalogue.register("systick.calib").unwrap().access,
            Access::Ro
        );
    }
}

#[test]
fn m_common_file_change_propagates_to_three_models_without_erasing_delta_sources() {
    let directory = std::env::temp_dir().join(format!(
        "debugtui-m-common-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&directory).unwrap();
    let common = include_str!("../../profiles/registers/armv7m-common.toml").replace(
        "SCB CPUID; reset and implementation-specific values are not assumed.",
        "Common CPUID override proof",
    );
    assert!(common.contains("Common CPUID override proof"));
    fs::write(directory.join("armv7m-common.toml"), common).unwrap();
    for (cpu, text) in [
        (
            "cortex-m3",
            include_str!("../../profiles/registers/cortex-m3.toml"),
        ),
        (
            "cortex-m4",
            include_str!("../../profiles/registers/cortex-m4.toml"),
        ),
        (
            "cortex-m7",
            include_str!("../../profiles/registers/cortex-m7.toml"),
        ),
    ] {
        let path = directory.join(format!("{cpu}.toml"));
        fs::write(&path, text).unwrap();
        let catalogue = Catalogue::load(&path).unwrap();
        let register = catalogue.register("scb.cpuid").unwrap();
        assert_eq!(register.description, "Common CPUID override proof");
        let origin = register.definition_origin.as_ref().unwrap();
        assert!(origin.declared_in.ends_with("armv7m-common.toml"));
        assert_eq!(origin.inheritance.len(), 2);
        if cpu != "cortex-m3" {
            let delta = catalogue
                .register("scb.cfsr")
                .unwrap()
                .definition_origin
                .as_ref()
                .unwrap();
            assert!(
                delta
                    .overrides
                    .as_ref()
                    .unwrap()
                    .ends_with("armv7m-common.toml")
            );
            assert!(delta.declared_in.ends_with(&format!("{cpu}.toml")));
        }
    }
    // Small isolated temporary fixture, created above; no user files or workspace cleanup.
    for entry in fs::read_dir(&directory).unwrap() {
        fs::remove_file(entry.unwrap().path()).unwrap();
    }
    fs::remove_dir(directory).unwrap();
}
