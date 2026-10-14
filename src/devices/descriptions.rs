//! Discover self-contained chip descriptions without rewriting the user's index.
use super::*;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Metadata {
    pub cores: Vec<u32>,
    pub backend: String,
    #[serde(default)]
    pub cpu: String,
    /// Optional default, relative to the chip file that declares it.
    #[serde(default)]
    pub register_catalogue: PathBuf,
}

impl Metadata {
    pub fn device(&self) -> Device {
        Device {
            cores: self.cores.clone(),
            backend: self.backend.clone(),
            cpu: self.cpu.clone(),
        }
    }
}

pub(crate) fn metadata(value: &toml::Value, path: &Path) -> Result<Option<Metadata>, String> {
    value
        .get("chip")
        .cloned()
        .map(|value| {
            let metadata: Metadata = value
                .try_into()
                .map_err(|e| format!("{} [chip]: {e}", path.display()))?;
            validate_device("description", &metadata.device())
                .map_err(|e| format!("{} [chip]: {e}", path.display()))?;
            Ok(metadata)
        })
        .transpose()
}

fn read(path: &Path) -> Result<Option<Metadata>, String> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("Chip profile {}: {e}", path.display()))?;
    let value = toml::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("Chip profile {}: {e}", path.display()))?;
    metadata(&value, path)
}

impl Catalogue {
    pub(crate) fn discover_directory(
        &mut self,
        directory: &Path,
        replace: bool,
    ) -> Result<(), String> {
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(format!("Chip directory {}: {e}", directory.display())),
        };
        for entry in entries {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.extension().is_none_or(|e| e != "toml") || !path.is_file() {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("Invalid chip filename")?;
            if !valid_id(name) {
                continue;
            }
            // Discovery must not make unrelated projects unusable. Selecting a
            // malformed file still reports the precise error in discover_file.
            if let Ok(Some(metadata)) = read(&path) {
                validate_device(name, &metadata.device())?;
                if replace || !self.devices.contains_key(name) {
                    self.devices.insert(name.into(), metadata.device());
                }
            }
        }
        Ok(())
    }

    pub(crate) fn discover_file(&mut self, path: &Path, chip: &str) -> Result<bool, String> {
        if let Some(metadata) = read(path)? {
            validate_device(chip, &metadata.device())?;
            self.devices.insert(chip.into(), metadata.device());
            return Ok(true);
        }
        Ok(false)
    }

    pub(crate) fn discovered_at(path: &Path) -> Result<Self, String> {
        let mut catalogue = Self::load(path)?;
        if let Ok(root) = crate::bundled_tools::root() {
            catalogue.discover_directory(&root.join("devices"), false)?;
        }
        catalogue.discover_directory(
            &path
                .parent()
                .ok_or("Catalogue needs a parent")?
                .join("chips"),
            true,
        )?;
        Ok(catalogue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_files_appear_without_modifying_the_index_and_bad_metadata_is_reported() {
        let fixture = super::super::tests::Fixture::new();
        let index = fixture.0.join("devices.toml");
        let chips = fixture.0.join("chips");
        fs::create_dir_all(&chips).unwrap();
        fs::write(&index, DEFAULTS).unwrap();
        let before = fs::read(&index).unwrap();
        let chip = chips.join("stm32f103.toml");
        fs::write(
            &chip,
            "[chip]\nbackend='stm32f1'\ncores=[0]\ncpu='cortex-m3'\n",
        )
        .unwrap();
        let catalogue = Catalogue::discovered_at(&index).unwrap();
        assert_eq!(catalogue.devices["stm32f103"].cpu, "cortex-m3");
        assert_eq!(catalogue.devices["stm32f103"].backend, "stm32f1");
        assert_eq!(fs::read(&index).unwrap(), before);
        fs::write(&chip, "[chip]\nbackend='stm32f1'\ncores=[0,0]\n").unwrap();
        let mut invalid = Catalogue::discovered_at(&index).unwrap();
        assert!(!invalid.devices.contains_key("stm32f103"));
        assert!(
            invalid
                .discover_file(&chip, "stm32f103")
                .unwrap_err()
                .contains("unique")
        );
        fs::write(
            &chip,
            "[chip]\nbackend='stm32f1'\ncores=[0]\nmisspelled=true\n",
        )
        .unwrap();
        assert!(
            invalid
                .discover_file(&chip, "stm32f103")
                .unwrap_err()
                .contains("unknown field")
        );
        fs::remove_file(chip).unwrap();
        assert!(
            !Catalogue::discovered_at(&index)
                .unwrap()
                .devices
                .contains_key("stm32f103")
        );
    }
}
