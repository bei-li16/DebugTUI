//! A GDB can expose a bitfield address and its declared integer sizeof.
//! Inspect the actual parent declaration instead of inferring a full scalar.
use super::{ScalarType, variable_lvalue};

fn unparenthesize(mut expression: &str) -> &str {
    expression = expression.trim();
    loop {
        if !expression.starts_with('(') || !expression.ends_with(')') {
            return expression;
        }
        let mut depth = 0;
        let encloses = expression.bytes().enumerate().all(|(i, c)| {
            if c == b'(' {
                depth += 1;
            } else if c == b')' {
                depth -= 1;
            }
            depth != 0 || i + 1 == expression.len()
        });
        if !encloses {
            return expression;
        }
        expression = expression[1..expression.len() - 1].trim();
    }
}
impl ScalarType {
    pub(crate) fn member_parent(expression: &str) -> Result<Option<(String, String)>, String> {
        let expression = unparenthesize(expression);
        // An array element cannot itself be a C/C++ bitfield.
        if expression.ends_with(']') {
            return Ok(None);
        }
        let dot = expression.rfind('.').map(|i| (i, 1, false));
        let arrow = expression.rfind("->").map(|i| (i, 2, true));
        let Some((i, len, pointer)) = dot.into_iter().chain(arrow).max_by_key(|(i, _, _)| *i)
        else {
            return Ok(None);
        };
        let field = expression[i + len..].trim();
        if field.is_empty()
            || !field
                .bytes()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
            || !field
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            return Err(
                "Qualified or ambiguous member requires a verified field descriptor".into(),
            );
        }
        let parent = if pointer {
            format!("*({})", &expression[..i])
        } else {
            expression[..i].trim().to_owned()
        };
        variable_lvalue(&parent)?;
        Ok(Some((parent, field.to_owned())))
    }
    pub(crate) fn validate_member(parent_type: &str, field: &str) -> Result<Option<u16>, String> {
        let signature = Self::type_signature(parent_type)?;
        if signature
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .any(|t| matches!(t, "const" | "volatile"))
        {
            return Err("Const or volatile parent storage cannot be edited".into());
        }
        let mut angles = 0usize;
        let start = parent_type
            .char_indices()
            .find_map(|(i, c)| {
                match c {
                    '<' => angles += 1,
                    '>' if angles > 0 => angles -= 1,
                    _ => {}
                }
                (c == '{' && angles == 0).then_some(i)
            })
            .ok_or("Parent type has no complete field declaration")?;
        let mut depth = 1usize;
        let mut declaration = String::new();
        let mut found = 0usize;
        let mut bitfield = None;
        for c in parent_type[start + 1..].chars() {
            match c {
                '{' => {
                    depth += 1;
                    declaration.push(' ');
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                ';' if depth == 1 => {
                    let bytes = declaration.as_bytes();
                    let mut i = 0;
                    let mut tail = None;
                    while i < bytes.len() {
                        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
                            let begin = i;
                            i += 1;
                            while i < bytes.len()
                                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
                            {
                                i += 1;
                            }
                            if &declaration[begin..i] == field {
                                tail = Some(declaration[i..].trim());
                            }
                        } else {
                            i += 1;
                        }
                    }
                    if let Some(tail) = tail {
                        if tail.is_empty() || tail.starts_with('[') {
                            found += 1;
                        } else if tail.starts_with(':') && !tail.starts_with("::") {
                            found += 1;
                            bitfield = Some(
                                tail[1..]
                                    .trim()
                                    .parse::<u16>()
                                    .map_err(|_| "Unknown bitfield width")?,
                            );
                        }
                    }
                    declaration.clear();
                }
                _ if depth == 1 => declaration.push(c),
                _ => {}
            }
        }
        if found != 1 {
            return Err(
                "Member declaration is missing, inherited or ambiguous; no verified scalar writer"
                    .into(),
            );
        }
        if bitfield.is_some_and(|bits| !(1..=64).contains(&bits)) {
            return Err("GDB bitfield writer needs 1..64 actual field bits".into());
        }
        Ok(bitfield)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parent_resolution_normalizes_outer_parens_and_keeps_arrays_and_namespaces() {
        assert_eq!(
            ScalarType::member_parent("((obj).member)").unwrap(),
            Some(("(obj)".into(), "member".into()))
        );
        assert_eq!(
            ScalarType::member_parent("arr[2]->member").unwrap(),
            Some(("*(arr[2])".into(), "member".into()))
        );
        assert_eq!(ScalarType::member_parent("obj.arr[2]").unwrap(), None);
        assert_eq!(ScalarType::member_parent("ns::counter").unwrap(), None);
        assert!(ScalarType::member_parent("obj.Base::member").is_err());
    }
    #[test]
    fn actual_parent_declarations_expose_width_and_reject_missing_or_ambiguous_members() {
        let parent = "struct P { public: uint32_t before; unsigned low : 5; int middle : 6; unsigned neighbour : 7; struct Nested { int middle; } nested; uint32_t after; }";
        assert!(ScalarType::validate_member(parent, "before").is_ok());
        assert!(ScalarType::validate_member(parent, "after").is_ok());
        assert!(ScalarType::validate_member(parent, "nested").is_ok());
        assert_eq!(ScalarType::validate_member(parent, "low").unwrap(), Some(5));
        assert_eq!(
            ScalarType::validate_member(parent, "middle").unwrap(),
            Some(6)
        );
        assert_eq!(
            ScalarType::validate_member(parent, "neighbour").unwrap(),
            Some(7)
        );
        assert!(ScalarType::validate_member(parent, "absent").is_err());
        assert!(ScalarType::validate_member("struct P {int x; int x;}", "x").is_err());
        assert!(ScalarType::validate_member("const struct P {int x;}", "x").is_err());
    }
    #[test]
    fn template_arguments_do_not_become_outer_pointer_or_qualifier_evidence() {
        assert!(ScalarType::validate_type("struct P<const int *> {int value;}").is_err());
        assert_eq!(
            ScalarType::validate_type("struct P<const int *> {int value;} *").unwrap(),
            (true, false, false)
        );
        assert!(ScalarType::validate_member("struct P<const int *> {int value;}", "value").is_ok());
        assert!(
            ScalarType::validate_member(
                "struct P<Nested {int value;}> {unsigned value:5;}",
                "value"
            )
            .is_ok()
        );
    }
}
