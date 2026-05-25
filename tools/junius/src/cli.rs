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
    /// deployment config. (Source-tree-only — gated behind the `develop` feature.)
    #[cfg(feature = "develop")]
    Sync(SyncArgs),
    /// Build a deployment binary.
    Build(BuildArgs),
    /// One-command developer loop. (Source-tree-only — gated behind `develop`.)
    #[cfg(feature = "develop")]
    Dev(DevArgs),
    /// Apply database migrations and emit per-plugin Postgres role grants.
    Migrate {
        #[command(subcommand)]
        subcommand: MigrateCmd,
    },
    /// Plugin inspection and lifecycle.
    Plugin {
        #[command(subcommand)]
        subcommand: PluginCmd,
    },
    /// Manage the junius source cache (`~/.cache/junius`).
    Cache {
        #[command(subcommand)]
        subcommand: CacheCmd,
    },
    /// Scaffold new plugins, components, RPC services, migrations, permissions.
    /// (Source-tree-only — gated behind the `develop` feature.)
    #[cfg(feature = "develop")]
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

    /// Git ref to diff against for breaking-change detection
    /// (`SQL.EXPOSED.NO_BREAKING`). No-op outside a git repo or if the ref is
    /// unresolvable. CI typically passes `origin/main`.
    #[arg(long, default_value = "main")]
    pub base: String,
}

#[cfg(feature = "develop")]
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
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent build flags, not a state machine"
)]
pub struct BuildArgs {
    /// Path to the deployment's `platform.toml`. Defaults to `./platform.toml`.
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Validate the deployment config (resolve source + check manifests) and
    /// stop — no compile, no database.
    #[arg(long)]
    pub check_config: bool,

    /// Rewrite `platform.lock` to match the resolved source instead of failing
    /// on a mismatch (like `cargo update`).
    #[arg(long)]
    pub update_lock: bool,

    /// Overwrite an existing `./platform` artifact.
    #[arg(long)]
    pub force: bool,

    /// Use the cached git source as-is (skip `git fetch`).
    #[arg(long)]
    pub offline: bool,
}

#[cfg(feature = "develop")]
#[derive(clap::Args, Debug)]
pub struct DevArgs {
    /// Path to the deployment's `platform.toml`. Defaults to `./platform.toml`.
    #[arg(long)]
    pub config: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum MigrateCmd {
    /// Apply all pending migrations, then emit per-plugin Postgres role grants.
    Up {
        /// Path to the deployment's `platform.toml`. Defaults to `./platform.toml`.
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Roll back the most recently applied migration. Not supported in v1
    /// (forward-fix discipline — write a new migration instead).
    Down,
    /// Show applied and pending migrations.
    Status {
        /// Path to the deployment's `platform.toml`. Defaults to `./platform.toml`.
        #[arg(long)]
        config: Option<PathBuf>,
    },
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
    /// Add a plugin to a deployment's `[plugins].enabled` (verifying its
    /// required dependencies are enabled) and re-sync.
    Enable {
        name: String,
        /// Path to platform.toml. Defaults to ./platform.toml.
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Remove a plugin from `[plugins].enabled` (refusing if another enabled
    /// plugin requires it) and re-sync.
    Disable {
        name: String,
        /// Path to platform.toml. Defaults to ./platform.toml.
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
pub enum CacheCmd {
    /// Evict cached git sources older than a duration (e.g. `30d`; `0d` = all).
    Prune {
        #[arg(long, default_value = "30d")]
        older_than: String,
    },
}

#[cfg(feature = "develop")]
#[derive(Subcommand, Debug)]
pub enum NewCmd {
    /// Scaffold a new plugin under `plugins/<NAME>/`. Pure scaffolding — does
    /// not require or touch a deployment `platform.toml`; run `junius sync
    /// --config <deployment>` afterward to wire it in.
    Plugin {
        /// Plugin name (must match `^[a-z][a-z0-9_-]*$`).
        name: String,
        /// Scaffold a backend-only plugin (no `frontend/` package).
        #[arg(long)]
        backend_only: bool,
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
