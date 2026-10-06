//! Declaration diagnostics captured during resolution, never access authority.
use super::*;
use serde_json::Value;

#[derive(Clone, Debug)]
pub(crate) struct Layer {
    /// A profile section, or the configured chip association that supplied CPU.
    pub section: String,
    pub values: toml::Value,
}

#[derive(Clone, Debug, Serialize)]
pub struct Declaration {
    pub source: String,
    pub value: Value,
}
#[derive(Clone, Debug)]
struct Trace {
    declaration: Declaration,
    overrides: Vec<Declaration>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Setting {
    /// Separate path segments preserve map keys containing '.' or ':'.
    pub path: Vec<String>,
    pub value: Value,
    pub source: String,
    pub overrides: Vec<Declaration>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Replacement {
    pub field: String,
    pub source: String,
    pub replaces_sources: Vec<String>,
    pub removed_paths: Vec<Vec<String>>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub core: String,
    pub settings: Vec<Setting>,
    pub replacements: Vec<Replacement>,
}
impl Report {
    pub fn lines(&self) -> Vec<String> {
        let mut lines = vec![format!(
            "Register configuration [{}]: declared values; not hardware capability evidence",
            self.core
        )];
        for setting in &self.settings {
            let path = display_path(&setting.path);
            lines.push(format!(
                "{path} = {}; source {}",
                setting.value, setting.source
            ));
            for previous in &setting.overrides {
                lines.push(format!(
                    "  overrides {path} = {} from {}",
                    previous.value, previous.source
                ));
            }
        }
        for replacement in &self.replacements {
            lines.push(format!(
                "{}: whole-map replacement by {}; inherited sources {}; removed keys {}",
                replacement.field,
                replacement.source,
                replacement.replaces_sources.join(", "),
                replacement
                    .removed_paths
                    .iter()
                    .map(|path| display_path(path))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        lines
    }
}
fn display_path(path: &[String]) -> String {
    let mut segments = path.iter();
    let mut result = segments.next().cloned().unwrap_or_default();
    for segment in segments {
        result.push_str(&format!("[{segment:?}]"));
    }
    result
}
fn config_value(config: &crate::registers::Config) -> Value {
    let mut value = serde_json::to_value(config).expect("register configuration serializes");
    // These runtime diagnostic values must remain visible when their persistent
    // serialization deliberately omits defaults.
    value["component_owners"] = serde_json::to_value(&config.component_owners).unwrap();
    value["mmio_probe"] = Value::Bool(config.mmio_probe);
    value
}
fn flatten(value: &Value, path: &mut Vec<String>, leaves: &mut BTreeMap<Vec<String>, Value>) {
    if let Some(table) = value.as_object().filter(|table| !table.is_empty()) {
        for (key, value) in table {
            path.push(key.clone());
            flatten(value, path, leaves);
            path.pop();
        }
    } else if !path.is_empty() {
        leaves.insert(path.clone(), value.clone());
    }
}
fn leaves(value: &Value) -> BTreeMap<Vec<String>, Value> {
    let mut leaves = BTreeMap::new();
    flatten(value, &mut vec![], &mut leaves);
    leaves
}

#[derive(Clone, Debug)]
pub struct Sources {
    entries: BTreeMap<Vec<String>, Trace>,
    core_values: BTreeMap<String, Value>,
    project_source: String,
}
impl Default for Sources {
    fn default() -> Self {
        Self {
            entries: leaves(&config_value(&crate::registers::Config::default()))
                .into_iter()
                .map(|(path, value)| {
                    (
                        path,
                        Trace {
                            declaration: Declaration {
                                source: "builtin defaults".into(),
                                value,
                            },
                            overrides: vec![],
                        },
                    )
                })
                .collect(),
            core_values: BTreeMap::new(),
            project_source: "project:inline".into(),
        }
    }
}
impl Sources {
    pub(super) fn overlay(&mut self, value: &toml::Value, source: String) {
        self.merge_value(&serde_json::to_value(value).unwrap(), &mut vec![], &source);
    }
    fn merge_value(&mut self, value: &Value, path: &mut Vec<String>, source: &str) {
        if let Some(table) = value.as_object().filter(|table| !table.is_empty()) {
            self.entries.remove(path); // An empty container becomes populated.
            for (key, value) in table {
                path.push(key.clone());
                self.merge_value(value, path, source);
                path.pop();
            }
            return;
        }
        if path.is_empty() {
            return;
        }
        // Root/profile/project tables recursively merge; {} does not erase the
        // existing entries. CoreConfig's whole-map replacement is handled below.
        if value.as_object().is_some()
            && self
                .entries
                .keys()
                .any(|key| key.len() > path.len() && key.starts_with(path))
        {
            return;
        }
        let previous = self.entries.remove(path);
        let mut overrides = previous
            .map(|previous| {
                let mut history = previous.overrides;
                history.push(previous.declaration);
                history
            })
            .unwrap_or_default();
        overrides.retain(|declaration| declaration.source != source);
        self.entries.retain(|key, _| !key.starts_with(path));
        self.entries.insert(
            path.clone(),
            Trace {
                declaration: Declaration {
                    source: source.into(),
                    value: value.clone(),
                },
                overrides,
            },
        );
    }
    pub(super) fn capture_cores(&mut self, project: &Project, source: String) {
        self.project_source = source;
        self.core_values = project
            .cores
            .iter()
            .filter_map(|core| {
                core.registers
                    .as_ref()
                    .map(|settings| (core.name.clone(), serde_json::to_value(settings).unwrap()))
            })
            .collect();
    }
    pub fn report(&self, core: &str, config: &crate::registers::Config) -> Report {
        let actual_value = config_value(config);
        let mut resolved = self.clone();
        let mut replacements = vec![];
        if let Some(fields) = self.core_values.get(core).and_then(Value::as_object) {
            let source = format!("{} [cores.registers for {core}]", self.project_source);
            for (field, value) in fields {
                let path = vec![field.clone()];
                if value.is_object() {
                    let inherited: Vec<_> = resolved
                        .entries
                        .iter()
                        .filter(|(key, _)| key.starts_with(&path))
                        .map(|(key, trace)| (key.clone(), trace.declaration.source.clone()))
                        .collect();
                    resolved.entries.retain(|key, _| !key.starts_with(&path));
                    resolved.merge_value(value, &mut path.clone(), &source);
                    if actual_value.get(field) == Some(value) {
                        replacements.push(Replacement {
                            field: field.clone(),
                            source: source.clone(),
                            replaces_sources: inherited
                                .iter()
                                .map(|(_, source)| source.clone())
                                .collect::<std::collections::BTreeSet<_>>()
                                .into_iter()
                                .collect(),
                            removed_paths: inherited
                                .into_iter()
                                .map(|(key, _)| key)
                                .filter(|key| !resolved.entries.contains_key(key))
                                .collect(),
                        });
                    }
                } else {
                    resolved.merge_value(value, &mut path.clone(), &source);
                }
            }
        }
        let settings = leaves(&actual_value)
            .into_iter()
            .map(|(path, value)| {
                let (source, overrides) = match resolved.entries.get(&path) {
                    Some(trace) if trace.declaration.value == value => {
                        (trace.declaration.source.clone(), trace.overrides.clone())
                    }
                    trace => {
                        let mut overrides = trace
                            .map(|trace| trace.overrides.clone())
                            .unwrap_or_default();
                        if let Some(trace) = trace {
                            overrides.push(trace.declaration.clone());
                        }
                        (
                            "runtime/inline configuration (origin unavailable)".into(),
                            overrides,
                        )
                    }
                };
                Setting {
                    path,
                    value,
                    source,
                    overrides,
                }
            })
            .collect();
        Report {
            core: core.into(),
            settings,
            replacements,
        }
    }
}

impl Project {
    pub fn register_configuration(&self, core: &str) -> Report {
        self.register_sources
            .report(core, &self.registers_for_core(core))
    }
    pub(crate) fn register_configuration_lines(&self, core: &str) -> Vec<String> {
        let mut lines = self.register_configuration(core).lines();
        for channel in &self.memory_access {
            if channel.cores.is_empty() || channel.cores.iter().any(|name| name == core) {
                lines.push(format!(
                    "Configured memory channel: {}; source {} (not an observed route)",
                    serde_json::to_string(channel).unwrap(),
                    if self.memory_access_source.is_empty() {
                        "configuration (origin unavailable)"
                    } else {
                        &self.memory_access_source
                    }
                ));
            }
        }
        lines
    }
}

#[cfg(test)]
mod tests;
