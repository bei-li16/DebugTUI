//! A bounded subset of C/C++ memory expressions, checked before any MI evaluation.
//! Parsing accepts only operations without language-level writes or calls. GDB's
//! may-call-functions policy separately blocks overloaded operators/conversions.

const ERROR: &str = "Watch memory expression must use read-only C/C++ syntax without calls, assignments or debugger variables";

pub(super) fn validate(text: &str) -> Result<(), String> {
    if text.is_empty()
        || text.len() > 256
        || !text.is_ascii()
        || text.bytes().any(|b| b.is_ascii_control())
        || text.contains("/*")
        || text.contains("//")
    {
        return Err(ERROR.into());
    }
    let tokens = tokens(text)?;
    let mut parser = Parser { tokens, next: 0 };
    parser.expression(1, 0)?;
    if parser.next != parser.tokens.len() {
        return Err(ERROR.into());
    }
    Ok(())
}

fn identifier(text: &str) -> bool {
    text.as_bytes()
        .first()
        .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
}

fn tokens(text: &str) -> Result<Vec<&str>, String> {
    let bytes = text.as_bytes();
    let mut result = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b' ' {
            i += 1;
            continue;
        }
        let start = i;
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if matches!(
                &text[start..i],
                "new"
                    | "delete"
                    | "throw"
                    | "operator"
                    | "asm"
                    | "__asm"
                    | "__asm__"
                    | "sizeof"
                    | "typeof"
                    | "__typeof"
                    | "__typeof__"
                    | "typeid"
                    | "alignof"
                    | "__alignof__"
                    | "co_await"
                    | "co_yield"
            ) {
                return Err(ERROR.into());
            }
        } else if bytes[i].is_ascii_digit() {
            let hexadecimal = text[i..].starts_with("0x") || text[i..].starts_with("0X");
            i += if hexadecimal { 2 } else { 0 };
            let digits = i;
            while i < bytes.len()
                && if hexadecimal {
                    bytes[i].is_ascii_hexdigit()
                } else {
                    bytes[i].is_ascii_digit()
                }
            {
                i += 1;
            }
            if i == digits {
                return Err(ERROR.into());
            }
            while i < bytes.len() && matches!(bytes[i], b'u' | b'U' | b'l' | b'L') {
                i += 1;
            }
        } else if i + 1 < bytes.len()
            && matches!(
                &text[i..i + 2],
                "->" | "::" | "<<" | ">>" | "<=" | ">=" | "==" | "!=" | "&&" | "||"
            )
        {
            i += 2;
        } else if i + 1 < bytes.len() && matches!(&text[i..i + 2], "++" | "--") {
            return Err(ERROR.into());
        } else if b"()[] .*&+-~!/%<>^|".contains(&bytes[i]) {
            i += 1;
        } else {
            return Err(ERROR.into());
        }
        result.push(&text[start..i]);
    }
    Ok(result)
}

