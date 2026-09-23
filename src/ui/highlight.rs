//! Small C-like lexer: only visible lines allocate spans; no syntax database.
use super::*;

pub(super) fn selected_syntax(
    line: &str,
    in_comment: &mut bool,
    selection: Option<(usize, usize)>,
) -> Vec<Span<'static>> {
    let mut result = Vec::new();
    let mut offset = 0;
    scan(line, in_comment, |text, color| {
        let end = offset + text.len();
        let (a, b) = selection.unwrap_or((0, 0));
        let mut cuts = vec![offset, end];
        if a > offset && a < end {
            cuts.push(a);
        }
        if b > offset && b < end {
            cuts.push(b);
        }
        cuts.sort_unstable();
        for pair in cuts.windows(2) {
            let style = if pair[0] >= a && pair[0] < b {
                Style::default().fg(theme::CANVAS).bg(theme::ACCENT)
            } else {
                Style::default().fg(color)
            };
            result.push(Span::styled(
                line[pair[0]..pair[1]].replace('\t', "    "),
                style,
            ));
        }
        offset = end;
    });
    result
}

pub(super) fn comment_starts(lines: &[String]) -> Vec<bool> {
    let mut comment = false;
    lines
        .iter()
        .map(|line| {
            let before = comment;
            scan(line, &mut comment, |_, _| {});
            before
        })
        .collect()
}

fn scan(line: &str, comment: &mut bool, mut emit: impl FnMut(&str, Color)) {
    let mut i = 0;
    while i < line.len() {
        let tail = &line[i..];
        let ch = tail.chars().next().unwrap();
        let (len, color) = if *comment {
            let end = tail.find("*/").map(|p| p + 2);
            *comment = end.is_none();
            (end.unwrap_or(tail.len()), theme::DIM)
        } else if tail.starts_with("//") {
            (tail.len(), theme::DIM)
        } else if tail.starts_with("/*") {
            *comment = true;
            (2, theme::DIM)
        } else if ch == '"' || ch == '\'' {
            let mut escaped = false;
            let mut end = tail.len();
            for (n, c) in tail.char_indices().skip(1) {
                if c == ch && !escaped {
                    end = n + c.len_utf8();
                    break;
                }
                escaped = c == '\\' && !escaped;
            }
            (end, theme::GREEN)
        } else if ch.is_alphanumeric() || ch == '_' || ch == '#' {
            let len = tail
                .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '#')
                .unwrap_or(tail.len());
            let word = &tail[..len];
            let color = if word.starts_with('#')
                || matches!(
                    word,
                    "if" | "else"
                        | "for"
                        | "while"
                        | "return"
                        | "switch"
                        | "case"
                        | "break"
                        | "continue"
                        | "do"
                        | "sizeof"
                        | "typedef"
                        | "struct"
                        | "enum"
                        | "union"
                ) {
                theme::VIOLET
            } else if matches!(
                word,
                "void"
                    | "const"
                    | "static"
                    | "volatile"
                    | "unsigned"
                    | "signed"
                    | "long"
                    | "short"
                    | "int"
                    | "char"
                    | "float"
                    | "double"
                    | "bool"
                    | "extern"
            ) || word.ends_with("_t")
            {
                theme::ACCENT
            } else if ch.is_ascii_digit() || matches!(word, "NULL" | "true" | "false") {
                theme::AMBER
            } else if tail[len..].trim_start().starts_with('(') {
                theme::GREEN
            } else {
                theme::TEXT
            };
            (len, color)
        } else {
            (
                ch.len_utf8(),
                if ch.is_whitespace() {
                    theme::TEXT
                } else {
                    theme::MUTED
                },
            )
        };
        emit(&line[i..i + len], color);
        i += len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comments_strings_unicode_and_scrolled_state_preserve_source() {
        let lines: Vec<String> = [
            "/* multiline",
            "comment */ int n = 42; // tail",
            "printf(\"中文 /* not a comment */ \\\"\");",
            "return n;",
        ]
        .map(str::to_owned)
        .into();
        let starts = comment_starts(&lines);
        assert_eq!(starts, [false, true, false, false]);
        for (line, mut state) in lines.iter().zip(starts) {
            let spans = selected_syntax(line, &mut state, None);
            assert_eq!(
                spans.iter().map(|s| s.content.as_ref()).collect::<String>(),
                *line
            );
        }
        let spans = selected_syntax(&lines[1], &mut true, None);
        assert_eq!(spans[0].style.fg, Some(theme::DIM));
        assert!(
            spans
                .iter()
                .any(|s| s.content == "42" && s.style.fg == Some(theme::AMBER))
        );
    }
}
