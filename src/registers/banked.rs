//! Names accepted by the pinned R52 banked adapter. Access legality is checked
//! against fresh current Debug state, distinct from stopped DSPSR.
use super::RawValue;
use serde::{Deserialize, Serialize};
pub const PROTOCOL: &str = "debugtui-armv8-banked-2 external-identity current-el dspsr dlr mrs physical-readback no-mode-change stop-on-fault";

pub const NAMES: [&str; 30] = [
    "r8_usr", "r9_usr", "r10_usr", "r11_usr", "r12_usr", "sp_usr", "lr_usr", "r8_fiq", "r9_fiq",
    "r10_fiq", "r11_fiq", "r12_fiq", "sp_fiq", "lr_fiq", "spsr_fiq", "lr_irq", "sp_irq",
    "spsr_irq", "lr_svc", "sp_svc", "spsr_svc", "lr_abt", "sp_abt", "spsr_abt", "lr_und", "sp_und",
    "spsr_und", "elr_hyp", "sp_hyp", "spsr_hyp",
];

pub fn valid_name(name: &str) -> bool {
    NAMES.contains(&name)
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReadMethod {
    Mov32,
    Mrs32,
    BankedMrs32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Evidence {
    pub midr: RawValue,
    pub dscr: RawValue,
    pub dspsr: RawValue,
    pub dlr: RawValue,
    pub read_method: ReadMethod,
}
impl Evidence {
    pub fn current_mode(&self) -> Result<Option<u32>, String> {
        let el = (self.dscr.integer()? >> 8) & 3;
        // Direct PSTATE reads are CONSTRAINED UNPREDICTABLE in Debug state.
        Ok(match el {
            0 => Some(0x10),
            2 => Some(0x1a),
            _ => None,
        })
    }
}
pub struct Response {
    pub value: RawValue,
    pub evidence: Evidence,
}
fn exact(text: &str) -> Result<RawValue, String> {
    if text.len() != 10
        || !text.starts_with("0x")
        || !text[2..].bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("Banked response requires exactly 32 raw bits".into());
    }
    RawValue::parse(text, 32)
}
fn method(name: &str, mode: u32) -> Option<ReadMethod> {
    if !valid_name(name) {
        return None;
    }
    let bank = if name.ends_with("_usr") {
        0x10
    } else if name.ends_with("_fiq") {
        0x11
    } else if name.ends_with("_irq") {
        0x12
    } else if name.ends_with("_svc") {
        0x13
    } else if name.ends_with("_abt") {
        0x17
    } else if name.ends_with("_und") {
        0x1b
    } else {
        0x1a
    };
    let current = (bank == mode && name != "elr_hyp")
        || (bank == 0x10
            && ((name.starts_with('r') && !name.starts_with("lr") && mode != 0x11)
                || mode == 0x1f
                || (name == "lr_usr" && mode == 0x1a)));
    if current {
        Some(if name.starts_with("spsr_") {
            ReadMethod::Mrs32
        } else {
            ReadMethod::Mov32
        })
    } else if mode == 0x10 || (bank == 0x1a && mode != 0x1a) {
        None
    } else {
        Some(ReadMethod::BankedMrs32)
    }
}
impl Response {
    pub fn parse(text: &str, name: &str) -> Result<Self, String> {
        let words: Vec<_> = text.split_whitespace().collect();
        if words.len() != 12
            || [words[0], words[2], words[4], words[6], words[8], words[10]]
                != ["midr", "dscr", "dspsr", "dlr", "value", "method"]
        {
            return Err("Malformed Banked current physical evidence/value response".into());
        }
        let evidence = Evidence {
            midr: exact(words[1])?,
            dscr: exact(words[3])?,
            dspsr: exact(words[5])?,
            dlr: exact(words[7])?,
            read_method: match words[11] {
                "mov32" => ReadMethod::Mov32,
                "mrs32" => ReadMethod::Mrs32,
                "banked_mrs32" => ReadMethod::BankedMrs32,
                _ => return Err("Unknown physical Banked transfer method".into()),
            },
        };
        let midr = evidence.midr.integer()?;
        let dscr = evidence.dscr.integer()?;
        let el = (dscr >> 8) & 3;
        let mode = evidence
            .current_mode()?
            .ok_or("Banked current Debug mode cannot be proven")?;
        if super::r52_debug::current_el(midr, dscr) != Some(el as u8)
            || method(name, mode) != Some(evidence.read_method)
        {
            return Err(
                "Banked sample contradicts current Debug state identity/mode/transfer".into(),
            );
        }
        Ok(Self {
            value: exact(words[9])?,
            evidence,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registers::{Catalogue, Config, Reader, Scope};

    fn reply(dscr: u32, method: &str) -> String {
        format!(
            "midr 0x411fd134 dscr 0x{dscr:08x} dspsr 0xa2000410 dlr 0x81234568 value 0x89abcdef method {method}"
        )
    }
    #[test]
    fn physical_bank_proof_distinguishes_current_mode_from_saved_dspsr() {
        for (dscr, name, method) in [
            (0x01000200, "sp_irq", "banked_mrs32"),
            (0x01000200, "sp_hyp", "mov32"),
            (0x01000200, "spsr_hyp", "mrs32"),
            (0x01000200, "lr_usr", "mov32"),
            (0x01000000, "sp_usr", "mov32"),
        ] {
            let response = Response::parse(&reply(dscr, method), name).unwrap();
            assert_eq!(response.value.hex, "0x89abcdef");
            assert_eq!(response.evidence.dspsr.hex, "0xa2000410");
            assert_eq!(
                response.evidence.current_mode().unwrap(),
                Some(if dscr == 0x01000000 { 0x10 } else { 0x1a })
            );
            let encoded = serde_json::to_value(&response.evidence).unwrap();
            let decoded: Evidence = serde_json::from_value(encoded.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), encoded);
        }
    }
    #[test]
    fn bank_proof_rejects_legacy_missing_forged_and_contradictory_fields() {
        let normal = reply(0x01000200, "banked_mrs32");
        for invalid in [
            "0x89abcdef".into(),
            normal.clone() + " extra",
            normal.replace("0x411fd134", "0x511fd134"),
            normal.replace("0x411fd134", "0x411ed134"),
            normal.replace("0x89abcdef", "0x1"),
            normal.replace("0x01000200", "0x01000100"),
            normal.replace("banked_mrs32", "mov32"),
            normal.replace("dlr", "dspsr"),
            reply(0x01008200, "banked_mrs32"),
            reply(0x01001200, "banked_mrs32"),
            reply(0x01000240, "banked_mrs32"),
            reply(0x00000200, "banked_mrs32"),
            reply(0x01000300, "banked_mrs32"),
        ] {
            assert!(Response::parse(&invalid, "sp_irq").is_err(), "{invalid}");
        }
        for name in ["sp_irq", "spsr_hyp", "sp_hyp", "sp_mon"] {
            assert!(Response::parse(&reply(0x01000000, "mov32"), name).is_err());
        }
        assert!(Response::parse(&reply(0x01000100, "banked_mrs32"), "elr_hyp").is_err());
    }

    #[test]
    fn builtins_use_the_verified_bank_route_without_a_legacy_fallback() {
        for cpu in ["cortex-r52", "cortex-r52+"] {
            let catalogue = Catalogue::builtin(cpu).unwrap();
            let names: Vec<_> = catalogue
                .registers
                .iter()
                .filter_map(|reg| {
                    if let Reader::Banked { name } = &reg.reader {
                        assert_eq!(reg.bits, 32);
                        assert_eq!(reg.scope, Scope::Core);
                        assert!(valid_name(name));
                        Some(name)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(names.len(), 23);
            assert!(names.iter().any(|name| name.as_str() == "elr_hyp"));
        }
    }

    #[test]
    fn bank_reader_rejects_unknown_names_width_scope_and_unsafe_commands() {
        for variant in 0..4 {
            let mut catalogue = Catalogue::builtin("cortex-r52").unwrap();
            let bank = catalogue
                .registers
                .iter_mut()
                .find(|reg| reg.id == "sp_irq")
                .unwrap();
            match variant {
                0 => {
                    bank.reader = Reader::Banked {
                        name: "sp_mon".into(),
                    }
                }
                1 => bank.bits = 64,
                2 => bank.scope = Scope::Chip,
                _ => bank.read_side_effect = true,
            }
            assert!(catalogue.validate().is_err());
        }
        assert!(
            Config {
                banked_command: "get_reg".into(),
                ..Config::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            Config {
                banked_command: "aarch64 banked".into(),
                ..Config::default()
            }
            .validate()
            .is_ok()
        );
        for invalid in ["spsr_usr", "sp_mon", "SP_IRQ", "sp_irq; reset"] {
            assert!(!valid_name(invalid));
        }
    }
}
