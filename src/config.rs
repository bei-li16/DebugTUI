use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};
// All per-core workers merge their preferences into the same project file.
pub(crate) static PREFERENCE_WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Legacy location strings remain valid; detailed records retain disabled and data breakpoints.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BreakpointSpec {
    Location(String),
    Detailed(BreakpointOptions),
}
impl From<&str> for BreakpointSpec {
    fn from(location: &str) -> Self {
        Self::Location(location.into())
    }
}
impl BreakpointSpec {
    pub fn options(&self) -> BreakpointOptions {
        match self {
            Self::Location(location) => BreakpointOptions {
                location: location.clone(),
                ..Default::default()
            },
            Self::Detailed(options) => options.clone(),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BreakpointKind {
    #[default]
    Code,
    Hardware,
    Write,
    Read,
    Access,
}
impl BreakpointKind {
    pub fn is_data(self) -> bool {
        matches!(self, Self::Write | Self::Read | Self::Access)
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Code => "Code",
            Self::Hardware => "Hardware",
            Self::Write => "Write",
            Self::Read => "Read",
            Self::Access => "Read/write",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BreakpointOptions {
    pub location: String,
    pub kind: BreakpointKind,
    pub enabled: bool,
    pub condition: String,
    pub ignore_count: u32,
    pub temporary: bool,
}
impl Default for BreakpointOptions {
    fn default() -> Self {
        Self {
            location: String::new(),
            kind: BreakpointKind::Code,
            enabled: true,
            condition: String::new(),
            ignore_count: 0,
            temporary: false,
        }
    }
}

#[cfg(test)]
mod breakpoint_tests {
    use super::*;
    #[test]
    fn legacy_and_detailed_breakpoints_round_trip_per_core() {
        let p: Project = toml::from_str("breakpoints=['main', { location='counter', kind='read', enabled=false, condition='counter > 1', ignore_count=3 }]\n[[cores]]\nname='core0'\nendpoint='localhost:3333'\nbreakpoints=[{location='*(unsigned int *)0x20000000',kind='access',enabled=false}]\n").unwrap();
        let text = toml::to_string_pretty(&p).unwrap();
        let p2: Project = toml::from_str(&text).unwrap();
        assert_eq!(p.breakpoints, p2.breakpoints);
        assert_eq!(p.cores[0].breakpoints, p2.cores[0].breakpoints);
        assert_eq!(p.breakpoints[1].options().kind, BreakpointKind::Read);
        assert!(!p.breakpoints[1].options().enabled);
        assert!(toml::from_str::<Project>("breakpoints=[{location='x',kind='invalid'}]").is_err());
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Project {
    pub version: u32,
    pub tools: Tools,
    pub gdb: Gdb,
    pub target: Target,
    pub service: Option<Service>,
    pub actions: Actions,
    pub program: Program,
    pub session: Session,
    pub watch: Vec<String>,
    pub breakpoints: Vec<BreakpointSpec>,
    pub source_map: Vec<SourceMap>,
    pub build: Option<Build>,
    pub tasks: Tasks,
    pub ui: Ui,
    pub cores: Vec<Core>,
    pub live_watch: Option<LiveWatchConfig>,
    pub sync: Option<SyncConfig>,
    pub memory_access: Vec<MemoryAccess>,
    #[serde(skip)]
    pub path: Option<PathBuf>,
    #[serde(skip)]
    pub(crate) preference_core: Option<String>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Motion {
    Off,
    #[default]
    Subtle,
    Full,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Radix {
    Binary,
    Octal,
    #[default]
    Decimal,
    Hex,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ui {
    pub animations: Motion,
    pub unicode: bool,
    pub formats: BTreeMap<String, Radix>,
    pub refresh: BTreeMap<String, RefreshPolicy>,
}
impl Default for Ui {
    fn default() -> Self {
        Self {
            animations: Motion::Subtle,
            unicode: true,
            formats: BTreeMap::new(),
            refresh: BTreeMap::new(),
        }
    }
}
/// Hardware mappings (DAP/AP/CTI) belong to the environment, never to the TUI.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MemoryAccess {
    pub id: String,
    pub label: String,
    pub tcl_endpoint: String,
    pub target: String,
    pub while_running: bool,
    pub cores: Vec<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RefreshPolicy {
    /// Empty means the current core's GDB connection (stopped only).
    pub channel: String,
    /// Zero disables polling. Nonzero values are milliseconds, 50..60000.
    pub interval_ms: u64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tools {
    pub root: PathBuf,
    pub profile: PathBuf,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Gdb {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: BTreeMap<String, String>,
    pub unset_env: Vec<String>,
    pub init: Vec<String>,
}
impl Default for Gdb {
    fn default() -> Self {
        Self {
            executable: "gdb".into(),
            args: vec![],
            cwd: None,
            env: BTreeMap::new(),
            unset_env: vec![],
            init: vec![],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Target {
    pub mode: String,
    pub endpoint: String,
    pub after_connect: Vec<String>,
}
impl Default for Target {
    fn default() -> Self {
        Self {
            mode: "remote".into(),
            endpoint: String::new(),
            after_connect: vec![],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Service {
    pub enabled: bool,
    pub command: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: BTreeMap<String, String>,
    pub unset_env: Vec<String>,
    pub ready: Vec<String>,
    pub timeout_ms: u64,
}
impl Default for Service {
    fn default() -> Self {
        Self {
            command: PathBuf::new(),
            enabled: true,
            args: vec![],
            cwd: None,
            env: BTreeMap::new(),
            unset_env: vec![],
            ready: vec![],
            timeout_ms: 15000,
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Actions {
    pub restart: Vec<String>,
    pub run: Vec<String>,
    pub download: Vec<String>,
    pub before_disconnect: Vec<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Program {
    pub elf: PathBuf,
    pub source_root: PathBuf,
    pub svd: PathBuf,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Session {
    pub on_exit: String,
    pub timeout_ms: u64,
    pub log_dir: Option<PathBuf>,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            on_exit: "detach".into(),
            timeout_ms: 8000,
            log_dir: None,
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceMap {
    pub from: String,
    pub to: PathBuf,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Core {
    pub name: String,
    pub endpoint: String,
    pub after_connect: Vec<String>,
    pub run: Vec<String>,
    pub init: Vec<String>,
    pub startup_order: i32,
    pub watch: Option<Vec<String>>,
    pub breakpoints: Option<Vec<BreakpointSpec>>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LiveWatchConfig {
    pub tcl_endpoint: String,
    pub bus_target: String,
    pub interval_ms: u64,
    pub elf: PathBuf,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SyncConfig {
    pub method: String,
    pub open: Vec<String>,
    pub tcl_endpoint: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Build {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: PathBuf,
}
/// User shell commands, always executed in program.source_root.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tasks {
    pub build: String,
    pub download: String,
    pub timeout_ms: u64,
}
impl Default for Tasks {
    fn default() -> Self {
        Self {
            build: String::new(),
            download: String::new(),
            timeout_ms: 300_000,
        }
    }
}
fn absolute(base: &Path, value: &Path) -> PathBuf {
    if value.is_absolute() {
        value.to_owned()
    } else {
        base.join(value)
    }
}
pub fn portable_path(path: &Path) -> String {
    let s = path.to_string_lossy();
    s.strip_prefix("\\\\?\\").unwrap_or(&s).replace('\\', "/")
}
fn read_toml(path: &Path) -> Result<toml::Value, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    toml::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("{}: {e}", path.display()))
}
fn merge(base: &mut toml::Value, overlay: toml::Value) {
    if let (Some(dst), Some(src)) = (base.as_table_mut(), overlay.as_table()) {
        for (key, value) in src {
            if let Some(old) = dst.get_mut(key) {
                merge(old, value.clone());
            } else {
                dst.insert(key.clone(), value.clone());
            }
        }
    } else {
        *base = overlay;
    }
}
fn expand(value: &mut toml::Value, directory: &str) {
    match value {
        toml::Value::String(s) => *s = s.replace("${profile_dir}", directory),
        toml::Value::Array(v) => v.iter_mut().for_each(|v| expand(v, directory)),
        toml::Value::Table(v) => v.iter_mut().for_each(|(_, v)| expand(v, directory)),
        _ => {}
    }
}
fn resolve_launch_paths(value: &mut toml::Value, base: &Path) {
    for (section, executable) in [("gdb", "executable"), ("service", "command")] {
        if let Some(table) = value.get_mut(section).and_then(toml::Value::as_table_mut) {
            for key in [executable, "cwd"] {
                if let Some(toml::Value::String(s)) = table.get_mut(key) {
                    // Bare names use PATH. Explicit relative paths use their defining file.
                    if key == "cwd" || s.contains('/') || s.contains('\\') {
                        *s = portable_path(&absolute(base, Path::new(s)));
                    }
                }
            }
        }
    }
}
impl Project {
    pub fn load(path: &Path) -> Result<Self, String> {
        Self::load_with_environment(Some(path), None)
    }
    pub fn load_with_environment(
        path: Option<&Path>,
        profile: Option<&Path>,
    ) -> Result<Self, String> {
        let path = path
            .map(fs::canonicalize)
            .transpose()
            .map_err(|e| e.to_string())?;
        let raw = if let Some(p) = &path {
            read_toml(p)?
        } else {
            toml::Value::Table(toml::Table::new())
        };
        Self::from_document(raw, path, profile)
    }
    /// Resolve an in-memory project without creating a file or starting processes.
    pub fn from_document(
        mut raw: toml::Value,
        path: Option<PathBuf>,
        profile: Option<&Path>,
    ) -> Result<Self, String> {
        let base = path
            .as_ref()
            .and_then(|p| p.parent())
            .map(Path::to_owned)
            .unwrap_or(env::current_dir().map_err(|e| e.to_string())?);
        if raw.get("server").is_some() {
            return Err("Legacy [server] configuration: move service/chip settings to tools/debug-env.toml and use [target]. See README migration notes.".into());
        }
        let tools: Tools = raw
            .get("tools")
            .cloned()
            .map(toml::Value::try_into)
            .transpose()
            .map_err(|e| format!("tools: {e}"))?
            .unwrap_or_default();
        let selected = if let Some(p) = profile {
            Some(absolute(&env::current_dir().map_err(|e| e.to_string())?, p))
        } else if !tools.profile.as_os_str().is_empty() {
            Some(absolute(&base, &tools.profile))
        } else if !tools.root.as_os_str().is_empty() {
            Some(absolute(&base, &tools.root).join("debug-env.toml"))
        } else {
            None
        };
        let mut environment = toml::Value::Table(toml::Table::new());
        let mut selected_path = None;
        if let Some(p) = selected {
            let p =
                fs::canonicalize(&p).map_err(|e| format!("Environment {}: {e}", p.display()))?;
            environment = read_toml(&p)?;
            for key in environment
                .as_table()
                .ok_or("Environment must be a TOML table")?
                .keys()
            {
                if !matches!(
                    key.as_str(),
                    "gdb" | "target" | "service" | "actions" | "session" | "sync" | "memory_access"
                ) {
                    return Err(format!("Unsupported environment section: {key}"));
                }
            }
            let directory = p.parent().unwrap();
            expand(&mut environment, &portable_path(directory));
            resolve_launch_paths(&mut environment, directory);
            selected_path = Some(p);
        }
        resolve_launch_paths(&mut raw, &base);
        merge(&mut environment, raw);
        let mut p: Self = environment
            .try_into()
            .map_err(|e| format!("Project: {e}"))?;
        if p.version > 2 {
            return Err(format!("Unsupported project format {}", p.version));
        }
        if let Some(profile) = selected_path {
            p.tools.root = profile.parent().unwrap().to_owned();
            p.tools.profile = profile;
        }
        if !p.program.elf.as_os_str().is_empty() {
            p.program.elf = absolute(&base, &p.program.elf);
        }
        p.program.source_root = absolute(&base, &p.program.source_root);
        if !p.program.svd.as_os_str().is_empty() {
            p.program.svd = absolute(&base, &p.program.svd);
        }
        if let Some(live) = &mut p.live_watch {
            live.elf = absolute(&base, &live.elf);
        }
        for map in &mut p.source_map {
            map.to = absolute(&base, &map.to);
        }
        if let Some(b) = &mut p.build {
            b.cwd = absolute(&base, &b.cwd);
        }
        if let Some(d) = &mut p.session.log_dir {
            *d = absolute(&base, d);
        }
        p.path = path;
        p.validate()?;
        Ok(p)
    }
    pub fn validate(&self) -> Result<(), String> {
        if !matches!(
            self.target.mode.as_str(),
            "remote" | "extended-remote" | "local"
        ) {
            return Err("target.mode must be remote, extended-remote or local".into());
        }
        if !matches!(
            self.session.on_exit.as_str(),
            "detach" | "resume" | "disconnect"
        ) {
            return Err("session.on_exit must be detach, resume or disconnect".into());
        }
        if self.session.timeout_ms == 0 {
            return Err("Session timeout must be positive".into());
        }
        if self.tasks.timeout_ms == 0 {
            return Err("Tasks timeout must be positive".into());
        }
        if [&self.tasks.build, &self.tasks.download]
            .iter()
            .any(|s| s.contains(['\0', '\r', '\n']))
        {
            return Err("Build / download commands must be single-line shell commands".into());
        }
        if self.gdb.executable.as_os_str().is_empty() {
            return Err("gdb.executable must not be empty".into());
        }
        if self.target.endpoint.chars().any(char::is_control) {
            return Err("Invalid target endpoint".into());
        }
        if let Some(s) = &self.service
            && s.enabled
        {
            if s.command.as_os_str().is_empty() || s.timeout_ms == 0 {
                return Err("service.command and positive timeout_ms are required".into());
            }
            if s.ready.iter().any(|s| s.is_empty()) {
                return Err("Service readiness markers must not be empty".into());
            }
        }
        for commands in [
            &self.gdb.init,
            &self.target.after_connect,
            &self.actions.restart,
            &self.actions.run,
            &self.actions.download,
            &self.actions.before_disconnect,
        ] {
            if commands
                .iter()
                .any(|s| s.trim().is_empty() || s.contains(['\n', '\r']))
            {
                return Err("Environment actions must be nonempty single-line GDB commands".into());
            }
        }
        let mut names = std::collections::HashSet::new();
        let mut endpoints = std::collections::HashSet::new();
        for core in &self.cores {
            if core.name.trim().is_empty() || core.name.chars().any(char::is_control) {
                return Err("Each core requires a nonempty name without control characters".into());
            }
            if !names.insert(&core.name) {
                return Err(format!("Duplicate core name: {}", core.name));
            }
            if core.endpoint.trim().is_empty() || core.endpoint.chars().any(char::is_control) {
                return Err(format!("Core {} requires an endpoint", core.name));
            }
            if self.target.mode != "local" && !endpoints.insert(&core.endpoint) {
                return Err(format!("Duplicate core endpoint: {}", core.endpoint));
            }
            for commands in [&core.after_connect, &core.run, &core.init] {
                if commands
                    .iter()
                    .any(|s| s.trim().is_empty() || s.contains(['\n', '\r']))
                {
                    return Err(format!(
                        "Core {} commands must be nonempty single-line GDB commands",
                        core.name
                    ));
                }
            }
        }
        let mut channels = std::collections::HashSet::new();
        for access in &self.memory_access {
            if access.id.is_empty()
                || !channels.insert(&access.id)
                || [
                    &access.id,
                    &access.label,
                    &access.target,
                    &access.tcl_endpoint,
                ]
                .iter()
                .any(|s| s.chars().any(char::is_control))
                || access.target.is_empty()
                || access.tcl_endpoint.is_empty()
            {
                return Err("memory_access needs unique ids, target and tcl_endpoint without control characters".into());
            }
            if access
                .cores
                .iter()
                .any(|name| !self.cores.iter().any(|c| &c.name == name))
            {
                return Err(format!(
                    "Memory access {} references an unknown core",
                    access.id
                ));
            }
        }
        for policy in self.ui.refresh.values() {
            if policy.interval_ms != 0 && !(50..=60000).contains(&policy.interval_ms) {
                return Err("Refresh interval must be 0 (off) or 50..60000 ms".into());
            }
        }
        if let Some(lw) = &self.live_watch {
            if lw.tcl_endpoint.is_empty()
                || lw.bus_target.is_empty()
                || lw.elf.as_os_str().is_empty()
            {
                return Err("live_watch requires tcl_endpoint, bus_target, and elf".into());
            }
            if lw.interval_ms == 0 {
                return Err("live_watch.interval_ms must be positive".into());
            }
            if lw.bus_target.chars().any(char::is_control) {
                return Err("live_watch.bus_target must not contain control characters".into());
            }
        }
        if let Some(sync) = &self.sync {
            if !sync.open.is_empty() && self.cores.is_empty() {
                return Err("sync.open requires an explicit [[cores]] configuration".into());
            }
            if !matches!(sync.method.as_str(), "" | "tcl" | "cti") {
                return Err("sync.method must be tcl or cti (configured TCL commands)".into());
            }
            if !sync.open.is_empty() && sync.tcl_endpoint.is_empty() && self.live_watch.is_none() {
                return Err(
                    "sync.open requires sync.tcl_endpoint or live_watch.tcl_endpoint".into(),
                );
            }
            if sync
                .open
                .iter()
                .any(|s| s.trim().is_empty() || s.contains(['\n', '\r', '\x1a']))
            {
                return Err("sync.open commands must be nonempty single-line strings".into());
            }
        }
        Ok(())
    }
    pub fn prepare(&mut self) -> Result<(), String> {
        self.validate()?;
        if self.target.mode != "local" && self.target.endpoint.is_empty() && self.cores.is_empty() {
            return Err(
                "Set --connect HOST:PORT or target.endpoint; use --local for a local inferior"
                    .into(),
            );
        }
        if !self.program.elf.as_os_str().is_empty() {
            self.program.elf = fs::canonicalize(&self.program.elf)
                .map_err(|e| format!("Program {}: {e}", self.program.elf.display()))?;
            if !self.program.elf.is_file() {
                return Err("Program must be a file".into());
            }
            if self.program.source_root.as_os_str().is_empty() {
                self.program.source_root = self.program.elf.parent().unwrap().to_owned();
            }
        }
        Ok(())
    }
    pub fn has_build(&self) -> bool {
        !self.tasks.build.trim().is_empty() || self.build.is_some()
    }
    /// A configured build can create the ELF before a debugger is connected.
    pub fn prepare_workspace(&mut self) -> Result<bool, String> {
        if self.has_build()
            && !self.program.elf.as_os_str().is_empty()
            && !self.program.elf.is_file()
        {
            self.validate()?;
            return Ok(false);
        }
        self.prepare()?;
        Ok(true)
    }
    pub fn has_download(&self) -> bool {
        !self.tasks.download.trim().is_empty() || !self.actions.download.is_empty()
    }
    pub fn source_path(&self, file: &str) -> Option<PathBuf> {
        let normalized = file.replace('\\', "/");
        for map in &self.source_map {
            let from = map.from.replace('\\', "/").trim_end_matches('/').to_owned();
            if normalized == from || normalized.starts_with(&(from.clone() + "/")) {
                let result = map
                    .to
                    .join(normalized[from.len()..].trim_start_matches('/'));
                if result.is_file() {
                    return Some(result);
                }
            }
        }
        [
            PathBuf::from(file),
            self.program.source_root.join(file),
            self.program
                .elf
                .parent()
                .unwrap_or(Path::new("."))
                .join(file),
        ]
        .into_iter()
        .find(|p| p.is_file())
    }
    pub fn save_preferences(
        &self,
        watch: Vec<String>,
        breakpoints: Vec<BreakpointSpec>,
    ) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let _guard = PREFERENCE_WRITE.lock().map_err(|e| e.to_string())?;
        let mut raw = read_toml(path)?;
        let root = raw.as_table_mut().ok_or("Project must be a TOML table")?;
        let table = if let Some(name) = &self.preference_core {
            if !root.contains_key("cores") {
                root.insert(
                    "cores".into(),
                    toml::Value::try_from(&self.cores).map_err(|e| e.to_string())?,
                );
            }
            root.get_mut("cores")
                .and_then(toml::Value::as_array_mut)
                .and_then(|cores| {
                    cores
                        .iter_mut()
                        .find(|c| c.get("name").and_then(toml::Value::as_str) == Some(name))
                })
                .and_then(toml::Value::as_table_mut)
                .ok_or_else(|| format!("Core {name} changed on disk; preferences were not saved"))?
        } else {
            root
        };
        table.insert(
            "watch".into(),
            toml::Value::Array(watch.into_iter().map(toml::Value::String).collect()),
        );
        table.insert(
            "breakpoints".into(),
            toml::Value::try_from(breakpoints).map_err(|e| e.to_string())?,
        );
        fs::write(
            path,
            toml::to_string_pretty(&raw).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    }
    /// Serialized by the session worker; merge with the latest project, never a tools profile.
    pub fn save_ui(&self, ui: &Ui) -> Result<bool, String> {
        let Some(path) = &self.path else {
            return Ok(false);
        };
        let _guard = PREFERENCE_WRITE.lock().map_err(|e| e.to_string())?;
        let mut raw = read_toml(path)?;
        raw.as_table_mut()
            .ok_or("Project must be a TOML table")?
            .insert(
                "ui".into(),
                toml::Value::try_from(ui).map_err(|e| e.to_string())?,
            );
        fs::write(
            path,
            toml::to_string_pretty(&raw).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        Ok(true)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generic_modes() {
        let mut p = Project::default();
        for mode in ["remote", "extended-remote", "local"] {
            p.target.mode = mode.into();
            for policy in ["detach", "resume", "disconnect"] {
                p.session.on_exit = policy.into();
                assert!(p.validate().is_ok());
            }
        }
        p.target.mode = "unknown".into();
        assert!(p.validate().is_err());
    }
    #[test]
    fn maps_windows_paths() {
        assert_eq!(
            portable_path(Path::new(r"\\?\G:\a b\工程.elf")),
            "G:/a b/工程.elf"
        );
    }
    #[test]
    fn standalone_needs_no_tools_or_symbols() {
        let mut p = Project::default();
        p.target.mode = "local".into();
        assert!(p.prepare().is_ok());
        assert!(p.service.is_none());
        assert!(p.actions.restart.is_empty());
    }
    #[test]
    fn environment_overlay_preserves_paths_and_preferences() {
        let base = env::temp_dir().join(format!("debugtui-env-{}", std::process::id()));
        fs::create_dir_all(base.join("tools")).unwrap();
        fs::write(base.join("tools/debug-env.toml"),"[gdb]\nexecutable='./custom-gdb'\nargs=['--data-directory=${profile_dir}/data']\n[target]\nmode='extended-remote'\nendpoint='localhost:3333'\n[actions]\nrestart=['monitor custom-reset']\n").unwrap();
        let path = base.join("project.toml");
        fs::write(&path,"version=2\n[tools]\nroot='tools'\n[target]\nendpoint='localhost:4444'\n[program]\nelf='../app.elf'\n").unwrap();
        let p = Project::load(&path).unwrap();
        assert_eq!(p.target.endpoint, "localhost:4444");
        assert_eq!(p.target.mode, "extended-remote");
        assert!(p.gdb.executable.ends_with("tools/custom-gdb"));
        assert!(!p.gdb.args[0].contains("${profile_dir}"));
        p.save_preferences(vec!["counter".into()], vec!["main".into()])
            .unwrap();
        let saved = read_toml(&path).unwrap();
        assert!(saved.get("gdb").is_none());
        assert_eq!(saved["tools"]["root"].as_str(), Some("tools"));
        assert_eq!(saved["program"]["elf"].as_str(), Some("../app.elf"));
        let profile_before = fs::read(base.join("tools/debug-env.toml")).unwrap();
        let mut ui = Ui {
            animations: Motion::Full,
            unicode: false,
            ..Default::default()
        };
        ui.formats.insert("watch:counter".into(), Radix::Binary);
        ui.refresh.insert(
            "single|watch:counter".into(),
            RefreshPolicy {
                channel: "bus".into(),
                interval_ms: 100,
            },
        );
        let mut open_setup = crate::launch::Document::open(&path).unwrap();
        open_setup.set("target", "endpoint", "localhost:5555".into());
        assert!(p.save_ui(&ui).unwrap());
        open_setup.save().unwrap();
        p.save_preferences(vec!["counter".into()], vec!["main".into()])
            .unwrap();
        let reloaded = Project::load(&path).unwrap();
        assert_eq!(reloaded.ui.animations, Motion::Full);
        assert_eq!(reloaded.target.endpoint, "localhost:5555");
        assert!(!reloaded.ui.unicode);
        assert_eq!(
            reloaded.ui.formats.get("watch:counter"),
            Some(&Radix::Binary)
        );
        assert_eq!(reloaded.watch, vec!["counter"]);
        assert_eq!(reloaded.ui.refresh["single|watch:counter"].interval_ms, 100);
        assert_eq!(
            profile_before,
            fs::read(base.join("tools/debug-env.toml")).unwrap()
        );
        fs::remove_file(path).unwrap();
        fs::remove_file(base.join("tools/debug-env.toml")).unwrap();
        fs::remove_dir(base.join("tools")).unwrap();
        fs::remove_dir(base).unwrap();
    }
    #[test]
    fn single_core_backward_compatible() {
        let mut p = Project::default();
        p.target.mode = "extended-remote".into();
        p.target.endpoint = "localhost:3333".into();
        assert!(p.validate().is_ok());
        assert!(p.cores.is_empty());
        assert!(p.live_watch.is_none());
        assert!(p.sync.is_none());
    }
    #[test]
    fn environment_memory_channels_are_portable_and_validate_core_restrictions() {
        let p = Project::from_document(
            toml::from_str("[tools]\nprofile='tools/debug-env-openocd.toml'").unwrap(),
            None,
            None,
        )
        .unwrap();
        assert_eq!(p.memory_access.len(), 2);
        assert!(p.memory_access[0].while_running);
        assert!(
            p.service
                .as_ref()
                .unwrap()
                .command
                .ends_with("tools/bin/openocd/bin/openocd.exe")
        );
        let mut p = p;
        p.memory_access[0].cores = vec!["absent-core".into()];
        assert!(p.validate().is_err());
        p.memory_access[0].cores.clear();
        p.memory_access[1].id = p.memory_access[0].id.clone();
        assert!(p.validate().is_err());
    }
    #[test]
    fn multicore_requires_unique_targets_and_allows_omitted_root_endpoint() {
        let mut p = Project {
            cores: vec![
                Core {
                    name: "cpu0".into(),
                    endpoint: "localhost:3333".into(),
                    ..Default::default()
                },
                Core {
                    name: "cpu1".into(),
                    endpoint: "localhost:3334".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert!(p.prepare().is_ok());
        p.cores[1].endpoint = "localhost:3333".into();
        assert!(p.validate().is_err());
        p.cores[1].endpoint = "localhost:3334".into();
        p.cores[1].name = "cpu0".into();
        assert!(p.validate().is_err());
    }
    #[test]
    fn concurrent_core_preferences_preserve_other_cores_and_root_settings() {
        let path = env::temp_dir().join(format!("debugtui-cores-{}.toml", std::process::id()));
        fs::write(&path,"watch=['root']\nbreakpoints=['main']\n[[cores]]\nname='cpu0'\nendpoint='localhost:3333'\n[[cores]]\nname='cpu1'\nendpoint='localhost:3334'\n").unwrap();
        let p = Project::load(&path).unwrap();
        let mut setup = crate::launch::Document::open(&path).unwrap();
        let workers: Vec<_> = (0..2)
            .map(|i| {
                let mut p = p.clone();
                p.preference_core = Some(format!("cpu{i}"));
                std::thread::spawn(move || {
                    p.save_preferences(vec![format!("counter{i}")], vec![])
                        .unwrap()
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        setup.save().unwrap();
        let p = Project::load(&path).unwrap();
        assert_eq!(p.watch, vec!["root"]);
        assert_eq!(p.breakpoints, vec![BreakpointSpec::from("main")]);
        assert_eq!(p.cores[0].watch, Some(vec!["counter0".into()]));
        assert_eq!(p.cores[1].watch, Some(vec!["counter1".into()]));
        assert_eq!(p.cores[0].breakpoints, Some(vec![]));
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn multi_core_config_parses() {
        let toml_text = r#"
version = 2
[gdb]
executable = "arm-none-eabi-gdb"
[program]
elf = "./build/app.elf"
[[cores]]
name = "core.0"
endpoint = "localhost:3333"
after_connect = ["monitor chipreset"]
run = ["tbreak _main", "continue"]
startup_order = 1
[[cores]]
name = "core.1"
endpoint = "localhost:3334"
startup_order = 0
[live_watch]
tcl_endpoint = "localhost:6666"
bus_target = "AHB_3"
interval_ms = 200
elf = "./build/app.elf"
[sync]
method = "cti"
open = ["targets APB_1; mww 0x80420140 0x3"]
"#;
        let raw: toml::Value = toml::from_str(toml_text).unwrap();
        let p: Project = raw.try_into().unwrap();
        assert_eq!(p.cores.len(), 2);
        assert_eq!(p.cores[0].name, "core.0");
        assert_eq!(p.cores[0].endpoint, "localhost:3333");
        assert_eq!(p.cores[1].startup_order, 0);
        assert!(p.live_watch.is_some());
        assert_eq!(p.live_watch.as_ref().unwrap().bus_target, "AHB_3");
        assert!(p.sync.is_some());
        assert!(p.validate().is_ok());
    }
}
