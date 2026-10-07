use crate::launch::Document;
use std::{
    env,
    path::{Path, PathBuf},
};

#[derive(Default)]
pub struct Options {
    pub help: bool,
    pub version: bool,
    pub init_profiles: bool,
    pub init_project: bool,
    pub headless: bool,
    pub script: Option<String>,
    pub demo: bool,
    pub snapshot: Option<String>,
    pub setup: bool,
    pub explicit_launch: bool,
    project: Option<PathBuf>,
    values: Vec<(String, String)>,
}
impl Options {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" => options.help = true,
                "--version" | "-V" => options.version = true,
                "--init-profiles" => options.init_profiles = true,
                "--init-project" => options.init_project = true,
                "--headless" | "--stdio" => options.headless = true,
                "--demo" => options.demo = true,
                "--setup" => options.setup = true,
                "--local" => {
                    options.values.push((arg, String::new()));
                    options.explicit_launch = true;
                }
                "--project" | "--tools-dir" | "--environment" | "--elf" | "--gdb" | "--gdb-arg"
                | "--connect" | "--target-mode" | "--log-dir" | "--script" | "--snapshot"
                | "--svd" | "--chip" | "--cores" | "--probe" | "--chip-profile" => {
                    let value = args
                        .next()
                        .ok_or_else(|| format!("{arg} requires a value"))?;
                    match arg.as_str() {
                        "--project" => {
                            options.project = Some(value.into());
                            options.explicit_launch = true;
                        }
                        "--script" => {
                            options.script = Some(value);
                            options.headless = true;
                        }
                        "--snapshot" => options.snapshot = Some(value),
                        _ => {
                            options.values.push((arg, value));
                            options.explicit_launch = true;
                        }
                    }
                }
                _ => return Err(format!("Unknown option: {arg}\nUse --help")),
            }
        }
        Ok(options)
    }
    pub fn project_path(&self) -> PathBuf {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let path = self
            .project
            .as_ref()
            .map(|p| {
                if p.is_absolute() {
                    p.clone()
                } else {
                    cwd.join(p)
                }
            })
            .unwrap_or(cwd);
        if path.is_dir() {
            path.join("debug.toml")
        } else {
            path
        }
    }
    pub fn document(&self) -> Result<Document, String> {
        let mut doc = if self.init_project
            || (!self.headless && !self.demo && (!self.explicit_launch || self.setup))
        {
            Document::startup(
                &self.project_path(),
                self.project.is_none() || self.project.as_ref().is_some_and(|p| p.is_dir()),
            )?
        } else {
            Document::open(&self.project_path())?
        };
        if doc.discovered
            && ((self.project.is_none() && self.explicit_launch)
                || self.values.iter().any(|(key, _)| key == "--gdb"))
        {
            doc.environment("");
        }
        let cwd = env::current_dir().map_err(|e| e.to_string())?;
        let absolute = |value: &str| {
            let p = Path::new(value);
            if p.is_absolute() {
                p.to_owned()
            } else {
                cwd.join(p)
            }
        };
        for (key, value) in &self.values {
            if key == "--environment" || key == "--tools-dir" {
                if key == "--environment" && value.starts_with("builtin:") {
                    doc.environment(value);
                    continue;
                }
                let path = absolute(value);
                let path = if key == "--tools-dir" {
                    path.join("debug-env.toml")
                } else {
                    path
                };
                doc.environment(&crate::config::portable_path(&path));
            }
        }
        let mut extra_args = vec![];
        if doc.raw_tools_missing()
            && self
                .values
                .iter()
                .any(|(key, _)| key == "--chip" || key == "--probe")
            && !self.values.iter().any(|(key, _)| key == "--gdb")
        {
            doc.environment(crate::bundled_tools::PROFILE);
        }
        for (key, value) in &self.values {
            match key.as_str() {
                "--probe" => {
                    crate::bundled_tools::validate_probe(value)?;
                    doc.set("tools", "probe", value.clone().into());
                }
                "--chip-profile" => doc.set_path("tools", "chip_profile", &absolute(value)),
                "--chip" => {
                    doc.enable_device_selection();
                    doc.set("debug", "chip", value.clone().into());
                }
                "--cores" => {
                    doc.enable_device_selection();
                    doc.set(
                        "debug",
                        "cores",
                        toml::Value::try_from(crate::devices::parse_ids(value)?)
                            .map_err(|e| e.to_string())?,
                    );
                }
                "--elf" => doc.set_path("program", "elf", &absolute(value)),
                "--svd" if value.starts_with("builtin:") => {
                    crate::bundled_tools::resolve_svd(Path::new(value), doc.base())?;
                    doc.set("program", "svd", value.clone().into());
                }
                "--svd" => doc.set_path("program", "svd", &absolute(value)),
                "--gdb" if value.contains(['/', '\\']) => {
                    doc.set_path("gdb", "executable", &absolute(value))
                }
                "--gdb" => doc.set("gdb", "executable", value.clone().into()),
                "--gdb-arg" => extra_args.push(value.clone()),
                "--local" => {
                    doc.set("target", "mode", "local".into());
                    doc.set("service", "enabled", false.into());
                }
                "--connect" => {
                    doc.set("target", "endpoint", value.clone().into());
                    if doc.project()?.target.mode == "local" {
                        doc.set("target", "mode", "remote".into());
                    }
                    doc.set("service", "enabled", false.into());
                }
                "--target-mode" => doc.set("target", "mode", value.clone().into()),
                "--log-dir" => doc.set_path("session", "log_dir", &absolute(value)),
                _ => {}
            }
        }
        if !extra_args.is_empty() {
            let mut all = doc.project()?.gdb.args;
            all.extend(extra_args);
            doc.set(
                "gdb",
                "args",
                toml::Value::Array(all.into_iter().map(toml::Value::String).collect()),
            );
        }
        Ok(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gdb_arguments_are_not_interpreted_as_tui_options() {
        let p =
            Options::parse(["--gdb-arg", "--help", "--gdb-arg", "--project"].map(str::to_owned))
                .unwrap();
        assert!(!p.help);
        assert!(p.project.is_none());
        assert_eq!(p.values.len(), 2);
        assert!(!Options::parse(Vec::new()).unwrap().explicit_launch);
        assert!(Options::parse(["--gdb".into()]).is_err());
    }
}
