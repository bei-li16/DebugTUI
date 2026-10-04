#![cfg(windows)]
//! Actual binary/JSONL preferences, with an intentionally nonexistent GDB.
use std::{path::Path, process::Command};

#[test]
fn actual_binary_register_preferences_preserve_other_clients_and_never_start_gdb() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/test-register-display.cjs"))
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
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("\"passed\":6,\"failed\":0,\"skipped\":0"),
        "{stdout}"
    );
}
