//! `junius build --check-config` resolves the source + validates the deployment
//! against the source's plugin manifests, without compiling.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{cmd_in, make_repo, tempdir};

#[test]
fn check_config_succeeds_and_produces_no_binary() {
    let tmp = tempdir();
    make_repo(tmp.path(), &["hello"]);
    cmd_in(tmp.path())
        .args(["build", "--check-config"])
        .assert()
        .success();
    // A validation-only run must not write the lock (it stops before the build).
    assert!(!tmp.path().join("platform.lock").exists());
}

#[test]
fn check_config_fails_on_missing_plugin() {
    let tmp = tempdir();
    make_repo(tmp.path(), &["hello"]);
    // Enable a plugin that doesn't exist in the source.
    let toml = std::fs::read_to_string(tmp.path().join("platform.toml")).unwrap();
    std::fs::write(
        tmp.path().join("platform.toml"),
        toml.replace("enabled = [\"hello\"]", "enabled = [\"hello\", \"ghost\"]"),
    )
    .unwrap();
    cmd_in(tmp.path())
        .args(["build", "--check-config"])
        .assert()
        .failure();
}
