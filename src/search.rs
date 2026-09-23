//! Shared case-insensitive fuzzy matching for file and symbol navigation.

/// Lower scores rank first: exact name, contiguous match, then subsequence.
pub(crate) fn score(text: &str, query: &str) -> Option<usize> {
    let text = text.replace('\\', "/").to_lowercase();
    let query = query.trim().replace('\\', "/").to_lowercase();
    if query.is_empty() || text == query {
        return Some(0);
    }
    if let Some(offset) = text.find(&query) {
        return Some(1 + offset);
    }
    let mut wanted = query.chars();
    let mut next = wanted.next()?;
    let mut first = None;
    for (position, ch) in text.chars().enumerate() {
        if ch == next {
            let start = *first.get_or_insert(position);
            if let Some(ch) = wanted.next() {
                next = ch;
            } else {
                return Some(10_000 + start + position);
            }
        }
    }
    None
}

/// GDB uses a regexp, not a glob. Escape input and fold ASCII identifier letters
/// explicitly instead of changing GDB's global case-sensitivity setting.
pub(crate) fn symbol_pattern(query: &str) -> String {
    query
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphabetic() {
                format!("[{}{}]", ch.to_ascii_lowercase(), ch.to_ascii_uppercase())
            } else if ".^$*+?()[]{}\\|".contains(ch) {
                format!("\\{ch}")
            } else {
                ch.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(".*")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_match_case_substrings_paths_and_unicode_without_regex_syntax() {
        for name in ["Spi.c", "Spi_Irq.c", "espi_hal.c", "espi_std.c"] {
            assert!(score(name, "spi").is_some());
        }
        assert!(score("Src/Spi_Irq.c", "sirq").is_some());
        assert!(score("Src\\Spi.c", "src/spi").is_some());
        assert!(score("驱动/串口.c", "串口").is_some());
        assert!(score("main.c", "spi").is_none());
        assert!(score("spi.c", ".*").is_none());
        assert!(score("spi", "spi") < score("espi_hal", "spi"));
        assert!(score("espi_hal", "spi") < score("sample_irq", "spi"));
        assert_eq!(symbol_pattern("a.b[0]"), "[aA].*\\..*[bB].*\\[.*0.*\\]");
    }
}
