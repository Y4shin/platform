//! `junius` — Junius management CLI. See `src/cli.rs` for the user-facing surface.

#[cfg(feature = "source-build")]
mod cache;
mod cli;
mod commands;
mod exit;
#[cfg(feature = "source-build")]
mod hash;
// `markers` is only used by `sync`/`build`/`plugin enable/disable` — gate
// on the same union so the trimmed in-container CLI doesn't carry it.
#[cfg(any(feature = "develop", feature = "source-build"))]
mod markers;
mod output;
// `source` stays compiled even without `source-build`: `commands::cross_check`
// calls `source::git` for the `SQL.EXPOSED.NO_BREAKING` rule on migration
// diffs, which `junius check` (always available) needs.
mod source;

/// Shared guard for tests that mutate the process-global current directory
/// (`std::env::set_current_dir`) or spawn subprocesses that inherit it. The CWD
/// is process-wide, so without serialisation a `set_current_dir` in one test
/// races a sibling test running concurrently in the same binary — the sibling's
/// `git`/subprocess can find the CWD removed mid-run ("Unable to read current
/// working directory"). Every such test holds this one lock for its full CWD-
/// sensitive window.
#[cfg(test)]
pub(crate) fn cwd_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    let args = cli::Args::parse();

    install_tracing(args.verbose);

    if let Some(cwd) = &args.cwd {
        if let Err(e) = std::env::set_current_dir(cwd) {
            eprintln!("junius: cannot chdir to {}: {e}", cwd.display());
            return ExitCode::from(exit::PARSE_ERROR.try_into().unwrap_or(1));
        }
    }

    let code = match args.command {
        cli::Command::Check(a) => commands::check::run(&a, args.format),
        #[cfg(feature = "develop")]
        cli::Command::Sync(a) => commands::sync::run(&a, args.format),
        #[cfg(feature = "source-build")]
        cli::Command::Build(a) => commands::build::run(&a),
        #[cfg(feature = "develop")]
        cli::Command::Dev(a) => commands::dev::run(&a),
        cli::Command::Migrate { subcommand } => commands::migrate::run(&subcommand),
        cli::Command::Plugin { subcommand } => commands::plugin_cmd::run(&subcommand, args.format),
        #[cfg(feature = "source-build")]
        cli::Command::Cache { subcommand } => commands::cache_cmd::run(&subcommand, args.format),
        #[cfg(feature = "develop")]
        cli::Command::New { subcommand } => commands::new::run(&subcommand),
        cli::Command::I18n { subcommand } => commands::i18n::run(&subcommand, args.format),
        cli::Command::Provision { subcommand } => commands::provision::run(&subcommand),
        cli::Command::Oidc { subcommand } => match subcommand {
            cli::OidcCmd::Resync {
                user,
                all,
                config,
                url,
            } => commands::oidc_resync::run(user.as_deref(), all, config, url.as_deref()),
        },
        #[cfg(feature = "develop")]
        cli::Command::Rpc { subcommand } => match subcommand {
            cli::RpcCmd::Scaffold(a) => commands::rpc_scaffold::run(&a),
        },
    };

    ExitCode::from(u8::try_from(code).unwrap_or(1))
}

fn install_tracing(verbosity: u8) {
    let default_level = match verbosity {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_level));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

/// Helper used by command modules to resolve a fixture/config path the user
/// provided. Returns the path unchanged if absolute, otherwise joined with cwd.
#[allow(dead_code)]
pub(crate) fn resolve_path(p: PathBuf) -> PathBuf {
    if p.is_absolute() {
        p
    } else if let Ok(cwd) = std::env::current_dir() {
        cwd.join(&p)
    } else {
        p
    }
}
