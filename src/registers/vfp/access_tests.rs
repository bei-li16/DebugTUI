use super::*;

fn wire(dspsr: u32) -> String {
    format!(
        "midr 0x411fd134 dscr 0x01000200 dspsr 0x{dspsr:08x} dlr 0x81234568 hcptr 0x00008000 mvfr0 0x10110222 mvfr1 0x12111111 fpexc 0x40000700 value 0x7ff8000012345678800000003f800000"
    )
}

#[test]
fn vfp_current_hyp_proof_accepts_all_stopped_modes_without_granting_lower_el_access() {
    for mode in [0x10, 0x11, 0x12, 0x13, 0x17, 0x1a, 0x1b, 0x1f] {
        let response = Response::parse(&wire(0xa2000400 | mode), Kind::Quad(0)).unwrap();
        assert_eq!(response.evidence.dscr.hex, "0x01000200");
        assert_eq!(
            response.evidence.dspsr.integer().unwrap() & 31,
            mode as u128
        );
        assert_eq!(response.evidence.dlr.hex, "0x81234568");
        // TASE does not authorize SIMD arithmetic; VMOV is a VFP bit transfer.
        assert_eq!(response.evidence.hcptr.hex, "0x00008000");
        let proof = response.pair_evidence(Kind::Quad(0)).unwrap();
        assert_eq!(proof.view("s0").unwrap().hex, "0x3f800000");
        let encoded = serde_json::to_value(&response.evidence).unwrap();
        let decoded: Evidence = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded, response.evidence);
        for dscr in ["0x01000000", "0x01000100"] {
            assert!(
                Response::parse(&wire(0xa200041a).replace("0x01000200", dscr), Kind::Quad(0))
                    .is_err()
            );
        }
    }
    let controls = wire(0xa2000410).replace("0x7ff8000012345678800000003f800000", "0x10110222");
    let control = Response::parse(&controls, Kind::Control).unwrap();
    assert!(control.pair_evidence(Kind::Control).is_none());
    control.evidence.validate().unwrap();
}

#[test]
fn vfp_current_proof_rejects_legacy_missing_forged_and_faulted_evidence() {
    let valid = wire(0xa200041a);
    for invalid in [
        valid
            .split("mvfr0")
            .nth(1)
            .map(|tail| format!("mvfr0{tail}"))
            .unwrap(),
        valid.clone() + " extra",
        valid.replace("0x411fd134", "0x511fd134"),
        valid.replace("0x411fd134", "0x411ed134"),
        valid.replace("0x01000200", "0x01001200"),
        valid.replace("0x01000200", "0x00000200"),
        valid.replace("0x01000200", "0x01000300"),
        valid.replace("0x01000200", "0x01008200"),
        valid.replace("0x01000200", "0x01000240"),
        valid.replace("0x00008000", "0x00008400"),
        valid.replace("0x81234568", "0x1"),
        valid.replace("dlr", "dspsr"),
        "0x3f800000".into(),
    ] {
        assert!(
            Response::parse(&invalid, Kind::Quad(0)).is_err(),
            "{invalid}"
        );
    }
    let mut proof = Response::parse(&valid, Kind::Quad(0)).unwrap().evidence;
    proof.dspsr.bits = 64;
    assert!(proof.validate().is_err());
}
