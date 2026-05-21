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
    /// Bring derived files in line with manifests. (Stub — lands in M03.)
    Sync(SyncArgs),
    /// Build a deployment binary. (Stub — lands in M04.)
    Build(BuildArgs),
    /// One-command developer loop. (Stub — lands in M04.)
    Dev(DevArgs),
    /// Database migrations.
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
    #[arg(long)]
    pub plugin: Option<String>,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub config: Option<PathBuf>,
}

#[derive(clap::Args, Debug)]
pub struct BuildArgs {
    #[arg(long)]
    pub release: bool,
}

#[derive(clap::Args, Debug)]
pub struct DevArgs {
    #[arg(long)]
    pub config: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum MigrateCmd {
    Up,
    Down,
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
    Plugin { name: String },
    Component { plugin: String, name: String },
    Rpc { plugin: String, service: String },
    Migration { plugin: String, name: String },
    Permission { plugin: String, perm: String },
}
