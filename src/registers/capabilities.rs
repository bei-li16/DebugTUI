//! Decode evidence from current physical-core samples, without guessing from errors.
use super::*;
pub const PROBE_IDS: &[&str] = &[
    "cpsr", "midr", "id_pfr1", "id_dfr0", "mpuir", "hmpuir", "cpacr", "pmcr", "icc_ctlr",
    "ich_vtr", "fpsid", "mvfr0", "mvfr1", "mvfr2", "fpexc",
];
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Identity {
    pub implementer: u8,
    pub variant: u8,
    pub architecture: u8,
    pub part: u16,
    pub revision: u8,
    pub model: Option<String>,
    pub revision_name: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Fact {
    pub value: u64,
    pub register: String,
    pub source: String,
    pub detail: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Probe {
    pub context: Context,
    pub thread: String,
    pub identity: Option<Identity>,
    pub facts: BTreeMap<String, Fact>,
    pub samples: Vec<Sample>,
    pub gdb_names: Vec<String>,
    pub notes: Vec<String>,
}
impl Probe {
    fn observed(&self, id: &str) -> Option<&Sample> {
        let owner = format!("core:{}", self.context.core);
        self.samples.iter().find(|s| {
            s.id == id
                && s.state == State::Valid
                && s.context == self.context
                && s.owner.as_deref() == Some(&owner)
                && s.value.as_ref().is_some_and(|v| v.bits == 32)
        })
    }
    pub fn raw(&self, id: &str) -> Option<u64> {
        self.observed(id)
            .and_then(|s| s.value.as_ref())
            .and_then(|v| v.integer().ok())
            .and_then(|v| u64::try_from(v).ok())
    }
    fn fact(&mut self, key: &str, value: u64, register: &str, detail: &str) {
        let source = self
            .observed(register)
            .map(|s| s.source.clone())
            .unwrap_or_default();
        self.facts.insert(
            key.into(),
            Fact {
                value,
                register: register.into(),
                source,
                detail: detail.into(),
            },
        );
    }
    pub fn decode(&mut self) {
        self.facts.clear();
        self.notes.retain(|note| !note.starts_with("Decode: "));
        self.identity = self.raw("midr").map(|n| {
            let implementer = (n >> 24) as u8;
            let part = ((n >> 4) & 0xfff) as u16;
            let architecture = ((n >> 16) & 15) as u8;
            let variant = ((n >> 20) & 15) as u8;
            let revision = (n & 15) as u8;
            Identity {
                implementer,
                variant,
                architecture,
                part,
                revision,
                model: (implementer == 0x41 && part == 0xd13 && architecture == 15)
                    .then(|| "Cortex-R52".into()),
                revision_name: format!("r{variant}p{revision}"),
            }
        });
        if let Some(n) = self.raw("cpsr") {
            self.fact(
                "cpu.mode",
                n & 31,
                "cpsr",
                "Current physical frame 0 CPSR.M; Hyp is 0x1a",
            );
        }
        let (mmio, notes) = super::mmio_probe::decode(self);
        self.facts.extend(mmio);
        self.notes.extend(notes);
        if !self
            .identity
            .as_ref()
            .is_some_and(|i| i.model.as_deref() == Some("Cortex-R52"))
        {
            if self.samples.iter().any(|s| s.id == "midr") {
                self.notes.push("Decode: Actual MIDR is unreadable or not an adapted Cortex-R52 identity; optional capability decoding remains unknown".into());
            }
            return;
        }
        if let Some(n) = self.raw("id_pfr1") {
            for (key, shift) in [
                ("el2.present", 12),
                ("timer.present", 16),
                ("gic.system_interface", 28),
            ] {
                let field = (n >> shift) & 15;
                if field <= 1 {
                    self.fact(key,field,"id_pfr1","Architectural feature field; presence does not prove present access permission");
                } else {
                    self.notes.push(format!(
                        "Decode: {key}: unadapted ID_PFR1 encoding {field}; remains unknown"
                    ));
                }
            }
        }
        if let Some(n) = self.raw("id_dfr0") {
            let version = (n >> 24) & 15;
            self.fact(
                "pmu.version",
                version,
                "id_dfr0",
                "ID_DFR0.PerfMon raw architectural encoding",
            );
            if version <= 3 {
                self.fact(
                    "pmu.present",
                    u64::from(version != 0),
                    "id_dfr0",
                    "Known PerfMon architecture encoding; no counters started or cleared",
                );
            }
        }
        for (id, key, shift) in [
            ("mpuir", "mpu.el1.regions", 8),
            ("hmpuir", "mpu.el2.regions", 0),
        ] {
            if let Some(n) = self.raw(id) {
                let count = (n >> shift) & 255;
                if matches!(count, 16 | 20 | 24) || (id == "hmpuir" && count == 0) {
                    self.fact(
                        key,
                        count,
                        id,
                        "R52 TRM MPUIR.DREGION[15:8] / HMPUIR.REGION[7:0]",
                    );
                } else {
                    self.notes.push(format!(
                        "Decode: {id}: region count {count} is outside the adapted R52 range"
                    ));
                }
            }
        }
        if let Some(n) = self.raw("cpacr") {
            self.fact(
                "vfp.cpacr_permission",
                (n >> 20) & 15,
                "cpacr",
                "CP10/CP11 permissions alone do not prove FPU presence or FPEXC.EN",
            );
        }
        if let Some(n) = self.raw("pmcr") {
            let count = (n >> 11) & 31;
            self.fact(
                "pmu.pmcr_n",
                count,
                "pmcr",
                "Raw PMCR.N; EL0/EL1 can report HDCR.HPMN instead of the physical count",
            );
            let physical = self.observed("pmcr").and_then(|sample| {
                let access = sample.provenance.as_ref()?.access.as_ref()?;
                let evidence = access.pmu.as_ref()?;
                (sample.source == "openocd:aarch64 pmu"
                    && sample.view == super::SampleView::PhysicalCore
                    && matches!(&access.route, super::provenance::Route::TclRegister {operation,..} if operation=="PMU read pmcr")
                    && access.context == self.context
                    && access.phase == super::provenance::Phase::Responded
                    && evidence.read_method == super::timer::ReadMethod::Mrc32
                    && evidence.pmcr.integer().ok()? == u128::from(n)
                    && evidence.midr.integer().ok()? == u128::from(self.raw("midr")?)
                    && evidence.physical_count() == Some(4))
                .then_some(evidence.hdcr.integer().ok()? as u64 & 31)
            });
            if let Some(guest_partition) = physical {
                self.fact("pmu.counters", count, "pmcr", "Fresh external identity/current Debug EL2 and native PMCR.N=4; stopped CPSR and PMCR.E do not establish capacity");
                self.fact(
                    "pmu.present",
                    1,
                    "pmcr",
                    "Fresh native ID_DFR0.PerfMon=3 in the same read transaction",
                );
                self.fact(
                    "pmu.version",
                    3,
                    "pmcr",
                    "Fresh native ID_DFR0.PerfMon=3; no newer PMUv3 extensions assumed",
                );
                self.fact(
                    "pmu.guest_partition",
                    guest_partition,
                    "pmcr",
                    "Raw HDCR.HPMN; not an EL1 permission grant or physical counter count",
                );
            } else {
                self.notes.push("Decode: Physical PMU count remains unknown: requires fresh current Debug EL2 PMU evidence; stopped CPSR.M=Hyp alone is insufficient and EL0/EL1 PMCR.N can reflect HDCR.HPMN".into());
            }
        }
        if let (Some(m0), Some(m1)) = (self.raw("mvfr0"), self.raw("mvfr1")) {
            if let Some(features) = super::vfp::features(m0, m1) {
                self.fact(
                    "vfp.present",
                    1,
                    "mvfr0",
                    "Observed R52 single-precision feature; independent of CPACR/FPEXC permissions",
                );
                self.fact(
                    "vfp.d_registers",
                    features.d_registers,
                    "mvfr0",
                    "MVFR0.SIMDReg physical D16/D32 capacity; not a GDB name-count inference",
                );
                self.fact(
                    "vfp.double_precision",
                    u64::from(features.double_precision),
                    "mvfr0",
                    "MVFR0.FPDP; D storage width does not imply double-precision arithmetic",
                );
                self.fact("vfp.neon",u64::from(features.neon),"mvfr1","MVFR1 SIMD load/store, integer and single-precision fields, consistent with MVFR0; raw Q storage view does not prove NEON execution permission");
            } else {
                self.notes.push("Decode: Unadapted or contradictory R52 MVFR0/MVFR1; floating-point capacity and NEON remain unknown".into());
            }
        }
        if let Some(n) = self.raw("fpexc") {
            self.fact(
                "vfp.enabled",
                (n >> 30) & 1,
                "fpexc",
                "Actual FPEXC.EN; never changed by this reader",
            );
        }
        if let Some(n) = self.raw("icc_ctlr") {
            self.fact("icc.ctlr_pribits", ((n >> 8) & 7) + 1, "icc_ctlr",
                "Raw ICC/ICV CTLR.PRIbits+1; stopped CPSR does not identify current Debug EL or interface");
            if let Some(evidence) = self.fresh_gic("icc_ctlr", super::gic::View::PhysicalIcc) {
                let bits = evidence.physical_priority_bits().unwrap();
                self.fact(
                    "icc.physical.pribits",
                    bits,
                    "icc_ctlr",
                    "Fresh native physical ICC_CTLR at current Debug EL2; R52 TRM Table 10-94",
                );
                self.fact("icc.physical.prebits", bits, "icc_ctlr", "Maximum physical AP preemption capacity; decoded from ICC_CTLR, independently of ICH_VTR");
                self.fact(
                    "gic.system_interface",
                    1,
                    "icc_ctlr",
                    "Fresh native ID_PFR1.GIC=1 in the same physical transaction",
                );
            } else {
                self.notes.push("Decode: Physical ICC AP capacity remains unknown: requires matching fresh native current Debug EL2 evidence; stopped CPSR.M=Hyp alone is insufficient".into());
            }
        }
        if let Some(n) = self.raw("ich_vtr") {
            self.fact(
                "ich.vtr_pribits",
                ((n >> 29) & 7) + 1,
                "ich_vtr",
                "Raw ICH_VTR.PRIbits+1; access and interface not inferred from stopped CPSR",
            );
            self.fact(
                "ich.vtr_prebits",
                ((n >> 26) & 7) + 1,
                "ich_vtr",
                "Raw ICH_VTR.PREbits+1; never physical ICC capacity",
            );
            self.fact(
                "ich.vtr_listregs",
                (n & 31) + 1,
                "ich_vtr",
                "Raw ICH_VTR.ListRegs+1",
            );
            if let Some(evidence) = self.fresh_gic("ich_vtr", super::gic::View::HypervisorIch) {
                let (pri, pre, count) = (
                    evidence.virtual_priority_bits().unwrap(),
                    evidence.virtual_preemption_bits().unwrap(),
                    evidence.list_count().unwrap(),
                );
                self.fact(
                    "icv.virtual.pribits",
                    pri,
                    "ich_vtr",
                    "Fresh native Hyp ICH_VTR.PRIbits+1; virtual interface only",
                );
                self.fact("icv.virtual.prebits", pre, "ich_vtr", "Fresh native Hyp ICH_VTR.PREbits+1; virtual AP backing capacity, never physical ICC");
                self.fact("ich.list_registers", count, "ich_vtr", "Fresh native Hyp ICH_VTR.ListRegs+1; LR and LRC remain separate 32-bit samples");
            } else {
                self.notes.push("Decode: Virtual ICV/ICH AP capacity remains unknown: requires matching fresh native current Debug EL2 ICH_VTR evidence".into());
            }
        }
    }
    fn fresh_gic(&self, id: &str, view: super::gic::View) -> Option<&super::gic::Evidence> {
        let sample = self.observed(id)?;
        let access = sample.provenance.as_ref()?.access.as_ref()?;
        let evidence = access.gic.as_ref()?;
        let raw = if id == "icc_ctlr" {
            &evidence.icc_ctlr
        } else {
            &evidence.ich_vtr
        };
        (sample.source == "openocd:aarch64 gic"
            && sample.view == super::SampleView::PhysicalCore
            && matches!(&access.route, super::provenance::Route::TclRegister {operation,..} if operation == &format!("GIC read {id}"))
            && access.context == self.context
            && access.phase == super::provenance::Phase::Responded
            && access.completed_ms.is_some_and(|n| n >= access.timestamp_ms)
            && evidence.view == view
            && evidence.midr.integer().ok()? == u128::from(self.raw("midr")?)
            && raw.integer().ok()? == u128::from(self.raw(id)?)
            && evidence.physical_priority_bits().is_some())
        .then_some(evidence)
    }

    pub fn effective(&self, declared: &BTreeMap<String, u64>) -> BTreeMap<String, u64> {
        let mut result = declared.clone();
        result.extend(self.facts.iter().map(|(k, v)| (k.clone(), v.value)));
        result
    }
    /// A bounded human-readable report for the TUI Log; JSON retains all evidence.
    pub fn report_lines(&self) -> Vec<String> {
        let mut lines = vec![format!(
            "core={} session={} generation={} frame={} thread={}",
            self.context.core,
            self.context.session,
            self.context.generation,
            self.context.frame,
            self.thread
        )];
        if let Some(identity) = &self.identity {
            lines.push(format!(
                "CPU={} {} implementer={:#04x} part={:#05x}; decoded from MIDR",
                identity.model.as_deref().unwrap_or("Unknown"),
                identity.revision_name,
                identity.implementer,
                identity.part
            ));
        }
        lines.extend(self.samples.iter().map(|s| {
            format!(
                "{}: {:?} raw={} source={} reason={:?} {}",
                s.id,
                s.state,
                s.value
                    .as_ref()
                    .map(|v| v.hex.as_str())
                    .unwrap_or("Unknown"),
                s.source,
                s.reason,
                s.detail
            )
        }));
        lines.extend(self.facts.iter().map(|(key, fact)| {
            format!(
                "{key}={} from {} via {}: {}",
                fact.value, fact.register, fact.source, fact.detail
            )
        }));
        lines.push(format!(
            "GDB exposes {} names; this alone proves neither widths nor backend side effects",
            self.gdb_names.len()
        ));
        lines.extend(self.notes.iter().cloned());
        lines
    }
}

#[cfg(test)]
mod tests;
