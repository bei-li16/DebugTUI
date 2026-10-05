//! Explicit VFP reads; a physical pair is cached only inside one read request.
use super::*;
use crate::registers::{
    RawValue, Reason,
    vfp::{self, Kind, Response},
};

impl Engine {
    pub(super) fn read_vfp_register(
        &mut self,
        name: &str,
        values: &mut super::registers::ReadCache,
    ) -> Result<RawValue, (Reason, String)> {
        if self.project.registers.vfp_command.is_empty() {
            return Err((
                Reason::ReaderUnsupported,
                "Configure registers.vfp_command for the verified VFP adapter".into(),
            ));
        }
        let kind = Kind::parse(name).ok_or((
            Reason::ReaderUnsupported,
            "Unknown VFP register name".into(),
        ))?;
        if let Some(pair) = kind.pair() {
            let key = format!(":vfp_pair:{pair}");
            if let (Some(value), Some(m0), Some(m1)) = (
                values.get(&key),
                values.get(&format!(":vfp_mvfr0:{pair}")),
                values.get(&format!(":vfp_mvfr1:{pair}")),
            ) {
                self.register_value_access =
                    values.provenance.get(&key).and_then(|p| p.access.clone());
                let features = vfp::features(
                    m0.integer().unwrap_or(0) as u64,
                    m1.integer().unwrap_or(0) as u64,
                )
                .ok_or((
                    Reason::ReaderUnsupported,
                    "Unadapted cached VFP evidence".into(),
                ))?;
                if kind.bits() == 128 && !features.neon {
                    return Err((
                        Reason::HardwareNotImplemented,
                        "Observed R52 D16 configuration has no NEON Q storage view".into(),
                    ));
                }
                return kind
                    .view(value)
                    .map_err(|error| (Reason::TransportError, error));
            }
        }
        let operation = format!(
            "if {{[catch {{aarch64 debugtui_vfp_protocol}} __dt_fp_protocol] || $__dt_fp_protocol ne \"{}\"}} {{error \"VFP adapter protocol unsupported\"}}; \
             if {{[catch {{{} {}}} __dt_fp_result]}} {{set __dt_fp_code $::errorCode; \
             if {{[[target current] curstate] ne \"halted\"}} {{error \"Core state restoration failed: VFP target state unknown\"}}; \
             if {{$__dt_fp_code eq [list OpenOCD -300]}} {{error \"VFP access unsupported: $__dt_fp_result\"}}; error $__dt_fp_result}}; set __dt_fp_result",
            vfp::PROTOCOL,
            self.project.registers.vfp_command,
            crate::live_watch::word(name)
        );
        let text = self
            .physical_adapter_read(&operation, &format!("VFP read {name}"))
            .map_err(|(reason, error)| {
                let observed = if error.contains("debugtui-vfp:feature-disabled") {
                    Some(Reason::FeatureDisabled)
                } else if error.contains("debugtui-vfp:not-implemented") {
                    Some(Reason::HardwareNotImplemented)
                } else if error.contains("debugtui-vfp:access-restricted") {
                    Some(Reason::AccessRestricted)
                } else if error.contains("VFP access unsupported")
                    || error.contains("VFP adapter protocol unsupported")
                {
                    Some(Reason::ReaderUnsupported)
                } else {
                    None
                };
                if observed.is_some() {
                    self.snapshot.register_probe = None;
                }
                (observed.unwrap_or(reason), error)
            })?;
        let response =
            Response::parse(&text, kind).map_err(|error| (Reason::ReaderUnsupported, error))?;
        if name == "fpscr" && response.fpexc.integer().unwrap_or(0) & (1 << 30) == 0 {
            return Err((
                Reason::ReaderUnsupported,
                "FPSCR response contradicts FPEXC.EN".into(),
            ));
        }
        let value = kind
            .view(&response.value)
            .map_err(|error| (Reason::TransportError, error))?;
        if let Some(pair) = kind.pair() {
            let mut provenance = crate::registers::provenance::Provenance::declared(
                &crate::registers::Reader::Vfp { name: name.into() },
            );
            provenance.access = self.register_value_access.clone();
            values
                .provenance
                .insert(format!(":vfp_pair:{pair}"), provenance);
            values.insert(format!(":vfp_pair:{pair}"), response.value);
            values.insert(format!(":vfp_mvfr0:{pair}"), response.mvfr0);
            values.insert(format!(":vfp_mvfr1:{pair}"), response.mvfr1);
        }
        Ok(value)
    }
}
