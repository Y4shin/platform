//! Repository primitives — the only sanctioned path from a plugin to its SQL
//! executor.
//!
//! [`PluginDb`] exposes no public pool. A repository type (written with the
//! [`#[repository]`](macro@crate::repository) attribute) owns a [`ScopedDb`]
//! constructed by the generated `new`, and reaches the executor only through the
//! [`RepoPool`] trait.
//!
//! The data-access guardrail is layered, not a hard seal: ordinary plugin code is
//! handed only an opaque [`PluginDb`] (not an executor), so a `&sqlx::PgPool` is
//! never in reach by accident. The `ScopedDb` constructor and `RepoPool::pool`
//! are `#[doc(hidden)]` because the `#[repository]`-generated code (which expands
//! in the *plugin* crate) has to call them — so a determined author could too;
//! `junius check` closes that gap by flagging `sqlx::query*` / pool access outside
//! `#[impl_repository]` blocks. `RepoPool` is additionally sealed so it can't be
//! implemented for other types.

use sqlx::PgPool;

use crate::db::PluginDb;

/// Opaque wrapper over a plugin's connection pool, held privately inside
/// repository types. Constructed only by the `#[repository]`-generated `new`;
/// the executor is reachable solely through the sealed [`RepoPool`] trait.
#[derive(Clone)]
pub struct ScopedDb(PgPool);

impl ScopedDb {
    /// Build from the plugin's DB handle. Hidden: called only by the generated
    /// `Repository::new`, never by plugin code.
    #[doc(hidden)]
    #[must_use]
    pub fn __from_plugin_db(db: &PluginDb) -> Self {
        Self(db.pool().clone())
    }
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::ScopedDb {}
}

/// Sealed accessor for the executor inside a [`ScopedDb`]. The
/// `#[repository]`-generated inherent `pool()` uses it; it cannot be implemented
/// downstream (sealed) and there is no other public way to obtain a
/// `&sqlx::PgPool`, which is what confines SQL to repository impls.
pub trait RepoPool: sealed::Sealed {
    #[doc(hidden)]
    fn pool(&self) -> &PgPool;
}

impl RepoPool for ScopedDb {
    fn pool(&self) -> &PgPool {
        &self.0
    }
}
