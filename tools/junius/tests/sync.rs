//! `junius sync` integration tests. Each test materialises a tiny pseudo-
//! repo in a tempdir, then runs the binary against it and asserts on the
//! generated files. Keeps the real workspace untouched.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::fs;

use predicates::prelude::*;

use common::{cmd_in, make_repo, tempdir, write_basic_plugin};

#[test]
fn sync_with_hello_writes_expected_files() {
    let tmp = tempdir();
    make_repo(tmp.path(), &["hello"]);

    // hello is an RPC-bearing plugin: a `proto/` dir makes `sync` emit its RPC
    // barrel. UI-only plugins (no `proto/`) get none — see `widgets`.
    let proto_dir = tmp.path().join("plugins/hello/proto/hello/v1");
    fs::create_dir_all(&proto_dir).unwrap();
    fs::write(
        proto_dir.join("hello.proto"),
        "syntax = \"proto3\";\npackage hello.v1;\n",
    )
    .unwrap();

    cmd_in(tmp.path()).args(["sync"]).assert().success();

    let plugins_rs =
        fs::read_to_string(tmp.path().join("platform/src/generated/plugins.rs")).unwrap();
    insta::assert_snapshot!("plugins_rs_hello", plugins_rs);

    let platform_cargo = fs::read_to_string(tmp.path().join("platform/Cargo.toml")).unwrap();
    assert!(platform_cargo.contains("hello-plugin = { path = \"../plugins/hello\" }"));

    let workspace_cargo = fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
    assert!(workspace_cargo.contains("    \"plugins/hello\","));

    let routes_ts =
        fs::read_to_string(tmp.path().join("platform/frontend/src/generated/routes.ts")).unwrap();
    insta::assert_snapshot!("routes_ts_hello", routes_ts);

    let registry_ts = fs::read_to_string(
        tmp.path()
            .join("platform/frontend/src/generated/component-registry.ts"),
    )
    .unwrap();
    insta::assert_snapshot!("registry_ts_empty", registry_ts);

    let rpc_barrel = fs::read_to_string(
        tmp.path()
            .join("packages/generated/src/plugins/hello/rpc.ts"),
    )
    .unwrap();
    insta::assert_snapshot!("rpc_barrel_hello", rpc_barrel);
}

#[test]
fn sync_empty_collapses_managed_regions() {
    let tmp = tempdir();
    make_repo(tmp.path(), &[]);

    cmd_in(tmp.path()).args(["sync"]).assert().success();

    let plugins_rs =
        fs::read_to_string(tmp.path().join("platform/src/generated/plugins.rs")).unwrap();
    assert!(plugins_rs.contains("Vec::new()"));

    let platform_cargo = fs::read_to_string(tmp.path().join("platform/Cargo.toml")).unwrap();
    assert!(!platform_cargo.contains("plugin"));
}

#[test]
fn sync_is_idempotent() {
    let tmp = tempdir();
    make_repo(tmp.path(), &["hello"]);

    cmd_in(tmp.path()).args(["sync"]).assert().success();
    // Second run: nothing pending; dry-run returns 0.
    cmd_in(tmp.path())
        .args(["sync", "--dry-run"])
        .assert()
        .success();
}

#[test]
fn dry_run_reports_changes_and_exits_nonzero() {
    let tmp = tempdir();
    make_repo(tmp.path(), &["hello"]);

    cmd_in(tmp.path())
        .args(["sync", "--dry-run"])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("updated"));

    assert!(
        !tmp.path()
            .join("platform/src/generated/plugins.rs")
            .exists()
    );
}

#[test]
fn sync_orders_plugins_by_enabled_list() {
    let tmp = tempdir();
    make_repo(tmp.path(), &["alpha", "bravo"]);

    cmd_in(tmp.path()).args(["sync"]).assert().success();

    let plugins_rs =
        fs::read_to_string(tmp.path().join("platform/src/generated/plugins.rs")).unwrap();
    let alpha_pos = plugins_rs.find("alpha_plugin").expect("alpha line");
    let bravo_pos = plugins_rs.find("bravo_plugin").expect("bravo line");
    assert!(alpha_pos < bravo_pos, "expected alpha before bravo");

    let routes_ts =
        fs::read_to_string(tmp.path().join("platform/frontend/src/generated/routes.ts")).unwrap();
    let alpha_pos = routes_ts.find("buildAlpha").expect("buildAlpha import");
    let bravo_pos = routes_ts.find("buildBravo").expect("buildBravo import");
    assert!(alpha_pos < bravo_pos, "expected alpha route ahead of bravo");
}

