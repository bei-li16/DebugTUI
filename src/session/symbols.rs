//! Source navigation queries use debug metadata only, never expression evaluation.
use super::*;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct Symbol {
    pub name: String,
    pub kind: String,
    pub file: String,
    pub line: u32,
    pub description: String,
}

fn parse_symbols(record: &Record, kind: &str) -> Vec<Symbol> {
    let mut found = Vec::new();
    if let Some(files) = record.data.field("symbols").and_then(|v| v.field("debug")) {
        for file in files.items() {
            let path = match file.string("fullname") {
                full if !full.is_empty() => full,
                _ => file.string("filename"),
            };
            if let Some(symbols) = file.field("symbols") {
                for entry in symbols.items() {
                    let name = entry.string("name");
                    if name.is_empty() || name.chars().any(char::is_control) {
                        continue;
                    }
                    found.push(Symbol {
                        name,
                        kind: kind.into(),
                        file: path.clone(),
                        line: entry.string("line").parse().unwrap_or(0),
                        description: entry.string("description"),
                    });
                }
            }
        }
    }
    found
}

impl Engine {
    pub(super) fn symbols(&mut self, query: &str) -> Result<Json, String> {
        self.inactive()?;
        let query = query.trim();
        if query.is_empty() || query.chars().count() > 128 || query.chars().any(char::is_control) {
            return Err("Enter 1..128 characters to search symbols".into());
        }
        let pattern = mi::quote(&crate::search::symbol_pattern(query));
        let mut symbols = Vec::new();
        let mut warnings = Vec::new();
        let mut truncated = false;
        let mut supported = 0;
        // Bound both DWARF expansion/output and the UI result list. Keep useful
        // partial results if an older/vendor GDB lacks one query category.
        for (command, kind) in [
            ("functions", "function"),
            ("variables", "variable"),
            ("types", "type"),
        ] {
            match self.mi(&format!(
                "-symbol-info-{command} --name {pattern} --max-results 200"
            )) {
                Ok(record) => {
                    supported += 1;
                    let found = parse_symbols(&record, kind);
                    truncated |= found.len() >= 200;
                    symbols.extend(found);
                }
                Err(error) => warnings.push(format!("{kind}: {error}")),
            }
        }
        if supported == 0 {
            return Err(format!(
                "GDB symbol search unavailable: {}",
                warnings.join("; ")
            ));
        }
        symbols.retain(|s| crate::search::score(&s.name, query).is_some());
        symbols.sort_by_key(|s| {
            (
                crate::search::score(&s.name, query),
                s.name.clone(),
                s.file.clone(),
                s.line,
                s.kind.clone(),
            )
        });
        symbols.dedup();
        Ok(json!({"symbols":symbols,"truncated":truncated,"warnings":warnings}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_locations_keep_duplicate_static_names_and_missing_lines() {
        let record = mi::parse(r#"1^done,symbols={debug=[{filename="one.c",fullname="/ci/src/one.c",symbols=[{line="12",name="Spi_Init",description="void Spi_Init(void);"}]},{filename="two.c",symbols=[{line="42",name="Spi_Init"},{name="spi_type"}]}]}"#).unwrap().unwrap();
        let found = parse_symbols(&record, "function");
        assert_eq!(found.len(), 3);
        assert_eq!((&*found[0].file, found[0].line), ("/ci/src/one.c", 12));
        assert_eq!((&*found[1].file, found[1].line), ("two.c", 42));
        assert_eq!(found[2].line, 0);
    }
}
