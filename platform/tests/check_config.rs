//! `juniusd --check-config` parses + resolves the deployment config (including
//! secret indirections) and exits 0 without binding a socket or connecting to
//! the database. The deployment-build CI smoke test relies on this.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;

// A complete `[config]` with literal values — the database_url is deliberately
// bogus to prove `--check-config` never connects.
const FIXTURE: &str = "\
[source]
path = \".\"

[plugins]
enabled = []

[config]
database_url = \"postgres://nobody@127.0.0.1:1/nope\"
oidc_issuer = \"http://localhost/\"
oidc_client_id = \"id\"
oidc_client_secret = \"secret\"
session_encryption_key = \"key\"
role_password_secret = \"rps\"
";

#[test]
fn check_config_exits_zero_without_a_database() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = tmp.path().join("platform.toml");
    std::fs::write(&cfg, FIXTURE).unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_juniusd"))
        .arg("--check-config")
        .arg("--config")
        .arg(&cfg)
        .status()
        .unwrap();
    assert!(status.success(), "juniusd --check-config should exit 0");
}
