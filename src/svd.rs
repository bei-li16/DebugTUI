//! Chip descriptions only. Target access remains a generic GDB/MI operation.
use std::{fs, path::Path};
use svd_parser::svd::{Access, Endian};

#[derive(Debug)]
pub struct Device {
    pub name: String,
    pub little_endian: Option<bool>,
    pub peripherals: Vec<Peripheral>,
}
#[derive(Debug)]
pub struct Peripheral {
    pub name: String,
    pub address: u64,
    pub registers: Vec<Register>,
}
#[derive(Debug)]
pub struct Register {
    pub name: String,
    pub description: String,
    pub address: u64,
    pub bits: u32,
    pub access: String,
    pub readable: bool,
    pub side_effect: bool,
    pub fields: Vec<Field>,
}
#[derive(Debug)]
pub struct Field {
    pub name: String,
    pub offset: u32,
    pub width: u32,
}
impl Field {
    pub fn value(&self, value: u64) -> u64 {
        (value >> self.offset) & (u64::MAX >> (64 - self.width))
    }
}
impl Register {
    pub fn readable_width(&self) -> bool {
        self.readable
            && matches!(self.bits, 8 | 16 | 32 | 64)
            && self.address.is_multiple_of(u64::from(self.bits / 8))
    }
    pub fn auto_read(&self) -> bool {
        self.readable_width() && !self.side_effect
    }
}
impl Device {
    pub fn load(path: &Path) -> Result<Self, String> {
        let metadata = fs::metadata(path).map_err(|e| format!("SVD {}: {e}", path.display()))?;
        if metadata.len() > 32 * 1024 * 1024 {
            return Err("SVD exceeds the 32 MiB file limit".into());
        }
        let xml = fs::read_to_string(path).map_err(|e| format!("SVD {}: {e}", path.display()))?;
        Self::parse(&xml).map_err(|e| format!("SVD {}: {e}", path.display()))
    }
    pub fn parse(xml: &str) -> Result<Self, String> {
        let config = svd_parser::Config::default()
            .ignore_enums(true)
            .expand(true)
            .expand_properties(true);
        let device = svd_parser::parse_with_config(xml, &config).map_err(|e| format!("{e:#}"))?;
        if device.address_unit_bits != 8 {
            return Err("SVD requires byte-addressed memory (addressUnitBits = 8)".into());
        }
        let little_endian = device.cpu.as_ref().and_then(|cpu| match cpu.endian {
            Endian::Little => Some(true),
            Endian::Big => Some(false),
            _ => None,
        });
        let mut peripherals = Vec::new();
        for peripheral in &device.peripherals {
            let mut registers = Vec::new();
            for r in peripheral.registers() {
                let address = peripheral
                    .base_address
                    .checked_add(u64::from(r.address_offset))
                    .ok_or("SVD register address overflow")?;
                let bits = r.properties.size.unwrap_or(device.width);
                let access = r.properties.access.unwrap_or(Access::ReadWrite);
                let mut fields = Vec::new();
                let mut side_effect = r.read_action.is_some();
                for field in r.fields.iter().flatten() {
                    side_effect |= field.read_action.is_some();
                    let offset = field.bit_offset();
                    let width = field.bit_width();
                    if width == 0 || offset >= 64 || width > 64 - offset || offset + width > bits {
                        return Err(format!(
                            "Invalid bit range: {}.{}.{}",
                            peripheral.name, r.name, field.name
                        ));
                    }
                    fields.push(Field {
                        name: field.name.clone(),
                        offset,
                        width,
                    });
                }
                fields.sort_by_key(|f| f.offset);
                registers.push(Register {
                    name: r.name.clone(),
                    description: r.description.clone().unwrap_or_default(),
                    address,
                    bits,
                    access: access.as_str().into(),
                    readable: !matches!(access, Access::WriteOnly | Access::WriteOnce),
                    side_effect,
                    fields,
                });
            }
            registers.sort_by_key(|r| r.address);
            peripherals.push(Peripheral {
                name: peripheral.name.clone(),
                address: peripheral.base_address,
                registers,
            });
        }
        peripherals.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Self {
            name: device.name,
            little_endian,
            peripherals,
        })
    }
}

/// Decode exactly one register; never treat a partial read as a valid value.
pub fn decode_register(hex: &str, bits: u32, little_endian: bool) -> Result<u64, String> {
    if !matches!(bits, 8 | 16 | 32 | 64) || hex.len() != bits as usize / 4 || !hex.is_ascii() {
        return Err("Incomplete register read or unsupported width".into());
    }
    let mut value = 0u64;
    for (i, pair) in hex.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let byte = u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16)
            .map_err(|_| "Invalid register bytes")?;
        if little_endian {
            value |= u64::from(byte) << (i * 8);
        } else {
            value = (value << 8) | u64::from(byte);
        }
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stm32f429_inherits_peripherals_and_decodes_fields() {
        let device = Device::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/svd/stm32/STM32F429.svd"),
        )
        .unwrap();
        assert_eq!(device.peripherals.len(), 84);
        assert_eq!(device.little_endian, Some(true));
        let gpio = device
            .peripherals
            .iter()
            .find(|p| p.name == "GPIOB")
            .unwrap();
        assert_eq!(gpio.address, 0x40020400);
        let moder = gpio.registers.iter().find(|r| r.name == "MODER").unwrap();
        assert_eq!(moder.address, 0x40020400);
        assert_eq!(moder.bits, 32);
        assert_eq!(moder.fields.len(), 16);
        assert!(moder.auto_read());
        assert_eq!(moder.fields[0].value(0b1011), 3);
    }
    #[test]
    fn exact_width_and_byte_order() {
        assert_eq!(decode_register("12345678", 32, true).unwrap(), 0x78563412);
        assert_eq!(decode_register("12345678", 32, false).unwrap(), 0x12345678);
        assert_eq!(
            decode_register("ffffffffffffffff", 64, true).unwrap(),
            u64::MAX
        );
        assert!(decode_register("12", 32, true).is_err());
        assert!(decode_register("zz", 8, true).is_err());
        assert!(Device::parse("<invalid/>").is_err());
    }
    #[test]
    fn arrays_clusters_inheritance_and_read_side_effects() {
        let device = Device::parse(include_str!("../tests/fixtures/peripherals.svd")).unwrap();
        let port = &device.peripherals[1];
        assert_eq!(port.name, "PORT2");
        assert_eq!(port.registers.len(), 8);
        assert_eq!(port.registers[0].address, 0x40001000);
        assert!(port.registers[0].auto_read());
        assert!(!port.registers[1].readable);
        assert!(!port.registers[2].auto_read());
        assert!(!port.registers[3].auto_read());
        assert!(port.registers[2].readable_width());
        assert_eq!(port.registers[7].address, 0x40001034);
        assert_eq!(port.registers[7].bits, 16);
        assert_eq!(
            Field {
                name: "ALL".into(),
                offset: 0,
                width: 64
            }
            .value(u64::MAX),
            u64::MAX
        );
    }
}
