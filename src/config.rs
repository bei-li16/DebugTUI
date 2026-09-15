use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Project {
    pub version: u32,
    pub tools: Tools,
    pub program: Program,
    pub server: Server,
    pub session: Session,
    pub watch: Vec<String>,
    pub breakpoints: Vec<String>,
    pub source_map: Vec<SourceMap>,
    pub build: Option<Build>,
    #[serde(skip)]
    pub path: Option<PathBuf>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tools {
    pub root: PathBuf,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Program {
    pub elf: PathBuf,
    pub source_root: PathBuf,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Server {
    pub mode: String,
    pub device: String,
    pub interface: String,
    pub speed_khz: u32,
    pub host: String,
    pub port: u16,
    pub serial: Option<String>,
}
impl Default for Server {
    fn default() -> Self {
        Self {
            mode: "managed".into(),
            device: "STM32F429IG".into(),
            interface: "SWD".into(),
            speed_khz: 4000,
            host: "127.0.0.1".into(),
            port: 3333,
            serial: None,
        }
    }
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
            on_exit: "resume".into(),
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
impl Project {
    pub fn load(path: &Path) -> Result<Self, String> {
        let path = fs::canonicalize(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let mut p: Self =
            toml::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
        if p.version > 1 {
            return Err(format!("Unsupported project format {}", p.version));
        }
        let base = path.parent().unwrap();
        if !p.tools.root.as_os_str().is_empty() {
            p.tools.root = absolute(base, &p.tools.root);
        }
        if !p.program.elf.as_os_str().is_empty() {
            p.program.elf = absolute(base, &p.program.elf);
        }
        p.program.source_root = absolute(base, &p.program.source_root);
        for map in &mut p.source_map {
            map.to = absolute(base, &map.to);
        }
        if let Some(b) = &mut p.build {
            b.cwd = absolute(base, &b.cwd);
        }
        if let Some(d) = &mut p.session.log_dir {
            *d = absolute(base, d);
        }
        p.path = Some(path);
        p.validate()?;
        Ok(p)
    }
    pub fn validate(&self) -> Result<(), String> {
        if !matches!(self.server.mode.as_str(), "managed" | "external") {
            return Err("server.mode must be managed or external".into());
        }
        if self.session.on_exit != "resume" {
            return Err("session.on_exit must be resume: this J-Link backend resumes the MCU when the server exits. Keep the session open to retain a halted target.".into());
        }
        if self.server.port == 0 || self.server.speed_khz == 0 || self.session.timeout_ms == 0 {
            return Err("Port, speed and timeout must be positive".into());
        }
        if self.server.host.is_empty()
            || !self
                .server
                .host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'))
        {
            return Err("Invalid TCP host".into());
        }
        Ok(())
    }
    pub fn resolve_tools(&mut self) -> Result<(), String> {
        if self.tools.root.as_os_str().is_empty() {
            let exe = env::current_exe().map_err(|e| e.to_string())?;
            let parent = exe.parent().unwrap();
            let candidates = [
                parent.join("tools"),
                parent.join("../tools"),
                env::current_dir().map_err(|e| e.to_string())?.join("tools"),
            ];
            self.tools.root = candidates
                .into_iter()
                .find(|p| p.join("bin/gdb/bin/arm-none-eabi-gdb.exe").is_file())
                .ok_or("Cannot find bundled tools. Use --tools-dir PATH.")?;
        }
        self.tools.root = fs::canonicalize(&self.tools.root).map_err(|e| format!("tools: {e}"))?;
        for file in [
            "bin/gdb/bin/arm-none-eabi-gdb.exe",
            "bin/gdb/arm-none-eabi/share/gdb",
            "bin/gdb/lib/debug",
        ] {
            if !self.tools.root.join(file).exists() {
                return Err(format!("Missing tools resource: {file}"));
            }
        }
        if self.server.mode == "managed" {
            for file in ["bin/jlink/JLinkGDBServerCL.exe", "bin/jlink/JLink_x64.dll"] {
                if !self.tools.root.join(file).is_file() {
                    return Err(format!("Missing tools dependency: {file}"));
                }
            }
        }
        if !self.program.elf.is_file() {
            return Err(format!("ELF not found: {}", self.program.elf.display()));
        }
        self.program.elf = fs::canonicalize(&self.program.elf).map_err(|e| e.to_string())?;
        if self.program.source_root.as_os_str().is_empty() {
            self.program.source_root = self.program.elf.parent().unwrap().to_owned();
        }
        self.validate()
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
        // Update only session preferences in the original file, preserving relative paths.
        let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
        let mut raw: toml::Value =
            toml::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_modes() {
        let mut p = Project::default();
        p.server.mode = "other".into();
        assert!(p.validate().is_err());
        p.server.mode = "managed".into();
        p.session.on_exit = "halt".into();
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
    fn saves_missing_preferences_without_changing_relative_paths() {
        let path = env::temp_dir().join(format!("debugtui-config-{}.toml", std::process::id()));
        fs::write(
            &path,
            "version = 1\n[program]\nelf = '../firmware/app.elf'\n",
        )
        .unwrap();
        let project = Project::load(&path).unwrap();
        project
            .save_preferences(vec!["xTickCount".into()], vec!["main".into()])
            .unwrap();
        let saved: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            saved["program"]["elf"].as_str(),
            Some("../firmware/app.elf")
        );
        assert_eq!(saved["watch"][0].as_str(), Some("xTickCount"));
        assert_eq!(saved["breakpoints"][0].as_str(), Some("main"));
        fs::remove_file(path).unwrap();
    }
}
