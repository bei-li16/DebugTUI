use super::*;
use crate::registers::display::{Format, Lane};

fn pair(first_d: u8, full: bool, raw: u128) -> PairEvidence {
    PairEvidence {
        first_d,
        mvfr0: RawValue::parse(if full { "0x10110222" } else { "0x10110021" }, 32).unwrap(),
        mvfr1: RawValue::parse(if full { "0x12111111" } else { "0x11000011" }, 32).unwrap(),
        fpexc: RawValue::parse("0x40000700", 32).unwrap(),
        raw: RawValue::from_integer(raw, 128).unwrap(),
    }
}

#[test]
fn all_aarch32_storage_views_follow_observed_d16_or_d32_capacity() {
    // Distinct lane witnesses include sign, exponent and nonzero NaN payloads.
    // Expected S/D/Q offsets come from the architecture register-bank mapping.
    let words = [0x80000000u32, 0x7f800000, 0x7fc00042, 0x7f800042];
    let raw = u128::from(words[0])
        | (u128::from(words[1]) << 32)
        | (u128::from(words[2]) << 64)
        | (u128::from(words[3]) << 96);
    for full in [false, true] {
        for first_d in (0..if full { 32 } else { 16 }).step_by(2) {
            let proof = pair(first_d, full, raw);
            for offset in 0..2u8 {
                let index = first_d + offset;
                let d = proof.view(&format!("d{index}")).unwrap();
                let expected = u64::from(words[usize::from(offset) * 2])
                    | (u64::from(words[usize::from(offset) * 2 + 1]) << 32);
                assert_eq!(d.integer().unwrap(), u128::from(expected));
                if index < 16 {
                    for half in 0..2u8 {
                        assert_eq!(
                            proof
                                .view(&format!("s{}", 2 * index + half))
                                .unwrap()
                                .integer()
                                .unwrap(),
                            u128::from(words[usize::from(offset) * 2 + usize::from(half)])
                        );
                    }
                }
            }
            let q = proof.view(&format!("q{}", first_d / 2));
            if full {
                assert_eq!(q.unwrap().integer().unwrap(), raw);
            } else {
                assert!(q.is_err());
            }
            assert!(proof.view(&format!("d{}", first_d + 2)).is_err());
        }
    }
    for invalid in [
        "s32",
        "s00",
        "S0",
        "s+0",
        "d32",
        "q16",
        "fpscr",
        "q0; resume",
    ] {
        assert!(pair(0, true, raw).view(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn malformed_pair_evidence_cannot_invent_capacity_views_or_enable() {
    let original = pair(0, true, 0x7ff00000000000428000000000000000);
    let encoded = serde_json::to_value(&original).unwrap();
    assert_eq!(
        serde_json::from_value::<PairEvidence>(encoded).unwrap(),
        original
    );
    for variant in 0..8 {
        let mut proof = original.clone();
        match variant {
            0 => proof.first_d = 1,
            1 => proof.first_d = 32,
            2 => proof.mvfr0.bits = 64,
            3 => proof.mvfr1.hex = "0x1".into(),
            4 => proof.fpexc = RawValue::parse("0x00000700", 32).unwrap(),
            5 => proof.raw.bits = 64,
            6 => proof.mvfr1 = RawValue::parse("0x11000011", 32).unwrap(),
            _ => proof.mvfr0 = RawValue::parse("0x10110322", 32).unwrap(),
        }
        assert!(proof.features().is_err(), "variant {variant}");
        assert!(proof.view("d0").is_err());
    }
    assert!(pair(16, false, 0).features().is_err());
    let low = pair(0, false, 0x7ff00000000000428000000000000000);
    assert!(!low.features().unwrap().double_precision);
    // Display is a bit interpretation; no double-precision execution is implied.
    assert_eq!(
        Format::Float { bits: 64 }
            .render(&low.view("d0").unwrap())
            .unwrap(),
        "-0.0"
    );
    assert!(low.view("q0").is_err());
}

#[test]
fn floating_storage_vectors_preserve_special_bits_and_lane_order() {
    for (raw, expected) in [
        (
            "0xff8000007f8000008000000000000000",
            "[0.0, -0.0, inf, -inf]",
        ),
        (
            "0xff800042ff8000017fc000427f800001",
            "[sNaN(0x7f800001), qNaN(0x7fc00042), -sNaN(0xff800001), -sNaN(0xff800042)]",
        ),
    ] {
        let proof = pair(
            0,
            true,
            RawValue::parse(raw, 128).unwrap().integer().unwrap(),
        );
        let before = proof.clone();
        assert_eq!(
            Format::Vector {
                lane_bits: 32,
                interpretation: Lane::Float
            }
            .render(&proof.view("q0").unwrap())
            .unwrap(),
            expected
        );
        for index in 0..4 {
            let s = proof.view(&format!("s{index}")).unwrap();
            let hex = Format::default().render(&s).unwrap();
            let _ = Format::Float { bits: 32 }.render(&s).unwrap();
            assert_eq!(s.hex, hex);
        }
        assert_eq!(proof, before);
    }
    let proof = pair(0, true, 0xfff80000000000427ff0000000000000);
    assert_eq!(
        Format::Vector {
            lane_bits: 64,
            interpretation: Lane::Float
        }
        .render(&proof.view("q0").unwrap())
        .unwrap(),
        "[inf, -qNaN(0xfff8000000000042)]"
    );
}
