//! Shared case-insensitive fuzzy matching for file and symbol navigation.

/// Lower scores rank first: exact name, contiguous match, then subsequence.
pub(crate) fn score(text: &str, query: &str) -> Option<usize> {
    Matcher::new(query).score(text)
}

/// One query normalized once and ranked against many candidates. ASCII
/// candidates (nearly all paths and symbols) reuse one buffer instead of
/// allocating two strings each; other text keeps full Unicode lowercasing.
pub(crate) struct Matcher {
    query: String,
    text: String,
}

impl Matcher {
    pub(crate) fn new(query: &str) -> Self {
        Self {
            query: normalize(query.trim()),
            text: String::new(),
        }
    }

    pub(crate) fn score(&mut self, text: &str) -> Option<usize> {
        if !text.is_ascii() {
            return rank(&normalize(text), &self.query);
        }
        self.text.clear();
        self.text.extend(text.bytes().map(|b| match b {
            b'\\' => '/',
            _ => char::from(b.to_ascii_lowercase()),
        }));
        rank(&self.text, &self.query)
    }
}

fn normalize(text: &str) -> String {
    text.replace('\\', "/").to_lowercase()
}

fn rank(text: &str, query: &str) -> Option<usize> {
    if query.is_empty() || text == query {
        return Some(0);
    }
    if let Some(offset) = text.find(query) {
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

    #[test]
    fn reused_matcher_ranks_exactly_like_one_off_scores() {
        let candidates = [
            "Spi.c",
            "D:\\Work\\MCAL\\Spi_Irq.c",
            "src/espi_hal.c",
            "SAMPLE_IRQ.C",
            "驱动/串口.c",
            "ΣΊΣΥΦΟΣ/spi.c",
            "",
        ];
        for query in ["spi", " SRC/ESPI ", "s\\i", "串口", "σ", "", "zz"] {
            let mut matcher = Matcher::new(query);
            for text in candidates {
                let expected = {
                    let text = text.replace('\\', "/").to_lowercase();
                    let query = query.trim().replace('\\', "/").to_lowercase();
                    rank(&text, &query)
                };
                assert_eq!(matcher.score(text), expected, "{text:?} / {query:?}");
                assert_eq!(score(text, query), expected, "{text:?} / {query:?}");
            }
        }
    }
}
