//! Architecture register descriptions. Access routes and implementation facts are
//! independent: a name in a catalogue never proves hardware or reader support.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub const MAX_CATALOGUE_BYTES: u64 = 4 * 1024 * 1024;
pub const OPENOCD_ADAPTER_PROTOCOL: &str =
    "debugtui-armv8-1 mrrc isb scratch-readback stop-on-fault";
pub mod banked;
pub mod capabilities;
pub mod display;
pub mod mpu;
pub mod selector;
pub mod vfp;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub cpu: String,
    pub catalogue: PathBuf,
    pub topology: Topology,
    /// Explicitly supplied capability facts; their source is shown as configuration.
    pub facts: BTreeMap<String, u64>,
    pub components: BTreeMap<String, Component>,
    pub tcl_endpoint: String,
    pub targets: BTreeMap<String, String>,
    /// Explicit backend command, verified for the configured OpenOCD build.
    pub cp15_command: String,
    /// Genuine MRRC from the pinned, explicitly selected ARMv8 adapter.
    pub cp15_64_command: String,
    /// State-preserving R52 banked MRS adapter; no legacy get_reg fallback.
    pub banked_command: String,
    /// Explicit adapter with physical VFP enable, capacity and scratch checks.
    pub vfp_command: String,
    /// Independent opt-in writer. Reader availability never enables data writes.
    pub vfp_write_command: String,
    /// Opt-in MCR used only for adapted, saved/restored selector transactions.
    pub selector_command: String,
    /// Genuine ISB; empty retains the guarded legacy CP15ISB route.
    pub isb_command: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub base: u64,
    pub channel: String,
    pub little_endian: bool,
}

