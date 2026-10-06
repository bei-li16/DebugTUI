//! Bounded catalogue graph resolution; no implicit last-parent-wins overrides.
use super::*;
use metadata::DefinitionOrigin;

pub const MAX_INHERITANCE_DEPTH: usize = 3;
const MAX_GRAPH_NODES: usize = 16;
const MAX_GRAPH_BYTES: usize = 16 * 1024 * 1024;

#[derive(Default)]
struct Loader {
    stack: Vec<String>,
    nodes: usize,
    bytes: usize,
}

pub(super) fn file(path: &Path) -> Result<Catalogue, String> {
    Loader::default().file(path)
}
pub(super) fn inline(text: &str) -> Result<Catalogue, String> {
    Loader::default().resolve(text, "inline".into(), None)
}
pub(super) fn builtin(cpu: &str, text: &str) -> Result<Catalogue, String> {
    Loader::default().resolve(text, format!("builtin:{cpu}"), None)
}

fn embedded(name: &str) -> Option<&'static str> {
    match name {
        "cortex-r52" => Some(include_str!("../../profiles/registers/cortex-r52.toml")),
        "cortex-r52+" => Some(include_str!("../../profiles/registers/cortex-r52+.toml")),
        "cortex-m4" => Some(include_str!("../../profiles/registers/cortex-m4.toml")),
        _ => None,
    }
}
impl Loader {
    fn file(&mut self, path: &Path) -> Result<Catalogue, String> {
        let path = fs::canonicalize(path)
            .map_err(|error| format!("Register catalogue {}: {error}", path.display()))?;
        let label = format!("file:{}", path.display());
        let metadata = fs::metadata(&path).map_err(|error| format!("{label}: {error}"))?;
        if !metadata.is_file() {
            return Err(format!("{label}: not a catalogue file"));
        }
        if metadata.len() > MAX_CATALOGUE_BYTES {
            return Err(format!("{label}: Register catalogue exceeds 4 MiB"));
        }
        let mut bytes = Vec::new();
        fs::File::open(&path)
            .map_err(|error| format!("{label}: {error}"))?
            .take(MAX_CATALOGUE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("{label}: {error}"))?;
        if bytes.len() as u64 > MAX_CATALOGUE_BYTES {
            return Err(format!("{label}: Register catalogue exceeds 4 MiB"));
        }
        let text =
            String::from_utf8(bytes).map_err(|error| format!("{label}: not UTF-8: {error}"))?;
        self.resolve(&text, label, path.parent())
    }
    fn parent(&mut self, name: &str, base: Option<&Path>) -> Result<Catalogue, String> {
        // Names search the declaring file's directory first, then embedded definitions.
        // Explicit paths never silently fall back to a built-in.
        if name.trim() != name || name.is_empty() || name.chars().any(char::is_control) {
            return Err("Invalid catalogue parent".into());
        }
        let named = !name.to_ascii_lowercase().ends_with(".toml")
            && crate::devices::valid_id(&name.replace('+', "plus"));
        if let Some(base) = base {
            let path = base.join(if named {
                format!("{name}.toml")
            } else {
                name.into()
            });
            match fs::symlink_metadata(&path) {
                Ok(_) => return self.file(&path),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound && named => {}
                Err(error) => return Err(format!("Parent catalogue {}: {error}", path.display())),
            }
        } else if !named {
            return Err(format!(
                "Relative parent '{name}' requires a declaring catalogue file"
            ));
        }
        let text = embedded(name).ok_or_else(|| format!("Missing parent catalogue '{name}'"))?;
        self.resolve(text, format!("builtin:{name}"), None)
    }
    fn resolve(
        &mut self,
        text: &str,
        label: String,
        base: Option<&Path>,
    ) -> Result<Catalogue, String> {
        if self.stack.contains(&label) {
            return Err(format!(
                "Catalogue inheritance cycle: {} -> {label}",
                self.stack.join(" -> ")
            ));
        }
        if self.stack.len() >= MAX_INHERITANCE_DEPTH {
            return Err("Catalogue inheritance exceeds 3 layers".into());
        }
        if text.len() as u64 > MAX_CATALOGUE_BYTES {
            return Err("Register catalogue exceeds 4 MiB".into());
        }
        self.nodes += 1;
        self.bytes += text.len();
        if self.nodes > MAX_GRAPH_NODES || self.bytes > MAX_GRAPH_BYTES {
            return Err("Catalogue inheritance graph exceeds resource limits".into());
        }
        let mut child: Catalogue = toml::from_str(text.trim_start_matches('\u{feff}'))
            .map_err(|error| format!("{label}: {error}"))?;
        if let Some(meta) = &child.meta {
            meta.validate()?;
        }
        self.stack.push(label.clone());
        let parents = std::mem::take(&mut child.extends);
        if parents.len() > 4 || parents.iter().collect::<BTreeSet<_>>().len() != parents.len() {
            return Err(format!(
                "{label}: at most four distinct parents are allowed"
            ));
        }
        let mut groups: Vec<Group> = Vec::new();
        let mut registers: Vec<Register> = Vec::new();
        for parent in parents {
            let inherited = self.parent(&parent, base)?;
            for group in inherited.groups {
                Self::merge_group(&mut groups, group, &label)?;
            }
            for mut register in inherited.registers {
                if registers.iter().any(|existing| existing.id == register.id) {
                    return Err(format!(
                        "{label}: ambiguous parent register {}",
                        register.id
                    ));
                }
                if let Some(origin) = &mut register.definition_origin {
                    origin.inheritance.insert(0, label.clone());
                }
                registers.push(register);
            }
        }
        let mut local_ids = BTreeSet::new();
        for mut register in std::mem::take(&mut child.registers) {
            if !local_ids.insert(register.id.clone()) {
                return Err(format!("{label}: duplicate local register {}", register.id));
            }
            if let Some(source) = &mut register.source {
                source.inherit_document(child.meta.as_ref());
            }
            register.validate_metadata(child.version)?;
            let existing = registers
                .iter()
                .position(|existing| existing.id == register.id);
            let overrides = match (existing, register.override_definition) {
                (Some(index), true) => registers[index]
                    .definition_origin
                    .as_ref()
                    .map(|origin| origin.declared_in.clone()),
                (Some(_), false) => {
                    return Err(format!(
                        "{label}: inherited register {} requires override = true",
                        register.id
                    ));
                }
                (None, true) => {
                    return Err(format!(
                        "{label}: override for {} has no inherited definition",
                        register.id
                    ));
                }
                (None, false) => None,
            };
            register.override_definition = false;
            register.definition_origin = Some(DefinitionOrigin {
                declared_in: label.clone(),
                inheritance: vec![label.clone()],
                overrides,
            });
            if let Some(index) = existing {
                registers[index] = register;
            } else {
                registers.push(register);
            }
        }
        let mut local_groups = BTreeSet::new();
        for group in std::mem::take(&mut child.groups) {
            if !local_groups.insert(group.id.clone()) {
                return Err(format!("{label}: duplicate local group {}", group.id));
            }
            Self::merge_group(&mut groups, group, &label)?;
        }
        child.groups = groups;
        child.registers = registers;
        child
            .validate()
            .map_err(|error| format!("{label}: {error}"))?;
        self.stack.pop();
        Ok(child)
    }
    fn merge_group(groups: &mut Vec<Group>, group: Group, label: &str) -> Result<(), String> {
        if let Some(existing) = groups.iter().find(|existing| existing.id == group.id) {
            if existing != &group {
                return Err(format!("{label}: conflicting group {}", group.id));
            }
        } else {
            groups.push(group);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
