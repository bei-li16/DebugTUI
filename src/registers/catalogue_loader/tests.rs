use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

fn fixture() -> PathBuf {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "debugtui-inheritance-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    root
}
fn header(cpu: &str, parents: &str) -> String {
    format!(
        "version=2\ncpu='{cpu}'\narchitecture='armv7-m'\nextends=[{parents}]\n[meta]\ndocument='Fixture manual'\nversion='test-1'\nnumber='TEST'\n"
    )
}
fn register(id: &str, replace: bool) -> String {
    format!(
        "[[registers]]\nid='{id}'\noverride={replace}\nname='{id}'\ngroup='core'\nbits=32\naccess='ro'\nreader={{kind='gdb',name='r0'}}\nsource={{section='test',page=1}}\nconfidence='high'\n"
    )
}
fn base() -> String {
    header("base", "") + "[[groups]]\nid='core'\nname='Core'\n" + &register("r0", false)
}

#[test]
fn catalogue_inheritance_resolves_files_named_parents_origins_and_flattened_roundtrip() {
    let root = fixture();
    let before = base();
    fs::write(root.join("base.toml"), &before).unwrap();
    fs::write(
        root.join("middle.toml"),
        header("middle", "'base'") + &register("r1", false),
    )
    .unwrap();
    fs::write(
        root.join("leaf.toml"),
        header("leaf", "'middle.toml'") + &register("r2", false),
    )
    .unwrap();
    let loaded = Catalogue::load(&root.join("leaf.toml")).unwrap();
    assert_eq!(loaded.cpu, "leaf");
    assert_eq!(
        loaded
            .registers
            .iter()
            .map(|r| r.id.as_str())
            .collect::<Vec<_>>(),
        ["r0", "r1", "r2"]
    );
    let inherited = loaded.register("r0").unwrap();
    assert_eq!(
        inherited
            .definition_origin
            .as_ref()
            .unwrap()
            .inheritance
            .len(),
        3
    );
    assert!(
        inherited
            .definition_origin
            .as_ref()
            .unwrap()
            .declared_in
            .ends_with("base.toml")
    );
    assert_eq!(
        inherited.source.as_ref().unwrap().document,
        "Fixture manual"
    );
    assert_eq!(inherited.source.as_ref().unwrap().number, "TEST");
    assert_eq!(inherited.confidence, metadata::Confidence::High);
    let roundtrip = Catalogue::parse(&toml::to_string(&loaded).unwrap()).unwrap();
    assert_eq!(roundtrip.registers.len(), 3);
    assert_eq!(
        roundtrip
            .register("r0")
            .unwrap()
            .definition_origin
            .as_ref()
            .unwrap()
            .declared_in,
        "inline"
    );
    assert_eq!(fs::read_to_string(root.join("base.toml")).unwrap(), before);
}

#[test]
fn catalogue_inheritance_requires_complete_explicit_override_and_preserves_declared_source() {
    let root = fixture();
    fs::write(root.join("base.toml"), base()).unwrap();
    let file = root.join("leaf.toml");
    fs::write(&file, header("leaf", "'base'") + &register("r0", false)).unwrap();
    assert!(
        Catalogue::load(&file)
            .unwrap_err()
            .contains("requires override = true")
    );
    let replacement = register("r0", true)
        .replace("page=1", "page=2")
        .replace("name='r0'", "name='changed'");
    fs::write(&file, header("leaf", "'base'") + &replacement).unwrap();
    let loaded = Catalogue::load(&file).unwrap();
    let changed = loaded.register("r0").unwrap();
    assert_eq!(changed.name, "changed");
    assert_eq!(changed.source.as_ref().unwrap().page, Some(2));
    assert!(
        changed
            .definition_origin
            .as_ref()
            .unwrap()
            .overrides
            .as_ref()
            .unwrap()
            .ends_with("base.toml")
    );
    assert!(
        changed
            .definition_origin
            .as_ref()
            .unwrap()
            .declared_in
            .ends_with("leaf.toml")
    );
    assert_eq!(loaded.registers.len(), 1);
    fs::write(&file, header("leaf", "'base'") + &register("r9", true)).unwrap();
    assert!(
        Catalogue::load(&file)
            .unwrap_err()
            .contains("no inherited definition")
    );
    let incomplete = replacement.replace("bits=32\n", "");
    fs::write(&file, header("leaf", "'base'") + &incomplete).unwrap();
    assert!(Catalogue::load(&file).unwrap_err().contains("bits"));
}

