use super::*;
#[test]
fn bao_snapshot_rejects_partial_wrong_identity_and_invalid_sp() {
    let config = SavedContext {
        cpu: 1,
        vm: 7,
        vcpu: 0,
        memory: vec![crate::images::MemoryRange {
            start: 0x1000,
            size: 0x1000,
        }],
        ..Default::default()
    };
    let mut w = [0u32; 28];
    w[..8].copy_from_slice(&[0x42474331, 1, 112, 4, 1, 1, 7, 0]);
    w[21] = 0x1f00;
    w[23] = 0x1100;
    w[24] = 0x13;
    w[25] = 1;
    let bytes = |w: &[u32; 28]| w.iter().flat_map(|w| w.to_le_bytes()).collect::<Vec<_>>();
    assert_eq!(decode(&bytes(&w), &config).unwrap().registers[13], 0x1f00);
    for (index, value) in [
        (0, 0),
        (1, 2),
        (2, 108),
        (3, 5),
        (4, 0),
        (5, 0),
        (6, 8),
        (7, 1),
        (21, 0),
        (24, 0x1a),
        (25, 0),
    ] {
        let mut bad = w;
        bad[index] = value;
        assert!(decode(&bytes(&bad), &config).is_err(), "word {index}");
    }
}
