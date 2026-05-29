//! `junius oidc resync` — M18 trigger #4 (the ops sweep).
//!
//! A thin client: it POSTs a Connect-JSON unary request to a running
//! `juniusd`'s `user.v1.UserService.ResyncOidcGroups`, authenticated with the
//! deployment's `admin_api_token`. The host does the actual work (token
//! decryption, `userinfo` call, reconcile) — this command never touches the
//! database, the session-encryption key, or the `IdP`. Contrast `migrate` /
//! `provision`, which run DB work directly because they are bootstrap-time.

use std::path::PathBuf;

use junius_manifest::{PlatformManifest, resolve_config};
use serde::Deserialize;

use crate::commands::migrate::env_lookup;
use crate::exit;

/// Default bind port the host listens on when `[config] bind_addr` is unset
/// (mirrors `platform::config`'s `127.0.0.1:18080`).
const DEFAULT_PORT: u16 = 18080;

pub fn run(user: Option<&str>, all: bool, config: Option<PathBuf>, url: Option<&str>) -> i32 {
    if user.is_none() && !all {
        eprintln!("junius oidc resync: specify --user <id> or --all");
        return exit::VALIDATION;
    }

    let (admin_token, base_url) = match load(config, url) {
        Ok(v) => v,
        Err(code) => return code,
    };
    let Some(admin_token) = admin_token else {
        eprintln!(
            "junius oidc resync: [config] admin_api_token is not set; the admin \
             sweep is disabled. Configure it (file:/env: secrets supported) and \
             restart juniusd."
        );
        return exit::VALIDATION;
    };

    let body = match (user, all) {
        (Some(u), _) => serde_json::json!({ "user": u }),
        (None, true) => serde_json::json!({ "all": true }),
        (None, false) => unreachable!("guarded above"),
    };

    tokio_runtime().block_on(call(&base_url, &admin_token, &body))
}

/// Load `admin_api_token` + derive the juniusd base URL from `platform.toml`.
/// `--url` overrides the derived base URL.
fn load(config: Option<PathBuf>, url: Option<&str>) -> Result<(Option<String>, String), i32> {
    let path = config.unwrap_or_else(|| PathBuf::from("platform.toml"));
    let src = std::fs::read_to_string(&path).map_err(|e| {
        eprintln!("junius oidc resync: read {}: {e}", path.display());
        exit::PARSE_ERROR
    })?;
    let manifest = PlatformManifest::parse(&src).map_err(|e| {
        eprintln!("junius oidc resync: parse {}: {e}", path.display());
        exit::PARSE_ERROR
    })?;
    let resolved = resolve_config(&manifest.config, &env_lookup()).map_err(|e| {
        eprintln!("junius oidc resync: resolve [config]: {e}");
        exit::PARSE_ERROR
    })?;

    let base_url = url.map_or_else(
        || {
            let port = resolved
                .bind_addr
                .as_deref()
                .and_then(|a| a.rsplit(':').next())
                .and_then(|p| p.parse::<u16>().ok())
                .unwrap_or(DEFAULT_PORT);
            format!("http://127.0.0.1:{port}")
        },
        str::to_string,
    );
    Ok((resolved.admin_api_token, base_url))
}

async fn call(base_url: &str, admin_token: &str, body: &serde_json::Value) -> i32 {
    let endpoint = format!("{base_url}/rpc/user.v1.UserService/ResyncOidcGroups");
    let client = reqwest::Client::new();
    let resp = match client
        .post(&endpoint)
        .bearer_auth(admin_token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("junius oidc resync: request to {endpoint} failed: {e}");
            return exit::PARSE_ERROR;
        }
    };

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        // Connect errors carry `{ "code", "message" }`.
        let detail = serde_json::from_str::<ConnectError>(&text)
            .map(|e| format!("{}: {}", e.code, e.message))
            .unwrap_or(text);
        eprintln!("junius oidc resync: juniusd returned {status}: {detail}");
        return exit::PARSE_ERROR;
    }

    let parsed: ResyncResponse = serde_json::from_str(&text).unwrap_or_default();
    print_results(&parsed.results);
    exit::OK
}

fn print_results(results: &[UserResult]) {
    if results.is_empty() {
        println!("junius oidc resync: no users matched (none with a live stored token).");
        return;
    }
    println!("junius oidc resync: {} user(s) processed", results.len());
    for r in results {
        let who = if r.email.is_empty() { &r.user_id } else { &r.email };
        match (&r.result, &r.skipped) {
            (Some(rc), _) => println!(
                "  {who}: {} claimed, {} added, {} reaped",
                rc.oidc_groups_claimed, rc.memberships_added, rc.memberships_reaped
            ),
            (None, Some(reason)) => println!("  {who}: skipped ({reason})"),
            (None, None) => println!("  {who}: no change"),
        }
    }
}

fn tokio_runtime() -> tokio::runtime::Runtime {
    #[allow(
        clippy::expect_used,
        reason = "tokio runtime construction failures are unrecoverable"
    )]
    tokio::runtime::Runtime::new().expect("tokio runtime")
}

#[derive(Deserialize)]
struct ConnectError {
    #[serde(default)]
    code: String,
    #[serde(default)]
    message: String,
}

#[derive(Deserialize, Default)]
struct ResyncResponse {
    #[serde(default)]
    results: Vec<UserResult>,
}

#[derive(Deserialize)]
struct UserResult {
    #[serde(rename = "userId", default)]
    user_id: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    result: Option<Reconcile>,
    #[serde(default)]
    skipped: Option<String>,
}

#[derive(Deserialize)]
struct Reconcile {
    #[serde(rename = "oidcGroupsClaimed", default)]
    oidc_groups_claimed: u32,
    #[serde(rename = "membershipsAdded", default)]
    memberships_added: u32,
    #[serde(rename = "membershipsReaped", default)]
    memberships_reaped: u32,
}