#[test]
fn catalogue_inheritance_rejects_missing_cycles_depth_and_duplicate_local_definitions() {
    let root = fixture();
    let leaf = root.join("leaf.toml");
    for parent in ["'missing'", "'./missing.toml'"] {
        fs::write(&leaf, header("leaf", parent)).unwrap();
        assert!(Catalogue::load(&leaf).is_err());
    }
    fs::write(&leaf, header("leaf", "'base'")).unwrap();
    fs::write(root.join("base.toml"), header("base", "'./leaf.toml'")).unwrap();
    assert!(Catalogue::load(&leaf).unwrap_err().contains("cycle"));
    fs::write(root.join("base.toml"), base()).unwrap();
    fs::write(root.join("middle.toml"), header("middle", "'base'")).unwrap();
    fs::write(&leaf, header("leaf", "'middle'")).unwrap();
    assert!(Catalogue::load(&leaf).is_ok());
    fs::write(root.join("too-deep.toml"), header("too-deep", "'leaf'")).unwrap();
    assert!(
        Catalogue::load(&root.join("too-deep.toml"))
            .unwrap_err()
            .contains("3 layers")
    );
    assert!(
        Catalogue::parse(&(base() + &register("r0", false)))
            .unwrap_err()
            .contains("duplicate local register")
    );
    assert!(
        Catalogue::parse(&(base() + "[[groups]]\nid='core'\nname='Core'\n"))
            .unwrap_err()
            .contains("duplicate local group")
    );
    assert!(
        Catalogue::parse(&header("inline", "'./base.toml'"))
            .unwrap_err()
            .contains("declaring catalogue file")
    );
}

#[test]
fn catalogue_inheritance_rejects_ambiguous_parents_group_conflicts_and_broken_local_overrides() {
    let root = fixture();
    fs::write(root.join("first.toml"), base()).unwrap();
    fs::write(root.join("second.toml"), base()).unwrap();
    let leaf = root.join("leaf.toml");
    fs::write(&leaf, header("leaf", "'first','second'")).unwrap();
    assert!(
        Catalogue::load(&leaf)
            .unwrap_err()
            .contains("ambiguous parent register")
    );
    fs::write(&leaf, header("leaf", "'first','first'")).unwrap();
    assert!(
        Catalogue::load(&leaf)
            .unwrap_err()
            .contains("distinct parents")
    );
    fs::write(
        &leaf,
        header("leaf", "'first'") + "[[groups]]\nid='core'\nname='Different'\n",
    )
    .unwrap();
    assert!(
        Catalogue::load(&leaf)
            .unwrap_err()
            .contains("conflicting group")
    );
    // Existing malformed local named parents must not silently select an embedded one.
    fs::write(root.join("cortex-m4.toml"), "not TOML").unwrap();
    fs::write(
        &leaf,
        "version=1\ncpu='leaf'\narchitecture='armv7-m'\nextends=['cortex-m4']\n",
    )
    .unwrap();
    assert!(Catalogue::load(&leaf).is_err());
    let embedded =
        Catalogue::parse("version=1\ncpu='leaf'\narchitecture='armv7-m'\nextends=['cortex-m4']\n")
            .unwrap();
    assert!(
        embedded
            .register("r0")
            .unwrap()
            .definition_origin
            .as_ref()
            .unwrap()
            .declared_in
            .starts_with("builtin:cortex-m4")
    );
}

#[test]
fn catalogue_metadata_validates_source_reset_confidence_and_hardware_evidence() {
    let valid = base();
    for malformed in [
        valid.replace("source={section='test',page=1}\n", ""),
        valid.replace("page=1", "page=0"),
        valid.replace("page=1", "page=1,unknown=true"),
        valid.replace("version='test-1'", "version=''"),
        valid.replace("confidence='high'", "confidence='guessed'"),
        valid.replace("confidence='high'", "confidence='verified'"),
        valid.replace("confidence='high'", "reset=0x100000000\nconfidence='high'"),
    ] {
        assert!(Catalogue::parse(&malformed).is_err(), "{malformed}");
    }
    let known =
        Catalogue::parse(&valid.replace("confidence='high'", "reset=0x1234\nconfidence='medium'"))
            .unwrap();
    assert_eq!(known.register("r0").unwrap().reset, Some(0x1234));
    assert!(
        known
            .register("r0")
            .unwrap()
            .definition_details()
            .join("\n")
            .contains("Reset: 0x1234")
    );
    let source_line = Catalogue::parse(&valid.replace(
        "page=1",
        "line=123,url='https://example.org/pinned/header.h'",
    ))
    .unwrap();
    assert_eq!(
        source_line
            .register("r0")
            .unwrap()
            .source
            .as_ref()
            .unwrap()
            .line,
        Some(123)
    );
    let verified = valid.replace("confidence='high'", "confidence='verified'\nverification={date='2026-10-06',board='fixture only',firmware='fixture',report='test report'}");
    assert!(Catalogue::parse(&verified).is_ok());
    assert!(Catalogue::parse(&verified.replace("2026-10-06", "2026-02-30")).is_err());
    assert!(Catalogue::parse(&verified.replace("firmware='fixture'", "firmware=''")).is_err());
    let legacy = valid
        .replace("version=2", "version=1")
        .replace("source={section='test',page=1}\n", "")
        .replace("confidence='high'\n", "");
    let legacy = Catalogue::parse(&legacy).unwrap();
    let old = legacy.register("r0").unwrap();
    assert!(old.reset.is_none() && old.source.is_none());
    assert_eq!(old.confidence, metadata::Confidence::Unknown);
    assert!(
        old.definition_details()
            .join("\n")
            .contains("Manual source: Unknown")
    );
}

