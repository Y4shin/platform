//! Shared domain types for the events plugin: the row id newtype and the
//! Postgres-enum mappings. The enum columns are real Postgres enums (declared in
//! the migrations), mapped to Rust enums via `#[derive(sqlx::Type)]` with the
//! schema-qualified `type_name` so `sqlx::query!` can decode them.

use base64::Engine;
use rand::RngCore;
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

/// `events.signup_kind` — a logged-in user's sign-up vs. an anonymous guest's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "events.signup_kind", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum SignupKind {
    User,
    Guest,
}

/// `events.signup_status` — whether a sign-up still counts toward the slot limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "events.signup_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum SignupStatus {
    Going,
    OptedOut,
}

impl SignupStatus {
    /// The wire string (`"going"` / `"opted_out"`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Going => "going",
            Self::OptedOut => "opted_out",
        }
    }
}

impl SignupKind {
    /// The wire string (`"user"` / `"guest"`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Guest => "guest",
        }
    }
}

/// A URL-safe, unguessable invite slug — 128 bits of entropy, base64url (no pad).
#[must_use]
pub fn generate_slug() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}
