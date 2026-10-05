//! R52 VFP evidence and fixed adapter wire format; no permissions inferred from CPACR.
use super::RawValue;
use serde::{Deserialize, Serialize};
mod writes;
pub use writes::{WRITE_PROTOCOL, WriteResponse, WriteView};

pub const PROTOCOL: &str = "debugtui-armv8-vfp-1 vmrs pair-readback dspsr no-enable stop-on-fault";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Control,
    Double(u8),
    Quad(u8),
}
impl Kind {
    pub fn parse(name: &str) -> Option<Self> {
        if ["fpsid", "fpscr", "mvfr0", "mvfr1", "mvfr2", "fpexc"].contains(&name) {
            return Some(Self::Control);
        }
        let (prefix, suffix) = name.split_at_checked(1)?;
        if suffix.is_empty()
            || suffix.len() > 2
            || !suffix.bytes().all(|n| n.is_ascii_digit())
            || (suffix.len() == 2 && suffix.starts_with('0'))
        {
            return None;
        }
        let index = suffix.parse::<u8>().ok()?;
        match prefix {
            "d" if index < 32 => Some(Self::Double(index)),
            "q" if index < 16 => Some(Self::Quad(index)),
            _ => None,
        }
    }
    pub fn bits(self) -> u16 {
        match self {
            Self::Control => 32,
            Self::Double(_) => 64,
            Self::Quad(_) => 128,
        }
    }
    pub fn pair(self) -> Option<u8> {
        match self {
            Self::Control => None,
            Self::Double(n) => Some(n / 2),
            Self::Quad(n) => Some(n),
        }
    }
    pub fn view(self, pair: &RawValue) -> Result<RawValue, String> {
        match self {
            Self::Double(index) => pair.slice(u16::from(index % 2) * 64, 64),
            Self::Quad(_) => pair.slice(0, 128),
            Self::Control => pair.slice(0, 32),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Features {
    pub d_registers: u64,
    pub double_precision: bool,
    pub neon: bool,
}
pub fn features(mvfr0: u64, mvfr1: u64) -> Option<Features> {
    let layout = (
        mvfr0 & 15,
        (mvfr0 >> 4) & 15,
        (mvfr0 >> 8) & 15,
        mvfr1 & 0x000fff00,
    );
    match layout {
        (1, 2, 0, 0) => Some(Features {
            d_registers: 16,
            double_precision: false,
            neon: false,
        }),
        (2, 2, 2, 0x00011100) => Some(Features {
            d_registers: 32,
            double_precision: true,
            neon: true,
        }),
        _ => None,
    }
}

pub struct Response {
    pub value: RawValue,
    pub features: Option<Features>,
    pub mvfr0: RawValue,
    pub mvfr1: RawValue,
    pub fpexc: RawValue,
}

/// Capacity and raw storage from one successful pair request. This describes
/// bit views, not permission to execute floating-point or SIMD instructions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairEvidence {
    pub first_d: u8,
    pub mvfr0: RawValue,
    pub mvfr1: RawValue,
    pub fpexc: RawValue,
    pub raw: RawValue,
}
impl PairEvidence {
    pub fn features(&self) -> Result<Features, String> {
        for raw in [&self.mvfr0, &self.mvfr1, &self.fpexc] {
            if raw.bits != 32 || exact_raw(&raw.hex, 32)? != *raw {
                return Err("VFP pair requires exact 32-bit feature evidence".into());
            }
        }
        if self.raw.bits != 128 || exact_raw(&self.raw.hex, 128)? != self.raw {
            return Err("VFP pair requires all 128 storage bits".into());
        }
        let features = features(self.mvfr0.integer()? as u64, self.mvfr1.integer()? as u64)
            .ok_or("Unadapted or contradictory VFP pair capacity")?;
        if !self.first_d.is_multiple_of(2)
            || u64::from(self.first_d) + 1 >= features.d_registers
            || self.fpexc.integer()? & (1 << 30) == 0
        {
            return Err("VFP pair contradicts capacity or enable evidence".into());
        }
        Ok(features)
    }

    /// AArch32 lane zero is at the least significant bits, independently of
    /// target memory byte order. S registers overlap only D0-D15.
    pub fn view(&self, name: &str) -> Result<RawValue, String> {
        let features = self.features()?;
        let (first_bit, bits) = if let Some(kind) = Kind::parse(name) {
            match kind {
                Kind::Double(index) if u64::from(index) < features.d_registers => {
                    (u16::from(index) * 64, 64)
                }
                Kind::Quad(index) if features.neon => (u16::from(index) * 128, 128),
                _ => return Err("Requested VFP storage view is not implemented".into()),
            }
        } else {
            let index = name
                .strip_prefix('s')
                .ok_or("Unknown VFP storage view")?
                .parse::<u8>()
                .map_err(|_| "Unknown VFP storage view")?;
            if index >= 32 || name != format!("s{index}") {
                return Err("Unknown VFP storage view".into());
            }
            (u16::from(index) * 32, 32)
        };
        let offset = first_bit
            .checked_sub(u16::from(self.first_d) * 64)
            .ok_or("Requested view belongs to another VFP pair")?;
        self.raw.slice(offset, bits)
    }
}
fn exact_raw(text: &str, bits: u16) -> Result<RawValue, String> {
    if text.len() != usize::from(bits / 4) + 2
        || !text.starts_with("0x")
        || !text[2..].bytes().all(|n| n.is_ascii_hexdigit())
    {
        return Err(format!(
            "Expected exactly {bits} raw bits in verified VFP response"
        ));
    }
    RawValue::parse(text, bits)
}
impl Response {
    pub fn pair_evidence(&self, kind: Kind) -> Option<PairEvidence> {
        kind.pair().map(|pair| PairEvidence {
            first_d: pair * 2,
            mvfr0: self.mvfr0.clone(),
            mvfr1: self.mvfr1.clone(),
            fpexc: self.fpexc.clone(),
            raw: self.value.clone(),
        })
    }
    pub fn parse(text: &str, kind: Kind) -> Result<Self, String> {
        let words: Vec<_> = text.split_whitespace().collect();
        if words.len() != 8
            || [words[0], words[2], words[4], words[6]] != ["mvfr0", "mvfr1", "fpexc", "value"]
        {
            return Err("Malformed VFP adapter evidence/value response".into());
        }
        let mvfr0 = exact_raw(words[1], 32)?;
        let mvfr1 = exact_raw(words[3], 32)?;
        let fpexc = exact_raw(words[5], 32)?;
        let features = features(mvfr0.integer()? as u64, mvfr1.integer()? as u64);
        let value = exact_raw(words[7], if kind.pair().is_some() { 128 } else { 32 })?;
        if let Some(pair) = kind.pair() {
            let features =
                features.ok_or("Unadapted or contradictory physical R52 MVFR evidence")?;
            if u64::from(pair) * 2 >= features.d_registers
                || (kind.bits() == 128 && !features.neon)
                || fpexc.integer()? & (1 << 30) == 0
            {
                return Err(
                    "VFP data contradicts its capacity, extension or enable evidence".into(),
                );
            }
        }
        Ok(Self {
            value,
            features,
            mvfr0,
            mvfr1,
            fpexc,
        })
    }
}

#[cfg(test)]
#[path = "vfp/view_tests.rs"]
mod view_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vfp_catalogues_validate_native_width_owner_aliases_and_explicit_commands() {
        use crate::registers::{Catalogue, Config, Reader, Scope};
        for cpu in ["cortex-r52", "cortex-r52+"] {
            let catalogue = Catalogue::builtin(cpu).unwrap();
            for index in 0..32 {
                assert!(
                    matches!(&catalogue.register(&format!("d{index}")).unwrap().reader,
                    Reader::Vfp {name} if name == &format!("d{index}"))
                );
                assert!(
                    matches!(&catalogue.register(&format!("s{index}")).unwrap().reader,
                    Reader::Alias {source,offset} if source == &format!("d{}",index/2) && *offset == 32*(index%2))
                );
            }
            for index in 0..16 {
                assert_eq!(catalogue.register(&format!("q{index}")).unwrap().bits, 128);
            }
            for variant in 0..4 {
                let mut invalid = catalogue.clone();
                let reg = invalid.registers.iter_mut().find(|r| r.id == "d0").unwrap();
                match variant {
                    0 => reg.bits = 32,
                    1 => reg.scope = Scope::Chip,
                    2 => {
                        reg.reader = Reader::Vfp {
                            name: "fpinst".into(),
                        }
                    }
                    _ => reg.read_side_effect = true,
                }
                assert!(invalid.validate().is_err());
            }
        }
        let old: Config = toml::from_str("cpu='cortex-r52'").unwrap();
        assert!(old.vfp_command.is_empty());
        for command in ["get_reg", "aarch64 vfp; resume", "arm vfp"] {
            assert!(
                Config {
                    vfp_command: command.into(),
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            Config {
                vfp_command: "aarch64 vfp".into(),
                ..Default::default()
            }
            .validate()
            .is_ok()
        );
    }
    #[test]
    fn vfp_capacity_depends_on_mvfr_pair_not_permission_or_enable() {
        assert_eq!(features(0x10110021, 0x11000011).unwrap().d_registers, 16);
        assert!(!features(0x10110021, 0x11000011).unwrap().neon);
        assert_eq!(features(0x10110222, 0x12111111).unwrap().d_registers, 32);
        for (a, b) in [
            (0, 0),
            (0x10110222, 0x11000011),
            (0x10110021, 0x12111111),
            (0x10110322, 0x12111111),
        ] {
            assert!(features(a, b).is_none());
        }
    }
    #[test]
    fn vfp_wire_is_exact_and_pair_views_preserve_overlapping_bits() {
        let text = "mvfr0 0x10110222 mvfr1 0x12111111 fpexc 0x40000700 value 0x7ff8000012345678800000003f800000";
        let response = Response::parse(text, Kind::Double(0)).unwrap();
        assert_eq!(
            Kind::Double(0).view(&response.value).unwrap().hex,
            "0x800000003f800000"
        );
        assert_eq!(
            Kind::Double(1).view(&response.value).unwrap().hex,
            "0x7ff8000012345678"
        );
        assert_eq!(response.value.slice(0, 32).unwrap().hex, "0x3f800000");
        assert_eq!(response.value.slice(32, 32).unwrap().hex, "0x80000000");
        for bad in [
            text.replace("0x10110222", "0x1"),
            text.replace("0x40000700", "0x00000700"),
            text.replace("mvfr1", "MVFR1"),
            text.replace("0x12111111", "0x11000011"),
        ] {
            assert!(Response::parse(&bad, Kind::Quad(0)).is_err());
        }
        for invalid in ["D0", "d32", "q16", "d00", "fpinst", "s0", "d0; reset", ""] {
            assert!(Kind::parse(invalid).is_none());
        }
    }
}
