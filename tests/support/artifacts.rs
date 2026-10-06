//! Keep generated fixtures with the caller's test evidence, outside source trees.
use std::path::PathBuf;

pub fn root() -> PathBuf {
    std::env::var_os("DEBUGTUI_TEST_ARTIFACT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("artifacts"))
}
