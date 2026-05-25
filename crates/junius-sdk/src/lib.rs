//! Public API surface that every Junius plugin imports.
//!
//! At M02 this is the smallest possible surface that lets a plugin compile:
//! the `Plugin` trait, plugin metadata, a stub `PluginResources`, and the
//! `plugin_metadata!()` macro re-export. DB, storage, jobs, email, and auth
//! handles land in later milestones (M06–M10) as fields on `PluginResources`.

pub mod auth;
pub mod authz;
pub mod config;
pub mod context;
pub mod db;
pub mod email;
pub mod error;
pub mod metadata;
pub mod permissions;
pub mod plugin;
pub mod repo;
pub mod resources;
pub mod secrets;
pub mod telemetry;

pub use junius_sdk_macros::{PluginCtx, impl_repository, permissions, plugin_metadata, repository};

pub use authz::{Authz, Principal, ShareRecord};
pub use context::{ApiError, BuildState, PluginContext, system_context};
pub use permissions::{And, Has, Permission, PermissionList};
pub use repo::{RepoPool, ScopedDb};
pub use telemetry::MetricSink;

pub use auth::{
    AuditEmitter, Auth, CurrentUser, GroupId, MaybeUser, Membership, Role, RoleId, User,
    UserDisplay, UserId, Users,
};
pub use config::PluginConfig;
pub use db::PluginDb;
pub use email::{Attachment, Email, EmailMessage, Transport, TransportError};
pub use error::{PluginError, RepoError};
pub use metadata::{
    DependencyDecl, ExposedComponentDecl, ExposedTableDecl, MountPoints, PermissionDecl,
    PluginMetadata,
};
pub use plugin::Plugin;
pub use resources::{PluginResourceCtx, PluginResources};
pub use secrets::{SecretStore, SecretString};
pub use telemetry::Telemetry;
