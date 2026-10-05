#![cfg(windows)]
use std::{path::Path, process::Command};
#[test]
fn actual_binary_register_matrix_is_read_only_and_preserves_isolated_configuration() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/test-register-matrix.cjs"))
        .args(["--binary", env!("CARGO_BIN_EXE_debugtui")])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("\"passed\":6,\"failed\":0,\"skipped\":0")
    );
}
