//! Cortex-M ID observations. Catalogue selection and configured facts are not ID evidence.
use super::{
    Catalogue, Context, Reader, Reason, Sample, SampleView, State,
    capabilities::{Fact, Identity, Probe},
    provenance::{Phase, Route},
};
use std::collections::BTreeMap;

pub const PROBE_IDS: &[&str] = &[
    "scb.cpuid",
    "scs.ictr",
    "mpu.type",
    "dcb.demcr",
    "fpb.ctrl",
    "dwt.ctrl",
    "fpu.mvfr0",
    "fpu.mvfr1",
    "fpu.mvfr2",
    "scb.clidr",
    "scb.ctr",
];
pub const FACT_KEYS: &[&str] = &[
    "nvic.banks",
    "nvic.lines_upper_bound",
    "mpu.regions",
    "mpu.present",
    "dwt.enabled",
    "dwt.comparators",
    "fpb.revision",
    "fpb.code_comparators",
    "fpb.literal_comparators",
    "vfp.present",
    "vfp.d_registers",
    "vfp.single",
    "vfp.double",
    "mcache.clidr",
    "mcache.ctr",
];

pub(super) fn current_facts(
    catalogue: &Catalogue,
    probe: Option<&Probe>,
    samples: &[Sample],
    context: &Context,
) -> BTreeMap<String, u64> {
    let mut current = Probe {
        context: context.clone(),
        thread: String::new(),
        identity: None,
        facts: BTreeMap::new(),
        samples: probe
            .into_iter()
            .flat_map(|p| p.samples.iter())
            .chain(samples)
            .filter(|s| PROBE_IDS.contains(&s.id.as_str()))
            .cloned()
            .collect(),
        nvic: None,
        gdb_names: vec![],
        notes: vec![],
    };
    decode(&mut current, catalogue);
    current
        .facts
        .into_iter()
        .map(|(key, fact)| (key, fact.value))
        .collect()
}

pub fn adapted_cpu(cpu: &str) -> bool {
    matches!(cpu, "cortex-m3" | "cortex-m4" | "cortex-m7")
}

fn model(n: u64) -> Option<&'static str> {
    if n >> 24 != 0x41 || (n >> 16) & 15 != 15 {
        return None;
    }
    match (n >> 4) & 0xfff {
        0xc23 => Some("cortex-m3"),
        0xc24 => Some("cortex-m4"),
        0xc27 => Some("cortex-m7"),
        _ => None,
    }
}

fn latest<'a>(
    samples: impl Iterator<Item = &'a Sample>,
    context: &Context,
) -> BTreeMap<&'a str, &'a Sample> {
    let mut result: BTreeMap<&str, &Sample> = BTreeMap::new();
    for sample in samples.filter(|s| s.context == *context) {
        if result
            .get(sample.id.as_str())
            .is_none_or(|p| sample.timestamp_ms >= p.timestamp_ms)
        {
            result.insert(sample.id.as_str(), sample);
        }
    }
    result
}

fn observed<'a>(
    catalogue: &Catalogue,
    samples: &BTreeMap<&str, &'a Sample>,
    context: &Context,
    id: &str,
) -> Option<&'a Sample> {
    let sample = *samples.get(id)?;
    let register = catalogue.register(id)?;
    let Reader::CorePrivate { address } = register.reader else {
        return None;
    };
    let provenance = sample.provenance.as_ref()?;
    let access = provenance.access.as_ref()?;
    if sample.state != State::Valid
        || sample.context != *context
        || context.frame != 0
        || sample.view != SampleView::PhysicalCore
        || sample.owner.as_deref() != Some(format!("core:{}", context.core).as_str())
        || register.bits != 32
        || sample.value.as_ref()?.bits != 32
        || sample.value.as_ref()?.integer().is_err()
        || provenance.catalogue_reader != register.reader
        || access.context != *context
        || access.phase != Phase::Responded
        || !access
            .completed_ms
            .is_some_and(|end| end >= access.timestamp_ms)
    {
        return None;
    }
    let expected = format!("0x{address:x}");
    let valid = match &access.route {
        Route::GdbMemory {
            endpoint: Some(endpoint),
            configured_endpoint,
            address,
            bits,
            ..
        } => {
            !endpoint.is_empty()
                && endpoint == configured_endpoint
                && address == &expected
                && *bits == 32
        }
        Route::TclMemory {
            endpoint,
            target,
            address,
            bits,
            bus_width,
            count,
            ..
        } => {
            !endpoint.is_empty()
                && !target.is_empty()
                && address == &expected
                && *bits == 32
                && *bus_width == 32
                && *count == 1
        }
        _ => false,
    };
    valid.then_some(sample)
}