struct Parser<'a> {
    tokens: Vec<&'a str>,
    next: usize,
}
impl Parser<'_> {
    fn take(&mut self, token: &str) -> bool {
        if self.tokens.get(self.next).is_some_and(|t| *t == token) {
            self.next += 1;
            true
        } else {
            false
        }
    }
    fn name(&mut self) -> Result<(), String> {
        if self.tokens.get(self.next).is_some_and(|t| identifier(t)) {
            self.next += 1;
            Ok(())
        } else {
            Err(ERROR.into())
        }
    }
    fn expression(&mut self, minimum: u8, depth: u8) -> Result<(), String> {
        if depth > 16 {
            return Err("Watch memory expression nesting exceeds 16 levels".into());
        }
        if self.take("*")
            || self.take("&")
            || self.take("+")
            || self.take("-")
            || self.take("~")
            || self.take("!")
        {
            self.expression(11, depth + 1)?;
        } else if self.take("(") {
            if let Some(end) = self.pointer_type_end() {
                self.next = end + 1;
                self.expression(11, depth + 1)?;
            } else {
                self.expression(1, depth + 1)?;
                if !self.take(")") {
                    return Err(ERROR.into());
                }
            }
        } else if self.take("::") {
            self.name()?;
        } else if self
            .tokens
            .get(self.next)
            .is_some_and(|t| identifier(t) || t.as_bytes().first().is_some_and(u8::is_ascii_digit))
        {
            self.next += 1;
        } else {
            return Err(ERROR.into());
        }
        loop {
            if self.take("[") {
                self.expression(1, depth + 1)?;
                if !self.take("]") {
                    return Err(ERROR.into());
                }
            } else if self.take(".") || self.take("->") || self.take("::") {
                self.name()?;
            } else {
                break;
            }
        }
        while let Some(precedence) = self.tokens.get(self.next).and_then(|t| precedence(t)) {
            if precedence < minimum {
                break;
            }
            self.next += 1;
            self.expression(precedence + 1, depth + 1)?;
        }
        Ok(())
    }
    fn pointer_type_end(&self) -> Option<usize> {
        let mut i = self.next;
        let mut name = false;
        let mut pointer = false;
        while let Some(token) = self.tokens.get(i) {
            match *token {
                ")" if name && pointer => return Some(i),
                "*" if name => pointer = true,
                "::" if name && !pointer => {
                    if !self.tokens.get(i + 1).is_some_and(|t| identifier(t)) {
                        return None;
                    }
                }
                t if identifier(t) && !pointer => name = true,
                "const" | "volatile" | "restrict" | "__restrict" | "__restrict__" if pointer => {}
                _ => return None,
            }
            i += 1;
        }
        None
    }
}

fn precedence(token: &str) -> Option<u8> {
    Some(match token {
        "||" => 1,
        "&&" => 2,
        "|" => 3,
        "^" => 4,
        "&" => 5,
        "==" | "!=" => 6,
        "<" | ">" | "<=" | ">=" => 7,
        "<<" | ">>" => 8,
        "+" | "-" => 9,
        "*" | "/" | "%" => 10,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readonly_watch_syntax_preserves_members_indices_and_typed_pointer_casts() {
        for expression in [
            "counter",
            "pair.negative",
            "(pair).negative",
            "ptr->member",
            "ns::value",
            "::value",
            "samples[index + 1]",
            "*ptr",
            "*(uint32_t *)(4294967300)",
            "*((const volatile struct ns::T * const *)(base + (index << 2)))",
            "counter+1",
            "a < b && c != 0",
            "a / 2 % 4",
            "*(uint32_t *)0x20000000UL",
        ] {
            assert!(validate(expression).is_ok(), "{expression}");
        }
    }
    #[test]
    fn readonly_watch_syntax_rejects_side_effects_and_calls_in_all_positions() {
        for expression in [
            "",
            "counter++",
            "++counter",
            "counter--",
            "counter += 1",
            "counter=1",
            "counter <<= 1",
            "func()",
            "(func)(1)",
            "samples[next()]",
            "ptr->method()",
            "*(uint32_t *)(func())",
            "(counter,other)",
            "counter;quit",
            "$pc",
            "$_exitcode",
            "new T",
            "delete *ptr",
            "throw value",
            "sizeof func()",
            "operator*(ptr)",
            "counter/* comment */",
            "counter// comment",
            "'x'",
            "\"x\"",
            "counter\n",
            "{int}0x2000",
            "reinterpret_cast<T*>(ptr)",
            "0x",
            "samples[]",
            "ptr->",
        ] {
            assert!(validate(expression).is_err(), "{expression}");
        }
    }
    #[test]
    fn readonly_watch_syntax_has_bounded_length_and_nesting() {
        assert!(validate(&"x".repeat(257)).is_err());
        assert!(validate(&format!("{}x{}", "(".repeat(17), ")".repeat(17))).is_err());
        assert!(validate(&format!("{}x{}", "(".repeat(16), ")".repeat(16))).is_ok());
        assert!(validate("变量").is_err());
    }
}
