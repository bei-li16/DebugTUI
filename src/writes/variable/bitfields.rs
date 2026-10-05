//! Layout printed by GDB from the target's debug information, never host ABI.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct BitfieldLayout {
    pub bit_offset: usize,
    pub bits: u16,
    pub declared_bytes: usize,
    pub parent_bytes: usize,
}
impl BitfieldLayout {
    pub fn parse(
        output: &str,
        field: &str,
        bits: u16,
        parent_bytes: usize,
    ) -> Result<Self, String> {
        if !(1..=64).contains(&bits) || !(1..=4096).contains(&parent_bytes) {
            return Err("Bitfield layout exceeds the bounded writer".into());
        }
        let mut depth = 0i32;
        let mut layouts = Vec::new();
        let mut total = None;
        for line in output.lines() {
            let (comment, declaration) = if let Some((comment, rest)) = line
                .trim()
                .strip_prefix("/*")
                .and_then(|s| s.split_once("*/"))
            {
                (comment.trim(), rest.trim())
            } else {
                ("", line.trim())
            };
            if let Some(size) = comment.strip_prefix("total size (bytes):")
                && depth == 1
            {
                total = size.trim().parse::<usize>().ok();
            }
            if depth == 0 && declaration.contains("type = ") {
                let header = declaration.split('{').next().unwrap_or("");
                if !header.contains("struct ") && !header.contains("class ") {
                    return Err("Bitfield parent needs a complete struct/class layout".into());
                }
            }
            if depth == 1 && declaration.ends_with(';') {
                let declaration = declaration.trim_end_matches(';').trim();
                if let Some((left, width)) = declaration.rsplit_once(':') {
                    let name = left.split_whitespace().last().unwrap_or("");
                    if name == field {
                        let (position, size) = comment
                            .split_once('|')
                            .ok_or("Missing DWARF field offset/size")?;
                        let (byte, bit) =
                            position.split_once(':').ok_or("Missing DWARF bit offset")?;
                        let byte = byte
                            .trim()
                            .parse::<usize>()
                            .map_err(|_| "Unknown DWARF byte offset")?;
                        let bit = bit
                            .trim()
                            .parse::<usize>()
                            .map_err(|_| "Unknown DWARF bit offset")?;
                        let declared_bytes = size
                            .trim()
                            .parse::<usize>()
                            .map_err(|_| "Unknown DWARF type size")?;
                        if bit >= 8
                            || !matches!(declared_bytes, 1 | 2 | 4 | 8)
                            || width.trim().parse::<u16>().ok() != Some(bits)
                        {
                            return Err("Inconsistent DWARF bitfield declaration".into());
                        }
                        let bit_offset = byte
                            .checked_mul(8)
                            .and_then(|n| n.checked_add(bit))
                            .ok_or("Bit offset overflow")?;
                        if bit_offset
                            .checked_add(usize::from(bits))
                            .is_none_or(|end| end > parent_bytes * 8)
                            || usize::from(bits) > declared_bytes * 8
                            || (bit + usize::from(bits)).div_ceil(8) > 8
                        {
                            return Err(
                                "Bitfield does not fit the parent or GDB's 64-bit write word"
                                    .into(),
                            );
                        }
                        layouts.push(Self {
                            bit_offset,
                            bits,
                            declared_bytes,
                            parent_bytes,
                        });
                    }
                }
            }
            for c in declaration.chars() {
                if c == '{' {
                    depth += 1;
                }
                if c == '}' {
                    depth -= 1;
                }
                if depth < 0 {
                    return Err("Malformed GDB layout".into());
                }
            }
        }
        if depth != 0 || total != Some(parent_bytes) || layouts.len() != 1 {
            return Err("Incomplete, inherited or ambiguous DWARF bitfield layout".into());
        }
        Ok(layouts.remove(0))
    }
    pub fn capture_bytes(&self, base: u64) -> Result<usize, String> {
        let offset = self.bit_offset / 8;
        let address = base
            .checked_add(offset as u64)
            .ok_or("Bitfield address overflow")?;
        let mut written = (self.bit_offset % 8 + usize::from(self.bits)).div_ceil(8);
        // GDB 14 value_assign can widen an aligned access to the declared type.
        if written < self.declared_bytes && address.is_multiple_of(self.declared_bytes as u64) {
            written = self.declared_bytes;
        }
        if written > 8 {
            return Err("GDB bitfield write span exceeds LONGEST".into());
        }
        Ok(self.parent_bytes.max(offset + written))
    }
    fn mapping(&self, index: usize, little: bool) -> (usize, u8, usize) {
        let physical = self.bit_offset + index;
        let byte = physical / 8;
        let bit = if little {
            physical % 8
        } else {
            7 - physical % 8
        };
        let logical = if little {
            index
        } else {
            usize::from(self.bits) - 1 - index
        };
        (byte, 1 << bit, logical)
    }
    pub fn extract(&self, bytes: &[u8], little: bool) -> Result<RawValue, String> {
        if bytes.len() < self.parent_bytes {
            return Err("Incomplete bitfield parent bytes".into());
        }
        let mut value = 0u128;
        for index in 0..usize::from(self.bits) {
            let (byte, mask, logical) = self.mapping(index, little);
            if bytes[byte] & mask != 0 {
                value |= 1 << logical;
            }
        }
        RawValue::from_integer(value, self.bits)
    }
    pub fn expected(&self, before: &[u8], raw: &RawValue, little: bool) -> Result<Vec<u8>, String> {
        self.extract(before, little)?;
        if raw.bits != self.bits {
            return Err("Bitfield value width changed".into());
        }
        let value = raw.integer()?;
        let mut bytes = before.to_vec();
        for index in 0..usize::from(self.bits) {
            let (byte, mask, logical) = self.mapping(index, little);
            bytes[byte] =
                (bytes[byte] & !mask) | if value & (1 << logical) != 0 { mask } else { 0 };
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const LAYOUT: &str = "/* offset | size */ type = struct Cells {\n/* 0 | 4 */ unsigned before;\n/* 4:0 | 4 */ unsigned low : 5;\n/* 4:5 | 4 */ int middle : 6;\n/* 5:3 | 4 */ unsigned neighbour : 7;\n/* 6:2 | 4 */ unsigned reserved : 14;\n/* 8 | 4 */ unsigned after;\n/* total size (bytes): 12 */\n}";
    #[test]
    fn actual_arm_little_and_big_data_have_the_same_dwarf_offsets() {
        let layout = BitfieldLayout::parse(LAYOUT, "middle", 6, 12).unwrap();
        for (little, word) in [
            (true, vec![0xc3, 0xaf, 0xf2, 0xaa]),
            (false, vec![0x1f, 0xd5, 0x6a, 0xbc]),
        ] {
            let mut bytes = vec![0x12, 0x34, 0x56, 0x78];
            bytes.extend(word);
            bytes.extend([0x87, 0x65, 0x43, 0x21]);
            assert_eq!(
                layout.extract(&bytes, little).unwrap().integer().unwrap(),
                62
            );
            let changed = layout
                .expected(&bytes, &RawValue::from_integer(32, 6).unwrap(), little)
                .unwrap();
            assert_eq!(
                layout.extract(&changed, little).unwrap().integer().unwrap(),
                32
            );
            for field in [("low", 5), ("neighbour", 7), ("reserved", 14)] {
                let other = BitfieldLayout::parse(LAYOUT, field.0, field.1, 12).unwrap();
                assert_eq!(
                    other.extract(&changed, little).unwrap(),
                    other.extract(&bytes, little).unwrap()
                );
            }
            assert_eq!(&changed[..4], &bytes[..4]);
            assert_eq!(&changed[8..], &bytes[8..]);
        }
    }
    #[test]
    fn incomplete_or_contradictory_layout_never_infers_field_positions() {
        for output in [
            LAYOUT.replace("4:5", "4:9"),
            LAYOUT.replace("6;", "65;"),
            LAYOUT.replace("12 */", "11 */"),
            LAYOUT.replace("/* 4:5 | 4 */", ""),
            LAYOUT.replace("struct Cells", "union Cells"),
            LAYOUT.replace(
                "/* 4:5 | 4 */ int middle : 6;",
                "/* 4:5 | 4 */ int middle : 6;\n/* 4:5 | 4 */ int middle : 6;",
            ),
        ] {
            assert!(BitfieldLayout::parse(&output, "middle", 6, 12).is_err());
        }
        let layout = BitfieldLayout::parse(LAYOUT, "middle", 6, 12).unwrap();
        assert!(layout.extract(&[0; 4], true).is_err());
        assert!(layout.capture_bytes(u64::MAX).is_err());
    }
    #[test]
    fn packed_writes_cover_the_actual_gdb_aligned_access_and_extra_neighbour_bytes() {
        let output = "/* offset | size */ type = struct P {\n/* 1:5 | 4 */ int middle : 6;\n/* total size (bytes): 3 */\n}";
        let layout = BitfieldLayout::parse(output, "middle", 6, 3).unwrap();
        assert_eq!(layout.capture_bytes(0x1000).unwrap(), 3);
        // Address 0x1004 lets GDB widen this two-byte field span to four bytes.
        assert_eq!(layout.capture_bytes(0x1003).unwrap(), 5);
        let before = [0x12, 0xc3, 0xaf, 0xaa, 0x55];
        let after = layout
            .expected(&before, &RawValue::from_integer(32, 6).unwrap(), true)
            .unwrap();
        assert_eq!(after[0], before[0]);
        assert_eq!(&after[3..], &before[3..]);
    }
}
