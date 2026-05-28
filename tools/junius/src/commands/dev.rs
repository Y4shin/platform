//! `junius dev` — the one-command developer loop.
//!
//! Two FE topologies, selected by `--frontend` (or auto-detected from
//! `[build] frontend` in `platform.toml`):
//!
//!   - **embedded** (today's default): Vite (`pnpm --filter @junius/shell
//!     dev`) at `:5173` + juniusd at `:18080`. Vite proxies `/h`, `/rpc`,
//!     `/api` to juniusd.
//!   - **ssr** (M23): the SSR Node server (`pnpm --filter @junius/shell-ssr
//!     dev`) at `:3000` + juniusd at `:18080`. The Node server reaches
//!     juniusd over the loopback address it gets via `JUNIUS_BE_INTERNAL_URL`;
//!     dev's `oidc_redirect_url` should point at `http://localhost:3000`.
//!
//! Children run in the foreground process group; Ctrl-C tears both down.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use junius_manifest::{FrontendDelivery, PlatformManifest};
use tokio::process::{Child, Command};
use tokio::runtime::Runtime;
use tokio::signal;

use crate::cli::{DevArgs, DevFrontend};
use crate::exit;

const DEFAULT_CONFIG: &str = "platform.toml";

const VITE_PNPM_FILTER: &str = "@junius/shell";
const SSR_PNPM_FILTER: &str = "@junius/shell-ssr";
const SSR_DEV_PORT: u16 = 3000;
const JUNIUSD_DEV_URL: &str = "http://127.0.0.1:18080";

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

    let mode = resolve_frontend_mode(args.frontend, &config_path);

    let (fe_label, fe_url) = match mode {
        DevFrontend::Embedded => ("Vite", "http://127.0.0.1:5173"),
        DevFrontend::Ssr => ("SSR Node", "http://127.0.0.1:3000"),
    };
    eprintln!("junius dev: starting {fe_label} + juniusd");
    eprintln!("  {fe_label}: {fe_url}");
    eprintln!("  juniusd:  {JUNIUSD_DEV_URL}");

    let mut frontend = match spawn_frontend(mode) {
        Ok(c) => c,
        Err(code) => return code,
    };
    let mut cargo = match spawn_cargo(&config_path) {
        Ok(c) => c,
        Err(code) => {
            drop(frontend);
            return code;
        }
    };

    // Children run in the same process group as junius. A Ctrl-C from the
    // tty sends SIGINT to every member, so all three of us receive it. We
    // only need to wait for the children to drain.
    tokio::select! {
        status = frontend.wait() => {
            eprintln!("junius dev: {fe_label} exited ({status:?})");
            wait_or_kill(&mut cargo).await;
            exit_status_to_code(&status)
        }
        status = cargo.wait() => {
            eprintln!("junius dev: juniusd exited ({status:?})");
            wait_or_kill(&mut frontend).await;
            exit_status_to_code(&status)
        }
        _ = signal::ctrl_c() => {
            eprintln!("\njunius dev: shutting down");
            wait_or_kill(&mut frontend).await;
            wait_or_kill(&mut cargo).await;
            exit::OK
        }
    }
}

/// Pick the dev FE topology: explicit `--frontend` wins; otherwise consult
/// `[build] frontend` in the deployment's `platform.toml` (`"none"` →
/// `ssr`, anything else → `embedded`). Errors loading the manifest fall
/// back to `embedded` (matches the today-default).
fn resolve_frontend_mode(arg: Option<DevFrontend>, config_path: &Path) -> DevFrontend {
    if let Some(mode) = arg {
        return mode;
    }
    match std::fs::read_to_string(config_path).ok().and_then(|s| {
        PlatformManifest::parse(&s)
            .ok()
            .map(|m| m.build.frontend)
    }) {
        Some(FrontendDelivery::None) => DevFrontend::Ssr,
        _ => DevFrontend::Embedded,
    }
}

fn spawn_frontend(mode: DevFrontend) -> Result<Child, i32> {
    match mode {
        DevFrontend::Embedded => Command::new("pnpm")
            .args(["--filter", VITE_PNPM_FILTER, "dev"])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                eprintln!("junius: failed to spawn pnpm (Vite): {e}");
                exit::PARSE_ERROR
            }),
        DevFrontend::Ssr => Command::new("pnpm")
            .args(["--filter", SSR_PNPM_FILTER, "dev"])
            // The SSR Node server needs the BE's address to proxy `/api` /
            // `/h` / `/rpc` to and to attach absolute URLs to SSR fetches.
            .env("JUNIUS_BE_INTERNAL_URL", JUNIUSD_DEV_URL)
            .env("PORT", SSR_DEV_PORT.to_string())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                eprintln!("junius: failed to spawn pnpm (SSR Node): {e}");
                exit::PARSE_ERROR
            }),
    }
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
