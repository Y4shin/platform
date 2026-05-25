//! End-to-end deployment build. `#[ignore]` by default — it compiles the whole
//! platform release (heavy) and regenerates composition glue in the source tree.
//! Run explicitly: `cargo test -p junius --test build_e2e -- --ignored`
//! (CI runs it only when `JUNIUS_E2E=1`).
//!
//! It builds `examples/example-deployment/` (whose `[source].path` is the
//! monorepo, with the same plugin set as dev, so the re-sync is a no-op) and
//! asserts the `./platform` artifact is produced.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;

#[test]
#[ignore = "heavy: compiles the platform release + writes into the source tree"]
fn full_build_produces_platform_binary() {
    let junius = env!("CARGO_BIN_EXE_junius");
    let example =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/example-deployment");

    let artifact = example.join("platform");
    let _ = std::fs::remove_file(&artifact);

    let status = Command::new(junius)
        .args(["build", "--update-lock", "--force"])
        .current_dir(&example)
        .status()
        .unwrap();
    assert!(status.success(), "junius build should succeed");
    assert!(artifact.is_file(), "the ./platform artifact should exist");
    assert!(
        example.join("platform.lock").is_file(),
        "platform.lock written"
    );
}
