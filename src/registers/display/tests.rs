use super::*;

#[test]
fn preference_schema_round_trips_and_rejects_invalid_formats_and_unbounded_input() {
    let mut preferences = Preferences::default();
    preferences.formats.insert(
        "[\"q0\",null]".into(),
        Format::Vector {
            lane_bits: 32,
            interpretation: Lane::Float,
        },
    );
    assert_eq!(
        toml::from_str::<Preferences>(&toml::to_string(&preferences).unwrap()).unwrap(),
        preferences
    );
    preferences.filter = 4;
    assert!(preferences.validate().is_err());
    preferences.filter = 0;
    preferences.query = "bad\nquery".into();
    assert!(preferences.validate().is_err());
    preferences.query.clear();
    preferences
        .formats
        .insert("invalid".into(), Format::Float { bits: 128 });
    assert!(preferences.validate().is_err());
    assert!(
        serde_json::from_value::<Format>(
            serde_json::json!({"kind":"signed","execute":"monitor resume"})
        )
        .is_err()
    );
}

#[test]
fn exact_signed_integer_formats_cover_all_widths_without_changing_raw_bits() {
    for bits in 1..=128 {
        let raw = RawValue::from_integer(
            if bits == 128 {
                u128::MAX
            } else {
                (1u128 << bits) - 1
            },
            bits,
        )
        .unwrap();
        let before = raw.clone();
        assert_eq!(Format::Signed {}.render(&raw).unwrap(), "-1");
        assert_eq!(Format::default().render(&raw).unwrap(), raw.hex);
        assert_eq!(raw, before);
    }
    let raw = RawValue::parse("0x80000000000000000000000000000000", 128).unwrap();
    assert_eq!(
        Format::Signed {}.render(&raw).unwrap(),
        i128::MIN.to_string()
    );
    assert_eq!(
        Format::Unsigned {
            radix: Radix::Decimal
        }
        .render(&raw)
        .unwrap(),
        (1u128 << 127).to_string()
    );
    assert_eq!(
        Format::Unsigned {
            radix: Radix::Binary
        }
        .render(&RawValue::parse("0x1", 4).unwrap())
        .unwrap(),
        "0b0001"
    );
}

#[test]
fn scalar_float_bits_keep_negative_zero_nan_payload_sign_and_subnormals() {
    for (bits, raw, expected) in [
        (32, "0x80000000", "-0.0"),
        (64, "0x8000000000000000", "-0.0"),
        (32, "0x7f800000", "inf"),
        (64, "0xfff0000000000000", "-inf"),
        (32, "0xffc00042", "-qNaN(0xffc00042)"),
        (64, "0x7ff0000100000000", "sNaN(0x7ff0000100000000)"),
        (64, "0xfff8000000000001", "-qNaN(0xfff8000000000001)"),
    ] {
        let raw = RawValue::parse(raw, bits).unwrap();
        assert_eq!(Format::Float { bits }.render(&raw).unwrap(), expected);
    }
    for (bits, hex) in [(32, "0x00000001"), (64, "0x0000000000000001")] {
        let raw = RawValue::parse(hex, bits).unwrap();
        let text = Format::Float { bits }.render(&raw).unwrap();
        assert!(
            text.contains('e'),
            "Subnormals should remain readable in a narrow popup"
        );
        if bits == 32 {
            assert_eq!(text.parse::<f32>().unwrap().to_bits(), 1);
        } else {
            assert_eq!(text.parse::<f64>().unwrap().to_bits(), 1);
        }
    }
    assert!(
        Format::Float { bits: 64 }
            .render(&RawValue::parse("0x1", 32).unwrap())
            .is_err()
    );
}

#[test]
fn vector_lanes_are_exact_low_bits_first_with_independent_signed_and_float_views() {
    let raw = RawValue::parse("0x800000003f8000007fc0004200000001", 128).unwrap();
    assert_eq!(
        Format::Vector {
            lane_bits: 32,
            interpretation: Lane::Hex
        }
        .render(&raw)
        .unwrap(),
        "[0x00000001, 0x7fc00042, 0x3f800000, 0x80000000]"
    );
    let vector = Format::Vector {
        lane_bits: 32,
        interpretation: Lane::Float,
    }
    .render(&raw)
    .unwrap();
    assert!(vector.contains("qNaN(0x7fc00042), 1, -0.0]"));
    let raw = RawValue::parse("0x807fff00", 32).unwrap();
    assert_eq!(
        Format::Vector {
            lane_bits: 8,
            interpretation: Lane::Signed
        }
        .render(&raw)
        .unwrap(),
        "[0, -1, 127, -128]"
    );
    assert_eq!(
        Format::Vector {
            lane_bits: 8,
            interpretation: Lane::Unsigned
        }
        .render(&raw)
        .unwrap(),
        "[0, 255, 127, 128]"
    );
    assert!(
        Format::Vector {
            lane_bits: 16,
            interpretation: Lane::Float
        }
        .validate()
        .is_err()
    );
    assert!(
        Format::Vector {
            lane_bits: 16,
            interpretation: Lane::Hex
        }
        .render(&RawValue::parse("0x1", 17).unwrap())
        .is_err()
    );
    assert!(choices(128, false).iter().any(|(f, _)| *f
        == Format::Vector {
            lane_bits: 64,
            interpretation: Lane::Float
        }));
    assert!(
        choices(32, true)
            .iter()
            .all(|(f, _)| matches!(f, Format::Unsigned { .. } | Format::Signed {}))
    );
}
