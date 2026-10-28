use super::*;

pub fn elf32(address: u32) -> Vec<u8> {
    let mut b = vec![0; 512];
    b[..7].copy_from_slice(b"\x7fELF\x01\x01\x01");
    for (offset, value) in [(16, 2_u16), (18, 40), (40, 52), (46, 40), (48, 4), (50, 2)] {
        b[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    b[32..36].copy_from_slice(&256_u32.to_le_bytes());
    let names = b"\0.text\0.shstrtab\0.debug_info\0";
    b[128..128 + names.len()].copy_from_slice(names);
    for (index, name, flags, addr, offset, size) in [
        (1, 1, 6, address, 64, 32),
        (2, 7, 0, 0, 128, names.len() as u32),
        (3, 17, 0, 0, 96, 1),
    ] {
        let at = 256 + index * 40;
        for (field, value) in [(0, name), (8, flags), (12, addr), (16, offset), (20, size)] {
            b[at + field..at + field + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
    b
}

#[test]
fn only_executable_sections_identify_images_and_data_never_claims_a_domain() {
    let definition = Image {
        id: "vm0".into(),
        domain: Domain::Guest,
        ..Default::default()
    };
    let image = parse(&definition, &elf32(0x10020000)).unwrap();
    assert_eq!(image.machine, 40);
    assert_eq!(image.executable_ranges.len(), 1);
    assert!(image.has_debug_info);
    assert_eq!(
        resolve(std::slice::from_ref(&image), 0x10020008, Some(0x13))
            .image
            .as_deref(),
        Some("vm0")
    );
    assert_eq!(
        resolve(std::slice::from_ref(&image), 0x10020008, Some(0x1a)).state,
        "unknown"
    );
    assert_eq!(resolve(&[image], 0x10027000, None).state, "unknown");
}

#[test]
fn explicit_image_prefix_retains_windows_paths_and_cpp_namespaces() {
    assert_eq!(
        scoped_expression("@vm0:D:\\src\\main.c:42").unwrap(),
        (Some("vm0"), "D:\\src\\main.c:42")
    );
    assert_eq!(
        scoped_expression("@vm1:ns::object.field").unwrap(),
        (Some("vm1"), "ns::object.field")
    );
    assert_eq!(
        scoped_expression("ns::object").unwrap(),
        (None, "ns::object")
    );
    for bad in ["@", "@:x", "@vm0:", "@bad id:x", "@vm0:   "] {
        assert!(scoped_expression(bad).is_err(), "{bad}");
    }
    let image = parse(
        &Image {
            offset: 0x2000,
            ..Default::default()
        },
        &elf32(0x1000),
    )
    .unwrap();
    assert_eq!(
        (
            image.allocated_ranges[0].linked_start,
            image.allocated_ranges[0].start,
            image.allocated_ranges[0].end
        ),
        (0x1000, 0x3000, 0x3020)
    );
}

#[test]
fn overlapping_images_are_ambiguous_until_a_real_mode_constraint_disambiguates() {
    let first = parse(
        &Image {
            id: "bao".into(),
            domain: Domain::Hyp,
            ..Default::default()
        },
        &elf32(0x08000000),
    )
    .unwrap();
    let second = parse(
        &Image {
            id: "vm0".into(),
            domain: Domain::Guest,
            ..Default::default()
        },
        &elf32(0x08000000),
    )
    .unwrap();
    let images = [first, second];
    assert_eq!(resolve(&images, 0x08000001, None).state, "ambiguous");
    assert_eq!(
        resolve(&images, 0x08000001, Some(0x1a)).image.as_deref(),
        Some("bao")
    );
    assert_eq!(
        resolve(&images, 0x08000001, Some(0x13)).image.as_deref(),
        Some("vm0")
    );
}

#[test]
fn relocation_and_section_overrides_are_bounded_and_content_versions_detect_rebuilds() {
    let definition = Image {
        id: "vm".into(),
        offset: 0x2000,
        ..Default::default()
    };
    let bytes = elf32(0x1000);
    let first = parse(&definition, &bytes).unwrap();
    assert_eq!(first.executable_ranges[0].start, 0x3000);
    let lowered = parse(
        &Image {
            offset: -0x800,
            ..definition.clone()
        },
        &bytes,
    )
    .unwrap();
    assert_eq!(lowered.executable_ranges[0].start, 0x800);
    let mut replaced = bytes.clone();
    replaced[70] = 9;
    assert_ne!(
        first.content_revision,
        parse(&definition, &replaced).unwrap().content_revision
    );
    let override_image = Image {
        sections: BTreeMap::from([(".text".into(), 0x8000)]),
        ..definition.clone()
    };
    assert_eq!(
        parse(&override_image, &bytes).unwrap().executable_ranges[0].start,
        0x8000
    );
    assert!(
        parse(
            &Image {
                offset: -0x2000,
                ..definition.clone()
            },
            &bytes
        )
        .is_err()
    );
    assert!(
        parse(
            &Image {
                sections: BTreeMap::from([(".missing".into(), 0)]),
                ..definition
            },
            &bytes
        )
        .is_err()
    );
    for length in [0, 4, 51, 255, 415] {
        assert!(parse(&Image::default(), &bytes[..length]).is_err());
    }
    let mut corrupt = bytes.clone();
    corrupt[32..36].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(parse(&Image::default(), &corrupt).is_err());
}

#[test]
fn image_configuration_retains_legacy_projects_and_per_core_ownership() {
    let legacy: Project = toml::from_str("[program]\nelf='bao.elf'\n").unwrap();
    assert!(legacy.images.is_empty());
    let project:Project=toml::from_str("[program]\nelf='bao.elf'\n[[cores]]\nname='core.0'\n[[cores]]\nname='core.1'\n[[images]]\nid='vm0'\nelf='guest0.elf'\ncores=['core.0']\ndomain='guest'\n[[images]]\nid='vm1'\nelf='guest1.elf'\ncores=['core.1']\n").unwrap();
    validate(&project).unwrap();
    let mut worker = project.clone();
    worker.preference_core = Some("core.1".into());
    assert_eq!(applicable(&worker)[0].id, "vm1");
    assert_eq!(applicable(&worker).len(), 1);
    let text = toml::to_string(&project).unwrap();
    let restored: Project = toml::from_str(&text).unwrap();
    assert_eq!(restored.images[0].domain, Domain::Guest);
    worker.images[0].cores = vec!["missing".into()];
    assert!(validate(&worker).is_err());
    worker = project.clone();
    worker.images[1].id = "vm0".into();
    assert!(validate(&worker).is_err());
    assert!(
        toml::from_str::<Project>("[[images]]\nid='vm'\nelf='vm.elf'\nunsafe_reader='mrc'\n")
            .is_err()
    );
}