fn raw(
    catalogue: &Catalogue,
    samples: &BTreeMap<&str, &Sample>,
    context: &Context,
    id: &str,
) -> Option<u64> {
    observed(catalogue, samples, context, id)?
        .value
        .as_ref()?
        .integer()
        .ok()?
        .try_into()
        .ok()
}

fn matched(catalogue: &Catalogue, samples: &BTreeMap<&str, &Sample>, context: &Context) -> bool {
    raw(catalogue, samples, context, "scb.cpuid").and_then(model) == Some(catalogue.cpu.as_str())
}

/// Keep raw samples for diagnostics, but only validated ID fields can authorize optional instances.
pub(super) fn field_allowed(
    catalogue: &Catalogue,
    id: &str,
    samples: &BTreeMap<&str, &Sample>,
    context: &Context,
) -> bool {
    if !adapted_cpu(&catalogue.cpu) || !PROBE_IDS.contains(&id) {
        return true;
    }
    if id == "scb.cpuid" {
        return raw(catalogue, samples, context, id).is_some();
    }
    if !matched(catalogue, samples, context) {
        return false;
    }
    let Some(n) = raw(catalogue, samples, context, id) else {
        return false;
    };
    match id {
        // DDI 0439B §6.3.2: 0..7; 7 describes a 256-line upper bound, with at most 240 IRQs.
        "scs.ictr" => n <= 7,
        // ID supplies the count. These are the supported implementation encodings, not inferred counts.
        "mpu.type" => {
            n & !0x00ffff01 == 0
                && n & 1 == 0
                && (n >> 16) & 255 == 0
                && match (n >> 8) & 255 {
                    0 | 8 => true,
                    16 => catalogue.cpu == "cortex-m7",
                    _ => false,
                }
        }
        "dwt.ctrl" => {
            raw(catalogue, samples, context, "dcb.demcr").is_some_and(|n| n & (1 << 24) != 0)
        }
        "fpb.ctrl" => n >> 28 <= 1,
        "scb.clidr" => catalogue.cpu == "cortex-m7" && super::m_cache::valid_clidr(n),
        "scb.ctr" => catalogue.cpu == "cortex-m7" && n == 0x8303c003,
        // The non-FPU response is not established by a zero value or an access error.
        "fpu.mvfr0" => {
            catalogue.cpu != "cortex-m3"
                && n & 15 == 1
                && (n >> 4) & 15 == 2
                && match (n >> 8) & 15 {
                    0 => true,
                    2 => catalogue.cpu == "cortex-m7",
                    _ => false,
                }
                && (12..=28).step_by(4).all(|shift| (n >> shift) & 15 <= 1)
        }
        "fpu.mvfr1" | "fpu.mvfr2" => {
            catalogue.cpu != "cortex-m3" && field_allowed(catalogue, "fpu.mvfr0", samples, context)
        }
        _ => true,
    }
}

pub fn probe_denial(probe: &Probe, catalogue: &Catalogue, id: &str) -> Option<(Reason, String)> {
    if id == "scb.cpuid" {
        return None;
    }
    let samples = latest(probe.samples.iter(), &probe.context);
    if !matched(catalogue, &samples, &probe.context) {
        return Some((Reason::Unknown, "Actual CPUID has not established the selected adapted Cortex-M model; no optional ID read sent".into()));
    }
    if id == "dwt.ctrl" {
        match raw(catalogue, &samples, &probe.context, "dcb.demcr") {
            Some(n) if n & (1 << 24) != 0 => {},
            Some(_) => return Some((Reason::FeatureDisabled, "NeedEnable: DEMCR.TRCENA is clear; DWT capacity remains Unknown; no automatic enable".into())),
            None => return Some((Reason::Unknown, "DEMCR.TRCENA has not been observed; DWT capacity remains Unknown".into())),
        }
    }
    None
}

