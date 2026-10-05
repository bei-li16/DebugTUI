//! R52 Timer routes and evidence from the current physical Debug state.
use super::{RawValue, Reader};
use serde::{Deserialize, Serialize};

pub const PROTOCOL: &str = "debugtui-armv8-timer-1 external-identity current-el dspsr dlr scratch-readback no-mode-change stop-on-fault";

pub fn route(reader: &Reader) -> Option<(&'static str, u16)> {
    Some(match reader {
        Reader::Cp15 {
            cp: 15,
            op1: 0,
            crn: 14,
            crm: 0,
            op2: 0,
        } => ("cntfrq", 32),
        Reader::Cp15 {
            cp: 15,
            op1: 0,
            crn: 14,
            crm: 1,
            op2: 0,
        } => ("cntkctl", 32),
        Reader::Cp15 {
            cp: 15,
            op1: 0,
            crn: 14,
            crm: 2,
            op2: 0,
        } => ("cntp_tval", 32),
        Reader::Cp15 {
            cp: 15,
            op1: 0,
            crn: 14,
            crm: 2,
            op2: 1,
        } => ("cntp_ctl", 32),
        Reader::Cp15 {
            cp: 15,
            op1: 0,
            crn: 14,
            crm: 3,
            op2: 0,
        } => ("cntv_tval", 32),
        Reader::Cp15 {
            cp: 15,
            op1: 0,
            crn: 14,
            crm: 3,
            op2: 1,
        } => ("cntv_ctl", 32),
        Reader::Cp15 {
            cp: 15,
            op1: 4,
            crn: 14,
            crm: 1,
            op2: 0,
        } => ("cnthctl", 32),
        Reader::Cp15 {
            cp: 15,
            op1: 4,
            crn: 14,
            crm: 2,
            op2: 0,
        } => ("cnthp_tval", 32),
        Reader::Cp15 {
            cp: 15,
            op1: 4,
            crn: 14,
            crm: 2,
            op2: 1,
        } => ("cnthp_ctl", 32),
        Reader::Cp15_64 {
            cp: 15,
            op1: 0,
            crm: 14,
        } => ("cntpct", 64),
        Reader::Cp15_64 {
            cp: 15,
            op1: 1,
            crm: 14,
        } => ("cntvct", 64),
        Reader::Cp15_64 {
            cp: 15,
            op1: 2,
            crm: 14,
        } => ("cntp_cval", 64),
        Reader::Cp15_64 {
            cp: 15,
            op1: 3,
            crm: 14,
        } => ("cntv_cval", 64),
        Reader::Cp15_64 {
            cp: 15,
            op1: 4,
            crm: 14,
        } => ("cntvoff", 64),
        Reader::Cp15_64 {
            cp: 15,
            op1: 6,
            crm: 14,
        } => ("cnthp_cval", 64),
        _ => return None,
    })
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Evidence {
    pub midr: RawValue,
    pub dscr: RawValue,
    pub dspsr: RawValue,
    pub dlr: RawValue,
    #[serde(default, skip_serializing_if = "ReadMethod::is_unknown")]
    pub read_method: ReadMethod,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReadMethod {
    #[default]
    Unknown,
    Mrc32,
    Mrrc64,
}
impl ReadMethod {
    fn is_unknown(&self) -> bool {
        *self == Self::Unknown
    }
}
pub struct Response {
    pub value: RawValue,
    pub evidence: Evidence,
}
fn exact(text: &str, bits: u16) -> Result<RawValue, String> {
    if text.len() != usize::from(bits / 4) + 2
        || !text.starts_with("0x")
        || !text[2..].bytes().all(|n| n.is_ascii_hexdigit())
    {
        return Err(format!("Timer response requires exactly {bits} raw bits"));
    }
    RawValue::parse(text, bits)
}
impl Response {
    pub fn parse(text: &str, name: &str, bits: u16) -> Result<Self, String> {
        let native_bits = match name {
            "cntfrq" | "cntkctl" | "cntp_tval" | "cntp_ctl" | "cntv_tval" | "cntv_ctl"
            | "cnthctl" | "cnthp_tval" | "cnthp_ctl" => 32,
            "cntpct" | "cntvct" | "cntp_cval" | "cntv_cval" | "cntvoff" | "cnthp_cval" => 64,
            _ => return Err("Unknown Timer register in physical response".into()),
        };
        if bits != native_bits {
            return Err("Timer response requires its native register width".into());
        }
        let words: Vec<_> = text.split_whitespace().collect();
        if words.len() != 10
            || [words[0], words[2], words[4], words[6], words[8]]
                != ["midr", "dscr", "dspsr", "dlr", "value"]
        {
            return Err("Malformed Timer physical evidence/value response".into());
        }
        let evidence = Evidence {
            midr: exact(words[1], 32)?,
            dscr: exact(words[3], 32)?,
            dspsr: exact(words[5], 32)?,
            dlr: exact(words[7], 32)?,
            read_method: if bits == 64 {
                ReadMethod::Mrrc64
            } else {
                ReadMethod::Mrc32
            },
        };
        let midr = evidence.midr.integer()?;
        let dscr = evidence.dscr.integer()?;
        let el = (dscr >> 8) & 3;
        let hyp = [
            "cnthctl",
            "cnthp_tval",
            "cnthp_ctl",
            "cntvoff",
            "cnthp_cval",
        ]
        .contains(&name);
        let physical = ["cntpct", "cntp_tval", "cntp_ctl", "cntp_cval"].contains(&name);
        if midr >> 24 != 0x41
            || (midr >> 16) & 15 != 15
            || (midr >> 4) & 0xfff != 0xd13
            || el == 0
            || el > 2
            || dscr & 0x1c0000c0 != 0
            || dscr & (1 << 24) == 0
            || dscr & (1 << (10 + el)) != 0
            || (el == 2 && dscr & (1 << 16) != 0)
            || (el == 1 && (hyp || physical))
        {
            return Err("Timer sample contradicts current Debug state identity/permissions".into());
        }
        Ok(Self {
            value: exact(words[9], bits)?,
            evidence,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timer_adapter_routes_follow_encodings_and_require_independent_opt_in() {
        use crate::registers::{Catalogue, Config};
        let old: Config = toml::from_str("").unwrap();
        assert!(old.timer_command.is_empty());
        for command in ["arm timer", "aarch64 timer; reset", "aarch64 mrrc"] {
            assert!(
                Config {
                    timer_command: command.into(),
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        let config: Config = toml::from_str("timer_command='aarch64 timer'").unwrap();
        config.validate().unwrap();
        assert!(config.cp15_command.is_empty() && config.cp15_64_command.is_empty());
        let encoded = toml::to_string(&config).unwrap();
        assert_eq!(
            toml::from_str::<Config>(&encoded).unwrap().timer_command,
            config.timer_command
        );
        let catalogue = Catalogue::builtin("cortex-r52").unwrap();
        let timers: Vec<_> = catalogue
            .registers
            .iter()
            .filter(|r| r.group == "timer")
            .collect();
        assert_eq!(timers.len(), 15);
        for r in timers {
            assert_eq!(route(&r.reader), Some((r.id.as_str(), r.bits)));
        }
        assert!(route(&catalogue.register("midr").unwrap().reader).is_none());
        assert!(
            route(&Reader::Cp15_64 {
                cp: 15,
                op1: 5,
                crm: 14
            })
            .is_none()
        );
    }
    #[test]
    fn timer_wire_rejects_truncation_and_unproven_current_el_permissions() {
        let text = "midr 0x411fd134 dscr 0x01000200 dspsr 0xa2000410 dlr 0x81234568 value 0xfedcba9876543210";
        let r = Response::parse(text, "cntpct", 64).unwrap();
        assert_eq!(r.value.hex, "0xfedcba9876543210");
        assert_eq!(r.evidence.dspsr.hex, "0xa2000410"); // stopped User is not current Debug EL.
        for bad in [
            text.replace("0xfedcba9876543210", "0x76543210"),
            text.replace("0x411fd134", "0x411fd143"),
            text.replace("0x01000200", "0x01000000"),
            text.replace("0x01000200", "0x01010200"),
            text.replace("0x01000200", "0x01001200"),
            text.replace("0x01000200", "0x01000240"),
            format!("{text} extra"),
            text.replace("dscr", "DSCR"),
        ] {
            assert!(Response::parse(&bad, "cntpct", 64).is_err(), "{bad}");
        }
        for dscr in ["0x01000100", "0x01010100"] {
            let guest = text.replace("0x01000200", dscr);
            assert!(Response::parse(&guest, "cntvct", 64).is_ok());
            assert!(Response::parse(&guest, "cntpct", 64).is_err());
            assert!(Response::parse(&guest, "cntvoff", 64).is_err());
        }
        assert!(Response::parse(text, "cntpct", 32).is_err());
    }

    #[test]
    fn timer_samples_keep_extreme_u64_values_native_reads_and_legacy_evidence() {
        let prefix = "midr 0x411fd134 dscr 0x01000200 dspsr 0xa2000410 dlr 0x81234568 value ";
        for name in [
            "cntpct",
            "cntvct",
            "cntp_cval",
            "cntv_cval",
            "cntvoff",
            "cnthp_cval",
        ] {
            for hex in [
                "0x0000000000000000",
                "0x00000000ffffffff",
                "0x0000000100000000",
                "0x7fffffffffffffff",
                "0x8000000000000000",
                "0xffffffff00000000",
                "0xffffffffffffffff",
            ] {
                let response = Response::parse(&format!("{prefix}{hex}"), name, 64).unwrap();
                assert_eq!(response.value.hex, hex);
                assert_eq!(response.evidence.read_method, ReadMethod::Mrrc64);
                assert_eq!(
                    response.value.integer().unwrap(),
                    u128::from_str_radix(&hex[2..], 16).unwrap()
                );
            }
        }
        let response = Response::parse(&format!("{prefix}0xffffffff"), "cntp_tval", 32).unwrap();
        assert_eq!(response.evidence.read_method, ReadMethod::Mrc32);
        assert!(Response::parse(&format!("{prefix}0x00000001"), "cntpct", 32).is_err());
        assert!(Response::parse(&format!("{prefix}0x0000000000000001"), "cntfrq", 64).is_err());
        assert!(Response::parse(&format!("{prefix}0x0000000000000001"), "unknown", 64).is_err());
        let mut old = serde_json::to_value(response.evidence).unwrap();
        old.as_object_mut().unwrap().remove("read_method");
        let decoded: Evidence = serde_json::from_value(old.clone()).unwrap();
        assert_eq!(decoded.read_method, ReadMethod::Unknown);
        assert_eq!(serde_json::to_value(decoded).unwrap(), old);
    }
}
