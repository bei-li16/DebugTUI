//! Project discovery and opt-in launch examples. Never save files or start tools.
use super::*;

pub(super) enum Choice {
    Project(PathBuf),
    Example {
        label: String,
        description: String,
        raw: toml::Value,
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
        }
    }

    pub fn description(&self) -> String {
        match self {
            Self::Project(path) => format!(
                "Load {} and refresh every Setup field.\nEnter / click: select. Esc: keep the current draft. F2: browse other directories.\nSelecting does not save the file or connect to hardware.",
                path.file_name().unwrap_or_default().to_string_lossy()
            ),
            Self::Example { description, .. } => format!(
                "{description}\nEnter / click applies this example to the draft; review project paths and Tools / profile.\nReplaces project draft settings; retains current tools unless another profile is selected. The file is written on Save / Start with Save to project = Yes."
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
