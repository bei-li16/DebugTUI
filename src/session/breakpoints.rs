//! GDB owns breakpoint identities and target capabilities. Persist definitions, not GDB numbers.
use super::*;
use crate::config::{BreakpointKind as Kind, BreakpointOptions as Options, BreakpointSpec};

impl Breakpoint {
    pub(super) fn from_mi(b: &Value) -> Self {
        let location = b.string("original-location");
        let file = b.string("fullname");
        Self {
            id: b.string("number"),
            location: if location.is_empty() {
                b.string("what")
            } else {
                location
            },
            kind: b.string("type"),
            enabled: b.string("enabled") == "y",
            temporary: b.string("disp") == "del",
            file: if file.is_empty() {
                b.string("file")
            } else {
                file
            },
            line: b.string("line").parse().unwrap_or(0),
            condition: b.string("cond"),
            ignore_count: b.string("ignore").parse().unwrap_or(0),
            hit_count: b.string("times").parse().unwrap_or(0),
            address: b.string("addr"),
            pending: b.field("pending").is_some() || b.string("addr") == "<PENDING>",
            ..Default::default()
        }
    }
    pub fn options(&self) -> Options {
        let kind = if self.kind.contains("watchpoint") {
            if self.kind.contains("acc") {
                Kind::Access
            } else if self.kind.contains("read") {
                Kind::Read
            } else {
                Kind::Write
            }
        } else if self.kind.contains("hw") {
            Kind::Hardware
        } else {
            Kind::Code
        };
        Options {
            location: self.location.clone(),
            kind,
            enabled: self.enabled,
            condition: self.condition.clone(),
            ignore_count: self.ignore_count,
            temporary: self.temporary,
        }
    }
}

