//! Cortex-M7 cache identification through a protected CSSELR transaction.
use super::{Catalogue, Context, RawValue, Reader, Sample, State};
use serde::{Deserialize, Serialize};

pub const IDS: [&str; 5] = [
    "scb.cpuid",
    "scb.clidr",
    "scb.ctr",
    "scb.csselr",
    "scb.ccsidr",
];

// DUI 0646B §4.5.1/Table 4-40: only L1 instruction/data caches, no unified cache.
pub fn valid_clidr(raw: u64) -> bool {
    raw == 0 || (1..=3).any(|n| raw == (0x09000000 | n))
}
pub struct Plan {
    pub cpuid: u32,
    pub clidr: u32,
    pub ctr: u32,
    pub selectors: Vec<u8>,
    addresses: [u64; 5],
}
impl Plan {
    pub fn new(c: &Catalogue, cpuid: u32, clidr: u32, ctr: u32) -> Result<Self, String> {
        if c.cpu != "cortex-m7"
            || cpuid >> 24 != 0x41
            || (cpuid >> 16) & 15 != 15
            || (cpuid >> 4) & 0xfff != 0xc27
            || !valid_clidr(clidr.into())
            || ctr != 0x8303c003
        {
            return Err(
                "Cache identity/configuration is outside the adapted Cortex-M7 encodings".into(),
            );
        }
        let mut addresses = [0; 5];
        for (i, id) in IDS.iter().enumerate() {
            let Reader::CorePrivate { address } =
                c.register(id).ok_or("Missing M7 cache definition")?.reader
            else {
                return Err("Cache transaction requires CorePrivate definitions".into());
            };
            addresses[i] = address;
        }
        let selectors = [0, 1]
            .into_iter()
            .filter(|s| clidr & (if *s == 0 { 2 } else { 1 }) != 0)
            .collect();
        Ok(Self {
            cpuid,
            clidr,
            ctr,
            selectors,
            addresses,
        })
    }
    pub fn script(&self, target: &str) -> String {
        let read = |index: usize, name: &str| {
            format!(
                "set __dtc_raw [$__dtc_target read_memory 0x{:x} 32 1]; if {{[llength $__dtc_raw] != 1 || ![regexp {{^(0[xX][0-9a-fA-F]+|[0-9]+)$}} [lindex $__dtc_raw 0]]}} {{error \"Invalid cache scalar response\"}}; set {name} [expr {{[lindex $__dtc_raw 0]+0}}]; if {{${name}<0 || ${name}>0xffffffff}} {{error \"Cache scalar exceeds 32 bits\"}};",
                self.addresses[index]
            )
        };
        let cpuid = read(0, "__dtc_cpuid");
        let clidr = read(1, "__dtc_clidr");
        let ctr = read(2, "__dtc_ctr");
        let save = read(3, "__dtc_old");
        let selected = read(3, "__dtc_selected");
        let restore = read(3, "__dtc_restored");
        let data = read(4, "__dtc_data");
        let selector = self.addresses[3];
        let list = self
            .selectors
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(" ");
        let identity = format!(
            "{cpuid} {clidr} {ctr} if {{$__dtc_cpuid!={} || $__dtc_clidr!={} || $__dtc_ctr!={}}} {{error \"Physical M7 cache identity changed\"}};",
            self.cpuid, self.clidr, self.ctr
        );
        format!(
            "set __dtc_target {}; set __dtc_saved 0; set __dtc_changed 0; set __dtc_old 0; set __dtc_restored 0; set __dtc_values \"\"; \
            set __dtc_rc [catch {{if {{[$__dtc_target curstate] ne \"halted\"}} {{error \"M7 cache transaction requires halt\"}}; {identity} \
            if {{[llength {{{list}}}]>0}} {{{save} if {{$__dtc_old>1}} {{error \"Original CSSELR is outside the verified restoration range\"}}; set __dtc_saved 1; \
            foreach __dtc_bank {{{list}}} {{set __dtc_changed 1; $__dtc_target write_memory 0x{selector:x} 32 [list $__dtc_bank]; {selected} if {{$__dtc_selected!=$__dtc_bank}} {{error \"CSSELR selection readback mismatch\"}}; {data} append __dtc_values [format \" %08x\" $__dtc_data];}}}} }} __dtc_error]; \
            if {{$__dtc_saved}} {{set __dtc_restore_rc [catch {{if {{$__dtc_changed}} {{$__dtc_target write_memory 0x{selector:x} 32 [list $__dtc_old]}}; {restore} if {{$__dtc_restored!=$__dtc_old}} {{error \"CSSELR restoration readback mismatch\"}}}} __dtc_restore_error]; if {{$__dtc_restore_rc}} {{error \"Selector restoration failed: $__dtc_restore_error\"}}}}; \
            if {{$__dtc_rc}} {{error $__dtc_error}}; if {{[$__dtc_target curstate] ne \"halted\"}} {{error \"Selector restoration failed: physical cache core changed\"}}; {identity} \
            format \"%08x %08x %08x %08x %08x%s\" $__dtc_cpuid $__dtc_clidr $__dtc_ctr $__dtc_old $__dtc_restored $__dtc_values",
            crate::live_watch::word(target)
        )
    }
    pub fn parse(&self, text: &str) -> Result<Vec<RawValue>, String> {
        let fields: Vec<_> = text.split_whitespace().collect();
        if fields.len() != 5 + self.selectors.len()
            || fields
                .iter()
                .any(|s| s.len() != 8 || !s.bytes().all(|c| c.is_ascii_hexdigit()))
        {
            return Err("Incomplete M7 cache transaction evidence".into());
        }
        let values = fields
            .iter()
            .map(|s| RawValue::parse(&format!("0x{s}"), 32))
            .collect::<Result<Vec<_>, _>>()?;
        if values[0].integer()? != self.cpuid.into()
            || values[1].integer()? != self.clidr.into()
            || values[2].integer()? != self.ctr.into()
            || values[3] != values[4]
            || values[3].integer()? > 1
            || (self.selectors.is_empty() && values[3].integer()? != 0)
        {
            return Err("Cache identity or selector restoration evidence mismatch".into());
        }
        Ok(values)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cache {
    pub selector: u8,
    pub kind: String,
    pub size_id: Sample,
}
impl Cache {
    /// DUI 0646B Table 4-43: adapted M7 cache sizes, independent of enable state.
    pub fn size_bytes(&self) -> Option<u32> {
        if self.size_id.state != State::Valid {
            return None;
        }
        let raw = self.size_id.value.as_ref()?.integer().ok()?;
        supported_size(self.selector, raw)
    }
}
fn supported_size(selector: u8, raw: u128) -> Option<u32> {
    let ids: [u32; 5] = match selector {
        0 => [0xf003e019, 0xf007e019, 0xf00fe019, 0xf01fe019, 0xf03fe019],
        1 => [0xf007e009, 0xf00fe009, 0xf01fe009, 0xf03fe009, 0xf07fe009],
        _ => return None,
    };
    ids.iter()
        .position(|id| u128::from(*id) == raw)
        .map(|i| 4096 << i)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct View {
    pub context: Context,
    pub owner: String,
    pub state: State,
    pub identity: Sample,
    pub clidr: Sample,
    pub ctr: Sample,
    pub original_selector: Option<RawValue>,
    pub restored_selector: Option<RawValue>,
    pub caches: Vec<Cache>,
}
impl View {
    pub fn valid_for(&self, context: &Context) -> bool {
        self.state == State::Valid
            && self.context == *context
            && context.frame == 0
            && self.owner == format!("core:{}", context.core)
    }
    pub fn stale(&mut self) {
        self.state = State::Stale;
        self.identity.stale();
        self.clidr.stale();
        self.ctr.stale();
        for cache in &mut self.caches {
            cache.size_id.stale();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn m7_cache_plan_requires_verified_identity_and_only_implemented_banks() {
        let c = Catalogue::builtin("cortex-m7").unwrap();
        for (clidr, selectors) in [
            (0, vec![]),
            (0x09000001, vec![1]),
            (0x09000002, vec![0]),
            (0x09000003, vec![0, 1]),
        ] {
            let plan = Plan::new(&c, 0x411fc271, clidr, 0x8303c003).unwrap();
            assert_eq!(plan.selectors, selectors);
            let mut response = format!("411fc271 {clidr:08x} 8303c003 00000000 00000000");
            for _ in &selectors {
                response.push_str(" f003e019");
            }
            assert_eq!(plan.parse(&response).unwrap().len(), 5 + selectors.len());
        }
        for raw in [1, 2, 3, 0x09000000, 0x09000004, 0xffffffff] {
            assert!(Plan::new(&c, 0x411fc271, raw, 0x8303c003).is_err());
        }
        for id in [0, 0x410fc241, 0x411ec271, 0x421fc271] {
            assert!(Plan::new(&c, id, 0x09000003, 0x8303c003).is_err());
        }
        assert!(Plan::new(&c, 0x411fc271, 0x09000003, 0).is_err());
        assert!(
            Plan::new(
                &Catalogue::builtin("cortex-m4").unwrap(),
                0x411fc271,
                0,
                0x8303c003
            )
            .is_err()
        );
    }
    #[test]
    fn m7_cache_transaction_evidence_rejects_missing_extra_or_unrestored_values() {
        let plan = Plan::new(
            &Catalogue::builtin("cortex-m7").unwrap(),
            0x411fc271,
            0x09000003,
            0x8303c003,
        )
        .unwrap();
        let valid = "411fc271 09000003 8303c003 00000001 00000001 f00fe019 f007e009";
        assert!(plan.parse(valid).is_ok());
        for invalid in [
            valid.replace("00000001 00000001", "00000000 00000001"),
            valid.replace("00000001 00000001", "00000002 00000002"),
            valid.replace("f007e009", "000000000"),
            valid.replace("411fc271", "410fc241"),
            format!("{valid} 00000000"),
            valid[..valid.len() - 9].into(),
        ] {
            assert!(plan.parse(&invalid).is_err(), "{invalid}");
        }
    }
    #[test]
    fn m7_cache_size_uses_supported_manual_encodings_and_preserves_unknowns() {
        for (selector, base) in [(0, 0x0019u32), (1, 0x0009)] {
            for i in 0..5 {
                let sets = (if selector == 0 { 32 } else { 64 }) << i;
                assert_eq!(
                    supported_size(selector, u128::from(0xf0000000 | ((sets - 1) << 13) | base)),
                    Some(4096 << i)
                );
            }
        }
        for raw in [0, 0xf00fe009, 0xf003e011, 0xffff_ffff] {
            // f00fe009 is a supported 8-KiB I-cache, but not a D-cache encoding.
            assert_eq!(supported_size(0, raw), None);
        }
        assert_eq!(supported_size(2, 0xf00fe019), None);
    }
}
