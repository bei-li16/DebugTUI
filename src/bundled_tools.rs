//! Relocatable, read-only tools shipped beside the application.
use crate::config::{Tools, absolute, portable_path};
use std::{
    env,
    path::{Component, Path, PathBuf},
};

pub const PROFILE: &str = "builtin:arm-openocd";
pub const PROJECT_TEMPLATE: &str = include_str!("../tools/debug.toml");
pub const PROBES: &[&str] = &["cmsis-dap", "jlink", "stlink"];

pub fn root() -> Result<PathBuf, String> {
    root_for_executable(&env::current_exe().map_err(|e| e.to_string())?)
}

fn root_for_executable(exe: &Path) -> Result<PathBuf, String> {
    let directory = exe.parent().ok_or("Executable needs a parent directory")?;
    let mut candidates = vec![directory.join("tools")];
    if directory.file_name().is_some_and(|n| n == "bin") {
        candidates.push(directory.parent().unwrap().join("tools"));
    }
    // Cargo builds use the checkout payload; installed builds never search cwd.
    let build = if directory.file_name().is_some_and(|n| n == "deps") {
        directory.parent().unwrap()
    } else {
        directory
    };
    if build
        .file_name()
        .is_some_and(|n| n == "debug" || n == "release")
        && let Some(target) = build.parent()
        && target.file_name().is_some_and(|n| n == "target")
    {
        candidates.push(target.parent().unwrap().join("tools"));
    }
    candidates.into_iter().find(|p| p.join("debug-env.toml").is_file())
        .ok_or_else(|| "Bundled ARM tools are missing. Reinstall the npm package or extract the complete portable ZIP beside debugtui.exe.".into())
}

pub fn resolve_profile(value: &Path, base: &Path) -> Result<PathBuf, String> {
    match value.to_str() {
        Some(PROFILE) => Ok(root()?.join("debug-env.toml")),
        Some(s) if s.starts_with("builtin:") => Err(format!("Unknown built-in tools profile: {s}")),
        _ => Ok(absolute(base, value)),
    }
}

pub fn resolve_svd(value: &Path, base: &Path) -> Result<PathBuf, String> {
    if let Some(value) = value.to_str().filter(|v| v.starts_with("builtin:")) {
        if !value.starts_with("builtin:svd/") {
            return Err("Built-in SVD must use builtin:svd/<file>".into());
        }
        resource(&root()?, value)
    } else {
        Ok(absolute(base, value))
    }
}

/// Keep selections from the installed payload portable when chosen in a browser.
pub fn reference(path: &Path) -> Option<String> {
    let root = std::fs::canonicalize(root().ok()?).ok()?;
    let path = std::fs::canonicalize(path).ok()?;
    let relative = path.strip_prefix(root).ok()?;
    if relative == Path::new("debug-env.toml") {
        Some(PROFILE.into())
    } else if relative.starts_with("svd") && path.is_file() {
        Some(format!("builtin:{}", portable_path(relative)))
    } else {
        None
    }
}

pub(crate) fn resource(root: &Path, value: &str) -> Result<PathBuf, String> {
    let relative = Path::new(
        value
            .strip_prefix("builtin:")
            .ok_or("Expected builtin: resource")?,
    );
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("Built-in resource must be a path within the tools directory".into());
    }
    Ok(root.join(relative))
}

pub fn validate_probe(probe: &str) -> Result<(), String> {
    if probe.is_empty() || PROBES.contains(&probe) {
        Ok(())
    } else {
        Err(format!(
            "Unknown probe '{probe}'; choose cmsis-dap, jlink or stlink"
        ))
    }
}

