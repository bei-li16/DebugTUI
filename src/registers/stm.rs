//! STM v1.1 control aperture, identified independently of the CPU or stimulus ports.
use super::{
    Component, Context,
    capabilities::{Fact, Probe},
    mmio_probe::{self, Aperture, Descriptor},
};
use std::collections::BTreeMap;

const fn d(id: &'static str, component: &'static str, offset: u64) -> Descriptor {
    Descriptor {
        id,
        component,
        offset,
        bits: 32,
    }
}
pub const CORE: &[Descriptor] = &[
    d("stm_cidr0", "stm", 0xff0),
    d("stm_cidr1", "stm", 0xff4),
    d("stm_cidr2", "stm", 0xff8),
    d("stm_cidr3", "stm", 0xffc),
    d("stm_devarch", "stm", 0xfbc),
    d("stm_devtype", "stm", 0xfcc),
    d("stm_pidr0", "stm", 0xfe0),
    d("stm_pidr1", "stm", 0xfe4),
    d("stm_pidr2", "stm", 0xfe8),
    d("stm_pidr3", "stm", 0xfec),
    d("stm_pidr4", "stm", 0xfd0),
    d("stm_devid", "stm", 0xfc8),
    d("stm_feat1r", "stm", 0xea0),
    d("stm_feat2r", "stm", 0xea4),
    d("stm_feat3r", "stm", 0xea8),
];
pub const HWE: &[Descriptor] = &[
    d("stm_heidr", "stm_hwe", 0xdfc),
    d("stm_hefeat1r", "stm_hwe", 0xdf8),
];
pub const DMA: &[Descriptor] = &[d("stm_dmaidr", "stm_dma", 0xcfc)];
pub fn component(name: &str) -> bool {
    matches!(name, "stm" | "stm_hwe" | "stm_dma")
}
pub fn descriptors(name: &str) -> &'static [Descriptor] {
    match name {
        "stm" => CORE,
        "stm_hwe" => HWE,
        "stm_dma" => DMA,
        _ => &[],
    }
}
fn observation<'a>(
    probe: &'a Probe,
    id: &str,
    after: bool,
) -> Result<(&'a super::Sample, u64, Aperture), String> {
    let descriptor = CORE
        .iter()
        .chain(HWE)
        .chain(DMA)
        .find(|d| d.id == id)
        .ok_or("Unknown STM proof register")?;
    mmio_probe::observed(probe, descriptor, after)
}
pub fn raw(probe: &Probe, id: &str) -> Result<u64, String> {
    observation(probe, id, false).map(|(_, v, _)| v)
}
pub fn consistent_pair(probe: &Probe, descriptor: &Descriptor) -> Result<(), String> {
    let (before, value, location) = observation(probe, descriptor.id, false)?;
    let (after, post, other) = observation(probe, descriptor.id, true)?;
    if value != post
        || location != other
        || before.owner != after.owner
        || before.owner_generation != after.owner_generation
    {
        return Err(format!(
            "{}: STM identity/control changed during verification",
            descriptor.id
        ));
    }
    Ok(())
}
pub fn guard(probe: &Probe, descriptor: &Descriptor) -> Result<(), String> {
    let value = raw(probe, descriptor.id)?;
    let expected = match descriptor.id {
        "stm_cidr0" => Some(0x0d),
        "stm_cidr1" => Some(0x90),
        "stm_cidr2" => Some(5),
        "stm_cidr3" => Some(0xb1),
        "stm_devarch" => Some(0x47710a63),
        "stm_devtype" => Some(0x63),
        "stm_pidr1" => {
            if value & 0xf0 != 0xb0 {
                return Err("STM designer differs from Arm".into());
            }
            None
        }
        "stm_pidr2" => {
            if value & 15 != 0xb {
                return Err("STM JEDEC designer encoding differs from Arm".into());
            }
            None
        }
        "stm_pidr4" => {
            if value & 15 != 4 || value >> 4 != 0 {
                return Err("STM identity does not describe the adapted 4 KiB aperture".into());
            }
            None
        }
        "stm_devid" => {
            if !(1..=65536).contains(&(value & 0x1ffff)) {
                return Err("STM stimulus port capacity is unknown".into());
            }
            None
        }
        "stm_heidr" => {
            if value & 0xff != 0x01 && value & 0xff != 0x11 || value >> 8 != 0 {
                return Err("STM hardware-event control class/revision is unadapted".into());
            }
            None
        }
        "stm_hefeat1r" => {
            if (value >> 15) & 0x1ff > 256
                || ((value >> 28) & 7) > 5
                || (raw(probe, "stm_heidr")? & 0xf0 == 0 && (value >> 28) & 7 != 0)
            {
                return Err("STM hardware-event capacity/mux encoding is unadapted".into());
            }
            None
        }
        "stm_dmaidr" => Some(2),
        _ => None,
    };
    if expected.is_some_and(|n| value != n) {
        return Err(format!(
            "{}: not an adapted STM v1.1 identity/control",
            descriptor.id
        ));
    }
    if descriptor.id.contains("pidr") && value > 255 {
        return Err("STM PIDR must contain a single identity byte".into());
    }
    Ok(())
}
fn stable_group(probe: &Probe, name: &str) -> Result<Aperture, String> {
    let mut geometry = None;
    let mut owner = None;
    let mut epoch = None;
    let mut before_end = 0;
    let mut after_start = u64::MAX;
    for d in descriptors(name) {
        guard(probe, d)?;
        let (before, value, location) = observation(probe, d.id, false)?;
        let (after, post, other) = observation(probe, d.id, true)?;
        if value != post
            || location != other
            || before.owner != after.owner
            || before.owner_generation != after.owner_generation
            || geometry.as_ref().is_some_and(|g| g != &location)
            || owner.as_ref().is_some_and(|o| o != &before.owner)
            || epoch
                .as_ref()
                .is_some_and(|e| e != &before.owner_generation)
        {
            return Err("STM proof changed value, aperture, transport, owner or lifetime".into());
        }
        let first = before.provenance.as_ref().unwrap().access.as_ref().unwrap();
        let last = after.provenance.as_ref().unwrap().access.as_ref().unwrap();
        before_end = before_end.max(first.completed_ms.unwrap());
        after_start = after_start.min(last.timestamp_ms);
        geometry = Some(location);
        owner = Some(before.owner.clone());
        epoch = Some(before.owner_generation);
    }
    if before_end > after_start {
        return Err("STM identity/capacity request groups overlap".into());
    }
    let location = geometry.ok_or("No STM component proof")?;
    if !location.base.is_multiple_of(0x1000) {
        return Err("STM control aperture must be explicitly 4 KiB aligned".into());
    }
    Ok(location)
}
pub fn proof(probe: &Probe, name: &str) -> Result<(), String> {
    if !component(name) {
        return Err("Unknown STM component".into());
    }
    let core = stable_group(probe, "stm")?;
    if name != "stm" {
        if stable_group(probe, name)? != core {
            return Err("STM optional controls are not in the same explicit aperture/route".into());
        }
        let (parent, _, _) = observation(probe, CORE[0].id, false)?;
        let (optional, _, _) = observation(probe, descriptors(name)[0].id, false)?;
        if parent.owner != optional.owner || parent.owner_generation != optional.owner_generation {
            return Err("STM optional controls use a different owner or shared lifetime".into());
        }
        let start = CORE
            .iter()
            .map(|d| {
                observation(probe, d.id, false).map(|(s, _, _)| {
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
        let end = CORE
            .iter()
            .map(|d| {
                observation(probe, d.id, true).map(|(s, _, _)| {
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
        for d in descriptors(name) {
            for after in [false, true] {
                let (sample, _, _) = observation(probe, d.id, after)?;
                let access = sample.provenance.as_ref().unwrap().access.as_ref().unwrap();
                if access.timestamp_ms < start || access.completed_ms.unwrap() > end {
                    return Err(
                        "STM optional proof is outside enclosing core identity observations".into(),
                    );
                }
            }
        }
    }
    Ok(())
}
pub fn applicable(
    probe: &Probe,
    context: &Context,
    name: &str,
    owner: Option<&str>,
    binding: &Component,
) -> bool {
    if probe.context != *context || proof(probe, name).is_err() {
        return false;
    }
    let Ok((sample, _, geometry)) = observation(probe, descriptors(name)[0].id, false) else {
        return false;
    };
    if sample.owner.as_deref() != owner || geometry.base != binding.base {
        return false;
    }
    let access = sample.provenance.as_ref().unwrap().access.as_ref().unwrap();
    match &access.route {
        super::provenance::Route::GdbMemory { byte_order, .. } => {
            binding.channel.is_empty()
                && (*byte_order == super::provenance::ByteOrder::Little) == binding.little_endian
        }
        super::provenance::Route::TclMemory {
            channel,
            byte_order,
            ..
        } => {
            *channel == binding.channel
                && (*byte_order == super::provenance::ByteOrder::Little) == binding.little_endian
        }
        _ => false,
    }
}
pub fn readable(name: &str, offset: u64, bits: u16) -> bool {
    bits == 32
        && match name {
            "stm" => matches!(
                offset,
                0xe00
                    | 0xe20
                    | 0xe60
                    | 0xe64
                    | 0xe68
                    | 0xe6c
                    | 0xe70
                    | 0xe80
                    | 0xe8c
                    | 0xe90
                    | 0xe94
                    | 0xea0
                    | 0xea4
                    | 0xea8
                    | 0xf00
                    | 0xfa0
                    | 0xfa4
                    | 0xfb4
                    | 0xfb8
                    | 0xfbc
                    | 0xfc8
                    | 0xfcc
                    | 0xfd0
                    | 0xfe0
                    | 0xfe4
                    | 0xfe8
                    | 0xfec
                    | 0xff0
                    | 0xff4
                    | 0xff8
                    | 0xffc
            ),
            "stm_hwe" => matches!(
                offset,
                0xd00 | 0xd20 | 0xd60 | 0xd64 | 0xd68 | 0xdf4 | 0xdf8 | 0xdfc
            ),
            "stm_dma" => matches!(offset, 0xc0c | 0xc10 | 0xcfc),
            _ => false,
        }
}
/// Refuse even a custom catalogue's claim that optional or unadapted slots exist.
pub fn data_allowed(probe: &Probe, name: &str, offset: u64, bits: u16) -> bool {
    if !readable(name, offset, bits) || proof(probe, name).is_err() {
        return false;
    }
    // Configuration layouts below the common identification area are adapted
    // for STM-500. Another Arm STM v1.1 part can expose its identity/features.
    let part = raw(probe, "stm_pidr0").unwrap() | ((raw(probe, "stm_pidr1").unwrap() & 15) << 8);
    if offset < 0xf00 && !matches!(offset, 0xea0 | 0xea4 | 0xea8) && part != 0x963 {
        return false;
    }
    let f1 = raw(probe, "stm_feat1r").unwrap();
    let f2 = raw(probe, "stm_feat2r").unwrap();
    match (name, offset) {
        ("stm", 0xe00) => f2 & 4 == 0,
        ("stm", 0xe20) => f2 & 3 == 2,
        ("stm", 0xe68 | 0xe6c) => f2 & 0x40 != 0,
        ("stm", 0xe70) => matches!((f1 >> 14) & 3, 1 | 2),
        ("stm", 0xe8c) => f1 & 0x40 != 0,
        ("stm", 0xe90) => matches!((f1 >> 8) & 3, 2 | 3),
        ("stm_hwe", 0xd20) => raw(probe, "stm_hefeat1r").unwrap() & 1 != 0,
        ("stm_hwe", 0xd60) => (raw(probe, "stm_hefeat1r").unwrap() >> 15) & 0x1ff > 32,
        ("stm_hwe", 0xd68) => (raw(probe, "stm_hefeat1r").unwrap() >> 28) & 7 != 0,
        _ => true,
    }
}
pub fn decode(probe: &Probe) -> (BTreeMap<String, Fact>, Vec<String>) {
    let mut result = BTreeMap::new();
    let mut notes = vec![];
    for name in ["stm", "stm_hwe", "stm_dma"] {
        if !probe
            .samples
            .iter()
            .any(|s| s.source == format!("mmio:{name}"))
        {
            continue;
        }
        if let Err(error) = proof(probe, name) {
            notes.push(format!(
                "Decode: STM {name}: {error}; capabilities remain unknown"
            ));
            continue;
        }
        let mut fact = |key: &str, value: u64, id: &str| {
            result.insert(key.into(),Fact {value,register:id.into(),source:format!("mmio:{name}"),detail:"Fresh STM v1.1 component/control proof; physical board mapping remains configuration".into()});
        };
        if name == "stm" {
            let f1 = raw(probe, "stm_feat1r").unwrap();
            let f2 = raw(probe, "stm_feat2r").unwrap();
            fact("stm.present", 1, "stm_devarch");
            fact(
                "stm.stimulus_ports",
                raw(probe, "stm_devid").unwrap() & 0x1ffff,
                "stm_devid",
            );
            fact(
                "stm.masters",
                (raw(probe, "stm_feat3r").unwrap() & 127) + 1,
                "stm_feat3r",
            );
            fact(
                "stm.part",
                raw(probe, "stm_pidr0").unwrap() | ((raw(probe, "stm_pidr1").unwrap() & 15) << 8),
                "stm_pidr0",
            );
            for (key, value, id) in [
                ("stm.sper", u64::from(f2 & 4 == 0), "stm_feat2r"),
                ("stm.override", (f2 >> 6) & 1, "stm_feat2r"),
                ("stm.timestamp_frequency", (f1 >> 6) & 1, "stm_feat1r"),
            ] {
                fact(key, value, id);
            }
            if matches!(f2 & 3, 1 | 2) {
                fact("stm.sptrigger", u64::from(f2 & 3 == 2), "stm_feat2r");
            }
            if (f1 >> 8) & 3 != 0 {
                fact("stm.sync", u64::from((f1 >> 8) & 3 >= 2), "stm_feat1r");
            }
            if matches!((f1 >> 14) & 3, 1 | 2) {
                fact("stm.trigger_control", (f1 >> 14) & 3, "stm_feat1r");
            }
        } else if name == "stm_hwe" {
            let value = raw(probe, "stm_hefeat1r").unwrap();
            fact("stm_hwe.present", 1, "stm_heidr");
            fact("stm_hwe.events", (value >> 15) & 0x1ff, "stm_hefeat1r");
            fact("stm_hwe.trigger", value & 1, "stm_hefeat1r");
            fact("stm_hwe.mux", (value >> 28) & 7, "stm_hefeat1r");
        } else {
            fact("stm_dma.present", 1, "stm_dmaidr");
        }
    }
    (result, notes)
}

#[cfg(test)]
mod tests;
