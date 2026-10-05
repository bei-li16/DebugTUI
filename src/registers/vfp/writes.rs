//! Fixed writer receipts independently checked against the requested raw bits.
use super::{Kind, RawValue, Response, exact_raw};
use crate::writes::Outcome;

pub const WRITE_PROTOCOL: &str =
    "debugtui-armv8-vfp-write-1 vmov raw-pair fresh-merge scratch-readback no-enable stop-on-fault";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteView {
    pub bits: u16,
    pub offset: u16,
    pub pair: u8,
    index: u8,
}
impl WriteView {
    pub fn parse(name: &str) -> Option<Self> {
        let (prefix, suffix) = name.split_at_checked(1)?;
        if suffix.is_empty()
            || suffix.len() > 2
            || !suffix.bytes().all(|n| n.is_ascii_digit())
            || (suffix.len() == 2 && suffix.starts_with('0'))
        {
            return None;
        }
        let index = suffix.parse::<u8>().ok()?;
        let (bits, pair, offset) = match prefix {
            "s" if index < 32 => (32, index / 4, u16::from(index % 4) * 32),
            "d" if index < 32 => (64, index / 2, u16::from(index % 2) * 64),
            "q" if index < 16 => (128, index, 0),
            _ => return None,
        };
        Some(Self {
            bits,
            offset,
            pair,
            index,
        })
    }
    pub fn reader_name(self) -> String {
        match self.bits {
            32 => format!("d{}", self.index / 2),
            64 => format!("d{}", self.index),
            _ => format!("q{}", self.index),
        }
    }
    pub fn view(self, pair: &RawValue) -> Result<RawValue, String> {
        pair.slice(self.offset, self.bits)
    }
    pub fn mask(self) -> u128 {
        if self.bits == 128 {
            u128::MAX
        } else {
            ((1u128 << self.bits) - 1) << self.offset
        }
    }
    pub fn merge(self, before: &RawValue, raw: &RawValue) -> Result<RawValue, String> {
        if before.bits != 128 || raw.bits != self.bits {
            return Err("VFP write/physical pair width changed".into());
        }
        RawValue::from_integer(
            (before.integer()? & !self.mask()) | (raw.integer()? << self.offset),
            128,
        )
    }
}

pub enum WriteResponse {
    NotSent(String),
    Completed {
        outcome: Outcome,
        before: RawValue,
        expected: RawValue,
        observed: RawValue,
    },
}
impl WriteResponse {
    pub fn parse(text: &str, view: WriteView, command: &RawValue) -> Result<Self, String> {
        let words: Vec<_> = text.split_whitespace().collect();
        if words.len() == 4 && words[0..3] == ["outcome", "not_sent", "reason"] {
            if ![
                "pending-register-write",
                "access-restricted",
                "feature-disabled",
                "not-implemented",
                "writer-unsupported",
            ]
            .contains(&words[3])
            {
                return Err("Unknown VFP pre-send refusal".into());
            }
            return Ok(Self::NotSent(words[3].into()));
        }
        if words.len() != 14
            || [
                words[0], words[2], words[4], words[6], words[8], words[10], words[12],
            ] != [
                "outcome", "before", "expected", "value", "mvfr0", "mvfr1", "fpexc",
            ]
        {
            return Err("Malformed VFP writer receipt; result unknown".into());
        }
        let before = exact_raw(words[3], 128)?;
        let expected = exact_raw(words[5], 128)?;
        let observed = exact_raw(words[7], 128)?;
        let kind = Kind::parse(&view.reader_name()).ok_or("Invalid VFP storage writer")?;
        Response::parse(
            &format!(
                "mvfr0 {} mvfr1 {} fpexc {} value {}",
                words[9], words[11], words[13], words[7]
            ),
            kind,
        )?;
        if expected != view.merge(&before, command)? {
            return Err(
                "VFP receipt contradicts requested raw bits or sibling preservation".into(),
            );
        }
        let outcome = match (words[1], expected == observed) {
            ("verified", true) => Outcome::Verified,
            ("mismatch", false) => Outcome::Mismatch,
            _ => return Err("VFP outcome contradicts full physical readback".into()),
        };
        Ok(Self::Completed {
            outcome,
            before,
            expected,
            observed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aliases_merge_exact_bits_and_receipts_must_prove_full_pair_and_features() {
        let before = RawValue::parse("0x8123456789abcdef7ff0123456789abc", 128).unwrap();
        for prefix in ['s', 'd', 'q'] {
            for index in 0..if prefix == 'q' { 16 } else { 32 } {
                let view = WriteView::parse(&format!("{prefix}{index}")).unwrap();
                let command = RawValue::parse("0x80000000", view.bits).unwrap();
                let expected = view.merge(&before, &command).unwrap();
                assert_eq!(view.view(&expected).unwrap(), command);
                assert_eq!(
                    (before.integer().unwrap() ^ expected.integer().unwrap()) & !view.mask(),
                    0
                );
                let wire = format!(
                    "outcome verified before {} expected {} value {} mvfr0 0x10110222 mvfr1 0x12111111 fpexc 0x40000700",
                    before.hex, expected.hex, expected.hex
                );
                assert!(matches!(
                    WriteResponse::parse(&wire, view, &command),
                    Ok(WriteResponse::Completed {
                        outcome: Outcome::Verified,
                        ..
                    })
                ));
                for invalid in [
                    wire.replace("verified", "mismatch"),
                    wire.replace("0x40000700", "0x00000700"),
                    wire.replace("0x12111111", "0x12113111"),
                    wire.replace("before", "value"),
                ] {
                    assert!(WriteResponse::parse(&invalid, view, &command).is_err());
                }
            }
        }
        for name in [
            "s32",
            "s01",
            "q16",
            "d32",
            "fpscr",
            "fpexc",
            "s0; reset",
            "",
        ] {
            assert!(WriteView::parse(name).is_none());
        }
        let view = WriteView::parse("s0").unwrap();
        let command = RawValue::parse("0x80000000", 32).unwrap();
        assert!(matches!(
            WriteResponse::parse(
                "outcome not_sent reason pending-register-write",
                view,
                &command
            ),
            Ok(WriteResponse::NotSent(_))
        ));
        assert!(
            WriteResponse::parse("outcome not_sent reason hardware-guess", view, &command).is_err()
        );
    }
}
