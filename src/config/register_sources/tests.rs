use super::*;
use crate::config::core_register_tests::fixture;

fn loaded(project: &str, profile: &str) -> Project {
    let root = fixture();
    fs::write(root.join("tools/debug-env.toml"), profile).unwrap();
    fs::write(root.join("debug.toml"), project).unwrap();
    let catalogue = crate::devices::Catalogue::parse(
        "version=1\n[devices.matrix]\ncores=[0,2]\nbackend='generic'\ncpu='cortex-m4'\n",
    )
    .unwrap();
    Project::from_document_with_catalogue(
        toml::from_str(project).unwrap(),
        Some(root.join("debug.toml")),
        None,
        Some(&catalogue),
    )
    .unwrap()
}
fn setting<'a>(report: &'a Report, path: &[&str]) -> &'a Setting {
    report
        .settings
        .iter()
        .find(|setting| setting.path == path)
        .unwrap()
}
const PROJECT: &str =
    "version=3\n[tools]\nprofile='tools/debug-env.toml'\n[debug]\nchip='matrix'\ncores=[0,2]\n";
const PROFILE: &str = "backend='generic'\n[core_targets.'0']\nendpoint='localhost:3333'\n[core_targets.'2']\nendpoint='localhost:3334'\n";

#[test]
fn register_sources_preserve_profile_backend_project_leaf_history_and_relative_paths() {
    let project = format!(
        "{PROJECT}[registers]\ncpu='cortex-m7'\ncatalogue=''\n[registers.facts]\n'route.capacity'=3\n"
    );
    let profile = format!(
        "{PROFILE}[registers]\ncpu='cortex-r52'\ncatalogue='default.toml'\n[registers.facts]\n'route.capacity'=1\ninherited=4\n[backends.generic.registers]\ncpu='cortex-m4'\n[backends.generic.registers.facts]\n'route.capacity'=2\nbackend_only=5\n"
    );
    let project = loaded(&project, &profile);
    let report = project.register_configuration("core.2");
    let cpu = setting(&report, &["cpu"]);
    assert_eq!(cpu.value, "cortex-m7");
    assert!(cpu.source.ends_with("debug.toml [registers]"));
    assert_eq!(
        cpu.overrides
            .iter()
            .map(|declaration| declaration.value.clone())
            .collect::<Vec<_>>(),
        vec![
            Value::from(""),
            Value::from("cortex-r52"),
            Value::from("cortex-m4")
        ]
    );
    assert!(
        cpu.overrides[1]
            .source
            .ends_with("debug-env.toml [registers]")
    );
    assert!(
        cpu.overrides[2]
            .source
            .ends_with("[backends.generic.registers]")
    );
    let catalogue = setting(&report, &["catalogue"]);
    assert_eq!(catalogue.value, "");
    assert!(
        catalogue
            .overrides
            .last()
            .unwrap()
            .value
            .as_str()
            .unwrap()
            .replace('\\', "/")
            .ends_with("/tools/default.toml")
    );
    let capacity = setting(&report, &["facts", "route.capacity"]);
    assert_eq!(capacity.value, 3);
    assert_eq!(
        capacity
            .overrides
            .iter()
            .map(|declaration| declaration.value.clone())
            .collect::<Vec<_>>(),
        vec![Value::from(1), Value::from(2)]
    );
    assert!(
        setting(&report, &["facts", "inherited"])
            .source
            .ends_with("[registers]")
    );
    assert!(
        setting(&report, &["facts", "backend_only"])
            .source
            .ends_with("[backends.generic.registers]")
    );
    assert!(
        report
            .settings
            .iter()
            .all(|setting| !setting.source.contains("origin unavailable"))
    );
    let serialized: toml::Value = toml::from_str(&toml::to_string(&project).unwrap()).unwrap();
    assert!(serialized.get("register_sources").is_none());
    assert!(serialized["registers"].get("overrides").is_none());
    assert!(
        serde_json::to_value(&project)
            .unwrap()
            .get("register_sources")
            .is_none()
    );
    assert!(toml::from_str::<Project>("[register_sources]\nsource='pretend-trusted'\n").is_err());
}

#[test]
fn register_sources_distinguish_global_empty_merge_core_map_replacement_and_explicit_clear() {
    let mut project = loaded(
        &format!(
            "{PROJECT}[registers.facts]\n[[cores]]\nname='core.0'\n[cores.registers]\ncpu=''\ncatalogue=''\n[cores.registers.facts]\n[[cores]]\nname='core.2'\n[cores.registers.facts]\nselected=9\n"
        ),
        &format!("{PROFILE}[registers]\ncpu='cortex-m4'\n[registers.facts]\ninherited=4\n"),
    );
    assert_eq!(
        project.registers.facts["inherited"], 4,
        "project empty table recursively merges"
    );
    let first = project.register_configuration("core.0");
    assert_eq!(setting(&first, &["cpu"]).value, "");
    assert!(
        setting(&first, &["cpu"])
            .source
            .ends_with("[cores.registers for core.0]")
    );
    assert_eq!(setting(&first, &["facts"]).value, serde_json::json!({}));
    let replacement = first
        .replacements
        .iter()
        .find(|replacement| replacement.field == "facts")
        .unwrap();
    assert_eq!(replacement.removed_paths, vec![vec!["facts", "inherited"]]);
    assert!(replacement.replaces_sources[0].ends_with("debug-env.toml [registers]"));
    let second = project.register_configuration("core.2");
    assert_eq!(setting(&second, &["facts", "selected"]).value, 9);
    assert!(
        second
            .settings
            .iter()
            .all(|setting| setting.path != ["facts", "inherited"])
    );
    assert_eq!(project.registers.facts["inherited"], 4);
    project.cores[0].registers.as_mut().unwrap().facts =
        Some(BTreeMap::from([("runtime".into(), 1)]));
    let current = project.register_configuration("core.0");
    assert!(
        setting(&current, &["facts", "runtime"])
            .source
            .contains("origin unavailable")
    );
    assert!(
        current
            .replacements
            .iter()
            .all(|replacement| replacement.field != "facts")
    );
}

