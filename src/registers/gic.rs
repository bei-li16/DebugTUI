//! R52 GIC physical/virtual capacities and observational native routes.
use super::{RawValue, Reader};
use serde::{Deserialize, Serialize};

pub const PROTOCOL: &str = "debugtui-armv8-gic-1 external-identity current-el fresh-capacity physical-icc hyp-ich no-ack no-enable stop-on-fault";
pub const SCALARS: &[(&str, u8, u8, u8, u8)] = &[
    ("icc_ctlr", 0, 12, 12, 4),
    ("icc_sre", 0, 12, 12, 5),
    ("icc_hsre", 4, 12, 9, 5),
    ("icc_pmr", 0, 4, 6, 0),
    ("icc_rpr", 0, 12, 11, 3),
    ("icc_bpr0", 0, 12, 8, 3),
    ("icc_bpr1", 0, 12, 12, 3),
    ("icc_igrpen0", 0, 12, 12, 6),
    ("icc_igrpen1", 0, 12, 12, 7),
    ("icc_hppir0", 0, 12, 8, 2),
    ("icc_hppir1", 0, 12, 12, 2),
    ("icc_ap0r0", 0, 12, 8, 4),
    ("icc_ap0r1", 0, 12, 8, 5),
    ("icc_ap0r2", 0, 12, 8, 6),
    ("icc_ap0r3", 0, 12, 8, 7),
    ("icc_ap1r0", 0, 12, 9, 0),
    ("icc_ap1r1", 0, 12, 9, 1),
    ("icc_ap1r2", 0, 12, 9, 2),
    ("icc_ap1r3", 0, 12, 9, 3),
    ("ich_vtr", 4, 12, 11, 1),
    ("ich_hcr", 4, 12, 11, 0),
    ("ich_misr", 4, 12, 11, 2),
    ("ich_eisr", 4, 12, 11, 3),
    ("ich_elrsr", 4, 12, 11, 5),
    ("ich_vmcr", 4, 12, 11, 7),
    ("ich_ap0r0", 4, 12, 8, 0),
    ("ich_ap0r1", 4, 12, 8, 1),
    ("ich_ap0r2", 4, 12, 8, 2),
    ("ich_ap0r3", 4, 12, 8, 3),
    ("ich_ap1r0", 4, 12, 9, 0),
    ("ich_ap1r1", 4, 12, 9, 1),
    ("ich_ap1r2", 4, 12, 9, 2),
    ("ich_ap1r3", 4, 12, 9, 3),
    ("ich_lr0", 4, 12, 12, 0),
    ("ich_lr1", 4, 12, 12, 1),
    ("ich_lr2", 4, 12, 12, 2),
    ("ich_lr3", 4, 12, 12, 3),
    ("ich_lrc0", 4, 12, 14, 0),
    ("ich_lrc1", 4, 12, 14, 1),
    ("ich_lrc2", 4, 12, 14, 2),
    ("ich_lrc3", 4, 12, 14, 3),
    ("icc_iar0", 0, 12, 8, 0),
    ("icc_iar1", 0, 12, 12, 0),
];
pub fn route(reader: &Reader) -> Option<(&'static str, u16)> {
    match reader {
        Reader::Cp15 {
            cp: 15,
            op1,
            crn,
            crm,
            op2,
        } => SCALARS
            .iter()
            .find(|(_, o, n, m, p)| (o, n, m, p) == (op1, crn, crm, op2))
            .map(|(name, ..)| (*name, 32)),
        _ => None,
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum View {
    PhysicalIcc,
    HypervisorIch,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Evidence {
    pub view: View,
    pub midr: RawValue,
    pub dscr: RawValue,
    pub dspsr: RawValue,
    pub dlr: RawValue,
    pub id_pfr1: RawValue,
    pub icc_hsre: RawValue,
    pub icc_sre: RawValue,
    pub icc_ctlr: RawValue,
    pub ich_vtr: RawValue,
    pub hcr: RawValue,
    pub ich_hcr: RawValue,
    pub hstr: RawValue,
    pub read_method: super::timer::ReadMethod,
}
impl Evidence {
    fn adapted_capacity(&self) -> Option<()> {
        let words = [
            &self.midr,
            &self.dscr,
            &self.dspsr,
            &self.dlr,
            &self.id_pfr1,
            &self.icc_hsre,
            &self.icc_sre,
            &self.icc_ctlr,
            &self.ich_vtr,
            &self.hcr,
            &self.ich_hcr,
            &self.hstr,
        ];
        if self.read_method != super::timer::ReadMethod::Mrc32
            || words
                .iter()
                .any(|v| v.bits != 32 || v.integer().ok().is_none_or(|n| n > u128::from(u32::MAX)))
        {
            return None;
        }
        let midr = self.midr.integer().ok()?;
        let dscr = self.dscr.integer().ok()?;
        (midr >> 24 == 0x41
            && (midr >> 16) & 15 == 15
            && (midr >> 4) & 0xfff == 0xd13
            && (dscr >> 8) & 3 == 2
            && dscr & 0x1c0110c0 == 0
            && dscr & (1 << 24) != 0
            && self.id_pfr1.integer().ok()? >> 28 == 1
            && self.icc_hsre.integer().ok()? == 15
            && self.icc_sre.integer().ok()? == 7
            && self.icc_ctlr.integer().ok()? & !3 == 0x400
            && self.ich_vtr.integer().ok()? == 0x90180003)
            .then_some(())
    }
    pub fn physical_priority_bits(&self) -> Option<u64> {
        self.adapted_capacity()?;
        Some(((self.icc_ctlr.integer().ok()? as u64 >> 8) & 7) + 1)
    }
    pub fn virtual_priority_bits(&self) -> Option<u64> {
        self.adapted_capacity()?;
        Some(((self.ich_vtr.integer().ok()? as u64 >> 29) & 7) + 1)
    }
    pub fn virtual_preemption_bits(&self) -> Option<u64> {
        self.adapted_capacity()?;
        Some(((self.ich_vtr.integer().ok()? as u64 >> 26) & 7) + 1)
    }
    pub fn list_count(&self) -> Option<u64> {
        self.adapted_capacity()?;
        Some((self.ich_vtr.integer().ok()? as u64 & 31) + 1)
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
        return Err("GIC response requires exactly 32 raw bits".into());
    }
    RawValue::parse(text, 32)
}
impl Response {
    pub fn parse(text: &str, name: &str, bits: u16) -> Result<Self, String> {
        if bits != 32 || !SCALARS.iter().any(|(n, ..)| *n == name) {
            return Err("GIC route requires a known native 32-bit register".into());
        }
        if name.starts_with("icc_iar") || (name.contains("_ap") && !name.ends_with('0')) {
            return Err(
                "GIC response claims acknowledgement or an unimplemented R52 AP register".into(),
            );
        }
        let w: Vec<_> = text.split_whitespace().collect();
        if w.len() != 28
            || [
                w[0], w[2], w[4], w[6], w[8], w[10], w[12], w[14], w[16], w[18], w[20], w[22],
                w[24], w[26],
            ] != [
                "view", "midr", "dscr", "dspsr", "dlr", "id_pfr1", "icc_hsre", "icc_sre",
                "icc_ctlr", "ich_vtr", "hcr", "ich_hcr", "hstr", "value",
            ]
        {
            return Err("Malformed GIC physical evidence/value response".into());
        }
        let view = if name.starts_with("ich_") {
            View::HypervisorIch
        } else {
            View::PhysicalIcc
        };
        if w[1]
            != if view == View::HypervisorIch {
                "hypervisor_ich"
            } else {
                "physical_icc"
            }
        {
            return Err("GIC response contradicts the physical ICC / Hyp ICH route".into());
        }
        let evidence = Evidence {
            view,
            midr: exact(w[3])?,
            dscr: exact(w[5])?,
            dspsr: exact(w[7])?,
            dlr: exact(w[9])?,
            id_pfr1: exact(w[11])?,
            icc_hsre: exact(w[13])?,
            icc_sre: exact(w[15])?,
            icc_ctlr: exact(w[17])?,
            ich_vtr: exact(w[19])?,
            hcr: exact(w[21])?,
            ich_hcr: exact(w[23])?,
            hstr: exact(w[25])?,
            read_method: super::timer::ReadMethod::Mrc32,
        };
        if evidence.physical_priority_bits().is_none() {
            return Err("GIC sample contradicts current Debug EL, identity or R52 capacity".into());
        }
        let value = exact(w[27])?;
        let control = match name {
            "icc_ctlr" => Some(&evidence.icc_ctlr),
            "icc_sre" => Some(&evidence.icc_sre),
            "icc_hsre" => Some(&evidence.icc_hsre),
            "ich_vtr" => Some(&evidence.ich_vtr),
            "ich_hcr" => Some(&evidence.ich_hcr),
            _ => None,
        };
        if control.is_some_and(|c| c != &value) {
            return Err("GIC value contradicts its preserved capability/control proof".into());
        }
        Ok(Self { value, evidence })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::registers::{Catalogue, Config};
    const WIRE: &str = "view physical_icc midr 0x411fd134 dscr 0x01000200 dspsr 0xa2000410 dlr 0x81234568 id_pfr1 0x10111011 icc_hsre 0x0000000f icc_sre 0x00000007 icc_ctlr 0x00000403 ich_vtr 0x90180003 hcr 0x00000038 ich_hcr 0x00007c01 hstr 0x00001000 value 0xf1234567";
    #[test]
    fn gic_routes_validate_independent_opt_in_native_encodings_and_no_synthesized_64_bit_lr() {
        assert!(Config::default().gic_command.is_empty());
        let c: Config = toml::from_str("gic_command='aarch64 gic'").unwrap();
        c.validate().unwrap();
        for command in ["arm gic", "aarch64 mrc", "aarch64 gic; resume"] {
            assert!(
                Config {
                    gic_command: command.into(),
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        let catalogue = Catalogue::builtin("cortex-r52").unwrap();
        for (name, op1, crn, crm, op2) in SCALARS {
            let reg = catalogue.register(name).unwrap();
            assert_eq!(
                reg.reader,
                Reader::Cp15 {
                    cp: 15,
                    op1: *op1,
                    crn: *crn,
                    crm: *crm,
                    op2: *op2
                }
            );
            assert_eq!(route(&reg.reader), Some((*name, 32)));
            assert_eq!(reg.bits, 32);
            assert!(reg.writer.is_none());
            assert_eq!(reg.read_side_effect, name.starts_with("icc_iar"));
        }
        assert!(
            route(&Reader::Cp15_64 {
                cp: 15,
                op1: 0,
                crm: 12
            })
            .is_none()
        );
        for bank in 0..2 {
            for n in 0..4 {
                let alias = catalogue.register(&format!("icv_ap{bank}r{n}")).unwrap();
                assert!(
                    matches!(&alias.reader,Reader::Alias{source,offset:0} if source==&format!("ich_ap{bank}r{n}"))
                );
            }
        }
    }
    #[test]
    fn gic_wire_rejects_privilege_view_capacity_and_acknowledgement_guesses() {
        let e = Response::parse(WIRE, "icc_ap0r0", 32).unwrap().evidence;
        assert_eq!(e.physical_priority_bits(), Some(5));
        assert_eq!(e.virtual_preemption_bits(), Some(5));
        assert_eq!(e.list_count(), Some(4));
        for (old, new) in [
            ("0x01000200", "0x01000100"),
            ("0x01000200", "0x01010200"),
            ("0x01000200", "0x01000240"),
            ("0x411fd134", "0x411fd164"),
            ("0x10111011", "0x00111011"),
            ("0x0000000f", "0x00000007"),
            ("0x00000403", "0x00000503"),
            ("0x90180003", "0x94180003"),
            ("0x90180003", "0xb0180003"),
            ("0xf1234567", "0x1234567"),
            ("physical_icc", "hypervisor_ich"),
        ] {
            assert!(
                Response::parse(&WIRE.replace(old, new), "icc_ap0r0", 32).is_err(),
                "{new}"
            );
        }
        for name in [
            "icc_ap0r1",
            "icc_ap1r3",
            "icc_iar0",
            "icc_iar1",
            "icv_ap0r0",
        ] {
            assert!(Response::parse(WIRE, name, 32).is_err());
        }
        assert!(Response::parse(WIRE, "icc_ctlr", 32).is_err());
        assert!(Response::parse(WIRE, "icc_ap0r0", 64).is_err());
        let hyp = WIRE.replace("physical_icc", "hypervisor_ich");
        assert_eq!(
            Response::parse(&hyp, "ich_ap0r0", 32)
                .unwrap()
                .evidence
                .view,
            View::HypervisorIch
        );
        assert!(Response::parse(&hyp, "ich_ap0r1", 32).is_err());
        assert!(Response::parse(&hyp, "ich_vtr", 32).is_err());
    }
    #[test]
    fn gic_builtin_ap_banks_filter_independent_physical_and_virtual_5_6_7_capacities() {
        use super::super::Implementation;
        use std::collections::BTreeMap;
        let catalogue = Catalogue::builtin("cortex-r52").unwrap();
        for physical in [5, 6, 7] {
            for virtual_bits in [5, 6, 7] {
                let facts = BTreeMap::from([
                    ("gic.system_interface".into(), 1),
                    ("icc.physical.prebits".into(), physical),
                    ("icv.virtual.prebits".into(), virtual_bits),
                ]);
                for (prefix, expected) in [
                    (
                        "icc",
                        if physical == 5 {
                            1
                        } else if physical == 6 {
                            2
                        } else {
                            4
                        },
                    ),
                    (
                        "ich",
                        if virtual_bits == 5 {
                            1
                        } else if virtual_bits == 6 {
                            2
                        } else {
                            4
                        },
                    ),
                    (
                        "icv",
                        if virtual_bits == 5 {
                            1
                        } else if virtual_bits == 6 {
                            2
                        } else {
                            4
                        },
                    ),
                ] {
                    for bank in 0..2 {
                        for n in 0..4 {
                            let reg = catalogue
                                .register(&format!("{prefix}_ap{bank}r{n}"))
                                .unwrap();
                            let implemented = reg.implementation(&facts).0;
                            assert_eq!(
                                implemented,
                                if n < expected {
                                    Implementation::Yes
                                } else {
                                    Implementation::No
                                },
                                "{physical}/{virtual_bits} {}",
                                reg.id
                            );
                            if implemented == Implementation::No {
                                assert!(!reg.auto_read(implemented));
                            }
                        }
                    }
                }
            }
        }
        let virtual_only = BTreeMap::from([
            ("gic.system_interface".into(), 1),
            ("icv.virtual.prebits".into(), 7),
        ]);
        assert_eq!(
            catalogue
                .register("icc_ap0r0")
                .unwrap()
                .implementation(&virtual_only)
                .0,
            Implementation::Unknown
        );
        let physical_only = BTreeMap::from([
            ("gic.system_interface".into(), 1),
            ("icc.physical.prebits".into(), 7),
        ]);
        assert_eq!(
            catalogue
                .register("ich_ap0r0")
                .unwrap()
                .implementation(&physical_only)
                .0,
            Implementation::Unknown
        );
    }
}
