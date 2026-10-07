use super::*;
use crate::devices::{Catalogue, DEFAULTS};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = env::temp_dir().join(format!(
            "debugtui-chip-files-{}-{}-中文 空格",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("tools/devices/family")).unwrap();
        fs::create_dir_all(root.join("project")).unwrap();
        let f = Self(root);
        f.write(
            "tools/debug-env.toml",
            "[chip_profiles]\ndirectory='devices'\n[gdb]\nexecutable='./gdb.exe'\n",
        );
        f
    }
    fn write(&self, file: &str, content: &str) {
        fs::write(self.0.join(file), content).unwrap();
    }
    fn project(&self, extra: &str) -> Result<Project, String> {
        let text = format!(
            "version=3\n[tools]\nprofile='../tools/debug-env.toml'\n[debug]\nchip='stm32f429'\ncores=[0]\n{extra}"
        );
        Project::from_document_with_catalogue(
            toml::from_str(&text).unwrap(),
            Some(self.0.join("project/debug.toml")),
            None,
            Some(&Catalogue::parse(DEFAULTS).unwrap()),
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn paths_inherited_from_each_file_and_project_overrides_keep_their_own_base() {
    let f = Fixture::new();
    f.write("tools/devices/family/base.toml", "backend='stm32f4'\n[program]\nsvd='${profile_dir}/svd/${chip_upper}.svd'\n[gdb]\nexecutable='./parent-gdb.exe'\n[service]\ncommand='./server.exe'\ncwd='.'\nargs=['${tools_dir}/scripts','${profile_dir}/target.cfg','${chip}']\n[core_targets.\"0\"]\nendpoint='localhost:3333'\n[registers]\ncpu='cortex-m4'\ntcl_endpoint='localhost:6666'\n");
    f.write("tools/devices/stm32f429.toml", "extends='family/base.toml'\n[gdb]\nexecutable='./child-gdb.exe'\n[registers]\ntcl_endpoint='localhost:7777'\n");
    let before = fs::read(f.0.join("tools/devices/stm32f429.toml")).unwrap();
    let p = f.project("").unwrap();
    assert!(p.gdb.executable.ends_with("tools/devices/child-gdb.exe"));
    assert!(
        p.service
            .as_ref()
            .unwrap()
            .command
            .ends_with("tools/devices/family/server.exe")
    );
    assert!(
        p.service
            .as_ref()
            .unwrap()
            .cwd
            .as_ref()
            .unwrap()
            .ends_with("tools/devices/family")
    );
    assert!(portable_path(&p.program.svd).ends_with("tools/devices/family/svd/STM32F429.svd"));
    assert!(p.service.as_ref().unwrap().args[0].ends_with("tools/scripts"));
    assert!(p.service.as_ref().unwrap().args[1].ends_with("tools/devices/family/target.cfg"));
    assert_eq!(p.service.as_ref().unwrap().args[2], "stm32f429");
    let report = p.register_configuration("core.0");
    let setting = report
        .settings
        .iter()
        .find(|s| s.path == ["tcl_endpoint"])
        .unwrap();
    assert!(setting.source.contains("devices/stm32f429.toml"));
    assert!(
        setting
            .overrides
            .iter()
            .any(|d| d.source.contains("family/base.toml"))
    );
    let p = f.project("[program]\nsvd='./own.svd'\n[gdb]\nexecutable='./project-gdb.exe'\n[registers]\ntcl_endpoint='localhost:8888'").unwrap();
    assert!(p.program.svd.ends_with("project/own.svd"));
    assert!(p.gdb.executable.ends_with("project/project-gdb.exe"));
    assert_eq!(p.registers.tcl_endpoint, "localhost:8888");
    assert!(
        f.project("[program]\nsvd=''")
            .unwrap()
            .program
            .svd
            .as_os_str()
            .is_empty()
    );
    assert_eq!(
        fs::read(f.0.join("tools/devices/stm32f429.toml")).unwrap(),
        before
    );
}

#[test]
fn invalid_or_missing_chip_files_are_errors_even_with_project_overrides() {
    let f = Fixture::new();
    let error = f.project("").unwrap_err();
    assert!(error.contains("stm32f429.toml"));
    for (text, expected) in [
        ("backend='tha6'", "does not match"),
        ("backend=3", "backend must be a string"),
        (
            "[target]\nendpoint='localhost:3333'",
            "must declare backend",
        ),
        (
            "backend='stm32f4'\n[unknown]\nx=1",
            "Unsupported chip profile section",
        ),
        (
            "backend='stm32f4'\n[program]\nelf='wrong.elf'",
            "Unsupported environment program field",
        ),
        ("backend='stm32f4'\n[program]\nsvd=42", "must be a string"),
        ("backend='stm32f4'\nextends=['a.toml']", "extends must be"),
        ("backend='stm32f4'\nextends='missing.toml'", "missing.toml"),
        ("broken=[", "Chip profile"),
    ] {
        f.write("tools/devices/stm32f429.toml", text);
        let error = f.project("[program]\nsvd=''\nelf='own.elf'").unwrap_err();
        assert!(error.contains(expected), "{text}: {error}");
    }
    f.write(
        "tools/devices/stm32f429.toml",
        "extends='family/base.toml'\n[program]\nsvd=''",
    );
    f.write(
        "tools/devices/family/base.toml",
        "backend='stm32f4'\n[program]\nsource_root='bad'",
    );
    assert!(f.project("").unwrap_err().contains("source_root"));
    f.write(
        "tools/devices/family/base.toml",
        "extends='../stm32f429.toml'",
    );
    assert!(f.project("").unwrap_err().contains("cycle"));
    for index in 0..9 {
        f.write(
            &format!("tools/devices/family/{index}.toml"),
            &format!("extends='{}.toml'\n", index + 1),
        );
    }
    f.write("tools/devices/stm32f429.toml", "extends='family/0.toml'");
    assert!(f.project("").unwrap_err().contains("depth exceeds 8"));
}

#[test]
fn probe_selection_is_independent_and_missing_or_ambiguous_settings_fail() {
    let f = Fixture::new();
    f.write("tools/devices/stm32f429.toml", "backend='stm32f4'\n[service]\ncommand='server'\nargs=['-f','${probe_config}']\n[core_targets.\"0\"]\nendpoint='localhost:3333'");
    assert!(f.project("").unwrap_err().contains("Set [probe]"));
    let common = "[chip_profiles]\ndirectory='devices'\n[probe]\n";
    for name in ["cmsis-dap", "jlink", "stlink"] {
        f.write(
            &format!("tools/{name}.cfg"),
            "# probe placeholder: loading must not execute this\n",
        );
        f.write(
            "tools/debug-env.toml",
            &format!("{common}config='./{name}.cfg'"),
        );
        let p = f.project("").unwrap();
        assert!(p.service.as_ref().unwrap().args[1].ends_with(&format!("tools/{name}.cfg")));
        assert_eq!(p.cores[0].endpoint, "localhost:3333");
    }
    for (profile, expected) in [
        (format!("{common}config='missing.cfg'"), "Probe config"),
        (format!("{common}config=''"), "must not be empty"),
        (format!("{common}config='devices'"), "not a file"),
        (
            format!("{common}config='jlink.cfg'\ninvalid=true"),
            "unknown field",
        ),
        ("[chip_profiles]\ndirectory=''".into(), "must not be empty"),
        (
            "[chip_profiles]\ndirectory='devices'\ninvalid=true".into(),
            "unknown field",
        ),
        (
            "[chip_profiles]\ndirectory='devices'\n[backends.stm32f4]".into(),
            "either",
        ),
        (
            "[probe]\nconfig='jlink.cfg'".into(),
            "requires [chip_profiles]",
        ),
    ] {
        f.write("tools/debug-env.toml", &profile);
        let error = f.project("").unwrap_err();
        assert!(error.contains(expected), "{profile}: {error}");
    }
}

#[test]
fn adding_chip_description_and_catalogue_entry_does_not_change_common_profile() {
    let f = Fixture::new();
    let common = fs::read(f.0.join("tools/debug-env.toml")).unwrap();
    f.write("tools/devices/custom-r52.toml", "backend='custom'\n[core_targets.\"1\"]\nendpoint='localhost:4501'\n[core_targets.\"3\"]\nendpoint='localhost:4503'");
    let mut catalogue = Catalogue::parse(DEFAULTS).unwrap();
    catalogue.devices.insert(
        "custom-r52".into(),
        Device {
            cores: vec![1, 3],
            backend: "custom".into(),
            cpu: "cortex-r52".into(),
        },
    );
    let raw = toml::from_str("version=3\n[tools]\nprofile='../tools/debug-env.toml'\n[debug]\nchip='custom-r52'\ncores=[3,1]").unwrap();
    let p = Project::from_document_with_catalogue(
        raw,
        Some(f.0.join("project/debug.toml")),
        None,
        Some(&catalogue),
    )
    .unwrap();
    assert_eq!(
        p.cores.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        ["core.3", "core.1"]
    );
    assert_eq!(
        p.cores
            .iter()
            .map(|c| c.endpoint.as_str())
            .collect::<Vec<_>>(),
        ["localhost:4503", "localhost:4501"]
    );
    assert_eq!(p.registers.cpu, "cortex-r52");
    assert_eq!(fs::read(f.0.join("tools/debug-env.toml")).unwrap(), common);
}