#[test]
fn catalogue_representative_definitions_preserve_fields_conditions_side_effects_and_sources() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("profiles/registers/examples");
    let catalogue = Catalogue::load(&root.join("r52-source-sample.toml")).unwrap();
    assert_eq!(catalogue.registers.len(), 5);
    assert!(catalogue.registers.iter().all(|r| r.source.is_some()
        && r.confidence != metadata::Confidence::Verified
        && r.writer.is_none()
        && r.reset.is_none()));
    let midr = catalogue.register("midr").unwrap();
    assert_eq!(midr.fields.len(), 5);
    let part = midr.fields.iter().find(|f| f.name == "PartNum").unwrap();
    assert_eq!(
        part.extract(&RawValue::parse("0x411fd135", 32).unwrap())
            .unwrap()
            .hex,
        "0xd13"
    );
    let mpu = catalogue.register("prbar0").unwrap();
    assert_eq!(
        catalogue.implementation(mpu, &BTreeMap::new()).0,
        Implementation::Unknown
    );
    assert_eq!(
        catalogue
            .implementation(mpu, &BTreeMap::from([("mpu.el1.regions".into(), 0)]))
            .0,
        Implementation::No
    );
    assert_eq!(
        catalogue
            .implementation(mpu, &BTreeMap::from([("mpu.el1.regions".into(), 16)]))
            .0,
        Implementation::Yes
    );
    let sticky = catalogue.register("edprsr").unwrap();
    assert!(sticky.read_side_effect && sticky.fields_missing);
    assert_eq!(catalogue.read_policy(sticky), (true, true));
    assert!(!catalogue.automatic_read(
        sticky,
        &BTreeMap::from([("debug_external.present".into(), 1)])
    ));
    let alias = catalogue.register("midr_partnum").unwrap();
    assert_eq!(
        catalogue
            .read_dependencies(alias)
            .unwrap()
            .iter()
            .map(|r| r.id.as_str())
            .collect::<Vec<_>>(),
        ["midr_partnum", "midr"]
    );
    assert!(
        midr.definition_origin
            .as_ref()
            .unwrap()
            .declared_in
            .ends_with("r52-source-common.toml")
    );
    assert!(
        mpu.definition_origin
            .as_ref()
            .unwrap()
            .overrides
            .as_ref()
            .unwrap()
            .ends_with("r52-source-common.toml")
    );
    assert_eq!(sticky.source.as_ref().unwrap().version, "M.b");
    assert_eq!(sticky.source.as_ref().unwrap().page, Some(15261));
    assert!(catalogue.confidence_summary().contains("high 4"));
    assert!(catalogue.confidence_summary().contains("medium 1"));
}

#[test]
fn catalogue_inheritance_bounds_wide_graphs_and_does_not_trust_serialized_origin() {
    let root = fixture();
    let leaf = root.join("leaf.toml");
    for parent in 0..4 {
        let mut names = Vec::new();
        for grand in 0..4 {
            let name = format!("p{parent}g{grand}");
            fs::write(root.join(format!("{name}.toml")), header(&name, "")).unwrap();
            names.push(format!("'{name}'"));
        }
        fs::write(
            root.join(format!("p{parent}.toml")),
            header(&format!("p{parent}"), &names.join(",")),
        )
        .unwrap();
    }
    fs::write(&leaf, header("leaf", "'p0','p1','p2','p3'")).unwrap();
    assert!(
        Catalogue::load(&leaf)
            .unwrap_err()
            .contains("resource limits")
    );
    let serialized =
        base() + "[registers.definition_origin]\ndeclared_in='FORGED'\ninheritance=['FORGED']\n";
    assert!(
        Catalogue::parse(&serialized)
            .unwrap_err()
            .contains("definition_origin")
    );
}
