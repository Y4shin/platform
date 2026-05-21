//! `junius build` — the production build pipeline.
//!
//! Steps, in order:
//!   1. `junius sync` (idempotent — keeps generated files in step with the
//!      manifest before the actual build).
//!   2. `pnpm --filter @junius/shell build` (Vite → `platform/frontend/dist/`).
//!   3. `cargo build -p platform --features embed-frontend [--release]`.
//!
//! At the end the deployment artifact is `target/{debug|release}/juniusd`
//! and has the SPA bundle embedded via `rust-embed`.

use std::process::Command;

use crate::cli::{BuildArgs, OutputFormat, SyncArgs};
use crate::exit;

const VITE_PNPM_FILTER: &str = "@junius/shell";

pub fn run(args: &BuildArgs) -> i32 {
    let sync_code = super::sync::run(
        &SyncArgs {
            plugin: None,
            dry_run: false,
            config: args.config.clone(),
        },
        OutputFormat::Plain,
    );
    if sync_code != exit::OK {
        eprintln!("junius: build: sync failed; aborting");
        return sync_code;
    }

    if let Some(code) = run_step("pnpm", &["--filter", VITE_PNPM_FILTER, "build"]) {
        return code;
    }

    let mut cargo_args: Vec<&str> = vec!["build", "-p", "platform", "--features", "embed-frontend"];
    if args.release {
        cargo_args.push("--release");
    }
    if let Some(code) = run_step("cargo", &cargo_args) {
        return code;
    }

    let profile = if args.release { "release" } else { "debug" };
    println!("junius: build complete — target/{profile}/juniusd");
    exit::OK
}

fn run_step(program: &str, args: &[&str]) -> Option<i32> {
    println!("junius: $ {program} {}", args.join(" "));
    match Command::new(program).args(args).status() {
        Ok(status) if status.success() => None,
        Ok(status) => {
            eprintln!("junius: build step `{program}` failed with {status}");
            Some(status.code().unwrap_or(1))
        }
        Err(e) => {
            eprintln!("junius: build step `{program}` failed to spawn: {e}");
            Some(exit::PARSE_ERROR)
        }
    }
}
