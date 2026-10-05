use std::path::PathBuf;
use std::process::Command;

#[test]
fn headless_fixture_session_drains_quit_response_before_rejecting_exit_and_keeps_failures() {
    let output = Command::new("node")
        .arg("--test")
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/test-support/session.test.cjs"),
        )
        .output()
        .expect("Node lifecycle regression driver");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("pass 4"));
}
