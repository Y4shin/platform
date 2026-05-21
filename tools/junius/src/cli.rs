//! Clap derive structs for the entire CLI surface.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "junius",
    version,
    about = "Junius management CLI",
    disable_help_subcommand = true
)]
pub struct Args {
    /// Output format for command results.
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Plain)]
    pub format: OutputFormat,

    /// Increase verbosity (repeat for more).
    #[arg(long, short, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Run as if junius were started in <CWD>.
    #[arg(long, global = true)]
    pub cwd: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum OutputFormat {
    Plain,
    Json,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Validate manifests against the schema and semantic rules.
    Check(CheckArgs),
    /// Rewrite derived files (`platform/src/generated/plugins.rs` and the
    /// `junius managed` regions of `Cargo.toml`s) so they match the
    /// deployment config.
    Sync(SyncArgs),
    /// Build a deployment binary. (Stub — lands in M04.)
    Build(BuildArgs),
    /// One-command developer loop. (Stub — lands in M04.)
    Dev(DevArgs),
    /// Database migrations. (Stub — lands in M06.)
    Migrate {
        #[command(subcommand)]
        subcommand: MigrateCmd,
    },
    /// Plugin inspection and lifecycle.
    Plugin {
        #[command(subcommand)]
        subcommand: PluginCmd,
    },
    /// Scaffold new plugins, components, RPC services, migrations, permissions.
    New {
        #[command(subcommand)]
        subcommand: NewCmd,
    },
}

#[derive(clap::Args, Debug)]
pub struct CheckArgs {
    /// Validate a single manifest at this path.
    #[arg(long)]
    pub manifest: Option<PathBuf>,

    /// Validate the manifest at plugins/<NAME>/plugin.toml.
    #[arg(long, conflicts_with = "manifest")]
    pub plugin: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct SyncArgs {
    /// Scope to a single plugin. (Not yet implemented — planned for M09.)
    #[arg(long)]
    pub plugin: Option<String>,

    /// Report pending changes without writing. Exits 1 if anything would
    /// change, 0 if everything is already in sync. Useful as a CI drift check.
    #[arg(long)]
    pub dry_run: bool,

    /// Path to the deployment's `platform.toml`. Defaults to `./platform.toml`.
    #[arg(long)]
    pub config: Option<PathBuf>,
}

#[derive(clap::Args, Debug)]
pub struct BuildArgs {
    /// Build with optimisations enabled.
    #[arg(long)]
    pub release: bool,

    /// Path to the deployment's `platform.toml`. Defaults to `./platform.toml`.
    #[arg(long)]
    pub config: Option<PathBuf>,
}

#[derive(clap::Args, Debug)]
pub struct DevArgs {
    /// Path to the deployment's `platform.toml`. Defaults to `./platform.toml`.
    #[arg(long)]
    pub config: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum MigrateCmd {
    /// Apply all pending migrations.
    Up,
    /// Roll back the most recently applied migration.
    Down,
    /// Show applied and pending migrations.
    Status,
}

#[derive(Subcommand, Debug)]
pub enum PluginCmd {
    /// List enabled plugins in a deployment config.
    List {
        /// Path to platform.toml. Defaults to ./platform.toml.
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Show a plugin's manifest summary.
    Info { name: String },
    /// Add a plugin to platform.toml. (Stub — lands in M11.)
    Enable { name: String },
    /// Remove a plugin from platform.toml. (Stub — lands in M11.)
    Disable { name: String },
}

#[derive(Subcommand, Debug)]
pub enum NewCmd {
    /// Scaffold a new plugin under `plugins/<NAME>/` and run `junius sync`.
    Plugin {
        /// Plugin name (must match `^[a-z][a-z0-9_-]*$`).
        name: String,
    },
    /// Add a new component to a plugin's frontend lib. (Stub — lands in M07.)
    Component { plugin: String, name: String },
    /// Add a new RPC service to a plugin's proto. (Stub — lands in M05.)
    Rpc { plugin: String, service: String },
    /// Create a new migration under `plugins/<PLUGIN>/migrations/`. (Stub — lands in M06.)
    Migration { plugin: String, name: String },
    /// Add a permission entry to a plugin's manifest. (Stub — lands in M07.)
    Permission { plugin: String, perm: String },
}
