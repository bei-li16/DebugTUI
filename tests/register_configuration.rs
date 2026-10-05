#![cfg(windows)]
//! The actual CLI resolves isolated user profiles without starting GDB.
use std::{path::Path, process::Command};
#[test]
fn actual_binary_register_configuration_priority_errors_and_multicore_identity_are_isolated() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/test-register-configuration.cjs"))
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
            .contains("\"passed\":10,\"failed\":0,\"skipped\":0")
    );
}
