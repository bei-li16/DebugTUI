use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};
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
    pub breakpoints: Vec<String>,
    pub source_map: Vec<SourceMap>,
    pub build: Option<Build>,
    pub tasks: Tasks,
    pub ui: Ui,
    #[serde(skip)]
    pub path: Option<PathBuf>,
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
}
impl Default for Ui {
    fn default() -> Self {
        Self {
            animations: Motion::Subtle,
            unicode: true,
            formats: BTreeMap::new(),
        }
    }
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
                    "gdb" | "target" | "service" | "actions" | "session"
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
        Ok(())
    }
    pub fn prepare(&mut self) -> Result<(), String> {
        self.validate()?;
        if self.target.mode != "local" && self.target.endpoint.is_empty() {
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
        breakpoints: Vec<String>,
    ) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let mut raw = read_toml(path)?;
        let table = raw.as_table_mut().ok_or("Project must be a TOML table")?;
        table.insert(
            "watch".into(),
            toml::Value::Array(watch.into_iter().map(toml::Value::String).collect()),
        );
        table.insert(
            "breakpoints".into(),
            toml::Value::Array(breakpoints.into_iter().map(toml::Value::String).collect()),
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
        assert_eq!(
            profile_before,
            fs::read(base.join("tools/debug-env.toml")).unwrap()
        );
        fs::remove_file(path).unwrap();
        fs::remove_file(base.join("tools/debug-env.toml")).unwrap();
        fs::remove_dir(base.join("tools")).unwrap();
        fs::remove_dir(base).unwrap();
    }
}
