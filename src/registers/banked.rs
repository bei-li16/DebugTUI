//! Names accepted by the pinned R52 banked adapter. Access legality is checked
//! against the physical CPSR and MIDR inside that adapter, not guessed from UI.
pub const PROTOCOL: &str =
    "debugtui-armv8-banked-1 mrs physical-readback no-mode-change stop-on-fault";

pub const NAMES: [&str; 30] = [
    "r8_usr", "r9_usr", "r10_usr", "r11_usr", "r12_usr", "sp_usr", "lr_usr", "r8_fiq", "r9_fiq",
    "r10_fiq", "r11_fiq", "r12_fiq", "sp_fiq", "lr_fiq", "spsr_fiq", "lr_irq", "sp_irq",
    "spsr_irq", "lr_svc", "sp_svc", "spsr_svc", "lr_abt", "sp_abt", "spsr_abt", "lr_und", "sp_und",
    "spsr_und", "elr_hyp", "sp_hyp", "spsr_hyp",
];

pub fn valid_name(name: &str) -> bool {
    NAMES.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registers::{Catalogue, Config, Reader, Scope};

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
