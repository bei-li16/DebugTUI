//! Fixed R52 memory-aperture capability proof, separate from CPU instruction permissions.
use super::{
    Context, Reader, Sample, SampleView, State,
    capabilities::{Fact, Probe},
    provenance::{Acquisition, ByteOrder, Phase, Route},
};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub struct Descriptor {
    pub id: &'static str,
    pub component: &'static str,
    pub offset: u64,
    pub bits: u16,
}
macro_rules! d {
    ($id:literal,$component:literal,$offset:literal,$bits:literal) => {
        Descriptor {
            id: $id,
            component: $component,
            offset: $offset,
            bits: $bits,
        }
    };
}
pub const DEBUG: &[Descriptor] = &[
    d!("ed_midr", "debug_external", 0xd00, 32),
    d!("edcidr0", "debug_external", 0xff0, 32),
    d!("edcidr1", "debug_external", 0xff4, 32),
    d!("edcidr2", "debug_external", 0xff8, 32),
    d!("edcidr3", "debug_external", 0xffc, 32),
    d!("eddevaff0", "debug_external", 0xfa8, 32),
    d!("eddevaff1", "debug_external", 0xfac, 32),
    d!("eddfr_word0", "debug_external", 0xd28, 32),
    d!("eddfr_word1", "debug_external", 0xd2c, 32),
];
pub const GICD: &[Descriptor] = &[
    d!("gicd_iidr", "gicd", 8, 32),
    d!("gicd_cidr0", "gicd", 0xfff0, 32),
    d!("gicd_cidr1", "gicd", 0xfff4, 32),
    d!("gicd_cidr2", "gicd", 0xfff8, 32),
    d!("gicd_cidr3", "gicd", 0xfffc, 32),
    d!("gicd_typer", "gicd", 4, 32),
];
pub const GICR: &[Descriptor] = &[
    d!("gicr_iidr", "gicr", 4, 32),
    d!("gicr_cidr0", "gicr", 0xfff0, 32),
    d!("gicr_cidr1", "gicr", 0xfff4, 32),
    d!("gicr_cidr2", "gicr", 0xfff8, 32),
    d!("gicr_cidr3", "gicr", 0xfffc, 32),
    d!("gicr_typer", "gicr", 8, 64),
];
pub fn descriptors(component: &str) -> &'static [Descriptor] {
    match component {
        "debug_external" => DEBUG,
        "gicd" => GICD,
        "gicr" => GICR,
        _ => &[],
    }
}
pub fn sample_id(d: &Descriptor, after: bool) -> String {
    format!("mmio_probe.{}{}", d.id, if after { ".after" } else { "" })
}
#[derive(PartialEq, Eq)]
enum Transport {
    Gdb(String, String, ByteOrder),
    Tcl(String, String, String, String, ByteOrder),
}
#[derive(PartialEq, Eq)]
pub(super) struct Aperture {
    pub(super) base: u64,
    transport: Transport,
}

