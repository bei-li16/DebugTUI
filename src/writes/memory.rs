//! Explicit writable address domains. Reader availability grants no write permission.
use super::*;
use crate::registers::Scope;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub regions: Vec<Region>,
    pub svd_overrides: Vec<SvdOverride>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Ram,
    Flash,
    Mmio,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub id: String,
    pub kind: MemoryKind,
    pub start: String,
    /// Exclusive end; literal addresses only, without a lossy JSON number.
    pub end: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub cores: Vec<String>,
    #[serde(default)]
    pub scope: Scope,
    #[serde(default)]
    pub little_endian: Option<bool>,
    /// RAM byte writers require evidence that byte accesses are valid here.
    #[serde(default)]
    pub byte_writable: bool,
    /// MMIO writes use exactly one declared, aligned 8/16/32-bit word operation.
    pub widths: Vec<u16>,
    /// Physical CPU targets checked on this region's TCL service; an AP is not a CPU.
    #[serde(default)]
    pub halted_targets: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SvdOverride {
    /// Fully qualified expanded peripheral.register name.
    pub register: String,
    pub reserved: Option<Reserved>,
    pub read_only_write: Option<ReadOnlyWrite>,
    pub verification: Option<Verification>,
}
fn address(text: &str) -> Result<u64, String> {
    u64::try_from(RawValue::parse(text, 64)?.integer()?)
        .map_err(|_| "Address exceeds 64 bits".into())
}
impl Region {
    pub fn bounds(&self) -> Result<(u64, u64), String> {
        let start = address(&self.start)?;
        let end = address(&self.end)?;
        if start >= end {
            return Err("Write region needs start < exclusive end".into());
        }
        Ok((start, end))
    }
    pub fn contains(&self, base: u64, count: usize) -> Result<bool, String> {
        let (start, end) = self.bounds()?;
        let limit = base
            .checked_add(count as u64)
            .ok_or("Write address overflow")?;
        Ok(count != 0 && base >= start && limit <= end)
    }
}
impl Config {
    pub fn validate(
        &self,
        channels: &[crate::config::MemoryAccess],
        cores: &[crate::config::Core],
    ) -> Result<(), String> {
        if self.regions.len() > 256 || self.svd_overrides.len() > 4096 {
            return Err("Too many write policies".into());
        }
        let mut ids = BTreeSet::new();
        for region in &self.regions {
            if !crate::devices::valid_id(&region.id)
                || !ids.insert(&region.id)
                || region.widths.is_empty()
                || region
                    .widths
                    .iter()
                    .any(|b| !matches!(b, 8 | 16 | 32 | 64 | 128))
                || region.widths.iter().collect::<BTreeSet<_>>().len() != region.widths.len()
                || (!region.channel.is_empty() && !channels.iter().any(|c| c.id == region.channel))
                || region
                    .cores
                    .iter()
                    .any(|c| !cores.iter().any(|known| &known.name == c))
                || region.cores.iter().collect::<BTreeSet<_>>().len() != region.cores.len()
                || region
                    .halted_targets
                    .iter()
                    .any(|s| s.is_empty() || s.chars().any(char::is_control))
                || region.halted_targets.iter().collect::<BTreeSet<_>>().len()
                    != region.halted_targets.len()
            {
                return Err(format!("Invalid write region {}", region.id));
            }
            region.bounds()?;
            for prior in self.regions.iter().take_while(|p| p.id != region.id) {
                let (a, b) = region.bounds()?;
                let (c, d) = prior.bounds()?;
                let owners_overlap = region.cores.is_empty()
                    || prior.cores.is_empty()
                    || region.cores.iter().any(|core| prior.cores.contains(core));
                if region.channel == prior.channel && owners_overlap && a < d && c < b {
                    return Err("Overlapping write regions are ambiguous".into());
                }
            }
        }
        let mut overrides = BTreeSet::new();
        for rule in &self.svd_overrides {
            if rule.register.is_empty()
                || rule.register.chars().any(char::is_control)
                || !overrides.insert(&rule.register)
            {
                return Err("Invalid or duplicate SVD write override".into());
            }
        }
        Ok(())
    }
    pub fn resolve(
        &self,
        base: u64,
        count: usize,
        channel: &str,
        core: &str,
    ) -> Result<&Region, String> {
        base.checked_add(count as u64)
            .ok_or("Write address overflow")?;
        let mut matches = Vec::new();
        for region in &self.regions {
            if region.channel == channel
                && (region.cores.is_empty() || region.cores.iter().any(|c| c == core))
                && region.contains(base, count)?
            {
                matches.push(region);
            }
        }
        if matches.len() != 1 {
            return Err(
                "No unambiguous declared write region covers this address, owner and channel"
                    .into(),
            );
        }
        Ok(matches[0])
    }
}

