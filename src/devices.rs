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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cpu: String,
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
    pub(crate) chip_profile_path: Option<PathBuf>,
    pub(crate) register_layers: Vec<crate::config::register_sources::Layer>,
    pub(crate) memory_access_source: Option<String>,
}

pub fn catalogue_path() -> Result<PathBuf, String> {
    Ok(user_dir()?.join("profiles/devices.toml"))
}
/// Per-user DebugTUI directory: `DEBUGTUI_CONFIG_DIR`, else the platform's
/// local configuration directory.
pub fn user_dir() -> Result<PathBuf, String> {
    if let Some(root) = env::var_os("DEBUGTUI_CONFIG_DIR").filter(|s| !s.is_empty()) {
        return Ok(PathBuf::from(root));
    }
    let root = if cfg!(windows) {
        env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    }
    .ok_or("Cannot locate user configuration directory; set DEBUGTUI_CONFIG_DIR")?;
    Ok(root.join("debugtui"))
}
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
}
pub fn validate_device(name: &str, device: &Device) -> Result<(), String> {
    if !device.cpu.is_empty() && !valid_id(&device.cpu.replace('+', "plus")) {
        return Err("CPU catalogue association must be a simple identifier".into());
    }
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
            cpu: String::new(),
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
    /// A configured association is a default, never observed CPU identity.
    pub(crate) fn cpu_association(
        &self,
        chip: &str,
    ) -> Result<Option<(String, &'static str)>, String> {
        let Some(device) = self.devices.get(chip) else {
            return Ok(None);
        };
        if !device.cpu.is_empty() {
            return Ok(Some((device.cpu.clone(), "user")));
        }
        Ok(Self::parse(DEFAULTS)?
            .devices
            .get(chip)
            .filter(|device| !device.cpu.is_empty())
            .map(|device| (device.cpu.clone(), "builtin")))
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
                cpu: String::new(),
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
    // Built-ins live in the executable. Leave this directory empty so future
    // upgrades can update defaults without replacing customer overrides.
    fs::create_dir_all(path.parent().unwrap_or(Path::new(".")).join("registers"))
        .map_err(|error| format!("Initialize register extension directory: {error}"))?;
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

pub(crate) fn expand_selection(value: &mut toml::Value, selection: &Selection, device: &Device) {
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
    resolve_with_profile(environment, raw, catalogue, None)
}

pub(crate) fn resolve_with_profile(
    environment: &mut toml::Value,
    raw: &mut toml::Value,
    catalogue: Option<&Catalogue>,
    profile: Option<&Path>,
) -> Result<Option<Plan>, String> {
    let mut register_layers = environment
        .get("registers")
        .cloned()
        .into_iter()
        .map(|values| crate::config::register_sources::Layer {
            section: "registers".into(),
            values,
        })
        .collect::<Vec<_>>();
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
    let chip_profiles = table.remove("chip_profiles");
    let probe = table.remove("probe");
    if chip_profiles.is_some() && (backend.is_some() || backends.is_some()) {
        return Err(
            "Use either [chip_profiles] or inline backend/backends in a tools profile".into(),
        );
    }
    if probe.is_some() && chip_profiles.is_none() {
        return Err("[probe] requires [chip_profiles] in the tools profile".into());
    }
    let root_targets = table.remove("core_targets");
    if selection.chip.is_empty() {
        if chip_profiles.is_some() {
            return Err(
                "This profile loads chip descriptions; select Chip and Core IDs in Setup first"
                    .into(),
            );
        }
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
            cpu: String::new(),
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
    let supplied_catalogue = catalogue.is_some();
    let owned;
    let catalogue = if let Some(c) = catalogue {
        c
    } else {
        owned = Catalogue::user()?;
        &owned
    };
    catalogue.selection(&selection)?;
    let device = &catalogue.devices[&selection.chip];
    let association = catalogue.cpu_association(&selection.chip)?;
    let cpu = association
        .as_ref()
        .map(|(cpu, _)| cpu.clone())
        .unwrap_or_default();
    let external = chip_profiles
        .map(|settings| {
            crate::config::chip_profiles::load(
                settings,
                probe,
                &selection,
                device,
                profile.ok_or("External chip descriptions require a tools profile file")?,
            )
        })
        .transpose()?;
    let external_group = external.is_some();
    let chip_profile_path = external.as_ref().map(|loaded| loaded.path.clone());
    let memory_access_source = external
        .as_ref()
        .and_then(|loaded| loaded.memory_access_source.clone());
    let group = if let Some(loaded) = external {
        register_layers.extend(loaded.register_layers);
        Some(loaded.value)
    } else {
        backends
            .as_ref()
            .and_then(|v| v.get(&device.backend))
            .cloned()
    };
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
                | "program"
                | "target"
                | "service"
                | "actions"
                | "session"
                | "sync"
                | "memory_access"
                | "registers"
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
    crate::config::validate_environment_program(&group)
        .map_err(|error| format!("backends.{}: {error}", device.backend))?;
    if let Some(values) = group.get("registers")
        && !external_group
    {
        register_layers.push(crate::config::register_sources::Layer {
            section: format!("backends.{}.registers", device.backend),
            values: values.clone(),
        });
    }
    crate::config::merge(environment, group);
    // Chip associations are defaults. Respect explicit project and selected
    // profile/backend selectors (including empty strings) before adding one.
    let mut selected_registers = false;
    for (value, source) in [(&*raw, "Project"), (&*environment, "Environment")] {
        if let Some(section) = value.get("registers") {
            let section = section
                .as_table()
                .ok_or_else(|| format!("{source}: registers must be a table"))?;
            selected_registers |= section.contains_key("cpu") || section.contains_key("catalogue");
        }
    }
    if !cpu.is_empty() && !selected_registers {
        let association_source: String = if association
            .as_ref()
            .is_some_and(|(_, source)| *source == "builtin")
        {
            "builtin:profiles/devices.toml".into()
        } else if !supplied_catalogue {
            format!("user:{}", portable_path(&catalogue_path()?))
        } else {
            "supplied device catalogue".into()
        };
        register_layers.push(crate::config::register_sources::Layer {
            section: format!("chip association:{} ({association_source})", selection.chip),
            values: toml::Value::Table(toml::Table::from_iter([(
                "cpu".into(),
                toml::Value::String(cpu.clone()),
            )])),
        });
        raw.as_table_mut()
            .ok_or("Project must be a table")?
            .entry("registers")
            .or_insert_with(|| toml::Value::Table(Default::default()))
            .as_table_mut()
            .unwrap()
            .insert("cpu".into(), toml::Value::String(cpu));
    }
    expand_selection(environment, &selection, device);
    expand_selection(raw, &selection, device);
    Ok(Some(Plan {
        selection,
        targets,
        chip_profile_path,
        register_layers,
        memory_access_source,
    }))
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
    fn register_extension_initialization_failure_does_not_replace_customer_files() {
        let fixture = Fixture::new();
        let directory = fixture.0.join("profiles");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("devices.toml");
        let blocked = directory.join("registers");
        let customer = "# Preserve formatting and the customer's chip\nversion=1\n[devices.customer]\ncores=[0,2]\nbackend='generic'\n";
        fs::write(&path, customer).unwrap();
        fs::write(
            &blocked,
            "Customer file occupies the extension directory path",
        )
        .unwrap();
        let before = fs::read(&blocked).unwrap();
        let error = ensure_at(&path).unwrap_err();
        assert!(error.starts_with("Initialize register extension directory:"));
        assert_eq!(fs::read_to_string(&path).unwrap(), customer);
        assert_eq!(fs::read(&blocked).unwrap(), before);
        assert!(blocked.is_file());
    }
    #[test]
    fn cpu_association_priority_and_register_extensions_survive_upgrade() {
        let fixture = Fixture::new();
        let path = fixture.0.join("devices.toml");
        ensure_at(&path).unwrap();
        let extensions = fixture.0.join("registers");
        assert_eq!(fs::read_dir(&extensions).unwrap().count(), 0);
        let custom = extensions.join("customer.toml");
        fs::write(&custom, "customer register data").unwrap();
        let before = fs::read(&path).unwrap();
        ensure_at(&path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(
            fs::read_to_string(&custom).unwrap(),
            "customer register data"
        );
        let mut catalogue = Catalogue::parse(DEFAULTS).unwrap();
        catalogue.devices.get_mut("tha6206").unwrap().cpu.clear();
        let base: toml::Value =
            toml::from_str("version=3\n[debug]\nchip='tha6206'\ncores=[0]\n").unwrap();
        let environment: toml::Value =
            toml::from_str("backend='tha6'\n[core_targets.\"0\"]\nendpoint='localhost:3333'\n")
                .unwrap();
        let mut raw = base.clone();
        resolve(&mut environment.clone(), &mut raw, Some(&catalogue)).unwrap();
        assert_eq!(raw["registers"]["cpu"].as_str(), Some("cortex-r52+"));
        catalogue.devices.get_mut("tha6206").unwrap().cpu = "cortex-m4".into();
        let mut raw = base.clone();
        resolve(&mut environment.clone(), &mut raw, Some(&catalogue)).unwrap();
        assert_eq!(raw["registers"]["cpu"].as_str(), Some("cortex-m4"));
        let mut raw = base.clone();
        raw.as_table_mut().unwrap().insert(
            "registers".into(),
            toml::from_str::<toml::Value>("cpu='cortex-r52+'\n").unwrap(),
        );
        resolve(&mut environment.clone(), &mut raw, Some(&catalogue)).unwrap();
        assert_eq!(raw["registers"]["cpu"].as_str(), Some("cortex-r52+"));
        raw["registers"]["cpu"] = "".into();
        resolve(&mut environment.clone(), &mut raw, Some(&catalogue)).unwrap();
        assert_eq!(raw["registers"]["cpu"].as_str(), Some(""));
    }
    #[test]
    fn register_selection_priority_respects_project_profile_backend_and_explicit_empty_selectors() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("tools")).unwrap();
        fs::write(
            fixture.0.join("tools/profile.toml"),
            include_str!("../profiles/registers/cortex-m4.toml"),
        )
        .unwrap();
        fs::write(
            fixture.0.join("customer.toml"),
            include_str!("../profiles/registers/cortex-r52.toml"),
        )
        .unwrap();
        let mut catalogue = Catalogue::parse(DEFAULTS).unwrap();
        catalogue.devices.insert(
            "matrix".into(),
            Device {
                cores: vec![0, 2],
                backend: "generic".into(),
                cpu: "cortex-r52+".into(),
            },
        );
        for (root, backend, project, cpu, source, model) in [
            (
                "",
                "",
                "",
                "cortex-r52+",
                "builtin:cortex-r52+",
                "cortex-r52+",
            ),
            (
                "cpu='cortex-m4'",
                "",
                "",
                "cortex-m4",
                "builtin:cortex-m4",
                "cortex-m4",
            ),
            (
                "cpu='cortex-r52'",
                "cpu='cortex-m4'",
                "",
                "cortex-m4",
                "builtin:cortex-m4",
                "cortex-m4",
            ),
            ("cpu='cortex-r52'", "cpu=''", "", "", "gdb", ""),
            (
                "cpu='cortex-m4'",
                "",
                "cpu='cortex-r52'",
                "cortex-r52",
                "builtin:cortex-r52",
                "cortex-r52",
            ),
            (
                "cpu='cortex-m4'\ncatalogue='profile.toml'",
                "",
                "cpu='cortex-r52'",
                "cortex-r52",
                "file:",
                "cortex-m4",
            ),
            (
                "cpu='cortex-m4'\ncatalogue='profile.toml'",
                "",
                "catalogue=''",
                "cortex-m4",
                "builtin:cortex-m4",
                "cortex-m4",
            ),
            (
                "cpu='cortex-m4'\ncatalogue='profile.toml'",
                "",
                "cpu=''\ncatalogue=''",
                "",
                "gdb",
                "",
            ),
            (
                "cpu='cortex-m4'",
                "",
                "cpu='unknown-cpu'\ncatalogue='customer.toml'",
                "unknown-cpu",
                "file:",
                "cortex-r52",
            ),
        ] {
            let profile = fixture.0.join("tools/debug-env.toml");
            fs::write(&profile,format!("backend='generic'\n[core_targets.\"0\"]\nendpoint='localhost:5000'\n[core_targets.\"2\"]\nendpoint='localhost:5002'\n[registers]\n{root}\n[backends.generic.registers]\n{backend}\n")).unwrap();
            let raw=toml::from_str(&format!("version=3\n[tools]\nprofile='tools/debug-env.toml'\n[debug]\nchip='matrix'\ncores=[0,2]\n[registers]\n{project}\n")).unwrap();
            let p = Project::from_document_with_catalogue(
                raw,
                Some(fixture.0.join("debug.toml")),
                None,
                Some(&catalogue),
            )
            .unwrap();
            assert_eq!(p.registers.cpu, cpu, "{root}/{backend}/{project}");
            assert_eq!(
                p.cores.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
                ["core.0", "core.2"]
            );
            match p.registers.load().unwrap() {
                None => assert_eq!(source, "gdb"),
                Some((loaded, origin)) => {
                    assert_eq!(loaded.cpu, model);
                    assert!(origin.starts_with(source), "{origin}");
                }
            }
            assert!(
                p.registers.targets.is_empty(),
                "chip catalogue selection cannot infer a system-register target"
            );
        }
    }
    #[test]
    fn register_configuration_errors_survive_selected_backend_and_chip_defaults() {
        let fixture = Fixture::new();
        let catalogue = Catalogue::parse(DEFAULTS).unwrap();
        for (profile, project, expected) in [
            ("[registers]\nunknown_setting=true", "", "unknown field"),
            (
                "[backends.tha6.registers]\nunknown_setting=true",
                "",
                "unknown field",
            ),
            ("", "[registers]\nunknown_setting=true", "unknown field"),
            (
                "registers=5",
                "[registers]\ncpu=''",
                "registers must be a table",
            ),
            ("", "registers=5\n", "registers must be a table"),
            ("[registers]\ncpu='unknown-cpu'", "", "unknown-cpu"),
        ] {
            fs::write(
                fixture.0.join("debug-env.toml"),
                format!(
                    "backend='tha6'\n{profile}\n[core_targets.\"0\"]\nendpoint='localhost:3333'\n"
                ),
            )
            .unwrap();
            // Put scalar register tables at the root, not inside [debug].
            let raw=toml::from_str(&format!("version=3\n{project}\n[tools]\nprofile='debug-env.toml'\n[debug]\nchip='tha6206'\ncores=[0]\n")).unwrap();
            let error = Project::from_document_with_catalogue(
                raw,
                Some(fixture.0.join("debug.toml")),
                None,
                Some(&catalogue),
            )
            .unwrap_err();
            assert!(error.contains(expected), "{profile}/{project}: {error}");
        }
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
                cpu: String::new(),
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
                    backend: "other".into(),
                    cpu: String::new(),
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
                            cpu: String::new(),
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
                cpu: String::new(),
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
    fn bundled_tha_mcal_example_resets_once_and_keeps_helper_policy_in_project() {
        let fixture = Fixture::new();
        let catalogue = Catalogue::parse(DEFAULTS).unwrap();
        let mut raw: toml::Value = toml::from_str(include_str!(
            "../profiles/tha6-bundled-project.toml.example"
        ))
        .unwrap();
        // Resolve the checkout payload without consulting per-user overrides.
        raw["tools"]["profile"] =
            portable_path(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/debug-env.toml"))
                .into();
        for (chip, count) in [("tha6104", 1), ("tha6206", 2), ("tha6412", 4)] {
            for mask in 1u32..(1 << count) {
                let ids: Vec<_> = (0..count).filter(|id| mask & (1 << id) != 0).collect();
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
                assert_eq!(p.cores.len(), ids.len());
                assert_eq!(p.live_watch.as_ref().unwrap().bus_target, "AHB_3");
                assert!(p.tasks.build.ends_with(chip));
                assert!(p.tasks.download.ends_with(chip));
                assert_eq!(!p.multicore.restart.is_empty(), ids.contains(&0));
                let others = (0..count)
                    .filter(|id| !ids.contains(id))
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                for (core, id) in p.cores.iter().zip(&ids) {
                    assert_eq!(core.endpoint, format!("127.0.0.1:{}", 3333 + id));
                    assert_eq!(
                        core.after_connect
                            .iter()
                            .filter(|s| s.contains("chipreset"))
                            .count(),
                        usize::from(*id == 0)
                    );
                    if *id == 0 {
                        assert!(core.after_connect[1].contains(&format!("{{{others}}}")));
                        assert_eq!(core.startup_order, 3);
                    } else {
                        assert!(!core.after_connect.iter().any(|s| s.contains("resume")));
                        assert_eq!(core.startup_order, *id as i32 - 1);
                    }
                }
            }
        }
    }
    #[test]
    fn universal_tools_switch_chip_without_leaking_svd_channels_or_core_ports() {
        let fixture = Fixture::new();
        let catalogue = Catalogue::parse(DEFAULTS).unwrap();
        let tools = Path::new(env!("CARGO_MANIFEST_DIR")).join("tools");
        let profile = tools.join("debug-env.toml");
        let original = fs::read(&profile).unwrap();
        let mut raw: toml::Value = toml::from_str(include_str!("../tools/debug.toml")).unwrap();
        raw["tools"]["profile"] = portable_path(&profile).into();
        raw["program"]["elf"] = "build/${chip_upper}.elf".into();
        for (chip, ids, mask) in [
            ("stm32f429", vec![0], 1),
            ("tha6104", vec![0], 1),
            ("tha6206", vec![0], 1),
            ("tha6206", vec![1], 2),
            ("tha6206", vec![0, 1], 3),
            ("tha6412", vec![1, 3], 10),
            ("tha6412", vec![0, 1, 2, 3], 15),
            ("stm32f429", vec![0], 1),
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
            assert_eq!(p.cores.len(), ids.len());
            for (core, id) in p.cores.iter().zip(&ids) {
                assert_eq!(core.name, format!("core.{id}"));
                assert_eq!(core.endpoint, format!("127.0.0.1:{}", 3333 + id));
            }
            let service = p.service.as_ref().unwrap();
            assert_eq!(service.ready.len(), ids.len());
            for (ready, id) in service.ready.iter().zip(&ids) {
                assert_eq!(
                    ready,
                    &format!("Listening on port {} for gdb connections", 3333 + id)
                );
            }
            assert_eq!(
                fs::canonicalize(&p.gdb.executable).unwrap(),
                fs::canonicalize(tools.join("bin/gdb/bin/arm-none-eabi-gdb.exe")).unwrap()
            );
            assert_eq!(
                fs::canonicalize(&service.command).unwrap(),
                fs::canonicalize(tools.join("bin/openocd/bin/openocd.exe")).unwrap()
            );
            assert_eq!(
                p.program.elf,
                fixture
                    .0
                    .join(format!("build/{}.elf", chip.to_ascii_uppercase()))
            );
            if chip == "stm32f429" {
                assert!(
                    service
                        .args
                        .last()
                        .unwrap()
                        .ends_with("/openocd/stm32f429.cfg")
                );
                assert_eq!(
                    fs::canonicalize(&p.program.svd).unwrap(),
                    fs::canonicalize(tools.join("svd/STM32F429.svd")).unwrap()
                );
                assert_eq!(p.registers.cpu, "cortex-m4");
                assert_eq!(p.memory_access.len(), 2);
                assert_eq!(p.memory_access[0].target, "stm32f4x.ahb");
                assert!(p.registers.cp15_command.is_empty());
                assert!(p.registers.targets.is_empty());
                assert!(p.actions.restart.iter().any(|s| s == "monitor reset halt"));
            } else {
                assert!(service.args.last().unwrap().ends_with("/openocd/tha6.cfg"));
                assert!(service.args.contains(&format!("set DEBUGCORE {mask}")));
                assert_eq!(
                    p.registers.cpu,
                    if chip == "tha6206" {
                        "cortex-r52+"
                    } else {
                        "cortex-r52"
                    }
                );
                assert_eq!(
                    fs::canonicalize(p.program.svd.parent().unwrap()).unwrap(),
                    fs::canonicalize(tools.join("svd")).unwrap()
                );
                assert_eq!(
                    p.program.svd.file_name().unwrap().to_str().unwrap(),
                    format!("{}.svd", chip.to_ascii_uppercase())
                );
                assert_eq!(p.memory_access.len(), 2);
                assert_eq!(p.memory_access[0].target, "AHB_3");
                assert_eq!(p.memory_access[1].target, "APB_1");
                assert!(p.memory_access.iter().all(|m| m.while_running));
                assert_eq!(!p.multicore.restart.is_empty(), ids.contains(&0));
                if ids.contains(&0) {
                    assert_eq!(p.multicore.restart_core, "core.0");
                    assert_eq!(p.multicore.restart[0], "monitor chipreset");
                }
                // Attaching does not reset the application or resume helper CPUs.
                assert_eq!(
                    p.target.after_connect,
                    ["monitor halt", "maintenance flush register-cache"]
                );
                assert!(!p.gdb.init.iter().any(|s| s.contains("chipreset")));
                assert!(p.gdb.init.iter().any(|s| s == "set architecture armv8-r"));
                assert!(
                    p.gdb
                        .init
                        .iter()
                        .any(|s| s == "mem 0x08000000 0x08600000 ro")
                );
                assert!(p.actions.restart.is_empty());
                assert!(p.actions.download.is_empty());
                assert!(p.actions.before_disconnect.is_empty());
                if chip == "tha6206" {
                    assert!(p.registers.cp15_command.is_empty());
                    assert!(p.registers.selector_command.is_empty());
                    assert!(p.registers.banked_command.is_empty());
                    assert!(p.registers.vfp_command.is_empty());
                    assert!(p.registers.timer_command.is_empty());
                    assert!(p.registers.pmu_command.is_empty());
                    assert!(p.registers.gic_command.is_empty());
                } else {
                    assert_eq!(p.registers.cp15_command, "aarch64 r52_read");
                }
                assert!(p.registers.vfp_write_command.is_empty());
                let available = if chip == "tha6104" {
                    1
                } else if chip == "tha6206" {
                    3
                } else {
                    15
                };
                assert!(
                    service
                        .args
                        .contains(&format!("set EXAMINECORE {available}"))
                );
                for id in ids {
                    assert_eq!(
                        p.registers.targets[&format!("core.{id}")],
                        format!("core.{id}")
                    );
                }
            }
            // Resolution never materializes selected-chip defaults into the draft.
            assert!(raw["program"].get("svd").is_none());
            assert!(raw.get("cores").is_none());
            assert_eq!(
                raw["program"]["elf"].as_str(),
                Some("build/${chip_upper}.elf")
            );
        }
        raw["debug"]["cores"] = toml::Value::try_from(vec![1]).unwrap();
        assert!(
            Project::from_document_with_catalogue(
                raw.clone(),
                Some(fixture.0.join("debug.toml")),
                None,
                Some(&catalogue)
            )
            .unwrap_err()
            .contains("supports core IDs")
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
        assert_eq!(fs::read(&profile).unwrap(), original);
    }
    #[test]
    fn backend_svd_is_profile_relative_and_project_override_does_not_hide_bad_fields() {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.0.join("shared tools/chip")).unwrap();
        fs::create_dir_all(fixture.0.join("project/chip")).unwrap();
        let profile = fixture.0.join("shared tools/debug-env.toml");
        let project = fixture.0.join("project/debug.toml");
        let catalogue = Catalogue::parse(DEFAULTS).unwrap();
        let project_text = "version=3\n[tools]\nprofile='../shared tools/debug-env.toml'\n[debug]\nchip='stm32f429'\ncores=[0]\n";
        let load_project = |override_text: &str| {
            Project::from_document_with_catalogue(
                toml::from_str(&format!("{project_text}{override_text}")).unwrap(),
                Some(project.clone()),
                None,
                Some(&catalogue),
            )
        };
        let environment =
            "[backends.stm32f4.target]\nendpoint='localhost:3333'\n[backends.stm32f4.program]\n";
        for path in [
            "chip/${chip_upper}.svd",
            "${profile_dir}/chip/${chip_upper}.svd",
        ] {
            fs::write(&profile, format!("{environment}svd='{path}'\n")).unwrap();
            let p = load_project("").unwrap();
            assert_eq!(
                fs::canonicalize(p.program.svd.parent().unwrap()).unwrap(),
                fs::canonicalize(fixture.0.join("shared tools/chip")).unwrap()
            );
            assert_eq!(p.program.svd.file_name().unwrap(), "STM32F429.svd");
            let p = load_project("[program]\nsvd='chip/custom.svd'\n").unwrap();
            assert_eq!(p.program.svd, fixture.0.join("project/chip/custom.svd"));
            assert!(
                load_project("[program]\nsvd=''\n")
                    .unwrap()
                    .program
                    .svd
                    .as_os_str()
                    .is_empty()
            );
        }
        for (bad, expected) in [
            (
                "elf='wrong.elf'",
                "Unsupported environment program field: elf",
            ),
            (
                "source_root='wrong'",
                "Unsupported environment program field: source_root",
            ),
            (
                "unknown='wrong'",
                "Unsupported environment program field: unknown",
            ),
            ("svd=42", "program.svd must be a string"),
        ] {
            fs::write(&profile, format!("{environment}{bad}\n")).unwrap();
            assert!(
                load_project("[program]\nsvd=''\n")
                    .unwrap_err()
                    .contains(expected),
                "{bad}"
            );
        }
        fs::write(
            &profile,
            "[backends.stm32f4]\nprogram=42\n[target]\nendpoint='localhost:3333'\n",
        )
        .unwrap();
        assert!(
            load_project("[program]\nsvd=''\n")
                .unwrap_err()
                .contains("must be a TOML table")
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