#[test]
fn register_sources_chip_defaults_are_associations_and_never_project_or_observed_identity() {
    let project = loaded(PROJECT, PROFILE);
    let report = project.register_configuration("core.0");
    let cpu = setting(&report, &["cpu"]);
    assert_eq!(cpu.value, "cortex-m4");
    assert_eq!(
        cpu.source,
        "chip association:matrix (supplied device catalogue)"
    );
    assert_eq!(cpu.overrides[0].source, "builtin defaults");
    let cleared = loaded(
        &format!("{PROJECT}[registers]\ncpu=''\ncatalogue=''\n"),
        PROFILE,
    );
    let report = cleared.register_configuration("core.0");
    assert!(setting(&report, &["cpu"]).source.ends_with("[registers]"));
    assert!(!report.lines().join("\n").contains("chip association:"));
}

#[test]
fn register_sources_reject_environment_core_arrays_instead_of_inventing_project_origins() {
    let root = fixture();
    let catalogue = crate::devices::Catalogue::parse(
        "version=1\n[devices.matrix]\ncores=[0,2]\nbackend='generic'\ncpu='cortex-m4'\n",
    )
    .unwrap();
    for (section, expected) in [
        ("cores", "Unsupported environment section: cores"),
        (
            "backends.generic.cores",
            "Unsupported backend section: cores",
        ),
    ] {
        let profile = format!(
            "{PROFILE}[[{section}]]\nname='core.0'\n[{section}.registers]\ncpu='cortex-m7'\n"
        );
        fs::write(root.join("tools/debug-env.toml"), &profile).unwrap();
        let error = Project::from_document_with_catalogue(
            toml::from_str(PROJECT).unwrap(),
            Some(root.join("debug.toml")),
            None,
            Some(&catalogue),
        )
        .unwrap_err();
        assert_eq!(error, expected);
        assert_eq!(
            fs::read_to_string(root.join("tools/debug-env.toml")).unwrap(),
            profile
        );
    }
}

#[test]
fn register_sources_explicit_environment_and_runtime_changes_do_not_borrow_old_origins() {
    let root = fixture();
    let selected = root.join("tools/explicit.toml");
    fs::write(
        &selected,
        "[registers]\ncpu='cortex-m4'\ntcl_endpoint='localhost:6668'\n",
    )
    .unwrap();
    let mut project = Project::from_document(
        toml::from_str("[tools]\nprofile='missing.toml'\n").unwrap(),
        Some(root.join("debug.toml")),
        Some(&selected),
    )
    .unwrap();
    let report = project.register_configuration("default");
    let endpoint = setting(&report, &["tcl_endpoint"]);
    assert!(endpoint.source.ends_with("explicit.toml [registers]"));
    assert_eq!(endpoint.value, "localhost:6668");
    project.registers.tcl_endpoint = "localhost:6670".into();
    let report = project.register_configuration("default");
    let endpoint = setting(&report, &["tcl_endpoint"]);
    assert_eq!(endpoint.value, "localhost:6670");
    assert!(endpoint.source.contains("origin unavailable"));
    assert!(
        endpoint
            .overrides
            .last()
            .unwrap()
            .source
            .ends_with("explicit.toml [registers]")
    );
}

#[test]
fn register_sources_paths_and_non_template_values_keep_their_declaring_layers() {
    let root = fixture();
    let project = format!(
        "{PROJECT}[registers]\ncatalogue='m7.toml'\n[[cores]]\nname='core.2'\n[cores.registers]\ncatalogue='m7.toml'\n"
    );
    let profile = format!(
        "{PROFILE}[registers]\ncpu='cortex-m4'\ntcl_endpoint='localhost:6666'\n[registers.targets]\n'core.0'='${{chip}}.cpu0'\n'core.2'='${{chip}}.cpu2'\n"
    );
    fs::write(root.join("tools/debug-env.toml"), &profile).unwrap();
    let catalogue = crate::devices::Catalogue::parse(
        "version=1\n[devices.matrix]\ncores=[0,2]\nbackend='generic'\n",
    )
    .unwrap();
    let project = Project::from_document_with_catalogue(
        toml::from_str(&project).unwrap(),
        Some(root.join("debug.toml")),
        None,
        Some(&catalogue),
    )
    .unwrap();
    let report = project.register_configuration("core.2");
    assert_eq!(
        setting(&report, &["targets", "core.2"]).value,
        "${chip}.cpu2"
    );
    assert!(
        setting(&report, &["targets", "core.2"])
            .source
            .ends_with("debug-env.toml [registers]")
    );
    assert!(
        setting(&report, &["catalogue"])
            .source
            .ends_with("[cores.registers for core.2]")
    );
    assert!(
        setting(&report, &["catalogue"])
            .value
            .as_str()
            .unwrap()
            .replace('\\', "/")
            .ends_with("/m7.toml")
    );
}
