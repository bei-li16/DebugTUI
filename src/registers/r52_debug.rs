//! R52 current Debug EL from external MIDR/EDSCR, never saved DSPSR.
//! DDI 0568A.c G2.1.8: HDD is bit 15; bit 16 is RES1, not SDD.

pub const STATE_MASK: u32 = 0x0005_bf00;

pub fn current_el(midr: u128, dscr: u128) -> Option<u8> {
    let el = ((dscr >> 8) & 3) as u8;
    (midr <= u128::from(u32::MAX)
        && dscr <= u128::from(u32::MAX)
        && midr & 0xff0ffff0 == 0x410fd130
        && el <= 2
        && dscr & 0x1c0000c0 == 0
        && dscr & (1 << 24) != 0
        && dscr & (1 << (10 + el)) == 0
        && (el != 2 || dscr & (1 << 15) == 0))
        .then_some(el)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r52_external_edscr_res1_is_not_hyp_debug_disabled() {
        // Independent Armv8-R G2.1.8 witness: ITE, RES1[18,16], EL2,
        // External debug request STATUS=0x13. A saved User mode is irrelevant.
        let actual = 0x0105_0213;
        assert_eq!(current_el(0x411f_d134, actual), Some(2));
        assert_eq!(current_el(0x411f_d135, actual), Some(2));
        assert_eq!(current_el(0x411f_d134, actual | (1 << 15)), None);
        for flags in [0, 1 << 29, 1 << 30, (1 << 29) | (1 << 30)] {
            assert_eq!(current_el(0x411f_d134, actual | flags), Some(2));
        }
        for bad in [
            actual | 0x40,
            actual | 0x80,
            actual | 0x1000,
            actual & !(1 << 24),
        ] {
            assert_eq!(current_el(0x411f_d134, bad), None);
        }
        assert_eq!(current_el(0x411f_d144, actual), None);
        assert_eq!(current_el(0x411f_d134, actual | (1u128 << 32)), None);
        assert_eq!(current_el(0x411f_d134, 0x0105_8113), Some(1));
        assert_ne!(STATE_MASK & (1 << 15), 0);
        assert_eq!(STATE_MASK & ((1 << 29) | (1 << 30)), 0);
    }

    #[test]
    fn r52_real_edscr_is_accepted_by_every_existing_physical_proof_parser() {
        use crate::registers::{banked, gic, pmu, timer, vfp};
        for (dscr, allowed) in [("0x01050213", true), ("0x01058213", false)] {
            let prefix = format!("midr 0x411fd134 dscr {dscr} dspsr 0xa2000410 dlr 0x81234568");
            assert_eq!(
                banked::Response::parse(
                    &format!("{prefix} value 0xf1234567 method banked_mrs32"),
                    "sp_irq"
                )
                .is_ok(),
                allowed
            );
            assert_eq!(
                timer::Response::parse(&format!("{prefix} value 0xf1234567"), "cntfrq", 32).is_ok(),
                allowed
            );
            assert_eq!(vfp::Response::parse(&format!("{prefix} hcptr 0x00000000 mvfr0 0x10110222 mvfr1 0x12111111 fpexc 0x40000700 value 0x7ff8000012345678800000003f800000"), vfp::Kind::Quad(0)).is_ok(), allowed);
            assert_eq!(pmu::Response::parse(&format!("{prefix} id_dfr0 0x03010066 pmcr 0x41132048 hdcr 0x00400e02 pmselr 0x00000003 value 0x41132048"), "pmcr", 32).is_ok(), allowed);
            assert_eq!(gic::Response::parse(&format!("view physical_icc {prefix} id_pfr1 0x10111011 icc_hsre 0x0000000f icc_sre 0x00000007 icc_ctlr 0x00000403 ich_vtr 0x90180003 hcr 0x00000038 ich_hcr 0x00007c01 hstr 0x00001000 value 0xf1234567"), "icc_pmr", 32).is_ok(), allowed);
        }
    }
}
