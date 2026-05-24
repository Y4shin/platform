//! `junius dev` — the one-command developer loop.
//!
//! At M04 this spawns Vite (`pnpm --filter @junius/shell dev`) and the host
//! binary (`cargo run -p platform`) concurrently and forwards their output to
//! the current terminal. Ctrl-C tears both children down via the foreground
//! process group; the parent only waits for them to drain.
//!
//! Vite serves the SPA at <http://127.0.0.1:5173> and proxies `/h`, `/rpc`,
//! `/api` to the host at <http://127.0.0.1:18080>.
//!
//! File-watching, proto regen, and manifest-driven re-sync land in M05/M06.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::process::{Child, Command};
use tokio::runtime::Runtime;
use tokio::signal;

use crate::cli::DevArgs;
use crate::exit;

const DEFAULT_CONFIG: &str = "platform.toml";

const VITE_PNPM_FILTER: &str = "@junius/shell";

pub fn run(args: &DevArgs) -> i32 {
    let runtime = match Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("junius: failed to start tokio runtime: {e}");
            return exit::PARSE_ERROR;
        }
    };
    runtime.block_on(run_async(args))
}

async fn run_async(args: &DevArgs) -> i32 {
    let sync_args = crate::cli::SyncArgs {
        plugin: None,
        dry_run: false,
        config: args.config.clone(),
    };
    let sync_code = super::sync::run(&sync_args, crate::cli::OutputFormat::Plain);
    if sync_code != exit::OK {
        eprintln!("junius: dev: sync failed; aborting");
        return sync_code;
    }

    // Apply migrations + (re-)emit role grants before booting the host.
    let migrate_code = super::migrate::up_async(args.config.clone()).await;
    if migrate_code != exit::OK {
        eprintln!("junius: dev: migrate up failed; aborting (is Postgres running?)");
        return migrate_code;
    }

    let config_path = args
        .config
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG));

    eprintln!("junius dev: starting Vite + juniusd");
    eprintln!("  Vite:    http://127.0.0.1:5173");
    eprintln!("  juniusd: http://127.0.0.1:18080");

    let mut vite = match spawn_vite() {
        Ok(c) => c,
        Err(code) => return code,
    };
    let mut cargo = match spawn_cargo(&config_path) {
        Ok(c) => c,
        Err(code) => {
            // Best-effort kill of the already-spawned child. tokio sends
            // SIGKILL via `kill_on_drop`; that's fine here — we're aborting.
            drop(vite);
            return code;
        }
    };

    // Children run in the same process group as junius. A Ctrl-C from the
    // tty sends SIGINT to every member, so all three of us receive it. We
    // only need to wait for the children to drain.
    tokio::select! {
        status = vite.wait() => {
            eprintln!("junius dev: Vite exited ({status:?})");
            wait_or_kill(&mut cargo).await;
            exit_status_to_code(&status)
        }
        status = cargo.wait() => {
            eprintln!("junius dev: juniusd exited ({status:?})");
            wait_or_kill(&mut vite).await;
            exit_status_to_code(&status)
        }
        _ = signal::ctrl_c() => {
            eprintln!("\njunius dev: shutting down");
            wait_or_kill(&mut vite).await;
            wait_or_kill(&mut cargo).await;
            exit::OK
        }
    }
}

fn spawn_vite() -> Result<Child, i32> {
    Command::new("pnpm")
        .args(["--filter", VITE_PNPM_FILTER, "dev"])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            eprintln!("junius: failed to spawn pnpm: {e}");
            exit::PARSE_ERROR
        })
}

fn spawn_cargo(config_path: &Path) -> Result<Child, i32> {
    // The host reads its config from JUNIUS_CONFIG; secret env vars
    // (OIDC_CLIENT_SECRET / SESSION_KEY / ROLE_PW_SECRET) are inherited from
    // this process so the same values used for `migrate up` reach the host.
    Command::new("cargo")
        .args(["run", "-p", "platform"])
        .env("JUNIUS_CONFIG", config_path)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            eprintln!("junius: failed to spawn cargo: {e}");
            exit::PARSE_ERROR
        })
}

async fn wait_or_kill(child: &mut Child) {
    // Give the child the same 2s it gets in production for an orderly exit
    // before resorting to SIGKILL.
    match tokio::time::timeout(Duration::from_secs(2), child.wait()).await {
        Ok(_) => {}
        Err(_) => {
            let _ = child.kill().await;
        }
    }
}

fn exit_status_to_code(status: &std::io::Result<std::process::ExitStatus>) -> i32 {
    match status {
        Ok(s) if s.success() => exit::OK,
        Ok(s) => s.code().unwrap_or(1),
        Err(_) => 1,
    }
}
