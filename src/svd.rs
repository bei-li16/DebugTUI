//! Chip descriptions only. Target access remains a generic GDB/MI operation.
use crate::writes;
use std::{fs, path::Path};
use svd_parser::svd::{Access, Endian, ModifiedWriteValues, Usage, WriteConstraint};

#[derive(Debug)]
pub struct Device {
    pub name: String,
    pub little_endian: Option<bool>,
    pub cpu_name: Option<String>,
    pub nvic_priority_bits: Option<u32>,
    pub interrupts: Vec<Interrupt>,
    pub peripherals: Vec<Peripheral>,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Interrupt {
    pub peripheral: String,
    pub name: String,
    pub description: String,
    pub value: u32,
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
    pub write: writes::Register,
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
            .ignore_enums(false)
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
        let mut interrupts = Vec::new();
        for peripheral in &device.peripherals {
            interrupts.extend(peripheral.interrupt.iter().map(|irq| Interrupt {
                peripheral: peripheral.name.clone(),
                name: irq.name.clone(),
                description: irq.description.clone().unwrap_or_default(),
                value: irq.value,
            }));
            let mut registers = Vec::new();
            for r in peripheral.registers() {
                let address = peripheral
                    .base_address
                    .checked_add(u64::from(r.address_offset))
                    .ok_or("SVD register address overflow")?;
                let bits = r.properties.size.unwrap_or(device.width);
                let access = r.properties.access.unwrap_or(Access::ReadWrite);
                let mut fields = Vec::new();
                let mut write_fields = Vec::new();
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
                    let mut enums = Vec::new();
                    for set in &field.enumerated_values {
                        let writable = !matches!(set.usage, Some(Usage::Read));
                        for item in &set.values {
                            if let Some(value) = item.value {
                                enums.push(writes::Enumeration {
                                    name: item.name.clone(),
                                    value: format!("0x{value:x}"),
                                    writable,
                                });
                            }
                        }
                    }
                    write_fields.push(writes::Field {
                        name: field.name.clone(),
                        segments: vec![crate::registers::Segment {
                            offset: offset as u16,
                            width: width as u16,
                        }],
                        access: field.access.map(|a| write_access(Some(a))),
                        effect: field.modified_write_values.map(write_effect),
                        constraint: field.write_constraint.map(write_constraint),
                        enums,
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
                    write: writes::Register {
                        bits: u16::try_from(bits)
                            .map_err(|_| "SVD write width exceeds 16-bit metadata")?,
                        access: write_access(r.properties.access),
                        // CMSIS-SVD specifies normal Modify semantics when this property is absent.
                        effect: r
                            .modified_write_values
                            .map(write_effect)
                            .unwrap_or(writes::Effect::Modify),
                        constraint: r.write_constraint.map(write_constraint).unwrap_or_default(),
                        read_side_effect: side_effect,
                        fields: write_fields,
                        reserved: writes::Reserved::Unknown,
                        read_only_write: writes::ReadOnlyWrite::Unknown,
                        verification: writes::Verification::Modified,
                    },
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
        interrupts.sort_by(|a, b| {
            (a.value, &a.peripheral, &a.name).cmp(&(b.value, &b.peripheral, &b.name))
        });
        Ok(Self {
            name: device.name,
            little_endian,
            cpu_name: device.cpu.as_ref().map(|cpu| cpu.name.clone()),
            nvic_priority_bits: device.cpu.as_ref().map(|cpu| cpu.nvic_priority_bits),
            interrupts,
            peripherals,
        })
    }
}

fn write_access(access: Option<Access>) -> writes::Access {
    match access {
        None => writes::Access::Unknown,
        Some(Access::ReadOnly) => writes::Access::ReadOnly,
        Some(Access::ReadWrite) => writes::Access::ReadWrite,
        Some(Access::WriteOnly) => writes::Access::WriteOnly,
        Some(Access::ReadWriteOnce) => writes::Access::ReadWriteOnce,
        Some(Access::WriteOnce) => writes::Access::WriteOnce,
    }
}
fn write_effect(effect: ModifiedWriteValues) -> writes::Effect {
    match effect {
        ModifiedWriteValues::Modify => writes::Effect::Modify,
        ModifiedWriteValues::OneToClear => writes::Effect::OneToClear,
        ModifiedWriteValues::OneToSet => writes::Effect::OneToSet,
        ModifiedWriteValues::OneToToggle => writes::Effect::OneToToggle,
        ModifiedWriteValues::ZeroToClear => writes::Effect::ZeroToClear,
        ModifiedWriteValues::ZeroToSet => writes::Effect::ZeroToSet,
        ModifiedWriteValues::ZeroToToggle => writes::Effect::ZeroToToggle,
        ModifiedWriteValues::Clear => writes::Effect::Clear,
        ModifiedWriteValues::Set => writes::Effect::Set,
    }
}
fn write_constraint(constraint: WriteConstraint) -> writes::Constraint {
    match constraint {
        WriteConstraint::WriteAsRead(true) => writes::Constraint::WriteAsRead,
        WriteConstraint::UseEnumeratedValues(true) => writes::Constraint::Enumerated,
        WriteConstraint::WriteAsRead(false) | WriteConstraint::UseEnumeratedValues(false) => {
            writes::Constraint::None
        }
        WriteConstraint::Range(range) => writes::Constraint::Range {
            min: format!("0x{:x}", range.min),
            max: format!("0x{:x}", range.max),
        },
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

    #[test]
    fn write_metadata_preserves_parent_field_overrides_enumerations_and_derived_registers() {
        let device = Device::parse(include_str!("../tests/fixtures/write-metadata.svd")).unwrap();
        assert_eq!(
            device.little_endian, None,
            "absence of CPU byte order is not proof of little endian"
        );
        let register = &device.peripherals[0].registers[0].write;
        assert_eq!(register.access, writes::Access::ReadWrite);
        assert_eq!(register.effect, writes::Effect::OneToClear);
        assert_eq!(
            register.constraint,
            writes::Constraint::Range {
                min: "0x1".into(),
                max: "0x3".into()
            }
        );
        let rw = &register.fields[0];
        assert_eq!(rw.effect, Some(writes::Effect::Modify));
        assert_eq!(rw.constraint, Some(writes::Constraint::Enumerated));
        assert_eq!(rw.enums.len(), 3);
        assert!(
            !rw.enums
                .iter()
                .find(|v| v.name == "ReadOnlyName")
                .unwrap()
                .writable
        );
        assert!(rw.enums.iter().find(|v| v.name == "On").unwrap().writable);
        assert_eq!(register.fields[1].effect, None);
        assert_eq!(register.fields[1].constraint, None);
        assert_eq!(register.fields[2].access, Some(writes::Access::ReadOnly));
        assert_eq!(
            register.fields[3].constraint,
            Some(writes::Constraint::WriteAsRead)
        );
        let inherited = &device.peripherals[0].registers[1].write;
        assert_eq!(inherited.effect, register.effect);
        assert_eq!(inherited.fields[0].enums, rw.enums);
        let wo = &device.peripherals[0].registers[2];
        assert_eq!(wo.write.access, writes::Access::WriteOnly);
        assert!(!wo.auto_read());
        assert!(device.peripherals[0].registers[3].write.read_side_effect);
    }
}
