//! Exact values and register write planning, shared by MI and the TUI.
//! Planning is pure: it never reads the target, assigns an expression or sends a write.
use crate::registers::{RawValue, Segment};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
mod memory;
mod variable;
pub use memory::{Config, MemoryKind, Region, SvdOverride, memory_bytes};
pub(crate) use variable::BitfieldLayout;
pub use variable::{ScalarType, variable_lvalue};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    #[default]
    Unknown,
    ReadOnly,
    ReadWrite,
    WriteOnly,
    ReadWriteOnce,
    WriteOnce,
}
impl Access {
    pub fn writable(self) -> bool {
        !matches!(self, Self::Unknown | Self::ReadOnly)
    }
    fn readable(self) -> bool {
        matches!(self, Self::ReadOnly | Self::ReadWrite | Self::ReadWriteOnce)
    }
    fn once(self) -> bool {
        matches!(self, Self::ReadWriteOnce | Self::WriteOnce)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    #[default]
    Unknown,
    Modify,
    OneToClear,
    OneToSet,
    OneToToggle,
    ZeroToClear,
    ZeroToSet,
    ZeroToToggle,
    Clear,
    Set,
}
impl Effect {
    fn neutral(self, bits: u128) -> Option<u128> {
        match self {
            Self::OneToClear | Self::OneToSet | Self::OneToToggle => Some(0),
            Self::ZeroToClear | Self::ZeroToSet | Self::ZeroToToggle => Some(bits),
            _ => None,
        }
    }
    fn result(self, before: u128, command: u128, bits: u128) -> Result<u128, String> {
        Ok(match self {
            Self::Modify => command,
            Self::OneToClear => before & !command,
            Self::OneToSet => before | command,
            Self::OneToToggle => before ^ command,
            Self::ZeroToClear => before & command,
            Self::ZeroToSet => before | (!command & bits),
            Self::ZeroToToggle => before ^ (!command & bits),
            Self::Clear => 0,
            Self::Set => bits,
            Self::Unknown => return Err("Write effects are unknown".into()),
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Constraint {
    #[default]
    None,
    Range {
        min: String,
        max: String,
    },
    Enumerated,
    WriteAsRead,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Enumeration {
    pub name: String,
    pub value: String,
    pub writable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub name: String,
    /// Logical least significant segment first (for example the two CPSR IT segments).
    pub segments: Vec<Segment>,
    pub access: Option<Access>,
    pub effect: Option<Effect>,
    pub constraint: Option<Constraint>,
    pub enums: Vec<Enumeration>,
}
impl Field {
    fn layout(&self, register_bits: u16) -> Result<(u16, u128), String> {
        if self.name.is_empty() || self.segments.is_empty() {
            return Err("Write field needs a name and bit segments".into());
        }
        let mut width = 0u16;
        let mut used = 0u128;
        for segment in &self.segments {
            if !(1..=128).contains(&segment.width)
                || segment.offset >= register_bits
                || segment
                    .offset
                    .checked_add(segment.width)
                    .is_none_or(|n| n > register_bits)
            {
                return Err(format!("Invalid write bit range for {}", self.name));
            }
            let covered = mask(segment.width) << segment.offset;
            if used & covered != 0 {
                return Err(format!("Overlapping segments in {}", self.name));
            }
            used |= covered;
            width += segment.width;
        }
        Ok((width, used))
    }
    fn extract(&self, raw: u128) -> u128 {
        let mut logical = 0;
        let mut shift = 0;
        for segment in &self.segments {
            logical |= ((raw >> segment.offset) & mask(segment.width)) << shift;
            shift += segment.width;
        }
        logical
    }
    fn insert(&self, logical: u128) -> u128 {
        let mut raw = 0;
        let mut shift = 0;
        for segment in &self.segments {
            raw |= ((logical >> shift) & mask(segment.width)) << segment.offset;
            shift += segment.width;
        }
        raw
    }
}

/// These rules require device documentation. SVD absence does not imply zero or ignored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reserved {
    #[default]
    Unknown,
    Preserve,
    Zero,
    One,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadOnlyWrite {
    #[default]
    Unknown,
    Ignored,
    Preserve,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Verification {
    None,
    /// Only ordinary readable Modify bits selected by the user.
    #[default]
    Modified,
    /// A device-specific stable mask, including effects that can safely be verified.
    Stable {
        mask: RawValue,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Register {
    pub bits: u16,
    pub access: Access,
    pub effect: Effect,
    pub constraint: Constraint,
    pub read_side_effect: bool,
    pub fields: Vec<Field>,
    pub reserved: Reserved,
    pub read_only_write: ReadOnlyWrite,
    pub verification: Verification,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OnceState {
    #[default]
    Unknown,
    /// Supplied only by a backend with evidence that the device reset the latch.
    Available,
    Consumed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Selection {
    Register,
    Field { name: String },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    #[default]
    Unsigned,
    Signed,
    Float,
    Bytes,
    Enumeration,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub kind: InputKind,
    pub text: String,
    #[serde(default)]
    pub little_endian: Option<bool>,
}
impl Input {
    pub fn raw(&self, bits: u16, enums: &[Enumeration]) -> Result<RawValue, String> {
        if !(1..=128).contains(&bits)
            || self.text.len() > 1024
            || self.text.chars().any(char::is_control)
        {
            return Err(
                "Write input requires 1..128 bits and at most 1024 printable characters".into(),
            );
        }
        let text = self.text.trim();
        match self.kind {
            InputKind::Unsigned => RawValue::parse(text, bits),
            InputKind::Signed => {
                let negative = text.starts_with('-');
                let magnitude =
                    RawValue::parse(text.strip_prefix('-').unwrap_or(text), 128)?.integer()?;
                let limit = 1u128 << (bits - 1);
                if (negative && magnitude > limit) || (!negative && magnitude >= limit) {
                    return Err(format!("Signed value exceeds {bits} bits"));
                }
                RawValue::from_integer(
                    if negative {
                        0u128.wrapping_sub(magnitude) & mask(bits)
                    } else {
                        magnitude
                    },
                    bits,
                )
            }
            InputKind::Float => {
                let value = match bits {
                    32 => {
                        let n = text.parse::<f32>().map_err(|_| "Invalid 32-bit float")?;
                        if n.is_infinite() && !explicit_infinity(text) {
                            return Err("32-bit float overflow".into());
                        }
                        u128::from(n.to_bits())
                    }
                    64 => {
                        let n = text.parse::<f64>().map_err(|_| "Invalid 64-bit float")?;
                        if n.is_infinite() && !explicit_infinity(text) {
                            return Err("64-bit float overflow".into());
                        }
                        u128::from(n.to_bits())
                    }
                    _ => return Err("Float input requires 32 or 64 bits".into()),
                };
                RawValue::from_integer(value, bits)
            }
            InputKind::Bytes => {
                if !bits.is_multiple_of(8) {
                    return Err("Byte input needs a whole number of bytes".into());
                }
                let little = self.little_endian.ok_or("Byte order must be explicit")?;
                let compact: String = text
                    .chars()
                    .filter(|c| *c == ' ' || c.is_ascii_hexdigit())
                    .collect();
                if compact != text {
                    return Err("Use hexadecimal byte pairs separated by spaces".into());
                }
                let compact = compact.replace(' ', "");
                if compact.len() != usize::from(bits / 4) {
                    return Err("Byte input does not match the write width".into());
                }
                let bytes = compact
                    .as_bytes()
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| {
                        u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16)
                            .map_err(|_| "Invalid byte".to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                RawValue::from_bytes(&bytes, bits, little)
            }
            InputKind::Enumeration => {
                let matches: Vec<_> = enums
                    .iter()
                    .filter(|e| e.name == text && e.writable)
                    .collect();
                if matches.len() != 1 {
                    return Err("Select an unambiguous writable enumeration".into());
                }
                RawValue::parse(&matches[0].value, bits)
            }
        }
    }
}
fn explicit_infinity(text: &str) -> bool {
    matches!(
        text.to_ascii_lowercase().as_str(),
        "inf" | "+inf" | "-inf" | "infinity" | "+infinity" | "-infinity"
    )
}
fn mask(bits: u16) -> u128 {
    if bits == 128 {
        u128::MAX
    } else {
        (1u128 << bits) - 1
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Prepared {
    pub selection: Selection,
    pub value: RawValue,
    pub selected_mask: RawValue,
    pub needs_fresh_read: bool,
    pub verification_mask: RawValue,
    #[serde(skip)]
    register: Register,
}
#[derive(Clone, Debug, Serialize)]
pub struct Resolved {
    pub command: RawValue,
    pub selected_mask: RawValue,
    pub expected: RawValue,
    pub verification_mask: RawValue,
}
impl Register {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=128).contains(&self.bits) {
            return Err("Unsupported write width".into());
        }
        let mut names = BTreeSet::new();
        let mut covered = 0;
        for field in self.regions() {
            let (bits, used) = field.layout(self.bits)?;
            if field.name.chars().any(char::is_control)
                || !names.insert(field.name.clone())
                || covered & used != 0
            {
                return Err("Ambiguous or overlapping write fields".into());
            }
            covered |= used;
            for item in &field.enums {
                if item.name.trim().is_empty() || item.name.chars().any(char::is_control) {
                    return Err("Invalid write enumeration name".into());
                }
                RawValue::parse(&item.value, bits)?;
            }
            if let Constraint::Range { min, max } =
                field.constraint.as_ref().unwrap_or(&self.constraint)
                && RawValue::parse(min, bits)?.integer()? > RawValue::parse(max, bits)?.integer()?
            {
                return Err("Invalid write constraint range".into());
            }
        }
        if let Verification::Stable { mask: stable } = &self.verification
            && (stable.bits != self.bits || stable.integer()? & !covered != 0)
        {
            return Err("Stable verification mask exceeds the defined write bits".into());
        }
        Ok(())
    }
    pub fn prepare(
        &self,
        selection: Selection,
        input: &Input,
        once: OnceState,
    ) -> Result<Prepared, String> {
        self.validate()?;
        let fields = self.regions();
        let mut names = BTreeSet::new();
        let mut covered = 0;
        for field in &fields {
            let (_, used) = field.layout(self.bits)?;
            if !names.insert(&field.name) || covered & used != 0 {
                return Err("Ambiguous or overlapping write fields".into());
            }
            covered |= used;
        }
        let (width, selected, enums) = match &selection {
            Selection::Register => {
                let mut selected = 0;
                for field in &fields {
                    if field.access.unwrap_or(self.access).writable() {
                        selected |= field.layout(self.bits)?.1;
                    }
                }
                if selected == 0 {
                    return Err("Register has no proven writable bits".into());
                }
                (self.bits, selected, &[][..])
            }
            Selection::Field { name } => {
                let field = fields
                    .iter()
                    .find(|f| &f.name == name)
                    .ok_or("Unknown write field")?;
                let (width, used) = field.layout(self.bits)?;
                (width, used, field.enums.as_slice())
            }
        };
        let value = input.raw(width, enums)?;
        let raw = match &selection {
            Selection::Register => value.integer()?,
            Selection::Field { name } => fields
                .iter()
                .find(|f| &f.name == name)
                .unwrap()
                .insert(value.integer()?),
        };
        let mut fresh = false;
        let mut verify = 0;
        for field in &fields {
            let (width, used) = field.layout(self.bits)?;
            let access = field.access.unwrap_or(self.access);
            let effect = field.effect.unwrap_or(self.effect);
            let constraint = field.constraint.as_ref().unwrap_or(&self.constraint);
            let selected_field = selected & used != 0;
            if access == Access::Unknown {
                return Err(format!("{}: write permission is unknown", field.name));
            }
            if access == Access::ReadOnly {
                if selected_field && matches!(selection, Selection::Field { .. }) {
                    return Err(format!("{} is read-only", field.name));
                }
                match self.read_only_write {
                    ReadOnlyWrite::Unknown => {
                        return Err(format!(
                            "{}: effect of writing read-only bits is unverified",
                            field.name
                        ));
                    }
                    ReadOnlyWrite::Ignored => {}
                    ReadOnlyWrite::Preserve => fresh = true,
                }
                continue;
            }
            if access.once() && once != OnceState::Available {
                return Err("Write-once latch availability is not proven by a reset".into());
            }
            if access.once() && !selected_field {
                return Err("Writing a neighbouring field could consume a write-once latch".into());
            }
            if effect == Effect::Unknown {
                return Err(format!("{}: write effects are unknown", field.name));
            }
            if selected_field {
                let logical = field.extract(raw);
                check_constraint(constraint, logical, width, &field.enums, None)?;
                fresh |= matches!(constraint, Constraint::WriteAsRead);
                if access.readable() && effect == Effect::Modify {
                    verify |= used;
                }
            } else if effect == Effect::Modify && access.readable() {
                fresh = true;
            } else if effect.neutral(used).is_none() {
                return Err(format!(
                    "{} cannot be preserved by a field write",
                    field.name
                ));
            }
        }
        let reserved = mask(self.bits) & !covered;
        if reserved != 0 {
            match self.reserved {
                Reserved::Unknown => return Err("Reserved-bit write policy is unknown".into()),
                Reserved::Preserve => fresh = true,
                Reserved::Zero | Reserved::One => {}
            }
        }
        match &self.verification {
            Verification::None => verify = 0,
            Verification::Modified => {}
            Verification::Stable { mask: stable } => {
                if stable.bits != self.bits || stable.integer()? & !covered != 0 {
                    return Err("Stable verification mask exceeds the defined write bits".into());
                }
                verify = stable.integer()? & selected;
                for field in &fields {
                    let (_, used) = field.layout(self.bits)?;
                    if used & verify != 0
                        && (!field.access.unwrap_or(self.access).readable()
                            || field.access.unwrap_or(self.access) == Access::ReadOnly)
                    {
                        return Err(
                            "Verification includes non-readable or read-only write bits".into()
                        );
                    }
                    fresh |=
                        used & verify != 0 && field.effect.unwrap_or(self.effect) != Effect::Modify;
                }
            }
        }
        // A read touches the whole register, including read-to-clear neighbours.
        if fresh && (!self.access.readable() || self.read_side_effect) {
            return Err("This write needs a fresh value but reading the register is unsafe".into());
        }
        if !self.access.readable() || self.read_side_effect {
            verify = 0;
        }
        Ok(Prepared {
            selection,
            value,
            selected_mask: RawValue::from_integer(selected, self.bits)?,
            needs_fresh_read: fresh,
            verification_mask: RawValue::from_integer(verify, self.bits)?,
            register: self.clone(),
        })
    }
    fn regions(&self) -> Vec<Field> {
        if self.fields.is_empty() {
            vec![Field {
                name: "register".into(),
                segments: vec![Segment {
                    offset: 0,
                    width: self.bits,
                }],
                access: None,
                effect: None,
                constraint: None,
                enums: vec![],
            }]
        } else {
            self.fields.clone()
        }
    }
}
impl Prepared {
    /// Called inside the service lease with a just-read value, never a UI cache.
    pub fn resolve(&self, fresh: Option<&RawValue>) -> Result<Resolved, String> {
        let spec = &self.register;
        let before = if let Some(value) = fresh {
            if value.bits != spec.bits {
                return Err("Fresh value width changed".into());
            }
            value.integer()?
        } else if self.needs_fresh_read {
            return Err("A fresh target read is required for this write".into());
        } else {
            0
        };
        let fields = spec.regions();
        let selected = self.selected_mask.integer()?;
        let raw = match &self.selection {
            Selection::Register => self.value.integer()?,
            Selection::Field { name } => fields
                .iter()
                .find(|f| &f.name == name)
                .ok_or("Write field disappeared")?
                .insert(self.value.integer()?),
        };
        let mut command = 0;
        let mut expected = before;
        let mut covered = 0;
        for field in &fields {
            let (width, used) = field.layout(spec.bits)?;
            covered |= used;
            let access = field.access.unwrap_or(spec.access);
            let effect = field.effect.unwrap_or(spec.effect);
            if access == Access::ReadOnly {
                if spec.read_only_write == ReadOnlyWrite::Preserve {
                    command |= before & used;
                }
                continue;
            }
            if selected & used != 0 {
                let logical = field.extract(raw);
                check_constraint(
                    field.constraint.as_ref().unwrap_or(&spec.constraint),
                    logical,
                    width,
                    &field.enums,
                    Some(field.extract(before)),
                )?;
                command |= raw & used;
                expected = (expected & !used) | effect.result(before & used, raw & used, used)?;
            } else {
                let preserved = if effect == Effect::Modify {
                    before & used
                } else {
                    effect.neutral(used).ok_or("No neutral write value")?
                };
                check_constraint(
                    field.constraint.as_ref().unwrap_or(&spec.constraint),
                    field.extract(preserved),
                    width,
                    &field.enums,
                    Some(field.extract(before)),
                )?;
                command |= preserved;
            }
        }
        let reserved = mask(spec.bits) & !covered;
        command |= match spec.reserved {
            Reserved::Preserve => before & reserved,
            Reserved::One => reserved,
            _ => 0,
        };
        Ok(Resolved {
            command: RawValue::from_integer(command, spec.bits)?,
            selected_mask: self.selected_mask.clone(),
            expected: RawValue::from_integer(expected, spec.bits)?,
            verification_mask: self.verification_mask.clone(),
        })
    }
}
impl Resolved {
    pub fn matches(&self, observed: &RawValue) -> Result<bool, String> {
        if observed.bits != self.command.bits {
            return Err("Verification width changed".into());
        }
        Ok(
            (observed.integer()? ^ self.expected.integer()?) & self.verification_mask.integer()?
                == 0,
        )
    }
}
fn check_constraint(
    constraint: &Constraint,
    value: u128,
    bits: u16,
    enums: &[Enumeration],
    fresh: Option<u128>,
) -> Result<(), String> {
    match constraint {
        Constraint::None => {}
        Constraint::Range { min, max } => {
            let min = RawValue::parse(min, bits)?.integer()?;
            let max = RawValue::parse(max, bits)?.integer()?;
            if min > max || value < min || value > max {
                return Err("Write value violates the permitted range".into());
            }
        }
        Constraint::Enumerated => {
            if !enums.iter().any(|e| {
                e.writable
                    && RawValue::parse(&e.value, bits).is_ok_and(|v| v.integer() == Ok(value))
            }) {
                return Err("Write value is not in the writable enumeration".into());
            }
        }
        Constraint::WriteAsRead => {
            if fresh.is_some_and(|v| value != v) {
                return Err("Write-as-read constraint does not match the fresh value".into());
            }
        }
    }
    Ok(())
}

/// A transport error after sending is not evidence that the hardware was unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    NotSent,
    Accepted,
    Verified,
    Mismatch,
    Partial,
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(kind: InputKind, text: &str) -> Input {
        Input {
            kind,
            text: text.into(),
            little_endian: None,
        }
    }
    fn register(bits: u16) -> Register {
        Register {
            bits,
            access: Access::ReadWrite,
            effect: Effect::Modify,
            constraint: Constraint::None,
            read_side_effect: false,
            fields: vec![],
            reserved: Reserved::Unknown,
            read_only_write: ReadOnlyWrite::Unknown,
            verification: Verification::Modified,
        }
    }
    fn field(name: &str, offset: u16, width: u16, effect: Effect) -> Field {
        Field {
            name: name.into(),
            segments: vec![Segment { offset, width }],
            access: None,
            effect: Some(effect),
            constraint: None,
            enums: vec![],
        }
    }
    fn edit(spec: &Register, name: &str, text: &str) -> Result<Prepared, String> {
        spec.prepare(
            Selection::Field { name: name.into() },
            &input(InputKind::Unsigned, text),
            OnceState::Unknown,
        )
    }
    #[test]
    fn exact_unsigned_signed_float_and_byte_input_never_crosses_json_as_a_number() {
        for bits in [32, 64, 128] {
            let max = mask(bits).to_string();
            let raw = input(InputKind::Unsigned, &max).raw(bits, &[]).unwrap();
            assert_eq!(raw.integer().unwrap(), mask(bits));
            assert!(
                serde_json::to_string(&raw)
                    .unwrap()
                    .contains("\"hex\":\"0x")
            );
            assert!(
                input(
                    InputKind::Unsigned,
                    &format!("0x1{}", "0".repeat(usize::from(bits / 4)))
                )
                .raw(bits, &[])
                .is_err()
            );
            let minimum = format!("-{}", 1u128 << (bits - 1));
            assert_eq!(
                input(InputKind::Signed, &minimum)
                    .raw(bits, &[])
                    .unwrap()
                    .integer()
                    .unwrap(),
                1u128 << (bits - 1)
            );
            assert_eq!(
                input(InputKind::Signed, "-1")
                    .raw(bits, &[])
                    .unwrap()
                    .integer()
                    .unwrap(),
                mask(bits)
            );
        }
        assert!(input(InputKind::Signed, "128").raw(8, &[]).is_err());
        assert!(input(InputKind::Signed, "-129").raw(8, &[]).is_err());
        assert_eq!(
            input(InputKind::Float, "-0").raw(32, &[]).unwrap().hex,
            "0x80000000"
        );
        assert!(
            input(InputKind::Float, "NaN")
                .raw(64, &[])
                .unwrap()
                .float()
                .unwrap()
                .contains("NaN")
        );
        assert!(input(InputKind::Float, "1e999").raw(32, &[]).is_err());
        assert!(input(InputKind::Float, "-inf").raw(64, &[]).is_ok());
        let mut bytes = input(InputKind::Bytes, "12 34 56 78");
        assert!(bytes.raw(32, &[]).is_err());
        bytes.little_endian = Some(true);
        assert_eq!(bytes.raw(32, &[]).unwrap().hex, "0x78563412");
        bytes.little_endian = Some(false);
        assert_eq!(bytes.raw(32, &[]).unwrap().hex, "0x12345678");
        for bad in ["1;reset", "1\ncontinue", "counter++", "func()", "0x100"] {
            assert!(input(InputKind::Unsigned, bad).raw(8, &[]).is_err());
        }
    }
    #[test]
    fn fields_use_a_fresh_value_and_keep_discontiguous_segments_in_logical_order() {
        let mut spec = register(32);
        spec.reserved = Reserved::Preserve;
        spec.fields.push(Field {
            segments: vec![
                Segment {
                    offset: 25,
                    width: 2,
                },
                Segment {
                    offset: 10,
                    width: 6,
                },
            ],
            ..field("IT", 0, 8, Effect::Modify)
        });
        let prepared = edit(&spec, "IT", "0xa5").unwrap();
        assert!(prepared.needs_fresh_read);
        assert!(prepared.resolve(None).is_err());
        let before = RawValue::parse("0x80000001", 32).unwrap();
        let resolved = prepared.resolve(Some(&before)).unwrap();
        assert_eq!(resolved.command.integer().unwrap(), 0x8200a401);
        assert!(resolved.matches(&resolved.command).unwrap());
        assert!(
            !resolved
                .matches(&RawValue::parse("0x8200a001", 32).unwrap())
                .unwrap()
        );
    }
    #[test]
    fn mixed_special_bits_receive_neutral_commands_instead_of_the_read_image() {
        let mut spec = register(16);
        spec.fields = vec![
            field("RW", 0, 4, Effect::Modify),
            field("W1C", 4, 4, Effect::OneToClear),
            field("W0C", 8, 4, Effect::ZeroToClear),
            field("SET", 12, 4, Effect::OneToSet),
        ];
        let fresh = RawValue::parse("0xffff", 16).unwrap();
        let rw = edit(&spec, "RW", "3").unwrap().resolve(None).unwrap();
        assert_eq!(rw.command.hex, "0x0f03");
        let w1c = edit(&spec, "W1C", "5")
            .unwrap()
            .resolve(Some(&fresh))
            .unwrap();
        assert_eq!(w1c.command.hex, "0x0f5f");
        assert_eq!(w1c.verification_mask.integer().unwrap(), 0);
        spec.verification = Verification::Stable {
            mask: RawValue::parse("0xffff", 16).unwrap(),
        };
        let w0c = edit(&spec, "W0C", "5")
            .unwrap()
            .resolve(Some(&fresh))
            .unwrap();
        assert_eq!(w0c.command.hex, "0x050f");
        assert_eq!(w0c.expected.hex, "0xf5ff");
        assert!(
            w0c.matches(&RawValue::parse("0x05a0", 16).unwrap())
                .unwrap()
        );
    }
    #[test]
    fn all_six_conditional_effects_and_unconditional_clear_set_have_defined_results() {
        for (effect, expected) in [
            (Effect::OneToClear, 0xa0),
            (Effect::OneToSet, 0xaf),
            (Effect::OneToToggle, 0xaa),
            (Effect::ZeroToClear, 5),
            (Effect::ZeroToSet, 0xf5),
            (Effect::ZeroToToggle, 0x55),
            (Effect::Clear, 0),
            (Effect::Set, 0xff),
        ] {
            let mut spec = register(8);
            spec.effect = effect;
            spec.verification = Verification::Stable {
                mask: RawValue::parse("0xff", 8).unwrap(),
            };
            let prepared = spec
                .prepare(
                    Selection::Register,
                    &input(InputKind::Unsigned, "15"),
                    OnceState::Unknown,
                )
                .unwrap();
            let resolved = prepared
                .resolve(Some(&RawValue::parse("0xa5", 8).unwrap()))
                .unwrap();
            assert_eq!(resolved.expected.integer().unwrap(), expected, "{effect:?}");
        }
    }
    #[test]
    fn unknown_reserved_readonly_once_and_unsafe_reads_are_not_guessed() {
        let mut spec = register(8);
        spec.fields = vec![field("A", 0, 4, Effect::Modify)];
        assert!(edit(&spec, "A", "1").unwrap_err().contains("Reserved"));
        spec.reserved = Reserved::Zero;
        let mut ro = field("RO", 4, 4, Effect::Modify);
        ro.access = Some(Access::ReadOnly);
        spec.fields.push(ro);
        assert!(edit(&spec, "A", "1").unwrap_err().contains("unverified"));
        spec.read_only_write = ReadOnlyWrite::Ignored;
        assert_eq!(
            edit(&spec, "A", "1")
                .unwrap()
                .resolve(None)
                .unwrap()
                .command
                .hex,
            "0x01"
        );
        assert!(edit(&spec, "RO", "1").is_err());
        spec.read_only_write = ReadOnlyWrite::Preserve;
        spec.read_side_effect = true;
        assert!(edit(&spec, "A", "1").is_err());
        let mut once = register(8);
        once.access = Access::WriteOnce;
        assert!(
            once.prepare(
                Selection::Register,
                &input(InputKind::Unsigned, "1"),
                OnceState::Unknown
            )
            .is_err()
        );
        assert!(
            once.prepare(
                Selection::Register,
                &input(InputKind::Unsigned, "1"),
                OnceState::Consumed
            )
            .is_err()
        );
        let prepared = once
            .prepare(
                Selection::Register,
                &input(InputKind::Unsigned, "1"),
                OnceState::Available,
            )
            .unwrap();
        assert!(!prepared.needs_fresh_read);
        assert_eq!(prepared.verification_mask.integer().unwrap(), 0);
    }
    #[test]
    fn field_constraints_override_parents_and_enumerations_respect_write_usage() {
        let mut spec = register(8);
        spec.constraint = Constraint::Range {
            min: "2".into(),
            max: "4".into(),
        };
        let mut a = field("A", 0, 4, Effect::Modify);
        a.constraint = Some(Constraint::Enumerated);
        a.enums = vec![
            Enumeration {
                name: "Enabled".into(),
                value: "1".into(),
                writable: true,
            },
            Enumeration {
                name: "ReadOnlyVariant".into(),
                value: "5".into(),
                writable: false,
            },
        ];
        spec.fields = vec![a, field("B", 4, 4, Effect::Modify)];
        assert!(edit(&spec, "A", "1").is_ok());
        assert!(edit(&spec, "A", "5").is_err());
        assert!(edit(&spec, "B", "1").is_err());
        assert!(
            spec.prepare(
                Selection::Field { name: "A".into() },
                &input(InputKind::Enumeration, "Enabled"),
                OnceState::Unknown
            )
            .is_ok()
        );
        spec.fields[0].constraint = Some(Constraint::WriteAsRead);
        let prepared = edit(&spec, "A", "3").unwrap();
        assert!(
            prepared
                .resolve(Some(&RawValue::parse("0x43", 8).unwrap()))
                .is_ok()
        );
        assert!(
            prepared
                .resolve(Some(&RawValue::parse("0x42", 8).unwrap()))
                .is_err()
        );
    }
    #[test]
    fn wo_and_read_clear_allow_only_writes_without_implicit_read_or_verification() {
        let mut spec = register(32);
        spec.access = Access::WriteOnly;
        let whole = spec
            .prepare(
                Selection::Register,
                &input(InputKind::Unsigned, "42"),
                OnceState::Unknown,
            )
            .unwrap();
        assert!(!whole.needs_fresh_read);
        assert_eq!(whole.verification_mask.integer().unwrap(), 0);
        spec.fields = vec![
            field("A", 0, 16, Effect::Modify),
            field("B", 16, 16, Effect::Modify),
        ];
        assert!(edit(&spec, "A", "1").is_err());
        spec.access = Access::ReadWrite;
        spec.read_side_effect = true;
        assert!(edit(&spec, "A", "1").is_err());
        assert_eq!(
            spec.prepare(
                Selection::Register,
                &input(InputKind::Unsigned, "42"),
                OnceState::Unknown
            )
            .unwrap()
            .verification_mask
            .integer()
            .unwrap(),
            0
        );
    }
}