impl Config {
    pub fn load(&self) -> Result<Option<(Catalogue, String)>, String> {
        if !self.catalogue.as_os_str().is_empty() {
            return Ok(Some((
                Catalogue::load(&self.catalogue)?,
                format!("file:{}", self.catalogue.display()),
            )));
        }
        if self.cpu.is_empty() {
            return Ok(None);
        }
        if !crate::devices::valid_id(&self.cpu.replace('+', "plus")) {
            return Err("CPU catalogue name must be a simple identifier".into());
        }
        let user = crate::devices::catalogue_path()?
            .parent()
            .unwrap()
            .join("registers");
        self.load_cpu_directory(&user)
    }
    fn load_cpu_directory(&self, directory: &Path) -> Result<Option<(Catalogue, String)>, String> {
        let user = directory.join(format!("{}.toml", self.cpu));
        if user.is_file() {
            return Ok(Some((
                Catalogue::load(&user)?,
                format!("user:{}", user.display()),
            )));
        }
        Ok(Some((
            Catalogue::builtin(&self.cpu)?,
            format!("builtin:{}", self.cpu),
        )))
    }
    pub fn validate(&self) -> Result<(), String> {
        self.topology.validate()?;
        if !matches!(self.cp15_command.as_str(), "" | "arm mrc" | "aarch64 mrc") {
            return Err("registers.cp15_command must be arm mrc or aarch64 mrc".into());
        }
        if !matches!(self.cp15_64_command.as_str(), "" | "aarch64 mrrc") {
            return Err("registers.cp15_64_command must be aarch64 mrrc".into());
        }
        if !matches!(self.banked_command.as_str(), "" | "aarch64 banked") {
            return Err("registers.banked_command must be aarch64 banked".into());
        }
        if !matches!(self.vfp_command.as_str(), "" | "aarch64 vfp") {
            return Err("registers.vfp_command must be aarch64 vfp".into());
        }
        if !matches!(self.vfp_write_command.as_str(), "" | "aarch64 vfp_write")
            || (!self.vfp_write_command.is_empty() && self.vfp_command != "aarch64 vfp")
        {
            return Err(
                "registers.vfp_write_command requires aarch64 vfp_write and aarch64 vfp".into(),
            );
        }
        if !self.isb_command.is_empty()
            && (self.isb_command != "aarch64 isb"
                || self.cp15_command != "aarch64 mrc"
                || self.selector_command != "aarch64 mcr")
        {
            return Err(
                "registers.isb_command requires aarch64 isb with the matching MRC/MCR family"
                    .into(),
            );
        }
        if !self.selector_command.is_empty()
            && !matches!(
                (self.cp15_command.as_str(), self.selector_command.as_str()),
                ("arm mrc", "arm mcr") | ("aarch64 mrc", "aarch64 mcr")
            )
        {
            return Err(
                "registers.selector_command must explicitly match the verified MRC command family"
                    .into(),
            );
        }
        if self.tcl_endpoint.chars().any(char::is_control)
            || self.targets.iter().any(|(core, target)| {
                !identifier(core) || target.is_empty() || target.chars().any(char::is_control)
            })
            || self.facts.keys().any(|name| !identifier(name))
            || self.components.iter().any(|(name, component)| {
                !identifier(name)
                    || (!component.channel.is_empty() && !identifier(&component.channel))
            })
            || self
                .topology
                .clusters
                .iter()
                .any(|(core, cluster)| !identifier(core) || !identifier(cluster))
            || (!self.topology.chip.is_empty() && !identifier(&self.topology.chip))
        {
            return Err("Invalid register access, capability or topology configuration".into());
        }
        self.load()?;
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalogue {
    pub version: u32,
    pub cpu: String,
    pub architecture: String,
    #[serde(default)]
    pub description: String,
    pub groups: Vec<Group>,
    pub registers: Vec<Register>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub description: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    Ro,
    #[default]
    Rw,
    Wo,
}
impl Access {
    pub fn readable(self) -> bool {
        self != Self::Wo
    }
    pub fn writable(self) -> bool {
        self != Self::Ro
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Ro => "RO",
            Self::Rw => "RW",
            Self::Wo => "WO",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    #[default]
    Core,
    Cluster,
    Chip,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Reader {
    Gdb {
        name: String,
    },
    Cp15 {
        cp: u8,
        op1: u8,
        crn: u8,
        crm: u8,
        op2: u8,
    },
    Cp15_64 {
        cp: u8,
        op1: u8,
        crm: u8,
    },
    Backend {
        name: String,
    },
    Banked {
        name: String,
    },
    Vfp {
        name: String,
    },
    Mmio {
        component: String,
        offset: u64,
    },
    Alias {
        source: String,
        offset: u16,
    },
}

/// Independent of the reader; only a writer explicitly present in the catalogue is offered.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Writer {
    /// GDB's MI writer uses LONGEST, so it cannot represent a 128-bit vector.
    GdbInteger { name: String },
    /// Fixed raw S/D/Q protocol with fresh physical alias merging and readback.
    Vfp { name: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Register {
    pub id: String,
    pub name: String,
    pub group: String,
    pub bits: u16,
    pub access: Access,
    pub reader: Reader,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer: Option<Writer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write: Option<crate::writes::Register>,
    #[serde(default)]
    pub scope: Scope,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub access_condition: String,
    #[serde(default)]
    pub read_side_effect: bool,
    #[serde(default)]
    pub fields: Vec<Field>,
    #[serde(default)]
    pub conditions: Vec<Condition>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Segment {
    pub offset: u16,
    pub width: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Least significant segment first, including discontiguous fields such as CPSR.IT.
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub access: Option<Access>,
    #[serde(default)]
    pub enums: Vec<EnumValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnumValue {
    pub value: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    /// Capability facts include their physical/virtual interface in the key.
    pub fact: String,
    pub min: u64,
    #[serde(default)]
    pub max: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Implementation {
    Yes,
    No,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    #[default]
    NotRead,
    Valid,
    Unavailable,
    Unsupported,
    Error,
    Stale,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    HardwareNotImplemented,
    ReaderUnsupported,
    AccessRestricted,
    FeatureDisabled,
    TransportError,
    WriteOnly,
    #[default]
    Unknown,
}

/// Raw values cross JSON as exact strings, including the high half of 128-bit values.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawValue {
    pub bits: u16,
    pub hex: String,
}

fn mask(bits: u16) -> u128 {
    if bits == 128 {
        u128::MAX
    } else {
        (1u128 << bits) - 1
    }
}
fn valid_width(bits: u16) -> Result<(), String> {
    if (1..=128).contains(&bits) {
        Ok(())
    } else {
        Err("Bit width must be 1..128".into())
    }
}
impl RawValue {
    pub fn from_integer(value: u128, bits: u16) -> Result<Self, String> {
        valid_width(bits)?;
        if value & !mask(bits) != 0 {
            return Err(format!("Value exceeds {bits} bits"));
        }
        Ok(Self {
            bits,
            hex: format!("0x{value:0width$x}", width = usize::from(bits.div_ceil(4))),
        })
    }
    pub fn parse(text: &str, bits: u16) -> Result<Self, String> {
        valid_width(bits)?;
        let text = text.trim();
        let (digits, base) =
            if let Some(s) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
                (s, 16)
            } else if let Some(s) = text.strip_prefix("0b") {
                (s, 2)
            } else if let Some(s) = text.strip_prefix("0o") {
                (s, 8)
            } else {
                (text, 10)
            };
        if digits.is_empty() || digits.starts_with(['+', '-']) {
            return Err("Expected unsigned raw register value".into());
        }
        let value = u128::from_str_radix(digits, base)
            .map_err(|e| format!("Invalid raw register value: {e}"))?;
        Self::from_integer(value, bits)
    }
    pub fn integer(&self) -> Result<u128, String> {
        // Deserialization is untrusted; validate rather than relying on construction.
        valid_width(self.bits)?;
        let parsed = Self::parse(&self.hex, self.bits)?;
        u128::from_str_radix(&parsed.hex[2..], 16).map_err(|e| e.to_string())
    }
    pub fn slice(&self, offset: u16, bits: u16) -> Result<Self, String> {
        valid_width(bits)?;
        if offset >= self.bits || offset.checked_add(bits).is_none_or(|end| end > self.bits) {
            return Err("Alias exceeds its source register".into());
        }
        Self::from_integer((self.integer()? >> offset) & mask(bits), bits)
    }
    pub fn from_bytes(bytes: &[u8], bits: u16, little: bool) -> Result<Self, String> {
        valid_width(bits)?;
        if bytes.len() != usize::from(bits.div_ceil(8)) {
            return Err("Raw byte count does not match width".into());
        }
        let mut raw = [0u8; 16];
        let value = if little {
            raw[..bytes.len()].copy_from_slice(bytes);
            u128::from_le_bytes(raw)
        } else {
            raw[16 - bytes.len()..].copy_from_slice(bytes);
            u128::from_be_bytes(raw)
        };
        Self::from_integer(value, bits)
    }
    pub fn bytes(&self, little: bool) -> Result<Vec<u8>, String> {
        let value = self.integer()?;
        let count = usize::from(self.bits.div_ceil(8));
        Ok(if little {
            value.to_le_bytes()[..count].to_vec()
        } else {
            value.to_be_bytes()[16 - count..].to_vec()
        })
    }
    pub fn float(&self) -> Result<String, String> {
        let n = self.integer()?;
        match self.bits {
            32 => Ok(format!("{}", f32::from_bits(n as u32))),
            64 => Ok(format!("{}", f64::from_bits(n as u64))),
            _ => Err("Floating-point interpretation requires 32 or 64 bits".into()),
        }
    }
}

impl Field {
    pub fn validate(&self, bits: u16) -> Result<(), String> {
        if self.name.trim().is_empty() || self.segments.is_empty() {
            return Err("Field needs a name and bit segments".into());
        }
        let mut used = 0u128;
        let mut width = 0u16;
        for segment in &self.segments {
            valid_width(segment.width)?;
            if segment.offset >= bits
                || segment
                    .offset
                    .checked_add(segment.width)
                    .is_none_or(|end| end > bits)
            {
                return Err(format!("Field {} exceeds register width", self.name));
            }
            let covered = mask(segment.width) << segment.offset;
            if used & covered != 0 {
                return Err(format!("Field {} has overlapping segments", self.name));
            }
            used |= covered;
            width += segment.width;
        }
        let mut enums = BTreeSet::new();
        for item in &self.enums {
            let value = RawValue::parse(&item.value, width)?.integer()?;
            if item.name.trim().is_empty() || !enums.insert(value) {
                return Err(format!("Invalid enum in field {}", self.name));
            }
        }
        Ok(())
    }
    pub fn extract(&self, raw: &RawValue) -> Result<RawValue, String> {
        self.validate(raw.bits)?;
        let value = raw.integer()?;
        let mut result = 0;
        let mut shift = 0;
        for segment in &self.segments {
            result |= ((value >> segment.offset) & mask(segment.width)) << shift;
            shift += segment.width;
        }
        RawValue::from_integer(result, shift)
    }
    pub fn enum_name(&self, raw: &RawValue) -> Option<&str> {
        let value = raw.integer().ok()?;
        self.enums
            .iter()
            .find(|entry| {
                RawValue::parse(&entry.value, raw.bits)
                    .and_then(|n| n.integer())
                    .ok()
                    == Some(value)
            })
            .map(|entry| entry.name.as_str())
    }
}

impl Register {
    pub fn implementation(&self, facts: &BTreeMap<String, u64>) -> (Implementation, String) {
        let mut missing = Vec::new();
        for condition in &self.conditions {
            match facts.get(&condition.fact) {
                Some(&value)
                    if value < condition.min || condition.max.is_some_and(|max| value > max) =>
                {
                    return (
                        Implementation::No,
                        format!("{}={value} excludes this register", condition.fact),
                    );
                }
                Some(_) => {}
                None => missing.push(condition.fact.as_str()),
            }
        }
        if !missing.is_empty() {
            (
                Implementation::Unknown,
                format!("Unknown capability: {}", missing.join(", ")),
            )
        } else if self.conditions.is_empty() {
            (
                Implementation::Unknown,
                "Target identity not yet verified".into(),
            )
        } else {
            (
                Implementation::Yes,
                "Capability conditions satisfied".into(),
            )
        }
    }
    pub fn auto_read(&self, implementation: Implementation) -> bool {
        self.access.readable() && !self.read_side_effect && implementation != Implementation::No
    }
}

fn identifier(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 128
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:/+".contains(&b))
}

impl Catalogue {
    pub fn builtin(cpu: &str) -> Result<Self, String> {
        let text = match cpu {
            "cortex-r52" => include_str!("../profiles/registers/cortex-r52.toml"),
            "cortex-r52+" => include_str!("../profiles/registers/cortex-r52+.toml"),
            "cortex-m4" => include_str!("../profiles/registers/cortex-m4.toml"),
            _ => {
                return Err(format!(
                    "Unknown CPU catalogue '{cpu}'; select an explicit register catalogue file"
                ));
            }
        };
        Self::parse(text)
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let metadata = fs::metadata(path)
            .map_err(|e| format!("Register catalogue {}: {e}", path.display()))?;
        if metadata.len() > MAX_CATALOGUE_BYTES {
            return Err("Register catalogue exceeds 4 MiB".into());
        }
        let file = fs::File::open(path)
            .map_err(|e| format!("Register catalogue {}: {e}", path.display()))?;
        Self::read(file).map_err(|e| format!("Register catalogue {}: {e}", path.display()))
    }
    fn read(reader: impl Read) -> Result<Self, String> {
        // Recheck while reading: the file can grow after metadata was sampled.
        let mut bytes = Vec::new();
        reader
            .take(MAX_CATALOGUE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_CATALOGUE_BYTES {
            return Err("Register catalogue exceeds 4 MiB".into());
        }
        let text = String::from_utf8(bytes)
            .map_err(|e| format!("Register catalogue is not UTF-8: {e}"))?;
        Self::parse(&text)
    }
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() as u64 > MAX_CATALOGUE_BYTES {
            return Err("Register catalogue exceeds 4 MiB".into());
        }
        let catalogue: Self =
            toml::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
        catalogue.validate()?;
        Ok(catalogue)
    }
    pub fn register(&self, id: &str) -> Option<&Register> {
        self.registers.iter().find(|r| r.id == id)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("Unsupported register catalogue version".into());
        }
        if !identifier(&self.cpu) || !identifier(&self.architecture) {
            return Err("Invalid CPU or architecture identifier".into());
        }
        if self.groups.len() > 256 || self.registers.len() > 4096 {
            return Err("Register catalogue has too many entries".into());
        }
        let mut groups = BTreeMap::new();
        for group in &self.groups {
            if !identifier(&group.id)
                || group.name.trim().is_empty()
                || groups.insert(&group.id, group).is_some()
            {
                return Err(format!("Invalid or duplicate group {}", group.id));
            }
        }
        for group in &self.groups {
            let mut path = BTreeSet::new();
            let mut current = group;
            while let Some(parent) = &current.parent {
                if !path.insert(&current.id) {
                    return Err(format!("Group cycle at {}", group.id));
                }
                current = groups
                    .get(parent)
                    .ok_or_else(|| format!("Unknown parent group {parent}"))?;
            }
        }
        let mut ids = BTreeSet::new();
        for register in &self.registers {
            if !identifier(&register.id)
                || register.name.trim().is_empty()
                || !ids.insert(&register.id)
            {
                return Err(format!("Invalid or duplicate register {}", register.id));
            }
            if !groups.contains_key(&register.group) {
                return Err(format!("Unknown group {}", register.group));
            }
            if !matches!(register.bits, 8 | 16 | 32 | 64 | 128) {
                return Err(format!("Invalid width for {}", register.id));
            }
            match (&register.writer, &register.write) {
                (None, None) => {}
                (Some(Writer::Vfp { name }), Some(write)) => {
                    let view = vfp::WriteView::parse(name)
                        .ok_or_else(|| format!("Invalid VFP writer for {}", register.id))?;
                    let reader_matches = match &register.reader {
                        Reader::Vfp { name: reader } => view.bits != 32 && reader == name,
                        Reader::Alias { source, offset } => {
                            view.bits == 32
                                && source == &view.reader_name()
                                && *offset == view.offset % 64
                                && self.register(source).is_some_and(|parent| {
                                    parent.bits == 64
                                        && parent.scope == Scope::Core
                                        && !parent.read_side_effect
                                        && matches!(&parent.reader, Reader::Vfp { name } if name == source)
                                })
                        }
                        _ => false,
                    };
                    if name != &register.id
                        || view.bits != register.bits
                        || write.bits != register.bits
                        || register.scope != Scope::Core
                        || register.access != Access::Rw
                        || register.read_side_effect
                        || write.read_side_effect
                        || !reader_matches
                        || write.access != crate::writes::Access::ReadWrite
                        || write.effect != crate::writes::Effect::Modify
                        || !write.fields.is_empty()
                        || !matches!(write.constraint, crate::writes::Constraint::None)
                        || !matches!(write.verification, crate::writes::Verification::Modified)
                    {
                        return Err(format!(
                            "VFP writer requires matching plain raw storage metadata: {}",
                            register.id
                        ));
                    }
                    write
                        .validate()
                        .map_err(|e| format!("Write metadata for {}: {e}", register.id))?;
                }
                (Some(Writer::GdbInteger { name }), Some(write)) => {
                    if !identifier(name)
                        || register.bits > 64
                        || write.bits != register.bits
                        || register.scope != Scope::Core
                    {
                        return Err(format!("Invalid GDB integer writer for {}", register.id));
                    }
                    write
                        .validate()
                        .map_err(|e| format!("Write metadata for {}: {e}", register.id))?;
                    if !register.access.writable() {
                        return Err(format!("Read-only register {} has a writer", register.id));
                    }
                    if register.read_side_effect && !write.read_side_effect {
                        return Err(format!(
                            "Write metadata hides read side effects for {}",
                            register.id
                        ));
                    }
                }
                _ => {
                    return Err(format!(
                        "Register {} needs both writer and write metadata",
                        register.id
                    ));
                }
            }
            match &register.reader {
                Reader::Vfp { name }
                    if vfp::Kind::parse(name).is_none_or(|kind| kind.bits() != register.bits)
                        || register.scope != Scope::Core
                        || register.read_side_effect =>
                {
                    return Err(format!("Invalid VFP reader for {}", register.id));
                }
                Reader::Banked { name }
                    if !banked::valid_name(name)
                        || register.bits != 32
                        || register.scope != Scope::Core
                        || register.read_side_effect =>
                {
                    return Err(format!(
                        "Invalid banked register reader for {}",
                        register.id
                    ));
                }
                Reader::Gdb { name } | Reader::Backend { name } if !identifier(name) => {
                    return Err(format!("Invalid backend register name for {}", register.id));
                }
                Reader::Cp15 {
                    cp,
                    op1,
                    crn,
                    crm,
                    op2,
                } if *cp > 15
                    || *op1 > 7
                    || *crn > 15
                    || *crm > 15
                    || *op2 > 7
                    || register.bits != 32 =>
                {
                    return Err(format!("Invalid 32-bit CP encoding for {}", register.id));
                }
                Reader::Cp15_64 { cp, op1, crm }
                    if *cp > 15 || *op1 > 15 || *crm > 15 || register.bits != 64 =>
                {
                    return Err(format!("Invalid 64-bit CP encoding for {}", register.id));
                }
                Reader::Mmio { component, offset }
                    if !identifier(component)
                        || register.bits > 64
                        || offset % u64::from(register.bits / 8) != 0 =>
                {
                    return Err(format!("Invalid MMIO route for {}", register.id));
                }
                _ => {}
            }
            let mut fields = BTreeSet::new();
            for field in &register.fields {
                field.validate(register.bits)?;
                if !fields.insert(&field.name) {
                    return Err(format!("Duplicate field {}", field.name));
                }
            }
            for condition in &register.conditions {
                if !identifier(&condition.fact)
                    || condition.max.is_some_and(|max| max < condition.min)
                {
                    return Err(format!("Invalid condition for {}", register.id));
                }
            }
        }
        for register in &self.registers {
            let mut path = BTreeSet::new();
            let mut current = register;
            while let Reader::Alias { source, offset } = &current.reader {
                if !path.insert(&current.id) {
                    return Err(format!("Alias cycle at {}", register.id));
                }
                let parent = self
                    .register(source)
                    .ok_or_else(|| format!("Unknown alias source {source}"))?;
                if offset
                    .checked_add(current.bits)
                    .is_none_or(|end| end > parent.bits)
                    || parent.scope != current.scope
                {
                    return Err(format!(
                        "Alias {} exceeds source or changes scope",
                        current.id
                    ));
                }
                current = parent;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Topology {
    pub chip: String,
    pub clusters: BTreeMap<String, String>,
}
impl Topology {
    pub fn validate(&self) -> Result<(), String> {
        let valid = |s: &str| s.len() <= 256 && s.trim() == s && !s.chars().any(char::is_control);
        if !valid(&self.chip)
            || self.clusters.len() > 1024
            || self.clusters.iter().any(|(core, cluster)| {
                core.is_empty() || cluster.is_empty() || !valid(core) || !valid(cluster)
            })
        {
            return Err("Invalid register topology: use explicit nonempty core/cluster identities without outer whitespace or control characters".into());
        }
        Ok(())
    }
    /// An unmapped producer may belong to any declared cluster; do not guess an exclusion.
    pub fn affected_shared_owners(&self, core: &str) -> BTreeSet<String> {
        let mut owners = BTreeSet::new();
        if let Some(owner) = self.owner(Scope::Chip, core) {
            owners.insert(owner);
        }
        if let Some(owner) = self.owner(Scope::Cluster, core) {
            owners.insert(owner);
        } else {
            owners.extend(
                self.clusters
                    .values()
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| format!("cluster:{s}")),
            );
        }
        owners
    }
    pub fn owner(&self, scope: Scope, core: &str) -> Option<String> {
        match scope {
            Scope::Core => (!core.trim().is_empty()).then(|| format!("core:{core}")),
            Scope::Cluster => self
                .clusters
                .get(core)
                .filter(|s| !s.trim().is_empty())
                .map(|cluster| format!("cluster:{cluster}")),
            Scope::Chip => (!self.chip.trim().is_empty()).then(|| format!("chip:{}", self.chip)),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Context {
    pub session: u64,
    pub generation: u64,
    pub core: String,
    pub frame: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SampleView {
    #[default]
    SelectedFrame,
    PhysicalCore,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sample {
    pub id: String,
    pub state: State,
    pub implementation: Implementation,
    pub reason: Reason,
    pub detail: String,
    pub value: Option<RawValue>,
    pub owner: Option<String>,
    pub context: Context,
    pub timestamp_ms: u64,
    pub source: String,
    /// Independent of the transport: a physical frame-0 probe may use GDB too.
    /// Older/unknown producers default to the conservative selected-frame view.
    #[serde(default)]
    pub view: SampleView,
    /// Shared coordinator lifetime; None for standalone/local worker samples.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_generation: Option<u64>,
}
impl Sample {
    /// UI/API caches must additionally check the shared owner's published lifetime.
    pub fn applies_at(
        &self,
        context: &Context,
        owner: Option<&str>,
        generations: &BTreeMap<String, u64>,
    ) -> bool {
        self.applies(context, owner)
            && match self.owner_generation {
                Some(generation) => {
                    self.owner.as_ref().and_then(|owner| generations.get(owner))
                        == Some(&generation)
                }
                None => {
                    !self.owner.as_deref().is_some_and(|owner| {
                        owner.starts_with("cluster:") || owner.starts_with("chip:")
                    }) || generations.is_empty()
                }
            }
    }
    pub fn stale(&mut self) {
        if self.state == State::Valid {
            self.state = State::Stale;
        }
    }
    pub fn applies(&self, context: &Context, owner: Option<&str>) -> bool {
        self.context.session == context.session
            && self.context.generation == context.generation
            && self.owner.as_deref() == owner
            && (self
                .owner
                .as_deref()
                .is_some_and(|s| !s.starts_with("core:"))
                || self.context.core == context.core)
            && (matches!(self.view, SampleView::PhysicalCore)
                || (self.context.frame == context.frame && self.context.core == context.core))
    }
}

#[cfg(test)]
mod tests;
