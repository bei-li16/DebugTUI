#![cfg(windows)]
//! Actual production packers, private npm prefix and disconnected register APIs.
use std::{path::Path, process::Command};

#[test]
fn actual_register_distribution_packages_init_upgrade_preservation_and_multicore_sources_are_verified()
 {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/test-register-distribution.cjs"))
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
            .contains("\"passed\":14,\"failed\":0,\"skipped\":0")
    );
}
