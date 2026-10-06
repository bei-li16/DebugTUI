//! Armv7-M MPU bank sampling. Definitions come from the pinned CMSIS catalogue.
use crate::registers::{Catalogue, Context, RawValue, Reader, Sample, State};
use serde::{Deserialize, Serialize};

pub const IDS: [&str; 6] = [
    "scb.cpuid",
    "mpu.type",
    "mpu.ctrl",
    "mpu.rnr",
    "mpu.rbar",
    "mpu.rasr",
];

pub struct Plan {
    pub count: u8,
    pub cpuid: u32,
    addresses: [u64; 6],
}
impl Plan {
    pub fn new(catalogue: &Catalogue, count: u64, cpuid: u32) -> Result<Self, String> {
        if !crate::registers::m_profile::adapted_cpu(&catalogue.cpu)
            || !(matches!(count, 0 | 8) || (catalogue.cpu == "cortex-m7" && count == 16))
        {
            return Err("MPU count is outside the adapted Cortex-M implementation".into());
        }
        let mut addresses = [0; 6];
        for (i, id) in IDS.iter().enumerate() {
            let register = catalogue
                .register(id)
                .ok_or("Built-in M MPU definition missing")?;
            let Reader::CorePrivate { address } = register.reader else {
                return Err("M MPU requires a CorePrivate definition".into());
            };
            addresses[i] = address;
        }
        Ok(Self {
            count: count as u8,
            cpuid,
            addresses,
        })
    }
    pub fn script(&self, target: &str) -> String {
        // Every command names the target; no global target selection or CPU injection.
        // Validate each scalar before using it as a selector restoration operand.
        let read = |i: usize, name: &str| {
            format!(
                "set __dtm_raw [$__dtm_target read_memory 0x{:x} 32 1]; \
             if {{[llength $__dtm_raw] != 1 || ![regexp {{^(0[xX][0-9a-fA-F]+|[0-9]+)$}} [lindex $__dtm_raw 0]]}} {{error \"Invalid MPU scalar response\"}}; \
             set {name} [expr {{[lindex $__dtm_raw 0] + 0}}]; \
             if {{${name} < 0 || ${name} > 0xffffffff}} {{error \"MPU scalar outside 32 bits\"}};",
                self.addresses[i]
            )
        };
        let rnr = self.addresses[3];
        let cpuid = read(0, "__dtm_cpuid");
        let mpu_type = read(1, "__dtm_type");
        let control = read(2, "__dtm_ctrl");
        let save = read(3, "__dtm_old");
        let selected = read(3, "__dtm_selected");
        let base = read(4, "__dtm_base");
        let rasr = read(5, "__dtm_rasr");
        let restore = read(3, "__dtm_restored");
        let count = self.count;
        let expected = self.cpuid;
        format!(
            "set __dtm_target {}; set __dtm_saved 0; set __dtm_changed 0; \
             set __dtm_rc [catch {{ \
             if {{[$__dtm_target curstate] ne \"halted\"}} {{error \"M MPU requires a halted core\"}}; \
             {cpuid} {mpu_type} \
             if {{$__dtm_cpuid != {expected} || $__dtm_type != ({count} << 8)}} {{error \"Physical M identity or MPU capacity changed\"}}; \
             if {{{count} > 0}} {{ {control} }} else {{set __dtm_ctrl 0}}; \
             set __dtm_pairs \"\"; set __dtm_old 0; set __dtm_restored 0; \
             if {{{count} > 0}} {{ \
             {save} if {{$__dtm_old >= {count}}} {{error \"Original MPU RNR is outside the restoration range\"}}; set __dtm_saved 1; \
             for {{set __dtm_i 0}} {{$__dtm_i < {count}}} {{incr __dtm_i}} {{ \
             set __dtm_changed 1; $__dtm_target write_memory 0x{rnr:x} 32 [list $__dtm_i]; \
             {selected} if {{$__dtm_selected != $__dtm_i}} {{error \"MPU RNR selection readback mismatch\"}}; \
             {base} {rasr} append __dtm_pairs [format \" %08x %08x\" $__dtm_base $__dtm_rasr]; \
             }} }} \
             }} __dtm_error]; \
             if {{$__dtm_saved}} {{set __dtm_restore_rc [catch {{ \
             if {{$__dtm_changed}} {{$__dtm_target write_memory 0x{rnr:x} 32 [list $__dtm_old]}}; \
             {restore} if {{$__dtm_restored != $__dtm_old}} {{error \"MPU RNR restoration readback mismatch\"}} \
             }} __dtm_restore_error]; if {{$__dtm_restore_rc}} {{error \"Selector restoration failed: $__dtm_restore_error\"}} }}; \
             if {{$__dtm_rc}} {{error $__dtm_error}}; \
             if {{[$__dtm_target curstate] ne \"halted\"}} {{error \"Selector restoration failed: physical M core changed\"}}; \
             {cpuid} {mpu_type} \
             if {{$__dtm_cpuid != {expected} || $__dtm_type != ({count} << 8)}} {{error \"Physical M identity or MPU capacity changed\"}}; \
             format \"%08x %08x %08x %08x %08x%s\" $__dtm_cpuid $__dtm_type $__dtm_ctrl $__dtm_old $__dtm_restored $__dtm_pairs",
            crate::live_watch::word(target)
        )
    }
    pub fn parse(&self, text: &str) -> Result<Vec<RawValue>, String> {
        let fields: Vec<_> = text.split_whitespace().collect();
        if fields.len() != 5 + usize::from(self.count) * 2
            || fields
                .iter()
                .any(|s| s.len() != 8 || !s.bytes().all(|c| c.is_ascii_hexdigit()))
        {
            return Err("Incomplete M MPU transaction evidence".into());
        }
        let values = fields
            .iter()
            .map(|s| RawValue::parse(&format!("0x{s}"), 32))
            .collect::<Result<Vec<_>, _>>()?;
        if values[0].integer()? != u128::from(self.cpuid)
            || values[1].integer()? != u128::from(self.count) << 8
            || values[3] != values[4]
            || (self.count > 0 && values[3].integer()? >= u128::from(self.count))
            || (self.count == 0 && (values[3].integer()? != 0 || values[2].integer()? != 0))
        {
            return Err("M MPU identity, capacity or restoration evidence mismatch".into());
        }
        Ok(values)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Region {
    pub index: u8,
    pub base: Sample,
    pub attributes: Sample,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct View {
    pub context: Context,
    pub owner: String,
    pub cpu: String,
    pub count: u8,
    pub state: State,
    pub identity: Sample,
    pub mpu_type: Sample,
    pub control: Option<Sample>,
    pub original_selector: Option<RawValue>,
    pub restored_selector: Option<RawValue>,
    pub regions: Vec<Region>,
}
impl View {
    pub fn valid_for(&self, context: &Context) -> bool {
        self.state == State::Valid
            && self.context == *context
            && self.owner == format!("core:{}", context.core)
            && context.frame == 0
    }
    pub fn stale(&mut self) {
        self.state = State::Stale;
        self.identity.stale();
        self.mpu_type.stale();
        if let Some(control) = &mut self.control {
            control.stale();
        }
        for region in &mut self.regions {
            region.base.stale();
            region.attributes.stale();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn m_mpu_plan_adapts_observed_capacity_without_guessing() {
        for cpu in ["cortex-m3", "cortex-m4", "cortex-m7"] {
            let catalogue = Catalogue::builtin(cpu).unwrap();
            assert!(Plan::new(&catalogue, 0, 0x410fc231).is_ok());
            assert!(Plan::new(&catalogue, 8, 0x410fc231).is_ok());
            assert_eq!(
                Plan::new(&catalogue, 16, 0x410fc231).is_ok(),
                cpu == "cortex-m7"
            );
            assert!(Plan::new(&catalogue, 4, 0x410fc231).is_err());
        }
    }
    #[test]
    fn m_mpu_parser_requires_complete_identity_and_exact_restoration() {
        let plan = Plan::new(&Catalogue::builtin("cortex-m3").unwrap(), 8, 0x410fc231).unwrap();
        let prefix = "410fc231 00000800 00000005 00000003 00000003";
        let text = format!("{prefix}{}", " 20000000 0307001f".repeat(8));
        assert_eq!(plan.parse(&text).unwrap().len(), 21);
        for bad in [
            text.replacen("410fc231", "410fc241", 1),
            text.replacen("00000800", "00001000", 1),
            text.replacen("00000003 00000003", "00000003 00000004", 1),
            text.replacen("00000003 00000003", "00000008 00000008", 1),
            format!("{text} 00000000"),
            format!("{prefix}{}", " 20000000 0307001f".repeat(7)),
        ] {
            assert!(plan.parse(&bad).is_err());
        }
    }
}
