//! Validate basic register operands without admitting GDB convenience variables.
//! Zero tokens below are syntax placeholders only; GDB always receives the
//! original expression and values observed from the real execution frame.
use super::*;

pub(super) fn syntax(text: &str) -> Result<(String, bool), String> {
    if !text.is_ascii() || text.len() > 256 {
        return Err("Image expression must contain at most 256 ASCII bytes".into());
    }
    let mut bytes = text.as_bytes().to_vec();
    let mut offset = 0;
    let mut registers = false;
    while offset < bytes.len() {
        if bytes[offset] != b'$' {
            offset += 1;
            continue;
        }
        let start = offset;
        offset += 1;
        while offset < bytes.len()
            && (bytes[offset].is_ascii_alphanumeric() || bytes[offset] == b'_')
        {
            offset += 1;
        }
        let name = &text[start + 1..offset];
        if !frame::NAMES.contains(&name) && !matches!(name, "r13" | "r14" | "r15") {
            return Err("Isolated image expressions support only observed ARM integer registers and CPSR; debugger convenience variables are unavailable".into());
        }
        bytes[start..offset].fill(b' ');
        bytes[start] = b'0';
        registers = true;
    }
    let syntax = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    memory::expression::validate(&syntax)?;
    Ok((syntax, registers))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn register_operands_do_not_hide_global_roots_or_side_effects() {
        for text in [
            "$r0 + counter",
            "counter + $sp",
            "*(uint32_t*)($r13 + index)",
            "$pc + 4",
            "$cpsr & 31",
            "$r15 + value",
        ] {
            let (parsed, registers) = syntax(text).unwrap();
            assert!(registers);
            assert_eq!(parsed.len(), text.len());
            let roots = memory::expression::roots(&parsed).unwrap();
            assert!(!roots.iter().any(|root| root.starts_with('$')));
            if text.contains("counter") {
                assert!(roots.contains(&"counter"));
            }
        }
        assert!(!syntax("counter + 4").unwrap().1);
        for text in [
            "$r0 = counter",
            "$r0++ + value",
            "$pc + next()",
            "$_exitcode",
            "$$foo",
            "$r32 + value",
            "$sp0 + value",
            "$r0;quit",
            "$r0\n",
        ] {
            assert!(syntax(text).is_err(), "{text}");
        }
    }
}
