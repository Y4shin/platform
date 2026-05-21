//! CLI integration tests via `assert_cmd`. Each test invokes the `junius` binary
//! built by cargo. `assert_cmd` sets CWD = the crate root (`tools/junius/`),
//! so fixture paths like `fixtures/...` resolve correctly.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use assert_cmd::Command;
use predicates::prelude::*;

fn cmd() -> Command {
    let mut c = Command::cargo_bin("junius").expect("junius binary not built");
    c.env("NO_COLOR", "1").env("CLICOLOR", "0");
    c
}

// --- --help and version -------------------------------------------------------

#[test]
fn root_help_snapshot() {
    let output = cmd().arg("--help").assert().success().get_output().clone();
    let stdout = String::from_utf8(output.stdout).unwrap();
    insta::assert_snapshot!("root_help", stdout);
}

#[test]
fn check_help_snapshot() {
    let output = cmd()
        .args(["check", "--help"])
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(output.stdout).unwrap();
    insta::assert_snapshot!("check_help", stdout);
}

#[test]
fn plugin_help_snapshot() {
    let output = cmd()
        .args(["plugin", "--help"])
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(output.stdout).unwrap();
    insta::assert_snapshot!("plugin_help", stdout);
}

// --- check --------------------------------------------------------------------

#[test]
fn check_valid_minimal_exits_zero() {
    cmd()
        .args([
            "check",
            "--manifest",
            "fixtures/plugins/valid-minimal/plugin.toml",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("OK"));
}

#[test]
fn check_valid_full_exits_zero() {
    cmd()
        .args([
            "check",
            "--manifest",
            "fixtures/plugins/valid-full/plugin.toml",
        ])
        .assert()
        .success();
}

#[test]
fn check_bad_name_exits_two() {
    cmd()
        .args([
            "check",
            "--manifest",
            "fixtures/plugins/invalid-bad-name/plugin.toml",
        ])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("PLUGIN.NAME.INVALID"));
}

#[test]
fn check_wrong_mount_prefix_exits_two() {
    cmd()
        .args([
            "check",
            "--manifest",
            "fixtures/plugins/invalid-wrong-mount-prefix/plugin.toml",
        ])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("MOUNT.ROUTE.PREFIX"));
}

#[test]
fn check_missing_schema_exits_one() {
    cmd()
        .args([
            "check",
            "--manifest",
            "fixtures/plugins/invalid-missing-schema/plugin.toml",
        ])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn check_json_output_for_invalid_manifest() {
    let output = cmd()
        .args([
            "--format",
            "json",
            "check",
            "--manifest",
            "fixtures/plugins/invalid-bad-name/plugin.toml",
        ])
        .assert()
        .failure()
        .code(2)
        .get_output()
        .clone();
    let v: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout was not valid JSON");
    assert_eq!(v["ok"], false);
    let codes: Vec<&str> = v["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"PLUGIN.NAME.INVALID"), "got {codes:?}");
}

#[test]
fn check_no_args_is_noop() {
    cmd().arg("check").assert().success();
}

// --- plugin list / info -------------------------------------------------------

#[test]
fn plugin_list_reads_fixture() {
    cmd()
        .args([
            "plugin",
            "list",
            "--config",
            "fixtures/deployments/valid-minimal/platform.toml",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello"))
        .stdout(predicate::str::contains("speakers"));
}

#[test]
fn plugin_list_json_shape() {
    let output = cmd()
        .args([
            "--format",
            "json",
            "plugin",
            "list",
            "--config",
            "fixtures/deployments/valid-minimal/platform.toml",
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    let v: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout was not valid JSON");
    let enabled: Vec<&str> = v["enabled"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect();
    assert_eq!(enabled, vec!["hello", "speakers"]);
}

// --- stub subcommands ---------------------------------------------------------

#[test]
fn sync_stub_exits_64() {
    cmd()
        .arg("sync")
        .assert()
        .failure()
        .code(64)
        .stderr(predicate::str::contains("not yet implemented"));
}

#[test]
fn migrate_up_stub_exits_64() {
    cmd()
        .args(["migrate", "up"])
        .assert()
        .failure()
        .code(64)
        .stderr(predicate::str::contains("not yet implemented"));
}

#[test]
fn new_plugin_stub_exits_64() {
    cmd()
        .args(["new", "plugin", "demo"])
        .assert()
        .failure()
        .code(64)
        .stderr(predicate::str::contains("not yet implemented"));
}

#[test]
fn plugin_enable_stub_exits_64() {
    cmd()
        .args(["plugin", "enable", "demo"])
        .assert()
        .failure()
        .code(64);
}
