//! Typed MI assignment accepts literal values and a restricted C lvalue grammar.
use super::*;

/// No function calls, casts, arithmetic, assignment, debugger variables or dynamic indices.
/// Parentheses and dereferences are allowed; GDB function calls are separately disabled.
pub fn variable_lvalue(text: &str) -> Result<(), String> {
    struct Parser<'a> {
        bytes: &'a [u8],
        pos: usize,
        depth: usize,
    }
    impl Parser<'_> {
        fn spaces(&mut self) {
            while self.bytes.get(self.pos) == Some(&b' ') {
                self.pos += 1;
            }
        }
        fn eat(&mut self, s: &[u8]) -> bool {
            self.spaces();
            if self.bytes[self.pos..].starts_with(s) {
                self.pos += s.len();
                true
            } else {
                false
            }
        }
        fn identifier(&mut self) -> bool {
            self.spaces();
            let start = self.pos;
            if self
                .bytes
                .get(self.pos)
                .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
            {
                self.pos += 1;
                while self
                    .bytes
                    .get(self.pos)
                    .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
                {
                    self.pos += 1;
                }
            }
            self.pos > start
        }
        fn value(&mut self) -> bool {
            self.depth += 1;
            if self.depth > 16 {
                return false;
            }
            let ok = if self.eat(b"*") {
                self.value()
            } else if self.eat(b"(") {
                self.value() && self.eat(b")")
            } else {
                self.identifier()
            };
            if !ok {
                return false;
            }
            loop {
                if self.eat(b".") || self.eat(b"->") || self.eat(b"::") {
                    if !self.identifier() {
                        return false;
                    }
                } else if self.eat(b"[") {
                    self.spaces();
                    let start = self.pos;
                    while self.bytes.get(self.pos).is_some_and(u8::is_ascii_digit) {
                        self.pos += 1;
                    }
                    if start == self.pos || self.pos - start > 10 || !self.eat(b"]") {
                        return false;
                    }
                } else {
                    break;
                }
            }
            self.depth -= 1;
            true
        }
    }
    if text.is_empty() || text.len() > 256 || !text.is_ascii() || text.chars().any(char::is_control)
    {
        return Err(
            "Use a bounded C variable/member/dereference with constant array indices".into(),
        );
    }
    let mut p = Parser {
        bytes: text.as_bytes(),
        pos: 0,
        depth: 0,
    };
    if !p.value() {
        return Err(
            "Editing expressions cannot contain calls, casts, assignments or arithmetic".into(),
        );
    }
    p.spaces();
    if p.pos != p.bytes.len() {
        return Err("Editing expression is not a supported lvalue".into());
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ScalarType {
    pub bits: u16,
    pub signed: bool,
    pub float: bool,
    pub pointer: bool,
    pub boolean: bool,
}
impl ScalarType {
    // ptype expands aggregate bodies. A member's pointer/qualifier is not the
    // aggregate's own declarator (nor the declarator of a pointer to it).
    pub(crate) fn type_signature(raw_type: &str) -> Result<String, String> {
        let mut signature = String::new();
        let mut depth = 0usize;
        for c in raw_type.chars() {
            match c {
                '{' => depth += 1,
                '}' => depth = depth.checked_sub(1).ok_or("Malformed expanded type")?,
                _ if depth == 0 => signature.push(c),
                _ => {}
            }
        }
        if depth != 0 {
            return Err("Incomplete expanded type".into());
        }
        Ok(signature)
    }
    pub fn validate_type(raw_type: &str) -> Result<(bool, bool, bool), String> {
        let signature = Self::type_signature(raw_type)?;
        let raw_type = signature.as_str();
        let compact = raw_type.split_whitespace().collect::<Vec<_>>();
        let pointee = raw_type.rfind('*');
        let top = if let Some(index) = pointee {
            &raw_type[index + 1..]
        } else {
            raw_type
        };
        if top
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .any(|t| matches!(t, "const" | "volatile"))
        {
            return Err("Const or volatile storage needs its own declared write semantics".into());
        }
        if raw_type.contains('&') || raw_type.contains('[') || raw_type.contains('(') {
            return Err("Reference, array or function-pointer writer is not adapted".into());
        }
        let pointer = pointee.is_some();
        if !pointer
            && compact
                .iter()
                .any(|t| matches!(*t, "struct" | "class" | "union"))
        {
            return Err("Expand the aggregate and select a scalar member".into());
        }
        let float = !pointer && compact.iter().any(|t| matches!(*t, "float" | "double"));
        if float && compact.contains(&"long") {
            return Err("Long-double writer is not adapted".into());
        }
        let boolean = !pointer && compact.iter().any(|t| matches!(*t, "bool" | "_Bool"));
        Ok((pointer, float, boolean))
    }
    pub fn assignment(&self, input: &Input) -> Result<(RawValue, String), String> {
        if !matches!(self.bits, 8 | 16 | 32 | 64 | 128) {
            return Err("Typed scalar width is unsupported".into());
        }
        if self.float && !matches!(input.kind, InputKind::Float | InputKind::Bytes) {
            return Err("Use Float for a floating value, or Bytes for its exact raw bits".into());
        }
        if !self.float && input.kind == InputKind::Float {
            return Err("Floating input cannot implicitly convert to an integer/pointer".into());
        }
        if !self.signed && input.kind == InputKind::Signed && input.text.trim().starts_with('-') {
            return Err("Negative input cannot fit an unsigned value or pointer".into());
        }
        let raw = input.raw(self.bits, &[])?;
        let n = raw.integer()?;
        if self.boolean && n > 1 {
            return Err("Boolean assignment needs 0 or 1".into());
        }
        if self.signed && input.kind == InputKind::Unsigned && n > (mask(self.bits) >> 1) {
            return Err("Unsigned input exceeds this signed type's positive range".into());
        }
        let expression = if self.float {
            let value = if self.bits == 32 {
                f64::from(f32::from_bits(n as u32))
            } else if self.bits == 64 {
                f64::from_bits(n as u64)
            } else {
                return Err("Typed float writer needs 32 or 64 bits".into());
            };
            if !value.is_finite() {
                return Err("This typed MI writer cannot construct NaN/Infinity without a raw-memory bypass".into());
            }
            if value == 0.0 && value.is_sign_negative() {
                "(-1.0 * 0.0)".into()
            } else {
                format!("{value:e}")
            }
        } else if self.bits == 128 {
            format!(
                "(((unsigned __int128)0x{:x} << 64) | (unsigned __int128)0x{:x})",
                n >> 64,
                n as u64
            )
        } else if self.signed
            && input.kind == InputKind::Signed
            && input.text.trim().starts_with('-')
        {
            let magnitude = (!n).wrapping_add(1) & mask(self.bits);
            format!("-{magnitude}")
        } else {
            raw.hex.clone()
        };
        Ok((raw, expression))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expanded_aggregate_members_never_change_the_root_type_or_qualifiers() {
        assert!(ScalarType::validate_type("struct P { int *p; const int value; }").is_err());
        assert_eq!(
            ScalarType::validate_type("struct P { volatile int value; } *").unwrap(),
            (true, false, false)
        );
        assert_eq!(
            ScalarType::validate_type("const struct P { int *p; } *").unwrap(),
            (true, false, false)
        );
        assert!(ScalarType::validate_type("struct P { int *p; } * const").is_err());
        assert!(ScalarType::validate_type("int (* const)(void)").is_err());
        assert!(ScalarType::validate_type("struct P { int value;").is_err());
    }
    #[test]
    fn typed_lvalues_allow_nested_members_and_reject_side_effects_and_injection() {
        for valid in [
            "counter",
            "outer.pair.value",
            "outer.items[12]",
            "ptr->value",
            "(*ptr).value",
            "ns::item",
            "*pointer",
        ] {
            assert!(variable_lvalue(valid).is_ok(), "{valid}");
        }
        for bad in [
            "function()",
            "a[i]",
            "a++",
            "a=b",
            "a; reset",
            "$r0",
            "((int*)0x40000000)[0]",
            "a\n",
            &"(".repeat(20),
        ] {
            assert!(variable_lvalue(bad).is_err(), "{bad}");
        }
        assert!(ScalarType::validate_type("const unsigned int").is_err());
        assert!(ScalarType::validate_type("unsigned int * const").is_err());
        assert_eq!(
            ScalarType::validate_type("const unsigned int *").unwrap(),
            (true, false, false)
        );
        assert!(ScalarType::validate_type("volatile int").is_err());
    }
    #[test]
    fn typed_assignments_check_signed_ranges_pointers_bool_float_and_128bit_literals() {
        let signed = ScalarType {
            bits: 32,
            signed: true,
            float: false,
            pointer: false,
            boolean: false,
        };
        let input = Input {
            kind: InputKind::Signed,
            text: "-2147483648".into(),
            little_endian: None,
        };
        assert_eq!(signed.assignment(&input).unwrap().1, "-2147483648");
        assert!(
            signed
                .assignment(&Input {
                    kind: InputKind::Unsigned,
                    text: "2147483648".into(),
                    ..input.clone()
                })
                .is_err()
        );
        assert!(
            ScalarType {
                signed: false,
                pointer: true,
                ..signed.clone()
            }
            .assignment(&input)
            .is_err()
        );
        assert!(
            ScalarType {
                boolean: true,
                ..signed.clone()
            }
            .assignment(&Input {
                kind: InputKind::Unsigned,
                text: "2".into(),
                ..input.clone()
            })
            .is_err()
        );
        let wide = ScalarType {
            bits: 128,
            signed: false,
            ..signed.clone()
        };
        let literal = wide
            .assignment(&Input {
                kind: InputKind::Unsigned,
                text: u128::MAX.to_string(),
                ..input.clone()
            })
            .unwrap();
        assert_eq!(literal.0.integer().unwrap(), u128::MAX);
        assert!(literal.1.contains("<< 64"));
        let float = ScalarType {
            float: true,
            ..signed
        };
        assert_eq!(
            float
                .assignment(&Input {
                    kind: InputKind::Float,
                    text: "-0".into(),
                    ..input.clone()
                })
                .unwrap()
                .0
                .hex,
            "0x80000000"
        );
        assert!(
            float
                .assignment(&Input {
                    kind: InputKind::Float,
                    text: "NaN".into(),
                    ..input
                })
                .is_err()
        );
    }
}
