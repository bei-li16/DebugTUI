//! Project discovery and opt-in launch examples. Never save files or start tools.
use super::*;

pub(super) enum Choice {
    Project(PathBuf),
    Example {
        label: String,
        description: String,
        raw: toml::Value,
    },
    Cpu {
        id: Option<String>,
        label: String,
        description: String,
    },
}

impl Choice {
    pub fn label(&self) -> String {
        match self {
            Self::Project(path) => path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            Self::Example { label, .. } => label.clone(),
            Self::Cpu { label, .. } => label.clone(),
        }
    }

    pub fn description(&self) -> String {
        match self {
            Self::Project(path) => format!(
                "Load {} and refresh every Setup field.\nEnter / click: select. Esc: keep the current draft. F2: browse other directories.\nSelecting does not save the file or connect to hardware.",
                path.file_name().unwrap_or_default().to_string_lossy()
            ),
            Self::Example { description, .. } => format!(
                "{description}\nEnter / click applies this example to the draft; review project paths and Tools / profile.\nReplaces project draft settings; retains current tools unless another profile is selected. Save config or Start writes the file."
            ),
            Self::Cpu { description, .. } => format!(
                "{description}\nApplies to the project draft. Save config or Start persists it; no hardware access."
            ),
        }
    }
}

pub(super) struct Picker {
    pub title: &'static str,
    pub choices: Vec<Choice>,
    pub selected: usize,
}

impl Picker {
    pub fn cpus() -> Result<Self, String> {
        Self::cpus_in(
            &crate::devices::catalogue_path()?
                .parent()
                .unwrap()
                .join("registers"),
        )
    }
    fn cpus_in(directory: &Path) -> Result<Self, String> {
        let mut choices = vec![
            Choice::Cpu { id: None, label: "Automatic / inherit profile and chip association".into(), description: "Remove project CPU and catalogue overrides. A user chip association takes precedence over the built-in association.".into() },
            Choice::Cpu { id: Some(String::new()), label: "GDB target description only".into(), description: "Use the original dynamic GDB register list without an architecture catalogue.".into() },
        ];
        for cpu in [
            "cortex-m3",
            "cortex-m4",
            "cortex-m7",
            "cortex-r52",
            "cortex-r52+",
        ] {
            choices.push(Choice::Cpu { id: Some(cpu.into()), label: cpu.into(), description: format!("Use the {cpu} register catalogue. User presets with this name override the embedded default; the catalogue does not prove hardware or reader support.") });
        }
        if directory.is_dir() {
            let mut presets: Vec<_> = fs::read_dir(directory)
                .map_err(|e| e.to_string())?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.is_file()
                        && path.extension().is_some_and(|extension| {
                            if cfg!(windows) {
                                extension.eq_ignore_ascii_case("toml")
                            } else {
                                extension == "toml"
                            }
                        })
                })
                .filter_map(|path| {
                    path.file_stem()
                        .and_then(|name| name.to_str())
                        .map(str::to_owned)
                })
                .filter(|cpu| {
                    crate::devices::valid_id(&cpu.replace('+', "plus"))
                        && ![
                            "cortex-m3",
                            "cortex-m4",
                            "cortex-m7",
                            "cortex-r52",
                            "cortex-r52+",
                        ]
                        .contains(&cpu.as_str())
                })
                .collect();
            presets.sort();
            for cpu in presets {
                choices.push(Choice::Cpu { id: Some(cpu.clone()), label: format!("{cpu} / user"), description: format!("Load {}. A broken preset reports its error and does not silently select another catalogue.", directory.join(format!("{cpu}.toml")).display()) });
            }
        }
        Ok(Self {
            title: "CPU / register catalogue",
            choices,
            selected: 0,
        })
    }

    pub fn projects(directory: &Path) -> Result<Self, String> {
        let mut paths = fs::read_dir(directory)
            .map_err(|e| format!("Project directory: {e}"))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| is_project_file(path))
            .collect::<Vec<_>>();
        paths.sort_by_key(|path| {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            (name != "debug.toml", name)
        });
        Ok(Self {
            title: "Projects / select a TOML",
            choices: paths.into_iter().map(Choice::Project).collect(),
            selected: 0,
        })
    }

    pub fn examples(directory: &Path) -> Self {
        let mut choices = Vec::new();
        for profile in [
            "debug-env.toml",
            ".vscode/debug-env.toml",
            "tools/debug-env.toml",
        ] {
            if directory.join(profile).is_file() {
                let mut raw = base_example();
                raw.as_table_mut().unwrap().insert(
                    "tools".into(),
                    toml::Value::Table(toml::Table::from_iter([(
                        "profile".into(),
                        profile.into(),
                    )])),
                );
                choices.push(Choice::Example {
                    label: format!("Use project tools / {profile}"),
                    description: format!("Inherit GDB, connection and server from {profile}. ELF: ./build/firmware.elf."),
                    raw,
                });
            }
        }
        for (label, description, settings) in [
            (
                "Single-core project (default example)",
                "ELF ./build/firmware.elf. Select an ARM / RISC-V / other tools profile for your board.",
                "",
            ),
            (
                "Local program project",
                "Executable ./build/app (app.exe on Windows). Select a tools profile with target.mode='local'.",
                "[program]\nelf='./build/app'\nsource_root='.'\n",
            ),
            (
                "Two-core project",
                "core0 :3333 / core1 :3334; shared ELF. Review [[cores]] in TOML and select a compatible tools profile.",
                "[multicore]\nscope='all'\nhalt_peers=true\n[[cores]]\nname='core0'\nendpoint='127.0.0.1:3333'\n[[cores]]\nname='core1'\nendpoint='127.0.0.1:3334'\n",
            ),
        ] {
            let mut raw = base_example();
            let extra: toml::Value = toml::from_str(settings).expect("built-in launch example");
            raw.as_table_mut()
                .unwrap()
                .extend(extra.as_table().unwrap().clone());
            if label == "Local program project" {
                raw["program"]["elf"] = if cfg!(windows) {
                    "./build/app.exe"
                } else {
                    "./build/app"
                }
                .into();
            }
            choices.push(Choice::Example {
                label: label.into(),
                description: description.into(),
                raw,
            });
        }
        Self {
            title: "Examples / choose a starting configuration",
            choices,
            selected: 0,
        }
    }
}