pub fn decode(probe: &mut Probe, catalogue: &Catalogue) {
    if probe.nvic.is_none() {
        probe.nvic = Some(nvic_metadata(None, "", None, &catalogue.cpu, None));
    }
    probe.facts.clear();
    probe.notes.retain(|n| !n.starts_with("Decode: "));
    let samples = latest(probe.samples.iter(), &probe.context);
    probe.identity = raw(catalogue, &samples, &probe.context, "scb.cpuid").map(|n| {
        let variant = ((n >> 20) & 15) as u8;
        let revision = (n & 15) as u8;
        Identity {
            implementer: (n >> 24) as u8,
            variant,
            architecture: ((n >> 16) & 15) as u8,
            part: ((n >> 4) & 0xfff) as u16,
            revision,
            model: model(n).map(|m| m.replacen("cortex", "Cortex", 1).replacen("-m", "-M", 1)),
            revision_name: format!("r{variant}p{revision}"),
        }
    });
    if !matched(catalogue, &samples, &probe.context) {
        probe.notes.push(format!("Decode: CPUID is missing, unadapted, or differs from selected {}; optional capabilities remain Unknown", catalogue.cpu));
        return;
    }
    let mut facts = BTreeMap::new();
    let mut insert = |key: &str, value: u64, id: &str, detail: &str| {
        facts.insert(
            key.into(),
            Fact {
                value,
                register: id.into(),
                source: samples[id].source.clone(),
                detail: detail.into(),
            },
        );
    };
    for id in PROBE_IDS.iter().copied().filter(|id| *id != "scb.cpuid") {
        let Some(n) = raw(catalogue, &samples, &probe.context, id) else {
            continue;
        };
        if !field_allowed(catalogue, id, &samples, &probe.context) {
            probe.notes.push(format!("Decode: {id} raw={n:#x} does not establish an adapted capacity/feature encoding; retained raw, effective fields Unknown"));
            continue;
        }
        match id {
            "scs.ictr" => {
                insert(
                    "nvic.banks",
                    n + 1,
                    id,
                    "ICTR.INTLINESNUM + 1; bank count, not actual interrupt count",
                );
                insert(
                    "nvic.lines_upper_bound",
                    (n + 1) * 32,
                    id,
                    "ICTR upper bound in groups of 32; actual IRQ list comes from SVD, maximum 240 for these models",
                );
            }
            "mpu.type" => {
                insert(
                    "mpu.regions",
                    (n >> 8) & 255,
                    id,
                    "MPU_TYPE.DREGION; validated unified MPU encoding",
                );
                insert(
                    "mpu.present",
                    u64::from(n & 0xff00 != 0),
                    id,
                    "Validated MPU_TYPE: zero regions means MPU is not implemented",
                );
            }
            "dcb.demcr" => insert(
                "dwt.enabled",
                (n >> 24) & 1,
                id,
                "DEMCR.TRCENA; enable state is separate from hardware presence",
            ),
            "dwt.ctrl" => insert(
                "dwt.comparators",
                n >> 28,
                id,
                "DWT_CTRL.NUMCOMP, observed only with TRCENA set",
            ),
            "fpb.ctrl" => {
                insert(
                    "fpb.revision",
                    n >> 28,
                    id,
                    "FP_CTRL.REV; only architecture revisions 0 and 1 are adapted",
                );
                insert(
                    "fpb.code_comparators",
                    ((n >> 4) & 15) | (((n >> 12) & 7) << 4),
                    id,
                    "FP_CTRL split NUM_CODE fields",
                );
                insert(
                    "fpb.literal_comparators",
                    (n >> 8) & 15,
                    id,
                    "FP_CTRL.NUM_LIT",
                );
            }
            "fpu.mvfr0" => {
                insert(
                    "vfp.present",
                    1,
                    id,
                    "Validated MVFR0 register-bank and single-precision feature encoding",
                );
                insert(
                    "vfp.d_registers",
                    16,
                    id,
                    "MVFR0.SIMDReg=1 supplies 16 D / 32 S registers; visibility/access is independently checked",
                );
                insert("vfp.single", 1, id, "MVFR0.FPSP=2");
                insert(
                    "vfp.double",
                    u64::from((n >> 8) & 15 == 2),
                    id,
                    "MVFR0.FPDP",
                );
            }
            "scb.clidr" => insert(
                "mcache.clidr",
                n,
                id,
                "DUI 0646B CLIDR: observed M7 L1 I/D implementation, not enable state",
            ),
            "scb.ctr" => insert(
                "mcache.ctr",
                n,
                id,
                "DUI 0646B CTR: observed adapted M7 cache architecture encoding",
            ),
            _ => {}
        }
    }
    probe.facts = facts;
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Priority {
    pub value: u8,
    pub source: String,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Nvic {
    /// Selected definition, retained for decoding; actual identity still requires CPUID evidence.
    pub catalogue_cpu: String,
    pub priority_bits: Option<Priority>,
    pub configured_priority_bits: Option<u64>,
    pub svd_priority_bits: Option<u32>,
    pub svd_source: Option<String>,
    pub svd_cpu: Option<String>,
    pub interrupts: Option<Vec<crate::svd::Interrupt>>,
    pub notes: Vec<String>,
}

/// SVD and explicit configuration are declarations with their own source, never observed ID fields.
pub fn nvic_metadata(
    device: Option<&crate::svd::Device>,
    source: &str,
    configured: Option<u64>,
    cpu: &str,
    upper: Option<u64>,
) -> Nvic {
    let mut n = Nvic {
        catalogue_cpu: cpu.into(),
        priority_bits: None,
        configured_priority_bits: configured,
        svd_priority_bits: device.and_then(|d| d.nvic_priority_bits),
        svd_source: device.map(|_| source.into()),
        svd_cpu: device.and_then(|d| d.cpu_name.clone()),
        interrupts: None,
        notes: vec![],
    };
    let expected = match cpu {
        "cortex-m3" => "CM3",
        "cortex-m4" => "CM4",
        "cortex-m7" => "CM7",
        _ => "",
    };
    if let Some(d) = device {
        if d.cpu_name.as_deref().is_some_and(|name| name != expected) {
            n.notes.push(format!(
                "SVD CPU {:?} differs from {cpu}; its priority/IRQ declarations are not used",
                d.cpu_name
            ));
        } else {
            n.interrupts = Some(d.interrupts.clone());
            if let Some(bits) = d.nvic_priority_bits {
                if (3..=8).contains(&bits) {
                    n.priority_bits = Some(Priority {
                        value: bits as u8,
                        source: format!("SVD {source}: /device/cpu/nvicPrioBits"),
                    });
                } else {
                    n.notes.push(format!(
                        "SVD nvicPrioBits={bits} is outside the adapted 3..8 range; Unknown"
                    ));
                }
            }
            for irq in &d.interrupts {
                if irq.value >= 240 || upper.is_some_and(|bound| u64::from(irq.value) >= bound) {
                    n.notes.push(format!("SVD IRQ {}:{}={} exceeds the adapted/observed interrupt range; no instances inferred from this entry", irq.peripheral, irq.name, irq.value));
                }
            }
        }
    }
    if let Some(bits) = configured {
        if !(3..=8).contains(&bits) {
            n.notes.push(format!(
                "Configured nvic.priority_bits={bits} is outside adapted 3..8 range; Unknown"
            ));
        } else if let Some(p) = &n.priority_bits {
            if u64::from(p.value) != bits {
                n.notes.push(format!("NVIC priority conflict: SVD={} configuration={bits}; SVD takes precedence; both declarations retained", p.value));
            }
        } else {
            n.priority_bits = Some(Priority {
                value: bits as u8,
                source: "configuration: registers.facts.nvic.priority_bits".into(),
            });
        }
    }
    n
}

#[cfg(test)]
mod tests;
