//! Actual binary entry; no GDB, target access or board execution.
use std::{path::Path, process::Command};

#[test]
fn actual_binary_inheritance_sources_overrides_and_errors_preserve_inputs_without_io() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/test-register-inheritance.cjs"))
        .arg("--binary")
        .arg(env!("CARGO_BIN_EXE_debugtui"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("\"passed\":3,\"failed\":0,\"skipped\":0")
    );
}
