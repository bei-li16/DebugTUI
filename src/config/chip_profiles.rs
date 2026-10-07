//! File-based chip settings. Loading only resolves configuration; it never runs tools.
use super::*;
use crate::devices::{Device, Selection, expand_selection};

#[cfg(test)]
mod tests;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Directory {
    directory: PathBuf,
    #[serde(default)]
    file: Option<PathBuf>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Probe {
    config: PathBuf,
}

pub(crate) struct Loaded {
    pub value: toml::Value,
    pub register_layers: Vec<register_sources::Layer>,
    pub memory_access_source: Option<String>,
}

struct Loader<'a> {
    tools_dir: &'a Path,
    probe: Option<PathBuf>,
    selection: &'a Selection,
    device: &'a Device,
    stack: Vec<PathBuf>,
    register_layers: Vec<register_sources::Layer>,
    memory_access_source: Option<String>,
}

pub(crate) fn load(
    settings: toml::Value,
    probe: Option<toml::Value>,
    selection: &Selection,
    device: &Device,
    profile: &Path,
) -> Result<Loaded, String> {
    let settings: Directory = settings
        .try_into()
        .map_err(|e| format!("chip_profiles: {e}"))?;
    if settings.directory.as_os_str().is_empty() {
        return Err("chip_profiles.directory must not be empty".into());
    }
    let tools_dir = profile
        .parent()
        .ok_or("Tools profile needs a parent directory")?;
    let probe = probe
        .map(|value| -> Result<PathBuf, String> {
            let probe: Probe = value.try_into().map_err(|e| format!("probe: {e}"))?;
            if probe.config.as_os_str().is_empty() {
                return Err("probe.config must not be empty".into());
            }
            let path = absolute(tools_dir, &probe.config);
            let path = fs::canonicalize(&path)
                .map_err(|e| format!("Probe config {}: {e}", path.display()))?;
            if !path.is_file() {
                return Err(format!("Probe config is not a file: {}", path.display()));
            }
            Ok(path)
        })
        .transpose()?;
    let path = settings
        .file
        .map(|p| absolute(tools_dir, &p))
        .unwrap_or_else(|| {
            absolute(tools_dir, &settings.directory).join(format!("{}.toml", selection.chip))
        });
    let mut loader = Loader {
        tools_dir,
        probe,
        selection,
        device,
        stack: Vec::new(),
        register_layers: Vec::new(),
        memory_access_source: None,
    };
    let mut value = loader.read(&path)?;
    let backend = value.as_table_mut().unwrap().remove("backend");
    if backend.is_none() {
        return Err(format!(
            "Chip profile {} must declare backend (directly or through extends)",
            path.display()
        ));
    }
    Ok(Loaded {
        value,
        register_layers: loader.register_layers,
        memory_access_source: loader.memory_access_source,
    })
}

impl Loader<'_> {
    fn read(&mut self, path: &Path) -> Result<toml::Value, String> {
        let path =
            fs::canonicalize(path).map_err(|e| format!("Chip profile {}: {e}", path.display()))?;
        if self.stack.contains(&path) {
            return Err(format!("Chip profile extends cycle at {}", path.display()));
        }
        if self.stack.len() >= 8 {
            return Err(format!(
                "Chip profile extends depth exceeds 8 at {}",
                path.display()
            ));
        }
        self.stack.push(path.clone());
        let result = self.read_document(&path);
        self.stack.pop();
        result.map_err(|e| format!("Chip profile {}: {e}", path.display()))
    }

    fn read_document(&mut self, path: &Path) -> Result<toml::Value, String> {
        let mut value = read_toml(path)?;
        let table = value.as_table_mut().ok_or("Expected a TOML table")?;
        for key in table.keys() {
            if !matches!(
                key.as_str(),
                "extends"
                    | "backend"
                    | "gdb"
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
                return Err(format!("Unsupported chip profile section: {key}"));
            }
        }
        if let Some(backend) = table.get("backend") {
            let backend = backend.as_str().ok_or("backend must be a string")?;
            if backend != self.device.backend {
                return Err(format!(
                    "Backend '{backend}' does not match chip '{}' catalogue backend '{}'",
                    self.selection.chip, self.device.backend
                ));
            }
        }
        let parent = table
            .remove("extends")
            .map(|value| {
                value
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .ok_or_else(|| "extends must be a non-empty file path string".to_owned())
            })
            .transpose()?;
        validate_environment_program(&value)?;
        let directory = path.parent().unwrap();
        let mut inherited = if let Some(parent) = parent {
            let parent = if parent.starts_with("builtin:") {
                crate::bundled_tools::resource(self.tools_dir, &parent)?
            } else {
                absolute(directory, Path::new(&parent))
            };
            self.read(&parent)?
        } else {
            toml::Value::Table(Default::default())
        };
        expand_selection(&mut value, self.selection, self.device);
        self.expand_paths(&mut value, directory)?;
        resolve_launch_paths(&mut value, directory);
        if let Some(toml::Value::String(svd)) =
            value.get_mut("program").and_then(|p| p.get_mut("svd"))
            && !svd.is_empty()
        {
            *svd = portable_path(&crate::bundled_tools::resolve_svd(
                Path::new(svd),
                directory,
            )?);
        }
        if let Some(registers) = value.get("registers") {
            self.register_layers.push(register_sources::Layer {
                section: format!("chip profile:{} [registers]", portable_path(path)),
                values: registers.clone(),
            });
        }
        if value.get("memory_access").is_some() {
            self.memory_access_source = Some(format!(
                "chip profile:{} [memory_access]",
                portable_path(path)
            ));
        }
        merge(&mut inherited, value);
        Ok(inherited)
    }

    fn expand_paths(&self, value: &mut toml::Value, directory: &Path) -> Result<(), String> {
        match value {
            toml::Value::String(text) => {
                *text = text
                    .replace("${profile_dir}", &portable_path(directory))
                    .replace("${tools_dir}", &portable_path(self.tools_dir));
                if text.contains("${probe_config}") {
                    let probe = self.probe.as_ref().ok_or("Set [probe] config in the common tools profile before using ${probe_config}")?;
                    *text = text.replace("${probe_config}", &portable_path(probe));
                }
            }
            toml::Value::Array(values) => {
                for value in values {
                    self.expand_paths(value, directory)?;
                }
            }
            toml::Value::Table(values) => {
                for (_, value) in values.iter_mut() {
                    self.expand_paths(value, directory)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