pub(crate) fn apply_selection(
    environment: &mut toml::Value,
    tools: &Tools,
    project: &toml::Value,
    base: &Path,
    profile: Option<&Path>,
) -> Result<(), String> {
    validate_probe(&tools.probe)?;
    let builtin = tools.profile == Path::new(PROFILE);
    let custom = if !tools.chip_profile.as_os_str().is_empty() {
        Some(absolute(base, &tools.chip_profile))
    } else if builtin {
        let chip = project
            .get("debug")
            .and_then(|d| d.get("chip"))
            .and_then(toml::Value::as_str)
            .unwrap_or_default();
        if crate::devices::valid_id(chip) {
            let candidate = crate::devices::user_dir()?
                .join("profiles/chips")
                .join(format!("{chip}.toml"));
            candidate.is_file().then_some(candidate)
        } else {
            None
        }
    } else {
        None
    };
    if tools.probe.is_empty() && custom.is_none() {
        return Ok(());
    }
    if environment.get("chip_profiles").is_none() {
        return Err("tools.probe / tools.chip_profile require a profile with [chip_profiles]; leave them empty for a legacy profile".into());
    }
    let directory = profile
        .and_then(Path::parent)
        .ok_or("Select a tools profile first")?;
    if !tools.probe.is_empty() {
        environment.as_table_mut().unwrap().insert(
            "probe".into(),
            toml::Value::Table(toml::Table::from_iter([(
                "config".into(),
                portable_path(&directory.join(format!("openocd/probes/{}.cfg", tools.probe)))
                    .into(),
            )])),
        );
    }
    if let Some(custom) = custom {
        environment
            .get_mut("chip_profiles")
            .and_then(toml::Value::as_table_mut)
            .ok_or("chip_profiles must be a table")?
            .insert("file".into(), portable_path(&custom).into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Project;
    #[test]
    fn builtin_selection_resolves_probe_custom_chip_and_declaring_paths() {
        let directory = env::temp_dir().join(format!("debugtui-builtin-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let catalogue = crate::devices::Catalogue::parse(crate::devices::DEFAULTS).unwrap();
        let raw: toml::Value = toml::from_str("version=3\n[tools]\nprofile='builtin:arm-openocd'\nprobe='stlink'\n[debug]\nchip='stm32f429'\ncores=[0]\n").unwrap();
        let load = |raw| {
            Project::from_document_with_catalogue(
                raw,
                Some(directory.join("debug.toml")),
                None,
                Some(&catalogue),
            )
        };
        let p = load(raw.clone()).unwrap();
        assert!(
            p.service
                .unwrap()
                .args
                .iter()
                .any(|a| a.ends_with("openocd/probes/stlink.cfg"))
        );
        assert!(p.tools.profile.ends_with("tools/debug-env.toml"));
        assert!(p.program.svd.ends_with("svd/STM32F429.svd"));
        let mut bad = raw.clone();
        bad["tools"]["probe"] = "unknown".into();
        assert!(load(bad).unwrap_err().contains("Unknown probe"));
        let mut custom = raw;
        custom["tools"]
            .as_table_mut()
            .unwrap()
            .insert("chip_profile".into(), "board.toml".into());
        std::fs::write(
            directory.join("board.toml"),
            "extends='builtin:devices/stm32f429.toml'\n[program]\nsvd='./customer.svd'\n",
        )
        .unwrap();
        let p = load(custom.clone()).unwrap();
        assert_eq!(
            portable_path(&p.program.svd),
            portable_path(&directory.join("customer.svd"))
        );
        std::fs::write(
            directory.join("board.toml"),
            "extends='builtin:../outside.toml'\n",
        )
        .unwrap();
        assert!(
            load(custom)
                .unwrap_err()
                .contains("within the tools directory")
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn bundled_resources_reject_unknown_aliases_and_parent_paths() {
        assert!(resolve_profile(Path::new("builtin:unknown"), Path::new(".")).is_err());
        assert!(resource(Path::new("payload"), "builtin:../secret").is_err());
        assert!(resolve_svd(Path::new("builtin:svd/../../secret"), Path::new(".")).is_err());
        assert!(resolve_svd(Path::new("builtin:devices/chip.toml"), Path::new(".")).is_err());
        let svd = resolve_svd(
            Path::new("builtin:svd/STM32F429.svd"),
            Path::new("unrelated"),
        )
        .unwrap();
        assert!(svd.is_file());
        assert_eq!(
            reference(&svd).as_deref(),
            Some("builtin:svd/STM32F429.svd")
        );
        assert_eq!(
            resource(Path::new("payload"), "builtin:devices/family.toml").unwrap(),
            Path::new("payload/devices/family.toml")
        );
    }
}
