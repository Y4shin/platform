//! CLI integration tests via `assert_cmd`. Each test invokes the `junius` binary
//! built by cargo. `assert_cmd` sets CWD = the crate root (`tools/junius/`),
//! so fixture paths like `fixtures/...` resolve correctly.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use assert_cmd::Command;
use predicates::prelude::*;

use common::{cmd_in, make_repo, tempdir, write_basic_plugin};

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

#[test]
fn sync_help_snapshot() {
    let output = cmd()
        .args(["sync", "--help"])
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(output.stdout).unwrap();
    insta::assert_snapshot!("sync_help", stdout);
}

#[test]
fn new_help_snapshot() {
    let output = cmd()
        .args(["new", "--help"])
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(output.stdout).unwrap();
    insta::assert_snapshot!("new_help", stdout);
}

#[test]
fn migrate_help_snapshot() {
    let output = cmd()
        .args(["migrate", "--help"])
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(output.stdout).unwrap();
    insta::assert_snapshot!("migrate_help", stdout);
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

#[test]
fn check_via_plugin_arg_resolves_manifest() {
    let tmp = tempdir();
    make_repo(tmp.path(), &[]);
    write_basic_plugin(tmp.path(), "demo");

    cmd_in(tmp.path())
        .args(["check", "--plugin", "demo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("OK"));
}

#[test]
fn check_via_plugin_arg_missing_plugin_errors() {
    let tmp = tempdir();
    make_repo(tmp.path(), &[]);

    cmd_in(tmp.path())
        .args(["check", "--plugin", "ghost"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("no manifest found"));
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

#[test]
fn plugin_info_outputs_summary() {
    let tmp = tempdir();
    make_repo(tmp.path(), &[]);
    write_basic_plugin(tmp.path(), "demo");

    cmd_in(tmp.path())
        .args(["plugin", "info", "demo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Plugin: demo (Demo)"))
        .stdout(predicate::str::contains("route_prefix = /p/demo"))
        .stdout(predicate::str::contains("rpc_prefix   = /rpc/demo"))
        .stdout(predicate::str::contains("http_prefix  = /h/demo"));
}

#[test]
fn plugin_info_json_shape() {
    let tmp = tempdir();
    make_repo(tmp.path(), &[]);
    write_basic_plugin(tmp.path(), "demo");

    let output = cmd_in(tmp.path())
        .args(["--format", "json", "plugin", "info", "demo"])
        .assert()
        .success()
        .get_output()
        .clone();
    let v: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout was not valid JSON");
    assert_eq!(v["name"], "demo");
    assert_eq!(v["display_name"], "Demo");
    assert_eq!(v["mount"]["http_prefix"], "/h/demo");
    assert_eq!(v["mount"]["rpc_prefix"], "/rpc/demo");
    assert_eq!(v["mount"]["route_prefix"], "/p/demo");
}

#[test]
fn plugin_info_missing_plugin_errors() {
    let tmp = tempdir();
    make_repo(tmp.path(), &[]);

    cmd_in(tmp.path())
        .args(["plugin", "info", "ghost"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("cannot read"));
}

// --- --cwd global flag -------------------------------------------------------

#[test]
fn cwd_flag_resolves_relative_paths_against_target_dir() {
    // Drive `junius` from the test crate root (assert_cmd default) but tell
    // it to operate in a tempdir via --cwd; relative paths in args should
    // resolve there.
    let tmp = tempdir();
    make_repo(tmp.path(), &[]);
    write_basic_plugin(tmp.path(), "demo");

    cmd()
        .arg("--cwd")
        .arg(tmp.path())
        .args(["plugin", "info", "demo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Plugin: demo"));
}

// --- stub subcommands ---------------------------------------------------------
// (sync and `new plugin` were stubs in M01 and are implemented in M03 — their
// real behaviour is covered by tests/sync.rs. The remaining stubs are still
// stubs.)

#[test]
fn migrate_down_unsupported_exits_64() {
    cmd()
        .args(["migrate", "down"])
        .assert()
        .failure()
        .code(64)
        .stderr(predicate::str::contains("not supported"));
}

#[test]
fn migrate_up_missing_config_errors() {
    cmd()
        .args(["migrate", "up", "--config", "does-not-exist.toml"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("cannot read"));
}

#[test]
fn plugin_enable_missing_config_errors() {
    // `plugin enable` is implemented (M11): with no deployment config it fails
    // reading `platform.toml` rather than returning the old NOT_IMPLEMENTED stub.
    cmd()
        .args([
            "plugin",
            "enable",
            "demo",
            "--config",
            "does-not-exist.toml",
        ])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("cannot read"));
}
