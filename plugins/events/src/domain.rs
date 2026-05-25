//! Shared domain types for the events plugin: the row id newtype and the
//! Postgres-enum mappings. The enum columns are real Postgres enums (declared in
//! `migrations/0001_event.up.sql`), mapped to Rust enums via `#[derive(sqlx::Type)]`
//! with the schema-qualified `type_name` so `sqlx::query!` can decode them.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A row id in `events.event`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventId(pub Uuid);

impl std::fmt::Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// `events.visibility` — whether an event bypasses the per-instance ACL on read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "events.visibility", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Private,
    Public,
}

/// `events.owner_kind` — whether an event is owned by a user or a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "events.owner_kind", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum OwnerKind {
    User,
    Group,
}
