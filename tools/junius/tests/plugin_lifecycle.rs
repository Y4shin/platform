//! `junius plugin enable/disable` edit `[plugins].enabled` (preserving comments)
//! and enforce dependency constraints.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::fs;

use common::{cmd_in, make_repo, tempdir, write_basic_plugin};

/// Write a plugin that requires `dep` (non-optional).
fn write_plugin_requiring(repo: &std::path::Path, name: &str, dep: &str) {
    let dir = repo.join("plugins").join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("plugin.toml"),
        format!(
            "[plugin]\nname = \"{name}\"\ndisplay_name = \"{name}\"\nmanifest_schema = 1\n\
             [dependencies.{dep}]\n"
        ),
    )
    .unwrap();
}

fn platform_toml(repo: &std::path::Path) -> String {
    fs::read_to_string(repo.join("platform.toml")).unwrap()
}

#[test]
fn enable_disable_round_trip_with_dep_checks() {
    let tmp = tempdir();
    make_repo(tmp.path(), &["hello"]);
    // Present in the source but not enabled.
    write_basic_plugin(tmp.path(), "widgets");
    write_plugin_requiring(tmp.path(), "greeter", "hello");
    // Add a comment we expect to survive the edits.
    let toml = platform_toml(tmp.path()).replace("[plugins]", "# deployment plugins\n[plugins]");
    fs::write(tmp.path().join("platform.toml"), toml).unwrap();

    // Enable a leaf plugin.
    cmd_in(tmp.path())
        .args(["plugin", "enable", "widgets"])
        .assert()
        .success();
    let after = platform_toml(tmp.path());
    assert!(after.contains("widgets"), "widgets added to enabled");
    assert!(after.contains("# deployment plugins"), "comment preserved");

    // Enable a plugin whose required dep (hello) is enabled.
    cmd_in(tmp.path())
        .args(["plugin", "enable", "greeter"])
        .assert()
        .success();

    // Disabling hello is refused while greeter requires it.
    cmd_in(tmp.path())
        .args(["plugin", "disable", "hello"])
        .assert()
        .failure();

    // Disabling a leaf works.
    cmd_in(tmp.path())
        .args(["plugin", "disable", "widgets"])
        .assert()
        .success();
    assert!(!platform_toml(tmp.path()).contains("\"widgets\""));
}

#[test]
fn enable_refuses_when_required_dep_not_enabled() {
    let tmp = tempdir();
    make_repo(tmp.path(), &[]);
    write_basic_plugin(tmp.path(), "hello");
    write_plugin_requiring(tmp.path(), "greeter", "hello");

    cmd_in(tmp.path())
        .args(["plugin", "enable", "greeter"])
        .assert()
        .failure();
    // enabled stays empty.
    assert!(!platform_toml(tmp.path()).contains("greeter"));
}
