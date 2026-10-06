//! Bounded R52 identity/control/MPU reads with a fresh native EL2 contract.
use super::{RawValue, Reader};
use serde::{Deserialize, Serialize};

pub const COMMAND: &str = "aarch64 r52_read";
pub const PROTOCOL: &str = "debugtui-r52-core-1 external-identity current-el2 dspsr dlr fresh-capacity scratch-readback stop-on-fault";

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapacityBank {
    None,
    El1,
    El2,
}
#[derive(Clone, Debug)]
pub struct Request {
    pub name: String,
    pub bank: CapacityBank,
    pub region: Option<u8>,
}
impl Request {
    pub fn from_reader(reader: &Reader) -> Option<Self> {
        let Reader::Cp15 {
            cp: 15,
            op1,
            crn,
            crm,
            op2,
        } = *reader
        else {
            return None;
        };
        use CapacityBank::{El1, El2, None as NoBank};
        for (name, encoding, bank) in [
            ("midr", (0, 0, 0, 0), NoBank),
            ("mpidr", (0, 0, 0, 5), NoBank),
            ("sctlr", (0, 1, 0, 0), NoBank),
            ("hsctlr", (4, 1, 0, 0), NoBank),
            ("cpacr", (0, 1, 0, 2), NoBank),
            ("hcr", (4, 1, 1, 0), NoBank),
            ("mpuir", (0, 0, 0, 4), NoBank),
            ("hmpuir", (4, 0, 0, 4), NoBank),
            ("prselr", (0, 6, 2, 1), El1),
            ("hprselr", (4, 6, 2, 1), El2),
            ("hprenr", (4, 6, 1, 1), El2),
            ("mair0", (0, 10, 2, 0), El1),
            ("mair1", (0, 10, 2, 1), El1),
            ("hmair0", (4, 10, 2, 0), El2),
            ("hmair1", (4, 10, 2, 1), El2),
        ] {
            if (op1, crn, crm, op2) == encoding {
                return Some(Self {
                    name: name.into(),
                    bank,
                    region: None,
                });
            }
        }
        if crn != 6
            || !(8..=15).contains(&crm)
            || !matches!(op2, 0 | 1 | 4 | 5)
            || !matches!(op1, 0 | 1 | 4 | 5)
        {
            return None;
        }
        let region = (op1 % 4) * 16 + (crm - 8) * 2 + op2 / 4;
        (region < 24).then(|| Self {
            name: format!(
                "{}pr{}ar{region}",
                if op1 >= 4 { "h" } else { "" },
                if op2 % 2 == 0 { "b" } else { "l" }
            ),
            bank: if op1 >= 4 { El2 } else { El1 },
            region: Some(region),
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Evidence {
    pub midr: RawValue,
    pub dscr: RawValue,
    pub dspsr: RawValue,
    pub dlr: RawValue,
    pub bank: CapacityBank,
    pub capacity: RawValue,
}
impl Evidence {
    /// State decoding only; does not authorize another request or an alias.
    pub fn current_el(&self) -> Option<u8> {
        [
            &self.midr,
            &self.dscr,
            &self.dspsr,
            &self.dlr,
            &self.capacity,
        ]
        .iter()
        .all(|value| value.bits == 32 && value.integer().is_ok())
        .then(|| super::r52_debug::current_el(self.midr.integer().ok()?, self.dscr.integer().ok()?))
        .flatten()
    }
    pub fn validate(&self, request: &Request) -> Result<(), String> {
        for value in [
            &self.midr,
            &self.dscr,
            &self.dspsr,
            &self.dlr,
            &self.capacity,
        ] {
            if value.bits != 32 {
                return Err("R52 proof requires 32-bit raw fields".into());
            }
        }
        if self.current_el() != Some(2) || self.bank != request.bank {
            return Err("R52 proof contradicts current Debug EL2/identity/capacity bank".into());
        }
        let capacity = self.capacity.integer()?;
        let count = match self.bank {
            CapacityBank::None if capacity == 0 => return Ok(()),
            CapacityBank::None => return Err("Unrequested MPU capacity proof".into()),
            CapacityBank::El1 => (capacity >> 8) & 255,
            CapacityBank::El2 => capacity & 255,
        };
        if !matches!(count, 16 | 20 | 24)
            || request
                .region
                .is_some_and(|region| u128::from(region) >= count)
        {
            return Err("R52 read exceeds its fresh physical bank capacity".into());
        }
        Ok(())
    }
}
pub struct Response {
    pub value: RawValue,
    pub evidence: Evidence,
}
impl Response {
    pub fn parse(text: &str, request: &Request) -> Result<Self, String> {
        let words: Vec<_> = text.split_whitespace().collect();
        if words.len() != 14
            || [
                words[0], words[2], words[4], words[6], words[8], words[10], words[12],
            ] != ["midr", "dscr", "dspsr", "dlr", "bank", "capacity", "value"]
        {
            return Err("Malformed R52 current Debug proof/value response".into());
        }
        let exact = |raw: &str| {
            if raw.len() != 10
                || !raw.starts_with("0x")
                || !raw[2..].bytes().all(|b| b.is_ascii_hexdigit())
            {
                Err("R52 response requires exactly 32 raw bits".into())
            } else {
                RawValue::parse(raw, 32)
            }
        };
        let evidence = Evidence {
            midr: exact(words[1])?,
            dscr: exact(words[3])?,
            dspsr: exact(words[5])?,
            dlr: exact(words[7])?,
            capacity: exact(words[11])?,
            bank: match words[9] {
                "none" => CapacityBank::None,
                "el1" => CapacityBank::El1,
                "el2" => CapacityBank::El2,
                _ => return Err("Unknown R52 capacity bank".into()),
            },
        };
        evidence.validate(request)?;
        let value = exact(words[13])?;
        if request.name == "midr" && value != evidence.midr {
            return Err("CPU MIDR contradicts the external physical identity".into());
        }
        Ok(Self { value, evidence })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn r52_bounded_routes_match_all_reviewed_definitions_and_exclude_other_encodings() {
        let catalogue = crate::registers::Catalogue::builtin("cortex-r52").unwrap();
        let routes: Vec<_> = catalogue
            .registers
            .iter()
            .filter_map(|r| Request::from_reader(&r.reader).map(|route| (r, route)))
            .collect();
        assert_eq!(routes.len(), 111);
        for (r, route) in routes {
            assert_eq!(r.id, route.name);
            assert_eq!(r.bits, 32);
            assert_eq!(r.scope, crate::registers::Scope::Core);
            assert!(!r.read_side_effect);
            assert!(
                r.source
                    .as_ref()
                    .is_some_and(|s| s.number == "100026_0104_01_en")
            );
        }
        for reader in [
            Reader::Cp15 {
                cp: 15,
                op1: 0,
                crn: 12,
                crm: 12,
                op2: 0,
            },
            Reader::Cp15 {
                cp: 15,
                op1: 1,
                crn: 6,
                crm: 12,
                op2: 0,
            },
            Reader::Cp15 {
                cp: 14,
                op1: 0,
                crn: 0,
                crm: 0,
                op2: 0,
            },
            Reader::Gdb {
                name: "midr".into(),
            },
        ] {
            assert!(Request::from_reader(&reader).is_none());
        }
    }
    #[test]
    fn r52_proof_keeps_saved_user_separate_and_rejects_missing_el_or_wrong_capacity() {
        let reader = Reader::Cp15 {
            cp: 15,
            op1: 5,
            crn: 6,
            crm: 9,
            op2: 4,
        }; // EL2 region19 BAR
        let request = Request::from_reader(&reader).unwrap();
        assert_eq!(request.name, "hprbar19");
        let normal = "midr 0x411fd134 dscr 0x01050213 dspsr 0xa2000410 dlr 0x81234568 bank el2 capacity 0x00000014 value 0x20010003";
        let response = Response::parse(normal, &request).unwrap();
        assert_eq!(response.value.hex, "0x20010003");
        assert_eq!(response.evidence.dspsr.hex, "0xa2000410");
        for bad in [
            normal.replace("0x01050213", "0x01058213"),
            normal.replace("0x01050213", "0x01050113"),
            normal.replace("0x411fd134", "0x511fd134"),
            normal.replace("0x00000014", "0x00000010"),
            normal.replace("0x00000014", "0x00000000"),
            normal.replace("bank el2", "bank el1"),
            normal.replace("0x20010003", "0x1"),
            normal.to_owned() + " extra",
            "0x20010003".into(),
        ] {
            assert!(Response::parse(&bad, &request).is_err(), "{bad}");
        }
    }
}