fn base_example() -> toml::Value {
    toml::from_str(
        "version=2\n[program]\nelf='./build/firmware.elf'\nsource_root='.'\n[session]\non_exit='detach'\nlog_dir='./debug_log'\n",
    ).expect("built-in launch defaults")
}

fn is_project_file(path: &Path) -> bool {
    if !path.is_file()
        || !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("toml"))
    {
        return false;
    }
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    if name == "cargo.toml" || name.starts_with("debug-env") {
        return false;
    }
    // Keep broken debug*.toml visible so selection can report its error. Other
    // TOML files need recognizable project fields (Cargo.toml is not a project).
    if name == "debug.toml" || name.starts_with("debug-") {
        return true;
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str::<toml::Value>(text.trim_start_matches('\u{feff}')).ok())
        .is_some_and(|raw| {
            // Register/device catalogues have their own version field but are
            // not projects. Keep them out of initial project discovery.
            if raw.get("registers").is_some_and(toml::Value::is_array)
                || raw.get("devices").is_some()
            {
                return false;
            }
            [
                "program",
                "tools",
                "cores",
                "multicore",
                "source_map",
                "watch",
                "breakpoints",
                "tasks",
                "gdb",
                "target",
            ]
            .iter()
            .any(|key| raw.get(key).is_some())
                || raw.get("version").is_some_and(toml::Value::is_integer)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::tests::Fixture;

    #[test]
    fn setup_cpu_picker_discovers_sorted_user_presets_without_loading_or_rewriting_them() {
        let fixture = Fixture::new();
        for name in [
            "z-user.toml",
            "a-user.toml",
            "cortex-r52+.toml",
            "bad name.toml",
            "customer.txt",
            "Upper.TOML",
        ] {
            fs::write(fixture.0.join(name), "broken=[").unwrap();
        }
        fs::create_dir(fixture.0.join("directory.toml")).unwrap();
        let picker = Picker::cpus_in(&fixture.0).unwrap();
        let labels: Vec<_> = picker.choices.iter().map(Choice::label).collect();
        assert_eq!(
            &labels[..7],
            [
                "Automatic / inherit profile and chip association",
                "GDB target description only",
                "cortex-m3",
                "cortex-m4",
                "cortex-m7",
                "cortex-r52",
                "cortex-r52+"
            ]
        );
        assert!(labels.contains(&"a-user / user".into()));
        assert!(labels.contains(&"z-user / user".into()));
        assert_eq!(
            labels
                .iter()
                .filter(|label| *label == "cortex-r52+")
                .count(),
            1
        );
        assert!(!labels.iter().any(|label| label.contains("bad name")
            || label.contains("customer.txt")
            || label.contains("directory")));
        assert_eq!(labels.contains(&"Upper / user".into()), cfg!(windows));
        let a = labels
            .iter()
            .position(|label| label == "a-user / user")
            .unwrap();
        let z = labels
            .iter()
            .position(|label| label == "z-user / user")
            .unwrap();
        assert!(a < z);
        assert!(
            picker.choices[a]
                .description()
                .contains(&fixture.0.join("a-user.toml").display().to_string())
        );
        assert_eq!(
            fs::read_to_string(fixture.0.join("cortex-r52+.toml")).unwrap(),
            "broken=["
        );
        assert_eq!(
            Picker::cpus_in(&fixture.0.join("missing"))
                .unwrap()
                .choices
                .len(),
            7
        );
        let projects = Fixture::new();
        fs::write(projects.0.join("debug-upper.TOML"), "version=2").unwrap();
        assert_eq!(Picker::projects(&projects.0).unwrap().choices.len(), 1);
    }
}