pub(super) fn observed<'a>(
    probe: &'a Probe,
    d: &Descriptor,
    after: bool,
) -> Result<(&'a Sample, u64, Aperture), String> {
    let id = sample_id(d, after);
    let mut samples = probe.samples.iter().filter(|s| s.id == id);
    let s = samples
        .next()
        .ok_or_else(|| format!("{id}: no current observation"))?;
    if samples.next().is_some() {
        return Err(format!("{id}: ambiguous duplicate observations"));
    }
    let value = s
        .value
        .as_ref()
        .ok_or_else(|| format!("{id}: no raw value"))?;
    let p = s
        .provenance
        .as_ref()
        .ok_or_else(|| format!("{id}: no request provenance"))?;
    let a = p
        .access
        .as_ref()
        .ok_or_else(|| format!("{id}: no completed request"))?;
    let core = format!("core:{}", probe.context.core);
    let owner_ok = if super::stm::component(d.component) {
        s.owner
            .as_deref()
            .is_some_and(|o| o.starts_with("chip:") && o.len() > 5)
    } else if d.component == "gicd" {
        s.owner
            .as_deref()
            .is_some_and(|o| o.starts_with("cluster:") && o.len() > 8)
    } else {
        s.owner.as_deref() == Some(&core)
    };
    if s.state != State::Valid
        || s.context != probe.context
        || !owner_ok
        || probe.context.frame != 0
        || s.view != SampleView::PhysicalCore
        || value.bits != d.bits
        || s.source != format!("mmio:{}", d.component)
        || p.acquisition != Acquisition::CapabilityProbe
        || p.catalogue_reader
            != (Reader::Mmio {
                component: d.component.into(),
                offset: d.offset,
                require_owner_mapping: true,
            })
        || a.context != probe.context
        || a.phase != Phase::Responded
        || !a.completed_ms.is_some_and(|n| n >= a.timestamp_ms)
    {
        return Err(format!(
            "{id}: width, owner, context or physical memory request is unproven"
        ));
    }
    let (address, transport) = match &a.route {
        Route::GdbMemory {
            endpoint: Some(endpoint),
            configured_endpoint,
            address,
            bits,
            byte_order,
        } if !endpoint.is_empty() && endpoint == configured_endpoint && *bits == d.bits => (
            address,
            Transport::Gdb(endpoint.clone(), configured_endpoint.clone(), *byte_order),
        ),
        Route::TclMemory {
            endpoint,
            target,
            channel,
            configuration_source,
            address,
            bits,
            bus_width,
            count,
            byte_order,
            atomic,
        } if !endpoint.is_empty()
            && !target.is_empty()
            && !channel.is_empty()
            && !configuration_source.is_empty()
            && *bits == d.bits
            && *bus_width == 32
            && *count == d.bits / 32
            && !atomic =>
        {
            (
                address,
                Transport::Tcl(
                    endpoint.clone(),
                    target.clone(),
                    channel.clone(),
                    configuration_source.clone(),
                    *byte_order,
                ),
            )
        }
        _ => {
            return Err(format!(
                "{id}: actual memory transport missing or inconsistent"
            ));
        }
    };
    let address = address
        .strip_prefix("0x")
        .and_then(|s| u64::from_str_radix(s, 16).ok())
        .ok_or_else(|| format!("{id}: address missing"))?;
    let base = address
        .checked_sub(d.offset)
        .ok_or_else(|| format!("{id}: address precedes aperture"))?;
    if !base.is_multiple_of(4) {
        return Err(format!("{id}: unaligned aperture"));
    }
    let raw = u64::try_from(value.integer()?).map_err(|_| format!("{id}: raw exceeds 64 bits"))?;
    Ok((s, raw, Aperture { base, transport }))
}
pub fn raw(probe: &Probe, id: &str, after: bool) -> Result<u64, String> {
    let d = DEBUG
        .iter()
        .chain(GICD)
        .chain(GICR)
        .find(|d| d.id == id)
        .ok_or("Not a fixed MMIO identity register")?;
    observed(probe, d, after).map(|(_, n, _)| n)
}
/// Reject the first mismatching identity before submitting later aperture reads.
pub fn guard(probe: &Probe, d: &Descriptor) -> Result<(), String> {
    let n = raw(probe, d.id, false)?;
    let expected = match d.id {
        "ed_midr" => {
            if n & 0xff0ffff0 != 0x410fd130 || probe.raw("midr").is_some_and(|m| m != n) {
                return Err("External MIDR is unadapted or conflicts with CPU MIDR".into());
            }
            None
        }
        "eddevaff0" => {
            if n & 255 > 3 || n & 0x80000000 == 0 {
                return Err("Unadapted R52 external affinity".into());
            }
            None
        }
        "eddevaff1" => Some(0),
        "gicd_iidr" | "gicr_iidr" => {
            let midr = raw(probe, "ed_midr", false)?;
            Some(0x0100043b | ((midr >> 20) & 15) << 16 | (midr & 15) << 12)
        }
        "edcidr0" | "gicd_cidr0" | "gicr_cidr0" => Some(0x0d),
        "edcidr1" => Some(0x90),
        "gicd_cidr1" | "gicr_cidr1" => Some(0xf0),
        "edcidr2" | "gicd_cidr2" | "gicr_cidr2" => Some(5),
        "edcidr3" | "gicd_cidr3" | "gicr_cidr3" => Some(0xb1),
        _ => None,
    };
    if expected.is_some_and(|value| n != value) {
        return Err(format!("{}: unexpected component identity", d.id));
    }
    Ok(())
}
/// Stage guard, before any optional capacity address is read.
pub fn identity(probe: &Probe, component: &str) -> Result<(), String> {
    let midr = raw(probe, "ed_midr", false)?;
    if midr & 0xff0ffff0 != 0x410fd130 {
        return Err("External MIDR is not an adapted Arm/D13 R52".into());
    }
    if probe.raw("midr").is_some_and(|n| n != midr) {
        return Err("External MIDR conflicts with current CPU MIDR".into());
    }
    if component != "debug_external" {
        let id = if component == "gicd" {
            "gicd_iidr"
        } else {
            "gicr_iidr"
        };
        let expected = 0x0100043b | ((midr >> 20) & 15) << 16 | (midr & 15) << 12;
        if raw(probe, id, false)? != expected {
            return Err("GIC IIDR product/implementer/revision differs from R52 MIDR".into());
        }
    }
    let ids: &[(&str, u64)] = match component {
        "debug_external" => &[
            ("edcidr0", 0x0d),
            ("edcidr1", 0x90),
            ("edcidr2", 5),
            ("edcidr3", 0xb1),
        ],
        "gicd" => &[
            ("gicd_cidr0", 0x0d),
            ("gicd_cidr1", 0xf0),
            ("gicd_cidr2", 5),
            ("gicd_cidr3", 0xb1),
        ],
        "gicr" => &[
            ("gicr_cidr0", 0x0d),
            ("gicr_cidr1", 0xf0),
            ("gicr_cidr2", 5),
            ("gicr_cidr3", 0xb1),
        ],
        _ => return Err("Unadapted MMIO component".into()),
    };
    for &(id, value) in ids {
        if raw(probe, id, false)? != value {
            return Err(format!("{id}: unexpected component identity"));
        }
    }
    Ok(())
}
/// Validate independent before/after observations, including their route and owner.
pub fn stable(probe: &Probe, component: &str) -> Result<(), String> {
    identity(probe, "debug_external")?;
    identity(probe, component)?;
    let mut common = None;
    let mut owner = None;
    let mut epoch = None;
    let mut before_end = 0;
    let mut after_start = u64::MAX;
    for d in descriptors(component) {
        let (before, n, route) = observed(probe, d, false)?;
        let (after, m, post) = observed(probe, d, true)?;
        if n != m
            || route != post
            || before.owner != after.owner
            || before.owner_generation != after.owner_generation
            || before
                .provenance
                .as_ref()
                .unwrap()
                .access
                .as_ref()
                .unwrap()
                .completed_ms
                .unwrap()
                > after
                    .provenance
                    .as_ref()
                    .unwrap()
                    .access
                    .as_ref()
                    .unwrap()
                    .timestamp_ms
        {
            return Err(format!(
                "{}: changed value/route/owner or invalid request order",
                d.id
            ));
        }
        if common.as_ref().is_some_and(|r| r != &route)
            || owner.as_ref().is_some_and(|o| o != &before.owner)
            || epoch
                .as_ref()
                .is_some_and(|e| e != &before.owner_generation)
        {
            return Err(format!(
                "{}: observations use different apertures or owners",
                d.id
            ));
        }
        common = Some(route);
        owner = Some(before.owner.clone());
        epoch = Some(before.owner_generation);
        before_end = before_end.max(
            before
                .provenance
                .as_ref()
                .unwrap()
                .access
                .as_ref()
                .unwrap()
                .completed_ms
                .unwrap(),
        );
        after_start = after_start.min(
            after
                .provenance
                .as_ref()
                .unwrap()
                .access
                .as_ref()
                .unwrap()
                .timestamp_ms,
        );
    }
    if before_end > after_start {
        return Err("Identity/capacity before/after request groups overlap".into());
    }
    Ok(())
}
pub fn capacities(probe: &Probe, component: &str) -> Result<BTreeMap<String, Fact>, String> {
    stable(probe, "debug_external")?;
    let aff = raw(probe, "eddevaff0", false)?;
    if aff & 255 > 3 || aff & 0x80000000 == 0 || raw(probe, "eddevaff1", false)? != 0 {
        return Err("Unadapted R52 external affinity; no core-name inference".into());
    }
    stable(probe, component)?;
    if component != "debug_external" {
        let debug_end = DEBUG
            .iter()
            .map(|d| {
                observed(probe, d, false).map(|(s, _, _)| {
                    s.provenance
                        .as_ref()
                        .unwrap()
                        .access
                        .as_ref()
                        .unwrap()
                        .completed_ms
                        .unwrap()
                })
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .max()
            .unwrap();
        let debug_after = DEBUG
            .iter()
            .map(|d| {
                observed(probe, d, true).map(|(s, _, _)| {
                    s.provenance
                        .as_ref()
                        .unwrap()
                        .access
                        .as_ref()
                        .unwrap()
                        .timestamp_ms
                })
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .min()
            .unwrap();
        for d in descriptors(component) {
            for after in [false, true] {
                let (s, _, _) = observed(probe, d, after)?;
                let access = s.provenance.as_ref().unwrap().access.as_ref().unwrap();
                if access.timestamp_ms < debug_end || access.completed_ms.unwrap() > debug_after {
                    return Err(
                        "GIC proof is outside the enclosing Debug identity observations".into(),
                    );
                }
            }
        }
    }
    let mut result = BTreeMap::new();
    let mut fact = |key: &str, value: u64, id: &str, detail: &str| {
        result.insert(
            key.into(),
            Fact {
                value,
                register: id.into(),
                source: format!("mmio:{component}"),
                detail: detail.into(),
            },
        );
    };
    match component {
        "debug_external" => {
            // Low four EDDFR bits are explicitly Reserved/UNKNOWN on R52.
            if raw(probe, "eddfr_word0", false)? != 0
                || raw(probe, "eddfr_word1", false)? & !15 != 0x10707100
            {
                return Err("Unadapted EDDFR layout; no comparator or architecture guess".into());
            }
            fact(
                "debug_external.present",
                1,
                "ed_midr",
                "Fresh R52 external MIDR/CoreSight class 9 and stable exact memory route; board owner association remains configuration",
            );
            fact(
                "debug_external.breakpoints",
                8,
                "eddfr_word1",
                "R52 BRPs[15:12]=7 at D2C, independently re-read; not inferred from GDB names",
            );
            fact(
                "debug_external.watchpoints",
                8,
                "eddfr_word1",
                "R52 WRPs[23:20]=7 at D2C; does not grant write permissions",
            );
            fact(
                "debug_external.context_breakpoints",
                2,
                "eddfr_word1",
                "R52 CTX_CMPs[31:28]=1; no EDPFR word-pair inference",
            );
            fact(
                "debug_external.aff0",
                aff & 255,
                "eddevaff0",
                "Observed affinity, independent of logical core name",
            );
        }
        "gicd" => {
            let typer = raw(probe, "gicd_typer", false)?;
            let lines = typer & 31;
            if typer & !31 != 0x02480000 || !(1..=30).contains(&lines) {
                return Err("Unadapted GICD_TYPER fixed fields or ITLinesNumber".into());
            }
            fact(
                "gicd.present",
                1,
                "gicd_iidr",
                "Fresh R52 GIC IIDR/CIDR, consistent MIDR revision, stable cluster owner/route",
            );
            fact(
                "gicd.interrupts",
                32 * (lines + 1),
                "gicd_typer",
                "Observed total INTID capacity including SGI/PPI; maximum R52 INTID991, not CPU-interface AP capacity",
            );
        }
        "gicr" => {
            let typer = raw(probe, "gicr_typer", false)?;
            let target = (typer >> 32) & 255;
            if typer & !0xff0000ff10 != 0 || (typer >> 8) & 0xffff != target || target != aff & 255
            {
                return Err("GICR_TYPER affinity/ProcessorNumber differs from this Debug core or unadapted LPI fields".into());
            }
            fact(
                "gicr.present",
                1,
                "gicr_iidr",
                "Fresh R52 GIC IIDR/CIDR and GICR target matching external Debug affinity; addresses are explicitly supplied",
            );
            fact(
                "gicr.target_id",
                target,
                "gicr_typer",
                "Observed Aff0/ProcessorNumber; never derives a base, core label or number of cores",
            );
        }
        _ => return Err("Unadapted MMIO component".into()),
    }
    Ok(result)
}
pub fn decode(probe: &Probe) -> (BTreeMap<String, Fact>, Vec<String>) {
    let mut facts = BTreeMap::new();
    let mut notes = vec![];
    if !probe.samples.iter().any(|s| s.source.starts_with("mmio:")) {
        return (facts, notes);
    }
    for component in ["debug_external", "gicd", "gicr"] {
        if !probe
            .samples
            .iter()
            .any(|s| s.source == format!("mmio:{component}"))
        {
            continue;
        }
        match capacities(probe, component) {
            Ok(observed) => facts.extend(observed),
            Err(error) => notes.push(format!(
                "Decode: MMIO {component}: {error}; physical capabilities remain unknown"
            )),
        }
    }
    (facts, notes)
}
pub fn applicable(probe: &Probe, context: &Context, component: &str) -> bool {
    probe.context == *context && capacities(probe, component).is_ok()
}

#[cfg(test)]
mod tests;
