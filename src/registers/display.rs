//! Pure bit presentation. Never queries a target or changes sampled bits.
use super::RawValue;
use crate::config::Radix;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lane {
    Hex,
    Unsigned,
    Signed,
    Float,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Format {
    Unsigned {
        radix: Radix,
    },
    Signed {},
    Float {
        bits: u16,
    },
    Vector {
        lane_bits: u16,
        interpretation: Lane,
    },
}
impl Default for Format {
    fn default() -> Self {
        Self::Unsigned { radix: Radix::Hex }
    }
}
impl Format {
    pub fn validate(self) -> Result<(), String> {
        match self {
            Self::Float { bits } if !matches!(bits, 32 | 64) => {
                Err("Float display requires IEEE binary32 or binary64".into())
            }
            Self::Vector {
                lane_bits,
                interpretation,
            } if !matches!(lane_bits, 8 | 16 | 32 | 64)
                || (interpretation == Lane::Float && !matches!(lane_bits, 32 | 64)) =>
            {
                Err("Unsupported vector lane format".into())
            }
            _ => Ok(()),
        }
    }
    pub fn render(self, raw: &RawValue) -> Result<String, String> {
        self.validate()?;
        let value = raw.integer()?;
        Ok(match self {
            Self::Unsigned { radix } => match radix {
                Radix::Hex => raw.hex.clone(),
                Radix::Binary => format!("0b{value:0width$b}", width = usize::from(raw.bits)),
                Radix::Octal => format!(
                    "0o{value:0width$o}",
                    width = usize::from(raw.bits.div_ceil(3))
                ),
                Radix::Decimal => value.to_string(),
            },
            Self::Signed {} => signed(value, raw.bits).to_string(),
            Self::Float { bits } => {
                if bits != raw.bits {
                    return Err("Scalar floating format must match the sampled width".into());
                }
                float(value, bits)
            }
            Self::Vector {
                lane_bits,
                interpretation,
            } => {
                if raw.bits < lane_bits || !raw.bits.is_multiple_of(lane_bits) {
                    return Err("Vector lanes must exactly cover the sampled bits".into());
                }
                let mask = (1u128 << lane_bits) - 1;
                let lanes: Vec<_> = (0..raw.bits / lane_bits)
                    .map(|index| {
                        let lane = (value >> (index * lane_bits)) & mask;
                        match interpretation {
                            Lane::Hex => {
                                format!("0x{lane:0width$x}", width = usize::from(lane_bits / 4))
                            }
                            Lane::Unsigned => lane.to_string(),
                            Lane::Signed => signed(lane, lane_bits).to_string(),
                            Lane::Float => float(lane, lane_bits),
                        }
                    })
                    .collect();
                format!("[{}]", lanes.join(", "))
            }
        })
    }
}
fn signed(value: u128, bits: u16) -> i128 {
    let shift = 128 - bits;
    ((value << shift) as i128) >> shift
}
fn float(value: u128, bits: u16) -> String {
    let (sign, exponent, fraction, quiet, is_zero, natural) = if bits == 32 {
        let raw = value as u32;
        let value = f32::from_bits(raw);
        (
            raw >> 31 != 0,
            raw & 0x7f800000 == 0x7f800000,
            u64::from(raw & 0x007fffff),
            raw & 0x00400000 != 0,
            raw & 0x7fffffff == 0,
            if value.is_finite() && value != 0.0 && !(1e-6..1e9).contains(&value.abs()) {
                format!("{value:e}")
            } else {
                value.to_string()
            },
        )
    } else {
        let raw = value as u64;
        let value = f64::from_bits(raw);
        (
            raw >> 63 != 0,
            raw & 0x7ff0000000000000 == 0x7ff0000000000000,
            raw & 0x000fffffffffffff,
            raw & 0x0008000000000000 != 0,
            raw & 0x7fffffffffffffff == 0,
            if value.is_finite() && value != 0.0 && !(1e-6..1e9).contains(&value.abs()) {
                format!("{value:e}")
            } else {
                value.to_string()
            },
        )
    };
    if exponent && fraction != 0 {
        format!(
            "{}{}NaN(0x{value:0width$x})",
            if sign { "-" } else { "" },
            if quiet { "q" } else { "s" },
            width = usize::from(bits / 4)
        )
    } else if is_zero {
        if sign { "-0.0".into() } else { "0.0".into() }
    } else {
        natural
    }
}

pub fn choices(bits: u16, field: bool) -> Vec<(Format, String)> {
    let mut result = vec![
        (Format::default(), "Hexadecimal".into()),
        (
            Format::Unsigned {
                radix: Radix::Binary,
            },
            "Binary".into(),
        ),
        (
            Format::Unsigned {
                radix: Radix::Octal,
            },
            "Octal".into(),
        ),
        (
            Format::Unsigned {
                radix: Radix::Decimal,
            },
            "Unsigned decimal".into(),
        ),
        (Format::Signed {}, "Signed decimal".into()),
    ];
    if !field && matches!(bits, 32 | 64) {
        result.push((
            Format::Float { bits },
            format!("Float{bits} · interpret bits"),
        ));
    }
    if !field {
        for lane_bits in [8, 16, 32, 64] {
            if lane_bits >= bits || !bits.is_multiple_of(lane_bits) {
                continue;
            }
            for (interpretation, label) in [
                (Lane::Hex, "hex"),
                (Lane::Unsigned, "unsigned"),
                (Lane::Signed, "signed"),
                (Lane::Float, "float"),
            ] {
                let format = Format::Vector {
                    lane_bits,
                    interpretation,
                };
                if format.validate().is_ok() {
                    result.push((
                        format,
                        format!("{} × {lane_bits}-bit {label}", bits / lane_bits),
                    ));
                }
            }
        }
    }
    result
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub open: BTreeSet<String>,
    pub fields: BTreeSet<String>,
    pub filter: u8,
    pub all_definitions: bool,
    pub query: String,
    pub formats: BTreeMap<String, Format>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            open: BTreeSet::from(["core".into()]),
            fields: BTreeSet::new(),
            filter: 0,
            all_definitions: false,
            query: String::new(),
            formats: BTreeMap::new(),
        }
    }
}
impl Preferences {
    pub fn validate(&self) -> Result<(), String> {
        if self.filter > 3
            || self.query.len() > 512
            || self.query.chars().any(char::is_control)
            || self.open.len() > 512
            || self.fields.len() > 512
            || self.formats.len() > 4096
            || self
                .open
                .iter()
                .chain(&self.fields)
                .chain(self.formats.keys())
                .any(|key| key.len() > 1024 || key.chars().any(char::is_control))
        {
            return Err("Invalid register display preferences".into());
        }
        for format in self.formats.values() {
            format.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
