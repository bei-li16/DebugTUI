//! R52 PMU native reads, current Debug-state proof and physical capacity.
use super::{RawValue, Reader};
use serde::{Deserialize, Serialize};

pub const PROTOCOL: &str = "debugtui-armv8-pmu-1 external-identity current-el fresh-count direct-index mrrc64 no-enable no-selector-write stop-on-fault";
// Fixed encodings, independent of catalogue names. No PMSWINC/read-write route.
pub const SCALARS: &[(&str, u8, u8, u8)] = &[
    ("pmcr", 9, 12, 0),
    ("pmcntenset", 9, 12, 1),
    ("pmcntenclr", 9, 12, 2),
    ("pmovsr", 9, 12, 3),
    ("pmselr", 9, 12, 5),
    ("pmceid0", 9, 12, 6),
    ("pmceid1", 9, 12, 7),
    ("pmxevtyper", 9, 13, 1),
    ("pmxevcntr", 9, 13, 2),
    ("pmuserenr", 9, 14, 0),
    ("pmintenset", 9, 14, 1),
    ("pmintenclr", 9, 14, 2),
    ("pmovsset", 9, 14, 3),
    ("pmccfiltr", 14, 15, 7),
    ("pmevcntr0", 14, 8, 0),
    ("pmevcntr1", 14, 8, 1),
    ("pmevcntr2", 14, 8, 2),
    ("pmevcntr3", 14, 8, 3),
    ("pmevtyper0", 14, 12, 0),
    ("pmevtyper1", 14, 12, 1),
    ("pmevtyper2", 14, 12, 2),
    ("pmevtyper3", 14, 12, 3),
];
pub fn route(reader: &Reader) -> Option<(&'static str, u16)> {
    match reader {
        Reader::Cp15 {
            cp: 15,
            op1: 0,
            crn,
            crm,
            op2,
        } => SCALARS
            .iter()
            .find(|(_, n, m, o)| (n, m, o) == (crn, crm, op2))
            .map(|(name, ..)| (*name, 32)),
        Reader::Cp15_64 {
            cp: 15,
            op1: 0,
            crm: 9,
        } => Some(("pmccntr", 64)),
        _ => None,
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Evidence {
    pub midr: RawValue,
    pub dscr: RawValue,
    pub dspsr: RawValue,
    pub dlr: RawValue,
    pub id_dfr0: RawValue,
    pub pmcr: RawValue,
    pub hdcr: RawValue,
    pub pmselr: RawValue,
    pub read_method: super::timer::ReadMethod,
}
impl Evidence {
    pub fn physical_count(&self) -> Option<u64> {
        let values = [
            &self.midr,
            &self.dscr,
            &self.dspsr,
            &self.dlr,
            &self.id_dfr0,
            &self.pmcr,
            &self.hdcr,
            &self.pmselr,
        ];
        if values.iter().any(|v| v.bits != 32) {
            return None;
        }
        let midr = self.midr.integer().ok()?;
        let dscr = self.dscr.integer().ok()?;
        let dfr = self.id_dfr0.integer().ok()?;
        let pmcr = self.pmcr.integer().ok()?;
        let selected = self.pmselr.integer().ok()?;
        (super::r52_debug::current_el(midr, dscr) == Some(2)
            && (dfr >> 24) & 15 == 3
            && pmcr & 0xffffff80 == 0x41132000
            && selected <= 31)
            .then_some(4)
    }
}
pub struct Response {
    pub value: RawValue,
    pub evidence: Evidence,
}
fn exact(text: &str, bits: u16) -> Result<RawValue, String> {
    if text.len() != usize::from(bits / 4) + 2
        || !text.starts_with("0x")
        || !text[2..].bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(format!("PMU response requires exactly {bits} raw bits"));
    }
    RawValue::parse(text, bits)
}
impl Response {
    pub fn parse(text: &str, name: &str, bits: u16) -> Result<Self, String> {
        let native_bits = if name == "pmccntr" {
            64
        } else if SCALARS.iter().any(|(n, ..)| *n == name) {
            32
        } else {
            return Err("Unknown PMU native register".into());
        };
        if bits != native_bits {
            return Err("PMU route requires its native register width".into());
        }
        let words: Vec<_> = text.split_whitespace().collect();
        if words.len() != 18
            || [
                words[0], words[2], words[4], words[6], words[8], words[10], words[12], words[14],
                words[16],
            ] != [
                "midr", "dscr", "dspsr", "dlr", "id_dfr0", "pmcr", "hdcr", "pmselr", "value",
            ]
        {
            return Err("Malformed PMU physical evidence/value response".into());
        }
        let evidence = Evidence {
            midr: exact(words[1], 32)?,
            dscr: exact(words[3], 32)?,
            dspsr: exact(words[5], 32)?,
            dlr: exact(words[7], 32)?,
            id_dfr0: exact(words[9], 32)?,
            pmcr: exact(words[11], 32)?,
            hdcr: exact(words[13], 32)?,
            pmselr: exact(words[15], 32)?,
            read_method: if bits == 64 {
                super::timer::ReadMethod::Mrrc64
            } else {
                super::timer::ReadMethod::Mrc32
            },
        };
        if evidence.physical_count().is_none() {
            return Err(
                "PMU sample contradicts current Debug EL, identity or physical capacity".into(),
            );
        }
        let selected = evidence.pmselr.integer()?;
        if (name == "pmxevcntr" && selected >= 4)
            || (name == "pmxevtyper" && selected >= 4 && selected != 31)
        {
            return Err("PMU selected register addresses an unimplemented event counter".into());
        }
        let value = exact(words[17], bits)?;
        if (name == "pmcr" && value != evidence.pmcr)
            || (name == "pmselr" && value != evidence.pmselr)
        {
            return Err("PMU value contradicts its preserved control/selector evidence".into());
        }
        Ok(Self { value, evidence })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registers::{Catalogue, Config};
    const WIRE: &str = "midr 0x411fd134 dscr 0x01000200 dspsr 0xa2000410 dlr 0x81234568 id_dfr0 0x03010066 pmcr 0x41132048 hdcr 0x00400e02 pmselr 0x00000003 value 0xfedcba9876543210";
    #[test]
    fn pmu_routes_validate_all_native_encodings_widths_and_independent_opt_in() {
        assert!(Config::default().pmu_command.is_empty());
        let config: Config = toml::from_str("pmu_command='aarch64 pmu'").unwrap();
        config.validate().unwrap();
        for command in ["arm pmu", "aarch64 mrc", "aarch64 pmu; resume"] {
            assert!(
                Config {
                    pmu_command: command.into(),
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        let catalogue = Catalogue::builtin("cortex-r52").unwrap();
        for (name, crn, crm, op2) in SCALARS {
            let register = catalogue.register(name).unwrap();
            assert_eq!(
                register.reader,
                Reader::Cp15 {
                    cp: 15,
                    op1: 0,
                    crn: *crn,
                    crm: *crm,
                    op2: *op2
                }
            );
            assert_eq!(route(&register.reader), Some((*name, 32)));
            assert_eq!(register.bits, 32);
            assert!(register.writer.is_none());
            assert!(!register.read_side_effect);
        }
        assert_eq!(
            route(&catalogue.register("pmccntr").unwrap().reader),
            Some(("pmccntr", 64))
        );
        for reader in [
            Reader::Cp15 {
                cp: 15,
                op1: 0,
                crn: 9,
                crm: 12,
                op2: 4,
            },
            Reader::Cp15 {
                cp: 15,
                op1: 0,
                crn: 14,
                crm: 8,
                op2: 4,
            },
            Reader::Cp15_64 {
                cp: 15,
                op1: 1,
                crm: 9,
            },
        ] {
            assert!(route(&reader).is_none());
        }
    }
    #[test]
    fn pmu_wire_rejects_virtual_counts_privilege_guesses_selection_alias_errors_and_truncation() {
        let result = Response::parse(WIRE, "pmccntr", 64).unwrap();
        assert_eq!(result.evidence.physical_count(), Some(4));
        assert_eq!(
            result.evidence.read_method,
            super::super::timer::ReadMethod::Mrrc64
        );
        for (old, new) in [
            ("0x01000200", "0x01000100"),
            ("0x01000200", "0x01008200"),
            ("0x01000200", "0x00000200"),
            ("0x01000200", "0x01001200"),
            ("0x01000200", "0x01000240"),
            ("0x411fd134", "0x411fd164"),
            ("0x03010066", "0x04010066"),
            ("0x41132048", "0x41131048"),
            ("0x00000003", "0x00000020"),
            ("0xfedcba9876543210", "0x76543210"),
        ] {
            assert!(
                Response::parse(&WIRE.replace(old, new), "pmccntr", 64).is_err(),
                "{new}"
            );
        }
        let scalar = WIRE.replace("0xfedcba9876543210", "0xf1234567");
        assert!(
            Response::parse(
                &scalar.replace("0x00000003", "0x0000001f"),
                "pmxevtyper",
                32
            )
            .is_ok()
        );
        for sel in [4, 30, 31] {
            assert!(
                Response::parse(
                    &scalar.replace("0x00000003", &format!("0x{sel:08x}")),
                    "pmxevcntr",
                    32
                )
                .is_err()
            );
        }
        assert!(Response::parse(&scalar, "pmcr", 32).is_err());
        assert!(Response::parse(&scalar, "pmselr", 32).is_err());
        assert!(Response::parse(WIRE, "pmccntr", 32).is_err());
    }
}