impl Engine {
    fn insert_breakpoint(&mut self, o: &Options, restoring: bool) -> Result<Record, String> {
        if o.location.trim().is_empty() {
            return Err("A code location or data expression is required".into());
        }
        if o.temporary && o.kind.is_data() {
            return Err("Temporary data breakpoints are not supported".into());
        }
        if !o.kind.is_data() {
            return self.mi(&format!(
                "-break-insert {} {} {} {} -i {} {} {}",
                if o.temporary { "-t" } else { "" },
                if o.kind == Kind::Hardware { "-h" } else { "" },
                if o.enabled { "" } else { "-d" },
                if restoring { "-f" } else { "" },
                o.ignore_count,
                if o.condition.trim().is_empty() {
                    String::new()
                } else {
                    format!("-c {}", mi::quote(&o.condition))
                },
                mi::quote(&o.location)
            ));
        }
        let r = self.mi(&format!(
            "-break-watch {} {}",
            match o.kind {
                Kind::Read => "-r",
                Kind::Access => "-a",
                _ => "",
            },
            mi::quote(&o.location)
        ))?;
        let id = ["wpt", "hw-rwpt", "hw-awpt", "bkpt"]
            .iter()
            .find_map(|key| r.data.field(key))
            .map(|v| v.string("number"))
            .filter(|s| valid_id(s))
            .ok_or("GDB did not return the data breakpoint number")?;
        // MI has no disabled/conditional watch insertion. Roll back a partial creation.
        let result = (|| {
            if !o.condition.is_empty() {
                self.mi(&format!(
                    "-break-condition {id} {}",
                    mi::quote(&o.condition)
                ))?;
            }
            if o.ignore_count > 0 {
                self.mi(&format!("-break-after {id} {}", o.ignore_count))?;
            }
            if !o.enabled {
                self.mi(&format!("-break-disable {id}"))?;
            }
            Ok::<_, String>(())
        })();
        if let Err(error) = result {
            if let Err(cleanup) = self.mi(&format!("-break-delete {id}")) {
                return Err(format!(
                    "{error}; cleanup of breakpoint {id} failed: {cleanup}"
                ));
            }
            return Err(error);
        }
        Ok(r)
    }
    pub(super) fn restore_breakpoints(&mut self) {
        self.unresolved_breakpoints.clear();
        for (index, spec) in self.saved_breakpoints.clone().iter().enumerate() {
            let o = spec.options();
            if let Err(error) = self.insert_breakpoint(&o, true) {
                self.log(
                    "error",
                    format!("Restore breakpoint {}: {error}", o.location),
                );
                self.unresolved_breakpoints.push(Breakpoint {
                    id: format!("restore-{}", index + 1),
                    location: o.location,
                    kind: match o.kind {
                        Kind::Read => "read watchpoint",
                        Kind::Write => "hw watchpoint",
                        Kind::Access => "acc watchpoint",
                        Kind::Hardware => "hw breakpoint",
                        _ => "breakpoint",
                    }
                    .into(),
                    enabled: o.enabled,
                    temporary: o.temporary,
                    condition: o.condition,
                    ignore_count: o.ignore_count,
                    pending: true,
                    restore_error: error,
                    ..Default::default()
                });
            }
        }
    }
    pub(super) fn remember_breakpoints(&mut self) {
        self.saved_breakpoints = self
            .snapshot
            .breakpoints
            .iter()
            .filter(|b| !b.temporary && !b.location.is_empty() && !b.id.contains('.'))
            .map(|b| BreakpointSpec::Detailed(b.options()))
            .collect();
    }
    fn persist_breakpoint_edit(&mut self) -> Result<(), String> {
        self.refresh_breakpoints()?;
        self.remember_breakpoints();
        self.publish();
        self.project
            .save_preferences(self.watch_names.clone(), self.saved_breakpoints.clone())
            .map_err(|e| format!("Breakpoint changed in GDB, but saving the project failed: {e}"))
    }
    fn update_breakpoint(&mut self, b: &Breakpoint, o: &Options) -> Result<(), String> {
        if let Some(index) = self
            .unresolved_breakpoints
            .iter()
            .position(|v| v.id == b.id)
        {
            if o.enabled {
                self.insert_breakpoint(o, true)?;
                self.unresolved_breakpoints.remove(index);
            } else {
                let stored = &mut self.unresolved_breakpoints[index];
                stored.enabled = false;
                stored.condition = o.condition.clone();
                stored.ignore_count = o.ignore_count;
            }
            return Ok(());
        }
        if !valid_id(&b.id) {
            return Err("Invalid breakpoint number".into());
        }
        let id = &b.id;
        let condition = |text: &str| {
            format!(
                "-break-condition {id} {}",
                if text.trim().is_empty() {
                    String::new()
                } else {
                    mi::quote(text)
                }
            )
        };
        let apply = (|| {
            if b.condition != o.condition {
                self.mi(&condition(&o.condition))?;
            }
            if b.ignore_count != o.ignore_count {
                self.mi(&format!("-break-after {id} {}", o.ignore_count))?;
            }
            if b.enabled != o.enabled {
                self.mi(&format!(
                    "-break-{} {id}",
                    if o.enabled { "enable" } else { "disable" }
                ))?;
            }
            Ok::<_, String>(())
        })();
        if let Err(error) = apply {
            let mut errors = vec![error];
            for command in [
                condition(&b.condition),
                format!("-break-after {id} {}", b.ignore_count),
                format!(
                    "-break-{} {id}",
                    if b.enabled { "enable" } else { "disable" }
                ),
            ] {
                if let Err(e) = self.mi(&command) {
                    errors.push(format!("Rollback: {e}"));
                }
            }
            return Err(errors.join("; "));
        }
        Ok(())
    }
    pub(super) fn breakpoint_command(&mut self, method: &str, p: &Json) -> Result<Json, String> {
        self.inactive()?;
        let text = |key: &str| p.get(key).and_then(Json::as_str).unwrap_or("");
        let mut result = json!({"updated":true});
        let action = (|| {
            match method {
                "break" | "data_break" => {
                    if method == "data_break" {
                        self.stopped()?;
                    }
                    let kind = if method == "data_break" {
                        match text("access") {
                            "" | "write" => Kind::Write,
                            "read" => Kind::Read,
                            "access" | "read-write" => Kind::Access,
                            _ => return Err("Data access must be read, write or read-write".into()),
                        }
                    } else if p.get("hardware").and_then(Json::as_bool).unwrap_or(false) {
                        Kind::Hardware
                    } else {
                        Kind::Code
                    };
                    let o = Options {
                        location: text(if method == "data_break" {
                            "expression"
                        } else {
                            "location"
                        })
                        .into(),
                        kind,
                        enabled: p.get("enabled").and_then(Json::as_bool).unwrap_or(true),
                        condition: text("condition").into(),
                        ignore_count: ignore_count(p)?.unwrap_or(0),
                        temporary: p.get("temporary").and_then(Json::as_bool).unwrap_or(false),
                    };
                    let r = self.insert_breakpoint(&o, false)?;
                    result = json!({"breakpoint":r.data});
                }
                "delete_break" => {
                    let id = text("number");
                    if id.is_empty() {
                        self.console("delete breakpoints")?;
                        self.unresolved_breakpoints.clear();
                    } else if self.unresolved_breakpoints.iter().any(|b| b.id == id) {
                        self.unresolved_breakpoints.retain(|b| b.id != id);
                    } else {
                        if !valid_id(id) {
                            return Err("Invalid breakpoint number".into());
                        }
                        self.mi(&format!("-break-delete {id}"))?;
                    }
                    result = json!({"deleted":true});
                }
                "enable_break" | "update_break" => {
                    self.refresh_breakpoints()?;
                    let id = text("number");
                    if id.is_empty()
                        && (method != "enable_break"
                            || p.get("all").and_then(Json::as_bool) != Some(true))
                    {
                        return Err(
                            "Select a breakpoint (or use all=true for enable/disable all)".into(),
                        );
                    }
                    let selected: Vec<_> = self
                        .snapshot
                        .breakpoints
                        .iter()
                        .filter(|b| id.is_empty() || b.id == id)
                        .cloned()
                        .collect();
                    if selected.is_empty() && !id.is_empty() {
                        return Err("Breakpoint no longer exists; select it again".into());
                    }
                    for b in selected {
                        let mut o = b.options();
                        if method == "enable_break" {
                            o.enabled = p
                                .get("enabled")
                                .and_then(Json::as_bool)
                                .ok_or("enabled must be true or false")?;
                        } else {
                            if let Some(enabled) = p.get("enabled") {
                                o.enabled =
                                    enabled.as_bool().ok_or("enabled must be true or false")?;
                            }
                            if let Some(condition) = p.get("condition") {
                                o.condition =
                                    condition.as_str().ok_or("condition must be text")?.into();
                            }
                            if let Some(count) = ignore_count(p)? {
                                o.ignore_count = count;
                            }
                        }
                        self.update_breakpoint(&b, &o)?;
                    }
                }
                _ => unreachable!(),
            }
            Ok::<_, String>(())
        })();
        // Always reconcile partial failures; never show an optimistic enabled state.
        let persisted = self.persist_breakpoint_edit();
        action?;
        persisted?;
        Ok(result)
    }
}
fn ignore_count(p: &Json) -> Result<Option<u32>, String> {
    p.get("ignore_count")
        .map(|v| {
            v.as_u64()
                .filter(|n| *n <= i32::MAX as u64)
                .map(|n| n as u32)
                .ok_or_else(|| "Ignore count must be between 0 and 2147483647".into())
        })
        .transpose()
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_watch_modes_and_retains_disabled_settings() {
        for (ty, kind) in [
            ("hw watchpoint", Kind::Write),
            ("read watchpoint", Kind::Read),
            ("acc watchpoint", Kind::Access),
            ("hw breakpoint", Kind::Hardware),
        ] {
            let b = Breakpoint {
                kind: ty.into(),
                location: "*(unsigned int *)0x20000000".into(),
                enabled: false,
                condition: "count > 3".into(),
                ignore_count: 2,
                hit_count: 7,
                ..Default::default()
            };
            assert_eq!(b.options().kind, kind);
            assert!(!b.options().enabled);
            assert_eq!(b.options().ignore_count, 2);
        }
        for bad in ["", ".", "1..2", "1\n-exec-continue", "all"] {
            assert!(!valid_id(bad));
        }
        assert!(valid_id("12.1"));
        assert!(ignore_count(&json!({"ignore_count":-1})).is_err());
    }
}
