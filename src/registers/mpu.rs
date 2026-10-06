//! R52 MPU region and MAIR presentation. All derived values require current-core evidence.
use super::{
    Context, RawValue, Sample, State,
    selector::{Kind, MpuRegion, decode_mpu},
};
use serde::{Deserialize, Serialize};
pub mod m_profile;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Bank {
    El1,
    El2,
}
impl Bank {
    pub fn kind(self) -> Kind {
        match self {
            Self::El1 => Kind::MpuEl1,
            Self::El2 => Kind::MpuEl2,
        }
    }
    pub fn count_id(self) -> &'static str {
        match self {
            Self::El1 => "mpuir",
            Self::El2 => "hmpuir",
        }
    }
    pub fn count_fact(self) -> &'static str {
        match self {
            Self::El1 => "mpu.el1.regions",
            Self::El2 => "mpu.el2.regions",
        }
    }
    pub fn control_id(self) -> &'static str {
        match self {
            Self::El1 => "sctlr",
            Self::El2 => "hsctlr",
        }
    }
    pub fn mair_ids(self) -> [&'static str; 2] {
        match self {
            Self::El1 => ["mair0", "mair1"],
            Self::El2 => ["hmair0", "hmair1"],
        }
    }
    pub fn count(self, raw: &RawValue) -> Result<u8, String> {
        if raw.bits != 32 {
            return Err("R52 MPUIR/HMPUIR must be 32 bits".into());
        }
        Ok(((raw.integer()? >> if self == Self::El1 { 8 } else { 0 }) & 255) as u8)
    }
    pub fn validate_count(self, count: u64) -> Result<u8, String> {
        if !matches!(count, 16 | 20 | 24) && !(self == Self::El2 && count == 0) {
            return Err("MPU count is outside the adapted Cortex-R52 implementation".into());
        }
        Ok(count as u8)
    }
    pub fn read_ids(self, count: u8) -> Result<Vec<String>, String> {
        self.validate_count(u64::from(count))?;
        if count == 0 {
            return Ok(vec![]);
        }
        let mut ids = vec![self.control_id().into()];
        ids.extend(self.mair_ids().map(String::from));
        if self == Self::El2 {
            ids.extend(["hcr".into(), "hprenr".into()]);
        }
        for index in 0..count {
            ids.extend(self.kind().ids(index));
        }
        Ok(ids)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachePolicy {
    pub policy: String,
    pub transient: Option<bool>,
    pub read_allocate: Option<bool>,
    pub write_allocate: Option<bool>,
}
fn cache(n: u8) -> CachePolicy {
    if n == 4 {
        return CachePolicy {
            policy: "Non-cacheable".into(),
            transient: None,
            read_allocate: None,
            write_allocate: None,
        };
    }
    CachePolicy {
        policy: if n < 4 || (8..12).contains(&n) {
            "Write-through"
        } else {
            "Write-back"
        }
        .into(),
        transient: Some(n < 8),
        read_allocate: Some(n & 2 != 0),
        write_allocate: Some(n & 1 != 0),
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MemoryType {
    Device {
        name: String,
    },
    Normal {
        outer: CachePolicy,
        inner: CachePolicy,
    },
    Unpredictable {
        reason: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attribute {
    pub index: u8,
    pub raw: String,
    pub memory: MemoryType,
}
impl Attribute {
    pub fn decode(index: u8, mair: &RawValue) -> Result<Self, String> {
        if index > 7 || mair.bits != 32 {
            return Err("MAIR entry requires index 0..7 and a 32-bit register".into());
        }
        let byte = ((mair.integer()? >> ((index % 4) * 8)) & 255) as u8;
        let outer = byte >> 4;
        let inner = byte & 15;
        let memory = if outer == 0 {
            match inner {
                0 | 4 | 8 | 12 => MemoryType::Device {
                    name: ["Device-nGnRnE", "Device-nGnRE", "Device-nGRE", "Device-GRE"]
                        [usize::from(inner / 4)]
                    .into(),
                },
                _ => MemoryType::Unpredictable {
                    reason: "Reserved Device attribute encoding".into(),
                },
            }
        } else if inner == 0 {
            MemoryType::Unpredictable {
                reason: "Normal memory with zero inner attribute is UNPREDICTABLE".into(),
            }
        } else {
            MemoryType::Normal {
                outer: cache(outer),
                inner: cache(inner),
            }
        };
        Ok(Self {
            index,
            raw: format!("0x{byte:02x}"),
            memory,
        })
    }
    pub fn label(&self) -> String {
        match &self.memory {
            MemoryType::Device { name } => name.clone(),
            MemoryType::Unpredictable { reason } => format!("UNPREDICTABLE: {reason}"),
            MemoryType::Normal { outer, inner } => format!(
                "Normal · outer {} · inner {}",
                policy_label(outer),
                policy_label(inner)
            ),
        }
    }
}
fn policy_label(p: &CachePolicy) -> String {
    match p.transient {
        None => p.policy.clone(),
        Some(transient) => format!(
            "{} {} RA={} WA={}",
            p.policy,
            if transient {
                "transient"
            } else {
                "non-transient"
            },
            u8::from(p.read_allocate.unwrap_or(false)),
            u8::from(p.write_allocate.unwrap_or(false))
        ),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Region {
    pub index: u8,
    pub base: Option<Sample>,
    pub limit: Option<Sample>,
    pub decoded: Option<MpuRegion>,
    pub attribute: Option<Attribute>,
    pub issues: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct View {
    pub bank: Bank,
    pub context: Context,
    pub owner: String,
    pub count: u8,
    pub control: Option<Sample>,
    pub global_enabled: Option<bool>,
    pub background_enabled: Option<bool>,
    pub hcr: Option<Sample>,
    pub hprenr: Option<Sample>,
    pub mair: [Option<Sample>; 2],
    pub regions: Vec<Region>,
    pub notes: Vec<String>,
}
pub fn current<'a>(sample: Option<&'a Sample>, context: &Context) -> Option<&'a RawValue> {
    let sample = sample?;
    if context.frame != 0
        || sample.state != State::Valid
        || sample.context != *context
        || sample.owner.as_deref() != Some(format!("core:{}", context.core).as_str())
    {
        return None;
    }
    sample.value.as_ref().filter(|v| v.bits == 32)
}
impl View {
    pub fn from_samples(
        bank: Bank,
        count: u64,
        context: &Context,
        samples: &[Sample],
    ) -> Result<Self, String> {
        let count = bank.validate_count(count)?;
        let find = |id: &str| {
            samples
                .iter()
                .find(|s| {
                    s.id == id
                        && s.context.core == context.core
                        && s.owner.as_deref() == Some(format!("core:{}", context.core).as_str())
                })
                .cloned()
        };
        let control = find(bank.control_id());
        let control_raw = current(control.as_ref(), context).and_then(|v| v.integer().ok());
        let mair = bank.mair_ids().map(find);
        let hprenr = (bank == Bank::El2).then(|| find("hprenr")).flatten();
        let mut regions = Vec::new();
        for index in 0..count {
            let [a, b] = bank.kind().ids(index).map(|id| find(&id));
            let mut issues = vec![];
            let decoded = match (current(a.as_ref(), context), current(b.as_ref(), context)) {
                (Some(a), Some(b)) => Some(decode_mpu(bank.kind(), a, b)?),
                _ => {
                    issues.push(
                        "Region pair is missing, unavailable or stale; raw evidence retained"
                            .into(),
                    );
                    None
                }
            };
            let attribute = decoded.as_ref().and_then(|r| {
                let raw = current(mair[usize::from(r.attribute_index / 4)].as_ref(), context)?;
                Attribute::decode(r.attribute_index, raw).ok()
            });
            if let Some(region) = &decoded {
                if attribute.is_none() {
                    issues.push(
                        "Selected MAIR entry is unavailable or stale; memory type is unknown"
                            .into(),
                    );
                }
                if region.enabled && !region.valid_range {
                    issues.push("Enabled region has base above limit".into());
                }
                if region.shareability.starts_with("UNPREDICTABLE")
                    && attribute
                        .as_ref()
                        .is_some_and(|a| matches!(a.memory, MemoryType::Normal { .. }))
                {
                    issues.push("Normal memory with SH=01 is UNPREDICTABLE".into());
                }
                if let Some(raw) = current(hprenr.as_ref(), context) {
                    let enabled = (raw.integer()? >> index) & 1 != 0;
                    if enabled != region.enabled {
                        issues
                            .push("HPRENR and HPRLAR.EN disagree; samples are inconsistent".into());
                    }
                }
            }
            regions.push(Region {
                index,
                base: a,
                limit: b,
                decoded,
                attribute,
                issues,
            });
        }
        Ok(Self {
            bank, context: context.clone(), owner: format!("core:{}", context.core), count,
            control, global_enabled: control_raw.map(|v| v & 1 != 0), background_enabled: control_raw.map(|v| v & (1 << 17) != 0),
            hcr: (bank == Bank::El2).then(|| find("hcr")).flatten(), hprenr, mair, regions,
            notes: vec!["Configuration overview; not a proof of effective permissions for a particular address".into(), "Direct reads preserve selectors; paused sampling is sequential, not an architectural atomic snapshot".into(), "MAIR shows programmed allocation policies; Cortex-R52 ignores the transient hint (TRM 100026_0104_01_en p171)".into(), "SH field decoding applies to Normal memory; R52 treats Device and Normal non-cacheable memory as Outer Shareable".into()],
        })
    }
}

#[cfg(test)]
mod tests;