/// Byte input is address order, unlike a register's endian-dependent byte view.
pub fn memory_bytes(input: &Input, bits: u16, little: Option<bool>) -> Result<Vec<u8>, String> {
    if input.kind == InputKind::Bytes {
        if bits == 0
            || !bits.is_multiple_of(8)
            || bits > 32768
            || input.text.len() > 16384
            || input.text.chars().any(char::is_control)
        {
            return Err(
                "Memory byte input needs 1..4096 bytes and printable hexadecimal pairs".into(),
            );
        }
        let compact: String = input.text.chars().filter(|c| *c != ' ').collect();
        if !compact.is_ascii() || compact.len() != usize::from(bits / 4) {
            return Err("Byte count does not match the memory write width".into());
        }
        compact
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| {
                u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16)
                    .map_err(|_| "Invalid memory byte input".into())
            })
            .collect()
    } else {
        if !matches!(bits, 8 | 16 | 32 | 64 | 128) {
            return Err("Scalar memory input needs 8/16/32/64/128 bits".into());
        }
        input
            .raw(bits, &[])?
            .bytes(little.ok_or("Scalar memory byte order must be declared in its write region")?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> Config {
        toml::from_str("[[regions]]\nid='ram'\nkind='ram'\nstart='0x100000000'\nend='0x100000010'\nwidths=[8,32,64,128]\nbyte_writable=true\nlittle_endian=true\n").unwrap()
    }
    #[test]
    fn write_regions_check_exact_bounds_channels_owners_overlap_and_overflow() {
        let mut config = policy();
        config.validate(&[], &[]).unwrap();
        assert!(config.resolve(0x100000000, 16, "", "default").is_ok());
        assert!(config.resolve(0x10000000f, 1, "", "default").is_ok());
        for (base, count) in [
            (0xffffffff, 1),
            (0x100000010, 1),
            (0x10000000f, 2),
            (u64::MAX, 1),
            (0x100000000, 0),
        ] {
            assert!(config.resolve(base, count, "", "default").is_err());
        }
        assert!(config.resolve(0x100000000, 1, "ap", "default").is_err());
        let mut duplicate = config.regions[0].clone();
        duplicate.id = "overlap".into();
        config.regions.push(duplicate);
        assert!(config.validate(&[], &[]).is_err());
        config.regions.pop();
        config.regions[0].cores = vec!["core0".into()];
        assert!(config.resolve(0x100000000, 1, "", "core1").is_err());
    }
    #[test]
    fn memory_bytes_keep_address_order_and_scalar_values_use_declared_endianness() {
        let input = Input {
            kind: InputKind::Bytes,
            text: "12 34 56 78".into(),
            little_endian: Some(false),
        };
        assert_eq!(
            memory_bytes(&input, 32, None).unwrap(),
            [0x12, 0x34, 0x56, 0x78]
        );
        let scalar = Input {
            kind: InputKind::Unsigned,
            text: "0x12345678".into(),
            little_endian: None,
        };
        assert_eq!(
            memory_bytes(&scalar, 32, Some(true)).unwrap(),
            [0x78, 0x56, 0x34, 0x12]
        );
        assert_eq!(
            memory_bytes(&scalar, 32, Some(false)).unwrap(),
            [0x12, 0x34, 0x56, 0x78]
        );
        assert!(memory_bytes(&scalar, 32, None).is_err());
        let bytes = Input {
            text: "ff".repeat(4096),
            ..input.clone()
        };
        assert_eq!(memory_bytes(&bytes, 32768, None).unwrap().len(), 4096);
        for bad in ["11; reset", "12\n34", "x/4wx"] {
            assert!(
                memory_bytes(
                    &Input {
                        text: bad.into(),
                        ..input.clone()
                    },
                    8,
                    None
                )
                .is_err()
            );
        }
    }
}
