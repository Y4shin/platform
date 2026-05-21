//! Fixture-driven tests for plugin and platform manifest parsing + validation.
//!
//! Tests run with CWD = `tools/junius/` (`assert_cmd` does this for binary tests;
//! for in-process tests like this one, we resolve paths via `CARGO_MANIFEST_DIR`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use junius_manifest::{PlatformManifest, PluginManifest};

fn fixture(p: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(p)
}

fn read(p: &str) -> String {
    let path = fixture(p);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[derive(Debug)]
enum Expect {
    Ok,
    ParseFail,
    ValidationFails(&'static [&'static str]),
}

struct Case {
    file: &'static str,
    expect: Expect,
}

const PLUGIN_CASES: &[Case] = &[
    Case {
        file: "plugins/valid-minimal/plugin.toml",
        expect: Expect::Ok,
    },
    Case {
        file: "plugins/valid-full/plugin.toml",
        expect: Expect::Ok,
    },
    Case {
        file: "plugins/invalid-bad-name/plugin.toml",
        expect: Expect::ValidationFails(&["PLUGIN.NAME.INVALID"]),
    },
    Case {
        file: "plugins/invalid-missing-schema/plugin.toml",
        expect: Expect::ParseFail,
    },
    Case {
        file: "plugins/invalid-wrong-mount-prefix/plugin.toml",
        expect: Expect::ValidationFails(&["MOUNT.ROUTE.PREFIX"]),
    },
    Case {
        file: "plugins/invalid-bad-perm-name/plugin.toml",
        expect: Expect::ValidationFails(&["PERM.NAME.FORMAT"]),
    },
];

const PLATFORM_CASES: &[Case] = &[
    Case {
        file: "deployments/valid-minimal/platform.toml",
        expect: Expect::Ok,
    },
    Case {
        file: "deployments/invalid-undefined-plugin/platform.toml",
        expect: Expect::ValidationFails(&["SOURCE.ONE_OF", "PLUGINS.ENABLED.UNIQUE"]),
    },
];

#[test]
fn plugin_fixtures() {
    for c in PLUGIN_CASES {
        let src = read(c.file);
        match (PluginManifest::parse(&src), &c.expect) {
            (Ok(m), Expect::Ok) => {
                let r = m.validate();
                assert!(r.is_ok(), "{}: expected valid, got {:?}", c.file, r.issues);
            }
            (Ok(m), Expect::ValidationFails(codes)) => {
                let r = m.validate();
                let got: Vec<&str> = r.issues.iter().map(|i| i.code).collect();
                for code in *codes {
                    assert!(
                        got.contains(code),
                        "{}: missing code {code} (got {got:?})",
                        c.file
                    );
                }
            }
            (Err(_), Expect::ParseFail) => {}
            (Ok(_), Expect::ParseFail) => panic!("{}: expected parse failure", c.file),
            (Err(e), Expect::Ok) => panic!("{}: expected valid, got parse error {e}", c.file),
            (Err(e), Expect::ValidationFails(_)) => {
                panic!(
                    "{}: expected validation failure, got parse error {e}",
                    c.file
                )
            }
        }
    }
}

#[test]
fn platform_fixtures() {
    for c in PLATFORM_CASES {
        let src = read(c.file);
        match (PlatformManifest::parse(&src), &c.expect) {
            (Ok(m), Expect::Ok) => {
                let r = m.validate();
                assert!(r.is_ok(), "{}: expected valid, got {:?}", c.file, r.issues);
            }
            (Ok(m), Expect::ValidationFails(codes)) => {
                let r = m.validate();
                let got: Vec<&str> = r.issues.iter().map(|i| i.code).collect();
                for code in *codes {
                    assert!(
                        got.contains(code),
                        "{}: missing code {code} (got {got:?})",
                        c.file
                    );
                }
            }
            (Err(_), Expect::ParseFail) => {}
            (Ok(_), Expect::ParseFail) => panic!("{}: expected parse failure", c.file),
            (Err(e), Expect::Ok) => panic!("{}: expected valid, got parse error {e}", c.file),
            (Err(e), Expect::ValidationFails(_)) => {
                panic!(
                    "{}: expected validation failure, got parse error {e}",
                    c.file
                )
            }
        }
    }
}
