//! Public API surface that every Junius plugin imports.
//!
//! At M02 this is the smallest possible surface that lets a plugin compile:
//! the `Plugin` trait, plugin metadata, a stub `PluginResources`, and the
//! `plugin_metadata!()` macro re-export. DB, storage, jobs, email, and auth
//! handles land in later milestones (M06–M10) as fields on `PluginResources`.

pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod metadata;
pub mod plugin;
pub mod resources;
pub mod rpc;
pub mod secrets;
pub mod telemetry;

pub use junius_sdk_macros::plugin_metadata;

pub use auth::{
    AuditEmitter, Auth, CurrentUser, GroupId, MaybeUser, Membership, Role, RoleId, User,
    UserDisplay, UserId, Users,
};
pub use config::PluginConfig;
pub use db::PluginDb;
pub use error::PluginError;
pub use metadata::{
    DependencyDecl, ExposedComponentDecl, ExposedTableDecl, MountPoints, PermissionDecl,
    PluginMetadata,
};
pub use plugin::Plugin;
pub use resources::{PluginResourceCtx, PluginResources};
pub use secrets::{SecretStore, SecretString};
pub use telemetry::Telemetry;
