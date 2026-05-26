//! `junius` — Junius management CLI. See `src/cli.rs` for the user-facing surface.

mod cache;
mod cli;
mod commands;
mod exit;
mod hash;
mod markers;
mod output;
mod source;

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
        cli::Command::Build(a) => commands::build::run(&a),
        #[cfg(feature = "develop")]
        cli::Command::Dev(a) => commands::dev::run(&a),
        cli::Command::Migrate { subcommand } => commands::migrate::run(&subcommand),
        cli::Command::Plugin { subcommand } => commands::plugin_cmd::run(&subcommand, args.format),
        cli::Command::Cache { subcommand } => commands::cache_cmd::run(&subcommand, args.format),
        #[cfg(feature = "develop")]
        cli::Command::New { subcommand } => commands::new::run(&subcommand),
        cli::Command::I18n { subcommand } => commands::i18n::run(&subcommand, args.format),
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
