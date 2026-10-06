use super::*;
use crate::registers::CoreConfig;

fn fixture() -> PathBuf {
    let path = env::temp_dir().join(format!(
        "debugtui-core-register-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&path).unwrap();
    fs::create_dir(path.join("tools")).unwrap();
    let definition = "version=1\ncpu='cortex-m7'\narchitecture='armv7e-m'\n[[groups]]\nid='core'\nname='Core'\n[[registers]]\nid='r0'\nname='R0'\ngroup='core'\nbits=32\naccess='ro'\nreader={kind='gdb',name='r0'}\n";
    fs::write(path.join("m7.toml"), definition).unwrap();
    fs::write(
        path.join("tools/default.toml"),
        definition.replace("cortex-m7", "cortex-m4"),
    )
    .unwrap();
    path
}

#[test]
fn per_core_register_selection_inherits_defaults_and_preserves_explicit_empty_values() {
    let defaults = crate::registers::Config {
        cpu: "cortex-r52".into(),
        catalogue: "customer.toml".into(),
        tcl_endpoint: "localhost:6666".into(),
        cp15_command: "aarch64 mrc".into(),
        targets: BTreeMap::from([("m7".into(), "soc.cpu0".into())]),
        ..Default::default()
    };
    let overrides: CoreConfig =
        toml::from_str("cpu='cortex-m7'\ncatalogue=''\ncp15_command=''\n").unwrap();
    let effective = overrides.apply(&defaults);
    assert_eq!(effective.cpu, "cortex-m7");
    assert!(effective.catalogue.as_os_str().is_empty() && effective.cp15_command.is_empty());
    assert_eq!(effective.targets, defaults.targets);
    assert_eq!(effective.tcl_endpoint, defaults.tcl_endpoint);
    assert_eq!(defaults.cpu, "cortex-r52");
    assert_eq!(defaults.catalogue, PathBuf::from("customer.toml"));
    let disabled: CoreConfig = toml::from_str("cpu=''\ncatalogue=''\n").unwrap();
    assert!(disabled.apply(&defaults).load().unwrap().is_none());
    assert!(
        !toml::to_string(&Core::default())
            .unwrap()
            .contains("registers")
    );
}

#[test]
fn per_core_register_paths_and_files_override_profile_defaults_without_mutation() {
    let root = fixture();
    let profile = "[registers]\ncpu='cortex-m4'\ncatalogue='default.toml'\n";
    fs::write(root.join("tools/debug-env.toml"), profile).unwrap();
    let project = "version=2\n[tools]\nprofile='tools/debug-env.toml'\n[[cores]]\nname='m7'\nendpoint='localhost:3333'\n[cores.registers]\ncpu='cortex-m7'\ncatalogue='m7.toml'\n[[cores]]\nname='m4'\nendpoint='localhost:3334'\n";
    fs::write(root.join("debug.toml"), project).unwrap();
    let loaded = Project::load(&root.join("debug.toml")).unwrap();
    let m7 = loaded.registers_for_core("m7");
    let m4 = loaded.registers_for_core("m4");
    assert_eq!(m7.catalogue, root.join("m7.toml"));
    assert_eq!(m4.catalogue, root.join("tools/default.toml"));
    assert_eq!(m7.load().unwrap().unwrap().0.cpu, "cortex-m7");
    assert_eq!(m4.load().unwrap().unwrap().0.cpu, "cortex-m4");
    assert_eq!(loaded.registers.cpu, "cortex-m4");
    assert_eq!(
        fs::read_to_string(root.join("debug.toml")).unwrap(),
        project
    );
    assert_eq!(
        fs::read_to_string(root.join("tools/debug-env.toml")).unwrap(),
        profile
    );
}

#[test]
fn per_core_register_routes_replace_only_supplied_maps_and_validate_after_inheritance() {
    let mut project = Project::default();
    project.registers.cp15_command = "aarch64 mrc".into();
    project.registers.selector_command = "aarch64 mcr".into();
    project.registers.tcl_endpoint = "localhost:6666".into();
    project
        .registers
        .targets
        .insert("m7".into(), "soc.cpu0".into());
    project.registers.facts.insert("capacity".into(), 4);
    project.cores = vec![Core {
        name: "m7".into(), endpoint: "localhost:3333".into(),
        registers: Some(toml::from_str("tcl_endpoint='localhost:6667'\ncp15_command='arm mrc'\nselector_command='arm mcr'\n[targets]\nm7='soc.other'\n[facts]\ncapacity=8\n").unwrap()),
        ..Default::default()
    }, Core { name: "m4".into(), endpoint: "localhost:3334".into(), ..Default::default() }];
    project.validate().unwrap();
    let selected = project.registers_for_core("m7");
    assert_eq!(selected.cp15_command, "arm mrc");
    assert_eq!(selected.selector_command, "arm mcr");
    assert_eq!(selected.tcl_endpoint, "localhost:6667");
    assert_eq!(selected.targets["m7"], "soc.other");
    assert_eq!(selected.facts["capacity"], 8);
    assert_eq!(project.registers_for_core("m4").facts["capacity"], 4);
    assert_eq!(
        project.registers_for_core("m4").tcl_endpoint,
        "localhost:6666"
    );
    project.cores[0]
        .registers
        .as_mut()
        .unwrap()
        .selector_command = Some("aarch64 mcr".into());
    let error = project.validate().unwrap_err();
    assert!(error.contains("Core m7 registers") && error.contains("match"));
    assert!(toml::from_str::<CoreConfig>("unknown=true").is_err());
    assert!(toml::from_str::<CoreConfig>("cpu=7").is_err());
}

#[test]
fn per_core_register_invalid_file_is_a_core_error_and_never_falls_back() {
    let root = fixture();
    fs::write(root.join("debug.toml"), "[[cores]]\nname='m7'\nendpoint='localhost:3333'\n[cores.registers]\ncpu='cortex-m4'\ncatalogue='missing.toml'\n").unwrap();
    let error = Project::load(&root.join("debug.toml")).unwrap_err();
    assert!(error.contains("Core m7 registers") && error.contains("missing.toml"));
}
