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
    /// Inspect and validate per-plugin i18n catalogs (M14).
    I18n {
        #[command(subcommand)]
        subcommand: I18nCmd,
    },
    /// Apply the deployment's declarative provisioning (M18 Stage D).
    Provision {
        #[command(subcommand)]
        subcommand: ProvisionCmd,
    },
    /// `#[rpc_service]` codemods (M15). Source-tree-only.
    #[cfg(feature = "develop")]
    Rpc {
        #[command(subcommand)]
        subcommand: RpcCmd,
    },
}

#[cfg(feature = "develop")]
#[derive(Subcommand, Debug)]
pub enum RpcCmd {
    /// Insert a `todo!()` stub for every proto method missing from its
    /// `#[rpc_service]` impl block. Idempotent — a second run is a no-op.
    Scaffold(RpcScaffoldArgs),
}

#[cfg(feature = "develop")]
#[derive(clap::Args, Debug)]
pub struct RpcScaffoldArgs {
    /// Plugin name (matches `plugins/<name>/`). Scaffolds every service in
    /// the plugin's proto.
    #[arg(long)]
    pub plugin: String,

    /// Report missing stubs without writing. Exits 1 if any are missing,
    /// 0 if every proto method already has a handler. `junius check`'s
    /// `RPC.SERVICE.UNIMPLEMENTED` invokes this in `--check` mode.
    #[arg(long)]
    pub check: bool,
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
    /// FE delivery in dev: `embedded` (Vite SPA at :5173 — today's default)
    /// or `ssr` (M23 SSR Node server at :3000 + juniusd at :18080). When
    /// omitted, auto-detect from `[build] frontend` in `platform.toml`.
    #[arg(long, value_enum)]
    pub frontend: Option<DevFrontend>,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum DevFrontend {
    Embedded,
    Ssr,
}

#[derive(Subcommand, Debug)]
pub enum ProvisionCmd {
    /// Apply the `[provisioning]` block to the deployment's database.
    /// Idempotent: a second run with the same config is hash-guarded to a
    /// single audit event. Pass `--force` to bypass the hash and re-apply.
    Apply {
        /// Path to the deployment's `platform.toml`.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Re-apply even if the config hash matches the last-applied hash.
        #[arg(long)]
        force: bool,
        /// Permit deleting `managed_by='config'` rows whose declaration is
        /// missing from the current config. Without this flag, drift is
        /// reported as a warning + the apply refuses to proceed.
        #[arg(long)]
        allow_delete: bool,
    },
    /// Show the planned changes (additive only — destructive drift is
    /// reported separately, never executed) without applying them.
    Diff {
        #[arg(long)]
        config: Option<PathBuf>,
    },
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
pub enum I18nCmd {
    /// Validate every enabled plugin's `i18n/*.po` catalogs (BE key-based
    /// + FE Lingui-format) and optionally fail on extract-drift.
    Check {
        /// Path to platform.toml. Defaults to ./platform.toml.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Also run `lingui extract` and fail if it changes any FE `.po`
        /// file (catalog has drifted from source). Used in CI; off locally.
        #[arg(long)]
        check_drift: bool,
    },
    /// Run `lingui extract` to scan frontend source for `t` / `<Trans>` calls
    /// and update each package's `i18n/<locale>.po`. Wraps the Lingui CLI so
    /// plugin authors don't need to know it directly.
    Extract {
        /// Pass `--clean` to drop catalog entries no longer referenced in
        /// source. Off by default to match `pnpm exec lingui extract`.
        #[arg(long)]
        clean: bool,
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
