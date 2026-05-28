//! `junius provision` — M18 Stage D declarative reconciliation (CLI front-end).
//!
//! The reconcile logic lives in [`junius_provision`] so the host can run the
//! same code path on boot (`auto_apply_on_boot = true`). This module owns
//! only the CLI plumbing: config loading, exit-code translation, and the
//! `apply`/`diff` user-facing summaries.

use std::path::{Path, PathBuf};

use junius_manifest::{PlatformManifest, ProvisioningConfig, resolve_config};
use junius_provision::{ApplyOutcome, ProvisionError, apply};
use sqlx::PgPool;

use crate::cli::ProvisionCmd;
use crate::commands::migrate::env_lookup;
use crate::exit;

pub fn run(cmd: &ProvisionCmd) -> i32 {
    match cmd {
        ProvisionCmd::Apply {
            config,
            force,
            allow_delete,
        } => tokio_runtime().block_on(run_apply(config.clone(), *force, *allow_delete)),
        ProvisionCmd::Diff { config } => tokio_runtime().block_on(run_diff(config.clone())),
    }
}

fn tokio_runtime() -> tokio::runtime::Runtime {
    #[allow(
        clippy::expect_used,
        reason = "tokio runtime construction failures are unrecoverable"
    )]
    tokio::runtime::Runtime::new().expect("tokio runtime")
}

async fn run_apply(config_path: Option<PathBuf>, force: bool, _allow_delete: bool) -> i32 {
    let Some((deployment_dir, provisioning, database_url)) = load(config_path) else {
        return exit::PARSE_ERROR;
    };

    let resolved = match provisioning.resolve_file(&deployment_dir) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("junius provision: failed to load `file = …` pointer: {e}");
            return exit::PARSE_ERROR;
        }
    };

    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("junius provision: connect to database failed: {e}");
            return exit::PARSE_ERROR;
        }
    };

    match apply(&pool, &resolved, force).await {
        Ok(ApplyOutcome {
            unchanged: true,
            hash,
            ..
        }) => {
            println!(
                "junius provision: already up-to-date (hash {})",
                &hash[..16]
            );
            exit::OK
        }
        Ok(ApplyOutcome {
            groups,
            user_roles,
            user_role_assignments,
            oidc_mappings,
            ..
        }) => {
            println!(
                "junius provision: applied {groups} group(s), {user_roles} user-role(s), \
                 {user_role_assignments} assignment(s), {oidc_mappings} oidc mapping(s)",
            );
            exit::OK
        }
        Err(ProvisionError::Db(e)) => {
            eprintln!("junius provision: apply failed: {e}");
            exit::PARSE_ERROR
        }
    }
}

#[allow(
    clippy::unused_async,
    reason = "uniform shape with apply(); a richer diff will await DB queries"
)]
async fn run_diff(config_path: Option<PathBuf>) -> i32 {
    let Some((deployment_dir, provisioning, _database_url)) = load(config_path) else {
        return exit::PARSE_ERROR;
    };
    let resolved = match provisioning.resolve_file(&deployment_dir) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("junius provision: failed to load `file = …` pointer: {e}");
            return exit::PARSE_ERROR;
        }
    };
    println!(
        "junius provision: {} group(s), {} user-role(s), {} assignment(s), {} oidc mapping(s) declared",
        resolved.groups.len(),
        resolved.user_roles.len(),
        resolved.user_role_assignments.len(),
        resolved.oidc_mappings.len(),
    );
    println!("content hash: {}", &resolved.content_hash()[..16]);
    // A richer diff (current state ↔ declaration) lands in a follow-up;
    // for now the hash + counters are the deterministic summary `apply`
    // uses.
    exit::OK
}

fn load(config_path: Option<PathBuf>) -> Option<(PathBuf, ProvisioningConfig, String)> {
    let path = config_path.unwrap_or_else(|| PathBuf::from("platform.toml"));
    let deployment_dir = path
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("junius provision: read {}: {e}", path.display());
            return None;
        }
    };
    let manifest = match PlatformManifest::parse(&src) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("junius provision: parse {}: {e}", path.display());
            return None;
        }
    };
    let Some(provisioning) = manifest.provisioning.clone() else {
        eprintln!(
            "junius provision: no [provisioning] block in {}",
            path.display()
        );
        return None;
    };
    let resolved = match resolve_config(&manifest.config, &env_lookup()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("junius provision: resolve [config]: {e}");
            return None;
        }
    };
    Some((deployment_dir, provisioning, resolved.database_url.clone()))
}