// --- error paths -------------------------------------------------------------

#[test]
fn sync_with_missing_config_errors() {
    let tmp = tempdir();
    // No platform.toml in the tempdir.
    cmd_in(tmp.path())
        .args(["sync"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("cannot read"));
}

#[test]
fn sync_with_invalid_platform_toml_errors() {
    let tmp = tempdir();
    fs::create_dir_all(tmp.path().join("platform/src")).unwrap();
    fs::write(tmp.path().join("platform.toml"), "this is = not toml").unwrap();

    cmd_in(tmp.path())
        .args(["sync"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("parse error"));
}

#[test]
fn sync_with_invalid_plugin_manifest_errors() {
    let tmp = tempdir();
    make_repo(tmp.path(), &["badname"]);
    // Overwrite the basic plugin manifest with one that fails validation
    // (uppercase letters in the name).
    fs::write(
        tmp.path().join("plugins/badname/plugin.toml"),
        "[plugin]\nname = \"BadName\"\ndisplay_name = \"Bad\"\nmanifest_schema = 1\n",
    )
    .unwrap();

    cmd_in(tmp.path())
        .args(["sync"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("PLUGIN.NAME.INVALID"));
}

#[test]
fn sync_with_referenced_plugin_missing_errors() {
    let tmp = tempdir();
    // platform.toml lists "ghost" but plugins/ghost/ doesn't exist.
    make_repo(tmp.path(), &[]);
    fs::write(
        tmp.path().join("platform.toml"),
        "[source]\npath = \".\"\n\n[plugins]\nenabled = [\"ghost\"]\n",
    )
    .unwrap();
    // No write_basic_plugin call.

    cmd_in(tmp.path())
        .args(["sync"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("plugins/ghost/plugin.toml"));
}

#[test]
fn sync_plugin_arg_is_not_yet_implemented() {
    let tmp = tempdir();
    make_repo(tmp.path(), &["hello"]);

    cmd_in(tmp.path())
        .args(["sync", "--plugin", "hello"])
        .assert()
        .failure()
        .code(64)
        .stderr(predicate::str::contains("not yet implemented"));
}

// --- scaffold ---------------------------------------------------------------

#[test]
fn new_plugin_scaffolds_expected_files() {
    let tmp = tempdir();
    make_repo(tmp.path(), &[]);

    cmd_in(tmp.path())
        .args(["new", "plugin", "demo"])
        .assert()
        .success();

    let dir = tmp.path().join("plugins/demo");
    assert!(dir.join("plugin.toml").exists());
    assert!(dir.join("Cargo.toml").exists());
    assert!(dir.join("src/lib.rs").exists());

    let manifest = fs::read_to_string(dir.join("plugin.toml")).unwrap();
    assert!(manifest.contains("name = \"demo\""));
    assert!(manifest.contains("display_name = \"Demo\""));

    let lib_rs = fs::read_to_string(dir.join("src/lib.rs")).unwrap();
    assert!(lib_rs.contains("pub struct DemoPlugin"));
    assert!(lib_rs.contains("junius_sdk::plugin_metadata!()"));
}

#[test]
fn new_plugin_rejects_existing_dir() {
    let tmp = tempdir();
    make_repo(tmp.path(), &[]);
    write_basic_plugin(tmp.path(), "demo");

    cmd_in(tmp.path())
        .args(["new", "plugin", "demo"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn new_plugin_rejects_bad_name() {
    let tmp = tempdir();
    make_repo(tmp.path(), &[]);

    cmd_in(tmp.path())
        .args(["new", "plugin", "Demo"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("invalid plugin name"));

    // Nothing was scaffolded.
    assert!(!tmp.path().join("plugins/Demo").exists());
}
