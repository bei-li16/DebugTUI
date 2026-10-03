//! User-owned device catalogue and explicit chip/core launch resolution.
use crate::config::{Core, Project, portable_path};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

pub const DEFAULTS: &str = include_str!("../profiles/devices.toml");

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Device {
    pub cores: Vec<u32>,
    pub backend: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Catalogue {
    pub version: u32,
    pub devices: BTreeMap<String, Device>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Selection {
    pub chip: String,
    pub cores: Vec<u32>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub watch: Option<Vec<String>>,
    pub breakpoints: Option<Vec<crate::config::BreakpointSpec>>,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Target {
    endpoint: String,
    ready: String,
}
pub struct Plan {
    selection: Selection,
    targets: BTreeMap<String, Target>,
}

pub fn catalogue_path() -> Result<PathBuf, String> {
    if let Some(root) = env::var_os("DEBUGTUI_CONFIG_DIR").filter(|s| !s.is_empty()) {
        return Ok(PathBuf::from(root).join("profiles/devices.toml"));
    }
    let root = if cfg!(windows) {
        env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    }
    .ok_or("Cannot locate user configuration directory; set DEBUGTUI_CONFIG_DIR")?;
    Ok(root.join("debugtui/profiles/devices.toml"))
}
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
}
pub fn validate_device(name: &str, device: &Device) -> Result<(), String> {
    if !valid_id(name) || !valid_id(&device.backend) {
        return Err("Chip/backend names use 1-64 letters, digits, '.', '-' or '_'".into());
    }
    if device.cores.is_empty()
        || device.cores.iter().any(|&id| id > 31)
        || device.cores.iter().collect::<BTreeSet<_>>().len() != device.cores.len()
    {
        return Err("Core IDs must be unique integers from 0 to 31; select at least one".into());
    }
    Ok(())
}
pub fn parse_ids(text: &str) -> Result<Vec<u32>, String> {
    let ids = text
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<u32>()
                .map_err(|_| "Core IDs must be integers, e.g. 0,1,2,3".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_device(
        "device",
        &Device {
            cores: ids.clone(),
            backend: "generic".into(),
        },
    )?;
    Ok(ids)
}
impl Catalogue {
    pub fn parse(text: &str) -> Result<Self, String> {
        let catalogue: Self = toml::from_str(text.trim_start_matches('\u{feff}'))
            .map_err(|e| format!("Device catalogue: {e}"))?;
        if catalogue.version != 1 {
            return Err("Unsupported device catalogue version".into());
        }
        for (name, device) in &catalogue.devices {
            validate_device(name, device)?;
        }
        Ok(catalogue)
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path)
            .map_err(|e| format!("Device catalogue {}: {e}", path.display()))?;
        Self::parse(&text)
    }
    pub fn user() -> Result<Self, String> {
        Self::load(&ensure_user_catalogue()?)
    }
    pub fn selection(&self, selection: &Selection) -> Result<(), String> {
        let device = self
            .devices
            .get(&selection.chip)
            .ok_or_else(|| format!("Unknown chip '{}'; add it in Setup > Chip", selection.chip))?;
        validate_device(
            &selection.chip,
            &Device {
                cores: selection.cores.clone(),
                backend: device.backend.clone(),
            },
        )?;
        if selection.cores.iter().any(|id| !device.cores.contains(id)) {
            return Err(format!(
                "{} supports core IDs {:?}; selected {:?}",
                selection.chip, device.cores, selection.cores
            ));
        }
        Ok(())
    }
}

// Publish complete files with a same-directory rename. The lock also covers the
// read/modify/write cycle so two TUI windows cannot silently lose additions.
struct Lock(PathBuf);
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn locked(path: &Path) -> Result<Lock, String> {
    fs::create_dir_all(path.parent().ok_or("Catalogue needs a parent directory")?)
        .map_err(|e| e.to_string())?;
    let lock = path.with_extension("toml.lock");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock)
        {
            Ok(mut f) => {
                let _ = writeln!(f, "{}", std::process::id());
                return Ok(Lock(lock));
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::AlreadyExists
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(20))
            }
            Err(e) => {
                return Err(format!(
                    "Device catalogue is locked or unwritable ({}): {e}",
                    lock.display()
                ));
            }
        }
    }
}
fn publish(path: &Path, text: &str) -> Result<(), String> {
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| format!("Create {}: {e}", temp.display()))?;
    let result = (|| {
        let written = f.write_all(text.as_bytes()).and_then(|_| f.sync_all());
        drop(f);
        written.map_err(|e| e.to_string())?;
        fs::rename(&temp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(|e| format!("Save catalogue {}: {e}", path.display()))
}
pub fn ensure_at(path: &Path) -> Result<(), String> {
    if path.is_file() {
        Catalogue::load(path)?;
        return Ok(());
    }
    let _lock = locked(path)?;
    if path.exists() {
        Catalogue::load(path)?;
    } else {
        publish(path, DEFAULTS)?;
    }
    Ok(())
}
pub fn ensure_user_catalogue() -> Result<PathBuf, String> {
    let path = catalogue_path()?;
    ensure_at(&path)?;
    Ok(path)
}
pub fn add(path: &Path, name: &str, mut device: Device) -> Result<(), String> {
    validate_device(name, &device)?;
    ensure_at(path)?;
    let _lock = locked(path)?;
    let original = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let catalogue = Catalogue::parse(&original)?;
    if catalogue
        .devices
        .keys()
        .any(|n| n.eq_ignore_ascii_case(name))
    {
        return Err(format!(
            "Chip '{name}' already exists; the existing entry was not changed"
        ));
    }
    device.cores.sort_unstable();
    let extra = Catalogue {
        version: 1,
        devices: BTreeMap::from([(name.to_owned(), device)]),
    };
    let text = toml::to_string_pretty(&extra).map_err(|e| e.to_string())?;
    let (_, entries) = text.split_once('\n').ok_or("Cannot encode chip entry")?;
    publish(path, &format!("{}\n{}", original.trim_end(), entries))
}

fn expand_selection(value: &mut toml::Value, selection: &Selection, device: &Device) {
    let mask = selection.cores.iter().fold(0u32, |m, id| m | (1u32 << id));
    let available_mask = device.cores.iter().fold(0u32, |m, id| m | (1u32 << id));
    let other_ids = device
        .cores
        .iter()
        .filter(|id| !selection.cores.contains(id))
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    let replacements = [
        ("${chip}", selection.chip.clone()),
        ("${chip_upper}", selection.chip.to_ascii_uppercase()),
        ("${core_mask}", mask.to_string()),
        ("${available_core_mask}", available_mask.to_string()),
        ("${unselected_core_ids}", other_ids),
    ];
    fn walk(value: &mut toml::Value, replacements: &[(&str, String)]) {
        match value {
            toml::Value::String(s) => {
                for (key, value) in replacements {
                    *s = s.replace(key, value);
                }
            }
            toml::Value::Array(a) => a.iter_mut().for_each(|v| walk(v, replacements)),
            toml::Value::Table(t) => t
                .iter_mut()
                .filter(|(key, _)| !matches!(key.as_str(), "watch" | "breakpoints"))
                .for_each(|(_, v)| walk(v, replacements)),
            _ => {}
        }
    }
    // Only launch paths and commands are templates; never expand user Watch,
    // saved breakpoints, source-map prefixes, or arbitrary runtime preferences.
    for key in [
        "gdb",
        "service",
        "target",
        "actions",
        "program",
        "tasks",
        "multicore",
        "cores",
        "live_watch",
    ] {
        if let Some(v) = value.get_mut(key) {
            walk(v, &replacements);
        }
    }
}

pub fn resolve(
    environment: &mut toml::Value,
    raw: &mut toml::Value,
    catalogue: Option<&Catalogue>,
) -> Result<Option<Plan>, String> {
    let selection: Selection = raw
        .get("debug")
        .cloned()
        .map(toml::Value::try_into)
        .transpose()
        .map_err(|e| format!("debug: {e}"))?
        .unwrap_or_default();
    let table = environment
        .as_table_mut()
        .ok_or("Environment must be a table")?;
    let backend = table
        .remove("backend")
        .and_then(|v| v.as_str().map(str::to_owned));
    let backends = table.remove("backends");
    let root_targets = table.remove("core_targets");
    if selection.chip.is_empty() {
        if !selection.cores.is_empty() {
            return Err("Select a chip before selecting core IDs".into());
        }
        let dummy = Selection {
            chip: "__select_chip__".into(),
            cores: vec![0],
        };
        let device = Device {
            cores: vec![0],
            backend: "generic".into(),
        };
        if [&*environment, &*raw].iter().any(|value| {
            let mut expanded = (*value).clone();
            expand_selection(&mut expanded, &dummy, &device);
            expanded != **value
        }) {
            return Err(
                "This profile uses chip templates; select Chip and Core IDs in Setup first".into(),
            );
        }
        return Ok(None);
    }
    let owned;
    let catalogue = if let Some(c) = catalogue {
        c
    } else {
        owned = Catalogue::user()?;
        &owned
    };
    catalogue.selection(&selection)?;
    let device = &catalogue.devices[&selection.chip];
    let group = backends
        .as_ref()
        .and_then(|v| v.get(&device.backend))
        .cloned();
    let mut group = if let Some(group) = group {
        group
    } else if backend.as_deref() == Some(&device.backend)
        || (backend.is_none() && device.backend == "generic")
    {
        toml::Value::Table(Default::default())
    } else {
        return Err(format!(
            "Chip '{}' requires backend '{}'; select a matching Tools / profile or define [backends.{}] in it",
            selection.chip, device.backend, device.backend
        ));
    };
    let group_table = group.as_table_mut().ok_or("Backend must be a table")?;
    for key in group_table.keys() {
        if !matches!(
            key.as_str(),
            "gdb"
                | "target"
                | "service"
                | "actions"
                | "session"
                | "sync"
                | "memory_access"
                | "multicore"
                | "core_targets"
        ) {
            return Err(format!("Unsupported backend section: {key}"));
        }
    }
    let targets = group_table.remove("core_targets").or(root_targets);
    let targets: BTreeMap<String, Target> = targets
        .map(toml::Value::try_into)
        .transpose()
        .map_err(|e| format!("core_targets: {e}"))?
        .unwrap_or_default();
    crate::config::merge(environment, group);
    expand_selection(environment, &selection, device);
    expand_selection(raw, &selection, device);
    Ok(Some(Plan { selection, targets }))
}

impl Plan {
    pub fn apply(self, project: &mut Project) -> Result<(), String> {
        let templates = std::mem::take(&mut project.cores);
        let mut ready = Vec::new();
        for id in &self.selection.cores {
            let name = format!("core.{id}");
            let target = self.targets.get(&id.to_string());
            let mut core = templates
                .iter()
                .find(|c| c.name == name)
                .cloned()
                .unwrap_or_else(|| Core {
                    name: name.clone(),
                    ..Default::default()
                });
            // A template's explicit endpoint wins; otherwise use the environment map.
            if core.endpoint.is_empty() {
                core.endpoint = if let Some(target) = target {
                    target.endpoint.clone()
                } else if *id == 0 && self.selection.cores.len() == 1 {
                    project.target.endpoint.clone()
                } else {
                    String::new()
                };
            }
            if core.endpoint.is_empty() {
                return Err(format!(
                    "No endpoint for {name}; add [core_targets.\"{id}\"] to the tools environment"
                ));
            }
            if let Some(target) = target
                && !target.ready.is_empty()
            {
                ready.push(target.ready.clone());
            }
            if let Some(preferences) = project
                .core_preferences
                .get(&self.selection.chip)
                .and_then(|p| p.get(&name))
            {
                core.watch = preferences.watch.clone().or(core.watch);
                core.breakpoints = preferences.breakpoints.clone().or(core.breakpoints);
            }
            project.cores.push(core);
        }
        if let Some(service) = &mut project.service
            && !ready.is_empty()
        {
            if ready.len() != project.cores.len() {
                return Err("Each selected core needs a core_targets ready marker when using per-core service readiness".into());
            }
            service.ready = ready;
        }
        // Filter shared reset when its designated owner is not selected. Do not
        // silently redirect a chip reset through a different core.
        if !project.multicore.restart.is_empty()
            && !project
                .cores
                .iter()
                .any(|c| c.name == project.multicore.restart_core)
        {
            project.multicore.restart.clear();
        }
        project.debug = self.selection;
        Ok(())
    }
}

pub fn display_path() -> String {
    catalogue_path()
        .map(|p| portable_path(&p))
        .unwrap_or_else(|e| e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = env::temp_dir().join(format!(
                "debugtui-devices-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn environment() -> String {
        let mut text = "backend='tha6'\n[gdb]\nexecutable='./tool/gdb.exe'\n[target]\nmode='extended-remote'\n[service]\ncommand='./tool/openocd.exe'\nargs=['-c','set CHIPNAME ${chip}','-c','set DEBUGCORE ${core_mask}']\ncwd='.'\n[multicore]\nrestart=['monitor chipreset']\nrestart_core='core.0'\n".to_owned();
        for id in 0..4 {
            text.push_str(&format!(
                "[core_targets.\"{id}\"]\nendpoint='127.0.0.1:{}'\nready='port {} ready'\n",
                3333 + id,
                3333 + id
            ));
        }
        text
    }
    fn load(root: &Path, chip: &str, cores: &[u32]) -> Result<Project, String> {
        let text = format!(
            "version=3\n[tools]\nprofile='debug-env.toml'\n[debug]\nchip='{chip}'\ncores={cores:?}\n[program]\nelf='build/${{chip_upper}}.elf'\nsource_root='.'\n[[source_map]]\nfrom='/ci/${{chip}}'\nto='.'\n"
        );
        Project::from_document_with_catalogue(
            toml::from_str(&text).unwrap(),
            Some(root.join("debug.toml")),
            None,
            Some(&Catalogue::parse(DEFAULTS).unwrap()),
        )
    }
    #[test]
    fn shipped_tha_templates_filter_hooks_and_expand_only_launch_values() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join(".vscode")).unwrap();
        fs::write(
            fixture.0.join(".vscode/debug-env-chip.toml"),
            include_str!("../profiles/tha6-environment.toml.example"),
        )
        .unwrap();
        let mut raw: toml::Value =
            toml::from_str(include_str!("../profiles/tha6-project.toml.example")).unwrap();
        let catalogue = Catalogue::parse(DEFAULTS).unwrap();
        for (chip, ids, all, others) in [
            ("tha6104", vec![0], 1, ""),
            ("tha6206", vec![0], 3, "1"),
            ("tha6206", vec![1], 3, "0"),
            ("tha6206", vec![0, 1], 3, ""),
            ("tha6412", vec![1, 3], 15, "0 2"),
            ("tha6412", vec![0, 1, 2, 3], 15, ""),
        ] {
            raw["debug"] = toml::Value::try_from(Selection {
                chip: chip.into(),
                cores: ids.clone(),
            })
            .unwrap();
            let p = Project::from_document_with_catalogue(
                raw.clone(),
                Some(fixture.0.join("debug.toml")),
                None,
                Some(&catalogue),
            )
            .unwrap();
            assert_eq!(
                p.cores.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
                ids.iter()
                    .map(|id| format!("core.{id}"))
                    .collect::<Vec<_>>()
            );
            assert!(
                p.service
                    .unwrap()
                    .args
                    .contains(&format!("set DEBUGCORE {all}"))
            );
            if ids.contains(&0) {
                assert!(p.cores[0].after_connect[1].contains(&format!("{{{others}}}")));
                assert!(!p.multicore.restart.is_empty());
            } else {
                assert!(p.multicore.restart.is_empty());
            }
        }
        let mut literal: toml::Value =
            toml::from_str("watch=['${chip}']\n[[cores]]\nwatch=['${chip}']\nrun=['echo ${chip}']")
                .unwrap();
        expand_selection(
            &mut literal,
            &Selection {
                chip: "tha6206".into(),
                cores: vec![0],
            },
            &catalogue.devices["tha6206"],
        );
        assert_eq!(literal["watch"][0].as_str(), Some("${chip}"));
        assert_eq!(literal["cores"][0]["watch"][0].as_str(), Some("${chip}"));
        assert_eq!(literal["cores"][0]["run"][0].as_str(), Some("echo tha6206"));
        let mut legacy: toml::Value = toml::from_str("watch=['${chip}']").unwrap();
        assert!(
            resolve(
                &mut toml::Value::Table(Default::default()),
                &mut legacy,
                Some(&catalogue)
            )
            .unwrap()
            .is_none()
        );
        raw.as_table_mut().unwrap().remove("debug");
        assert!(
            Project::from_document_with_catalogue(
                raw,
                Some(fixture.0.join("debug.toml")),
                None,
                Some(&catalogue)
            )
            .unwrap_err()
            .contains("select Chip")
        );
    }
    #[test]
    fn catalogue_install_upgrade_add_and_concurrent_writes_preserve_user_entries() {
        let fixture = Fixture::new();
        let path = fixture.0.join("devices.toml");
        ensure_at(&path).unwrap();
        let original = fs::read_to_string(&path).unwrap();
        add(
            &path,
            "s32k144",
            Device {
                cores: vec![0],
                backend: "generic".into(),
            },
        )
        .unwrap();
        let custom = fs::read(&path).unwrap();
        ensure_at(&path).unwrap();
        assert_eq!(custom, fs::read(&path).unwrap());
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .starts_with(original.trim_end())
        );
        assert!(
            add(
                &path,
                "S32K144",
                Device {
                    cores: vec![1],
                    backend: "other".into()
                }
            )
            .is_err()
        );
        assert_eq!(custom, fs::read(&path).unwrap());
        std::thread::scope(|scope| {
            for name in ["chip-a", "chip-b"] {
                let path = &path;
                scope.spawn(move || {
                    add(
                        path,
                        name,
                        Device {
                            cores: vec![2, 0],
                            backend: "generic".into(),
                        },
                    )
                    .unwrap()
                });
            }
        });
        let c = Catalogue::load(&path).unwrap();
        assert!(
            c.devices.contains_key("s32k144")
                && c.devices.contains_key("chip-a")
                && c.devices.contains_key("chip-b")
        );
        assert_eq!(c.devices["chip-a"].cores, [0, 2]);
        fs::write(&path, "broken = [").unwrap();
        assert!(ensure_at(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "broken = [");
    }
    #[test]
    fn selection_resolves_all_eight_tha_modes_and_preserves_physical_core_identity() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("debug-env.toml"), environment()).unwrap();
        for (chip, combinations) in [
            ("tha6104", vec![vec![0]]),
            ("tha6206", vec![vec![0], vec![1], vec![0, 1]]),
            (
                "tha6412",
                vec![vec![0], vec![1], vec![0, 1], vec![0, 1, 2, 3]],
            ),
        ] {
            for ids in combinations {
                let p = load(&fixture.0, chip, &ids).unwrap();
                assert_eq!(p.cores.len(), ids.len());
                assert!(
                    p.program
                        .elf
                        .ends_with(format!("build/{}.elf", chip.to_ascii_uppercase()))
                );
                assert_eq!(p.source_map[0].from, "/ci/${chip}");
                let service = p.service.as_ref().unwrap();
                assert!(service.command.is_absolute());
                assert!(service.args.contains(&format!(
                    "set DEBUGCORE {}",
                    ids.iter().fold(0u32, |m, id| m | (1 << id))
                )));
                for (core, id) in p.cores.iter().zip(&ids) {
                    assert_eq!(core.name, format!("core.{id}"));
                    assert_eq!(core.endpoint, format!("127.0.0.1:{}", 3333 + id));
                }
                assert_eq!(service.ready.len(), ids.len());
                assert_eq!(!p.multicore.restart.is_empty(), ids.contains(&0));
            }
        }
        for (chip, ids) in [
            ("tha6104", vec![1]),
            ("tha6206", vec![2]),
            ("tha6412", vec![]),
            ("tha6412", vec![0, 0]),
            ("tha6412", vec![32]),
            ("unknown", vec![0]),
        ] {
            assert!(load(&fixture.0, chip, &ids).is_err());
        }
        assert!(
            load(&fixture.0, "stm32f429", &[0])
                .unwrap_err()
                .contains("requires backend")
        );
    }
    #[test]
    fn backend_groups_generic_chip_and_missing_or_duplicate_endpoints() {
        let fixture = Fixture::new();
        let path = fixture.0.join("debug-env.toml");
        fs::write(&path,"[gdb]\nexecutable='./gdb.exe'\n[backends.stm32f4.target]\nmode='extended-remote'\nendpoint='localhost:3333'\n[backends.stm32f4.actions]\nrestart=['monitor reset halt']\n").unwrap();
        let p = load(&fixture.0, "stm32f429", &[0]).unwrap();
        assert_eq!(p.actions.restart, ["monitor reset halt"]);
        assert_eq!(p.cores[0].endpoint, "localhost:3333");
        assert!(p.gdb.executable.is_absolute());
        let mut c = Catalogue::parse(DEFAULTS).unwrap();
        c.devices.insert(
            "s32k144".into(),
            Device {
                cores: vec![0],
                backend: "generic".into(),
            },
        );
        fs::write(
            &path,
            "[target]\nmode='extended-remote'\nendpoint='localhost:2345'\n",
        )
        .unwrap();
        let raw: toml::Value = toml::from_str(
            "version=3\n[tools]\nprofile='debug-env.toml'\n[debug]\nchip='s32k144'\ncores=[0]",
        )
        .unwrap();
        let p = Project::from_document_with_catalogue(
            raw,
            Some(fixture.0.join("debug.toml")),
            None,
            Some(&c),
        )
        .unwrap();
        assert_eq!(p.cores[0].endpoint, "localhost:2345");
        fs::write(
            &path,
            "backend='tha6'\n[target]\nendpoint='localhost:3333'\n",
        )
        .unwrap();
        assert!(
            load(&fixture.0, "tha6206", &[1])
                .unwrap_err()
                .contains("No endpoint")
        );
        fs::write(
            &path,
            environment().replace("127.0.0.1:3334", "127.0.0.1:3333"),
        )
        .unwrap();
        assert!(
            load(&fixture.0, "tha6206", &[0, 1])
                .unwrap_err()
                .contains("Duplicate core endpoint")
        );
    }
    #[test]
    fn core_preferences_are_saved_per_chip_without_expanding_generated_cores() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("debug-env.toml"), environment()).unwrap();
        let path = fixture.0.join("debug.toml");
        fs::write(
            &path,
            "version=3\n[tools]\nprofile='debug-env.toml'\n[debug]\nchip='tha6206'\ncores=[1]\n",
        )
        .unwrap();
        let catalogue = Catalogue::parse(DEFAULTS).unwrap();
        let mut p = Project::from_document_with_catalogue(
            toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap(),
            Some(path.clone()),
            None,
            Some(&catalogue),
        )
        .unwrap();
        p.preference_core = Some("core.1".into());
        p.save_preferences(vec!["counter1".into()], vec![]).unwrap();
        p.debug.chip = "tha6412".into();
        p.save_preferences(vec!["different_counter".into()], vec![])
            .unwrap();
        let raw: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert!(raw.get("cores").is_none());
        let p =
            Project::from_document_with_catalogue(raw, Some(path), None, Some(&catalogue)).unwrap();
        assert_eq!(p.cores[0].name, "core.1");
        assert_eq!(p.cores[0].watch, Some(vec!["counter1".into()]));
        assert_eq!(
            p.core_preferences["tha6412"]["core.1"].watch,
            Some(vec!["different_counter".into()])
        );
    }
}
