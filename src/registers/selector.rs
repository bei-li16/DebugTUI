//! Fixed R52 selector transactions. Arbitrary encodings/writes are never accepted.
use super::{Context, RawValue};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    MpuEl1,
    MpuEl2,
    Pmu,
}
impl Kind {
    pub fn ids(self, index: u8) -> [String; 2] {
        match self {
            Self::MpuEl1 => [format!("prbar{index}"), format!("prlar{index}")],
            Self::MpuEl2 => [format!("hprbar{index}"), format!("hprlar{index}")],
            Self::Pmu => [format!("pmevtyper{index}"), format!("pmevcntr{index}")],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub context: Context,
    pub kind: Kind,
    pub index: u8,
}
#[derive(Clone, Debug)]
pub struct Plan {
    pub kind: Kind,
    pub index: u8,
    pub count: u8,
    pub mode: u8,
    pub selector: &'static str,
    pub ids: [String; 2],
    selector_encoding: &'static str,
    data: [&'static str; 2],
    capability: &'static str,
    count_shift: u8,
    count_mask: u8,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Evidence {
    pub original: RawValue,
    pub restored: RawValue,
    pub selected: u8,
    pub values: [RawValue; 2],
    pub synchronization: String,
}
pub const NATIVE_COMMAND: &str = "aarch64 r52_select";
pub const NATIVE_PROTOCOL: &str = "debugtui-r52-selector-1 external-identity current-el2 dspsr dlr fresh-capacity selector-readback isb scratch-readback stop-on-fault";

/// Bounded request metadata, never a permission grant based on saved CPSR.
pub struct NativePlan {
    pub kind: Kind,
    pub index: u8,
    pub count: u8,
    pub selector: &'static str,
    pub ids: [String; 2],
}
impl NativePlan {
    pub fn new(kind: Kind, index: u8, count: u64) -> Result<Self, String> {
        let count = u8::try_from(count).map_err(|_| "Invalid selector count")?;
        if kind == Kind::Pmu || !matches!(count, 16 | 20 | 24) || index >= count {
            return Err("Native selector only supports adapted R52 MPU banks and indices".into());
        }
        Ok(Self {
            kind,
            index,
            count,
            selector: if kind == Kind::MpuEl2 {
                "hprselr"
            } else {
                "prselr"
            },
            ids: kind.ids(index),
        })
    }
    pub fn script(&self) -> String {
        format!(
            "if {{[catch {{aarch64 debugtui_r52_selector_protocol}} __dt_r52_protocol] || $__dt_r52_protocol ne \"{NATIVE_PROTOCOL}\"}} {{error \"R52 selector adapter protocol unsupported\"}}; \
             if {{[catch {{{NATIVE_COMMAND} {} {} {}}} __dt_r52_result]}} {{set __dt_r52_code $::errorCode; \
             if {{[[target current] curstate] ne \"halted\"}} {{error \"Core state restoration failed: R52 selector target state unknown\"}}; \
             if {{$__dt_r52_code eq [list OpenOCD -300]}} {{error \"R52 selector reader unsupported: $__dt_r52_result\"}}; error $__dt_r52_result}}; set __dt_r52_result",
            if self.kind == Kind::MpuEl2 {
                "el2"
            } else {
                "el1"
            },
            self.index,
            self.count
        )
    }
    pub fn parse(&self, text: &str) -> Result<(Evidence, super::r52_core::Evidence), String> {
        let words: Vec<_> = text.split_whitespace().collect();
        if words.len() != 22
            || [words[12], words[14], words[16], words[18], words[20]]
                != ["original", "restored", "selected", "base", "limit"]
        {
            return Err("Malformed R52 selector current Debug response".into());
        }
        let request = super::r52_core::Request {
            name: self.ids[0].clone(),
            bank: if self.kind == Kind::MpuEl2 {
                super::r52_core::CapacityBank::El2
            } else {
                super::r52_core::CapacityBank::El1
            },
            region: Some(self.index),
        };
        let response = super::r52_core::Response::parse(
            &format!("{} value {}", words[..12].join(" "), words[19]),
            &request,
        )?;
        let actual_count = response.evidence.capacity.integer()?
            >> if self.kind == Kind::MpuEl2 { 0 } else { 8 }
            & 255;
        if actual_count != u128::from(self.count) {
            return Err("R52 selector fresh capacity contradicts its request".into());
        }
        let exact = |raw: &str| {
            if raw.len() != 10
                || !raw.starts_with("0x")
                || !raw[2..].bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err("R52 selector response requires exactly 32 raw bits".into());
            }
            RawValue::parse(raw, 32)
        };
        let original = exact(words[13])?;
        let restored = exact(words[15])?;
        if original != restored
            || original.integer()? >= actual_count
            || exact(words[17])?.integer()? != u128::from(self.index)
        {
            return Err(
                "R52 selector selection/restoration evidence contradicts its request".into(),
            );
        }
        Ok((
            Evidence {
                original,
                restored,
                selected: self.index,
                values: [response.value, exact(words[21])?],
                synchronization:
                    "Genuine ISB in native R52 transaction; selector restore readback verified"
                        .into(),
            },
            response.evidence,
        ))
    }
}
pub fn register_selection(id: &str) -> Option<(Kind, u8)> {
    for (prefix, kind) in [
        ("hprbar", Kind::MpuEl2),
        ("hprlar", Kind::MpuEl2),
        ("prbar", Kind::MpuEl1),
        ("prlar", Kind::MpuEl1),
        ("pmevtyper", Kind::Pmu),
        ("pmevcntr", Kind::Pmu),
    ] {
        if let Some(index) = id.strip_prefix(prefix) {
            return Some((kind, index.parse().ok()?));
        }
    }
    None
}
impl Plan {
    pub fn new(kind: Kind, index: u8, count: u64, mode: u64) -> Result<Self, String> {
        if !matches!(mode, 0x11 | 0x12 | 0x13 | 0x17 | 0x1a | 0x1b | 0x1f) {
            return Err(
                "Selector transactions require a known privileged physical CPSR mode".into(),
            );
        }
        let count = u8::try_from(count).map_err(|_| "Invalid selector count")?;
        if (kind == Kind::Pmu && !(1..=4).contains(&count))
            || (kind != Kind::Pmu && !matches!(count, 16 | 20 | 24))
            || index >= count
        {
            return Err(
                "Selector index/count is outside the adapted Cortex-R52 implementation".into(),
            );
        }
        if kind == Kind::MpuEl2 && mode != 0x1a {
            return Err("EL2 MPU selector requires physical Hyp mode".into());
        }
        let (selector, selector_encoding, data, capability, count_shift, count_mask) = match kind {
            Kind::MpuEl1 => (
                "prselr",
                "15 0 6 2 1",
                ["15 0 6 3 0", "15 0 6 3 1"],
                "15 0 0 0 4",
                8,
                255,
            ),
            Kind::MpuEl2 => (
                "hprselr",
                "15 4 6 2 1",
                ["15 4 6 3 0", "15 4 6 3 1"],
                "15 4 0 0 4",
                0,
                255,
            ),
            Kind::Pmu => (
                "pmselr",
                "15 0 9 12 5",
                ["15 0 9 13 1", "15 0 9 13 2"],
                "15 0 9 12 0",
                11,
                31,
            ),
        };
        Ok(Self {
            kind,
            index,
            count,
            mode: mode as u8,
            selector,
            selector_encoding,
            data,
            capability,
            count_shift,
            count_mask,
            ids: kind.ids(index),
        })
    }
    pub fn script(&self, mrc: &str, mcr: &str) -> Result<String, String> {
        self.script_with_sync(mrc, mcr, "")
    }
    pub fn script_with_sync(&self, mrc: &str, mcr: &str, isb: &str) -> Result<String, String> {
        if !matches!(
            (mrc, mcr),
            ("arm mrc", "arm mcr") | ("aarch64 mrc", "aarch64 mcr")
        ) {
            return Err("Explicit verified MRC/MCR command pair required".into());
        }
        if !isb.is_empty() && (isb != "aarch64 isb" || mrc != "aarch64 mrc") {
            return Err("Genuine ISB requires the verified aarch64 adapter command family".into());
        }
        let Self {
            selector_encoding: s,
            data: [a, b],
            capability: c,
            count_shift,
            count_mask,
            count,
            index,
            ..
        } = self;
        let selector_mask = if self.kind == Kind::Pmu || *count > 16 {
            31
        } else {
            15
        };
        let original_limit = if self.kind == Kind::Pmu {
            format!("($__dts_old >= {count} && $__dts_old != 31)")
        } else {
            format!("($__dts_old >= {count})")
        };
        let control = if self.mode == 0x1a {
            "15 4 1 0 0"
        } else {
            "15 0 1 0 0"
        };
        // Legacy CP15ISB requires CP15BEN. The explicit adapter executes genuine
        // ISB, verifies its protocol and tests the barrier before changing state.
        // Inaccessible/disabled barriers stop before any selector MCR. The original
        // selector is validated before it can become a restoration operand.
        let (guard, barrier) = if isb.is_empty() {
            (
                format!(
                    "if {{([{mrc} {control}] & 32) == 0}} {{error \"CP15ISB not enabled; selector synchronization unsupported\"}}"
                ),
                format!("{mcr} 15 0 7 5 4 0"),
            )
        } else {
            (
                format!(
                    "if {{[catch {{aarch64 debugtui_adapter}} __dt_adapter] || $__dt_adapter ne \"{}\"}} {{error \"ISB adapter protocol unsupported\"}}; {isb}",
                    super::OPENOCD_ADAPTER_PROTOCOL
                ),
                isb.into(),
            )
        };
        Ok(format!(
            "set __dts_changed 0; set __dts_saved 0; set __dts_rc [catch {{\
            set __dts_midr [{mrc} 15 0 0 0 0]; if {{($__dts_midr & 0xff0ffff0) != 0x410fd130}} {{error \"Physical MIDR is not an adapted R52\"}}; \
            set __dts_n [expr {{([{mrc} {c}] >> {count_shift}) & {count_mask}}}]; if {{$__dts_n != {count}}} {{error \"Physical capability count changed\"}}; \
            {guard}; \
            set __dts_old [{mrc} {s}]; if {{($__dts_old & ~{selector_mask}) != 0 || {original_limit}}} {{error \"Original selector is outside the known restoration range\"}}; set __dts_saved 1; \
            if {{$__dts_old != {index}}} {{set __dts_changed 1; {mcr} {s} {index}; {barrier}}}; \
            if {{[{mrc} {s}] != {index}}} {{error \"Selector did not select the requested object\"}}; \
            set __dts_a [{mrc} {a}]; set __dts_b [{mrc} {b}]; \
            }} __dts_value]; \
            if {{$__dts_saved}} {{set __dts_restore [catch {{\
            if {{$__dts_changed}} {{{mcr} {s} $__dts_old; {barrier}}}; \
            set __dts_now [{mrc} {s}]; if {{$__dts_now != $__dts_old}} {{error \"Selector restore readback mismatch\"}} \
            }} __dts_restore_error]; if {{$__dts_restore}} {{error \"Selector restoration failed: $__dts_restore_error\"}}}}; \
            if {{$__dts_rc}} {{error $__dts_value}}; \
            if {{[[target current] curstate] ne \"halted\"}} {{error \"Selector restoration failed: physical core changed after restore\"}}; \
            format \"%08x %08x %08x %08x\" $__dts_old $__dts_now $__dts_a $__dts_b"
        ))
    }
    pub fn parse(&self, response: &str) -> Result<Evidence, String> {
        let fields: Vec<_> = response.split_whitespace().collect();
        if fields.len() != 4
            || fields
                .iter()
                .any(|s| s.len() != 8 || !s.bytes().all(|c| c.is_ascii_hexdigit()))
        {
            return Err(
                "Incomplete selector transaction evidence; reconnect before further access".into(),
            );
        }
        let values: Vec<_> = fields
            .iter()
            .map(|s| RawValue::parse(&format!("0x{s}"), 32))
            .collect::<Result<_, _>>()?;
        if values[0] != values[1] {
            return Err("Selector restoration mismatch in response".into());
        }
        let original = values[0].integer()?;
        if original >= u128::from(self.count) && !(self.kind == Kind::Pmu && original == 31) {
            return Err("Original selector evidence is outside the known restoration range".into());
        }
        Ok(Evidence {
            original: values[0].clone(),
            restored: values[1].clone(),
            selected: self.index,
            values: [values[2].clone(), values[3].clone()],
            synchronization: "CP15ISB after selector changes; controlling CP15BEN observed enabled"
                .into(),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MpuRegion {
    pub base: String,
    pub limit_inclusive: String,
    pub enabled: bool,
    pub valid_range: bool,
    pub execute_never: bool,
    pub access: String,
    pub shareability: String,
    pub attribute_index: u8,
}
pub fn decode_mpu(kind: Kind, base: &RawValue, limit: &RawValue) -> Result<MpuRegion, String> {
    if kind == Kind::Pmu {
        return Err("PMU values are not an MPU region".into());
    }
    if base.bits != 32 || limit.bits != 32 {
        return Err("R52 MPU pair must be 32 bits".into());
    }
    let a = base.integer()? as u32;
    let b = limit.integer()? as u32;
    let start = a & !63;
    let end = (b & !63) | 63;
    Ok(MpuRegion {
        base: format!("0x{start:08x}"),
        limit_inclusive: format!("0x{end:08x}"),
        enabled: b & 1 != 0,
        valid_range: start <= end,
        execute_never: a & 1 != 0,
        access: (if kind == Kind::MpuEl1 {
            [
                "EL1 RW; EL0 denied",
                "EL1/EL0 RW",
                "EL1 RO; EL0 denied",
                "EL1/EL0 RO",
            ]
        } else {
            [
                "EL2 RW; EL1/EL0 denied",
                "EL2/EL1/EL0 RW",
                "EL2 RO; EL1/EL0 denied",
                "EL2/EL1/EL0 RO",
            ]
        })[((a >> 1) & 3) as usize]
            .into(),
        shareability: [
            "Non-shareable",
            "UNPREDICTABLE for Normal memory",
            "Outer Shareable",
            "Inner Shareable",
        ][((a >> 3) & 3) as usize]
            .into(),
        attribute_index: ((b >> 1) & 7) as u8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_selector_proof_binds_bank_count_index_identity_and_restoration() {
        let plan = NativePlan::new(Kind::MpuEl2, 19, 20).unwrap();
        let good = "midr 0x411fd134 dscr 0x01050213 dspsr 0xa2000410 dlr 0x81234568 bank el2 capacity 0x00000014 original 0x00000002 restored 0x00000002 selected 0x00000013 base 0x3013001b limit 0x3013ffc1";
        let (evidence, proof) = plan.parse(good).unwrap();
        assert_eq!(evidence.selected, 19);
        assert_eq!(evidence.original.hex, "0x00000002");
        assert_eq!(proof.current_el(), Some(2));
        assert_eq!(proof.dspsr.hex, "0xa2000410");
        for bad in [
            good.replace("bank el2", "bank el1"),
            good.replace("0x00000014", "0x00000018"),
            good.replace("0x00000014", "0x00000000"),
            good.replace("restored 0x00000002", "restored 0x00000003"),
            good.replace("0x00000002", "0x00000014"),
            good.replace("selected 0x00000013", "selected 0x00000012"),
            good.replace("0x3013001b", "0x3013"),
            good.replace("0x3013ffc1", "0x13013ffc1"),
            good.replace("0x01050213", "0x01058213"),
            good.replace("0x01050213", "0x01050113"),
            good.replace("0x411fd134", "0x511fd134"),
            format!("{good} extra"),
        ] {
            assert!(plan.parse(&bad).is_err(), "{bad}");
        }
        assert!(NativePlan::new(Kind::Pmu, 0, 4).is_err());
        assert!(NativePlan::new(Kind::MpuEl2, 0, 0).is_err());
        assert!(NativePlan::new(Kind::MpuEl1, 16, 16).is_err());
        assert!(NativePlan::new(Kind::MpuEl1, 0, 256).is_err());
        assert!(plan.script().contains("aarch64 r52_select el2 19 20"));
        assert!(!plan.script().contains(" mcr"));
    }
    #[test]
    fn native_selector_configuration_requires_matching_reader_and_internal_isb() {
        let mut config = crate::registers::Config {
            cp15_command: super::super::r52_core::COMMAND.into(),
            selector_command: NATIVE_COMMAND.into(),
            ..Default::default()
        };
        config.validate().unwrap();
        config.isb_command = "aarch64 isb".into();
        assert!(config.validate().is_err());
        config.isb_command.clear();
        config.cp15_command = "arm mrc".into();
        assert!(config.validate().is_err());
    }
    #[test]
    fn selector_bounds_privilege_and_known_restoration_encodings_are_strict() {
        for kind in [Kind::MpuEl1, Kind::MpuEl2, Kind::Pmu] {
            let n = if kind == Kind::Pmu { 4 } else { 24 };
            assert!(Plan::new(kind, n, n.into(), 0x1a).is_err());
            assert!(Plan::new(kind, 0, n.into(), 0x10).is_err());
            let p = Plan::new(kind, 0, n.into(), 0x1a).unwrap();
            assert!(p.script("arm mrc", "").is_err());
            assert!(p.script("arm mrc", "aarch64 mcr").is_err());
            assert!(p.parse("00000001 00000002 00000000 00000000").is_err());
            assert!(p.parse("00000000 00000000 00000000").is_err());
            assert!(p.parse("00000000 00000000 000000000 00000000").is_err());
            assert!(p.parse("0000001f 0000001f 00000000 00000000").is_ok() == (kind == Kind::Pmu));
        }
        assert!(Plan::new(Kind::MpuEl2, 0, 16, 0x13).is_err());
        assert!(Plan::new(Kind::Pmu, 0, 6, 0x1a).is_err());
        assert!(Plan::new(Kind::MpuEl1, 0, 0, 0x1a).is_err());
    }
    #[test]
    fn selector_script_saves_checks_synchronizes_and_restores_before_returning_data() {
        let p = Plan::new(Kind::MpuEl2, 23, 24, 0x1a).unwrap();
        let script = p.script("arm mrc", "arm mcr").unwrap();
        assert!(script.contains("arm mrc 15 4 6 3 0"));
        assert!(script.contains("arm mrc 15 4 1 0 0"));
        assert!(script.contains("arm mcr 15 4 6 2 1 23; arm mcr 15 0 7 5 4 0"));
        assert!(script.contains("arm mcr 15 4 6 2 1 $__dts_old; arm mcr 15 0 7 5 4 0"));
        assert!(
            script.find("set __dts_changed 1").unwrap()
                < script.find("arm mcr 15 4 6 2 1 23").unwrap()
        );
        assert!(
            script.find("Selector restoration failed").unwrap() < script.find("format \"").unwrap()
        );
        for forbidden in ["core_state", "resume", "; halt", "15 0 9 12 0 1", "FPEXC"] {
            assert!(!script.contains(forbidden));
        }
    }
    #[test]
    fn genuine_isb_checks_adapter_before_selecting_and_does_not_require_cp15ben() {
        let p = Plan::new(Kind::Pmu, 3, 4, 0x1a).unwrap();
        let script = p
            .script_with_sync("aarch64 mrc", "aarch64 mcr", "aarch64 isb")
            .unwrap();
        assert!(script.contains(super::super::OPENOCD_ADAPTER_PROTOCOL));
        assert!(
            script.find("aarch64 debugtui_adapter").unwrap()
                < script.find("set __dts_old").unwrap()
        );
        assert!(script.contains("aarch64 mcr 15 0 9 12 5 3; aarch64 isb"));
        assert!(script.contains("aarch64 mcr 15 0 9 12 5 $__dts_old; aarch64 isb"));
        assert!(!script.contains("& 32") && !script.contains("15 0 7 5 4 0"));
        assert!(
            p.script_with_sync("arm mrc", "arm mcr", "aarch64 isb")
                .is_err()
        );
        assert!(
            p.script_with_sync("aarch64 mrc", "aarch64 mcr", "aarch64 isb; resume")
                .is_err()
        );
    }
    #[test]
    fn mpu_pair_decodes_inclusive_limit_permissions_and_disabled_invalid_range() {
        let region = decode_mpu(
            Kind::MpuEl1,
            &RawValue::parse("0x2000001f", 32).unwrap(),
            &RawValue::parse("0x2000ffcf", 32).unwrap(),
        )
        .unwrap();
        assert_eq!(region.base, "0x20000000");
        assert_eq!(region.limit_inclusive, "0x2000ffff");
        assert!(region.enabled && region.valid_range && region.execute_never);
        assert_eq!(region.attribute_index, 7);
        assert_eq!(region.access, "EL1/EL0 RO");
        assert_eq!(region.shareability, "Inner Shareable");
        let region = decode_mpu(
            Kind::MpuEl2,
            &RawValue::parse("0x20010000", 32).unwrap(),
            &RawValue::parse("0x20000000", 32).unwrap(),
        )
        .unwrap();
        assert!(!region.enabled && !region.valid_range);
        assert_eq!(region.access, "EL2 RW; EL1/EL0 denied");
    }
}
