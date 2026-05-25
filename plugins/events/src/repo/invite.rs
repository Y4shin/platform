//! `InviteRepo<P>` — the invite page configuration + the public invite read.
//!
//! Owner-side methods (`get_for_event`, `list_signups`, `create`, `update`) are
//! permission-gated *and* scoped in SQL to events the caller can access/write.
//! The viewer-side [`get_page_by_slug`](InviteRepo::get_page_by_slug) is an
//! **ungated** method (a plain `impl<P>` block, so it is callable on
//! `InviteRepo<()>`): the public invite surface has no compile-time permission
//! witness, only the runtime ACL — a private invite the caller can't read is
//! indistinguishable from a missing one (`NotFound`/404, no existence leak).

// The invite DTOs carry one bool per `show_*` visibility toggle (mirroring the
// table columns) — independent flags, not a state machine.
#![allow(clippy::struct_excessive_bools)]

use chrono::{DateTime, Utc};
use junius_sdk::permissions::Has;
use junius_sdk::{RepoError, impl_repository, repository};
use uuid::Uuid;

use crate::domain::{SignupKind, SignupStatus};
use crate::permissions::{EventsRead, EventsWrite};

/// The owner's full view of an invite's configuration.
#[derive(Debug, Clone)]
pub struct Invite {
    pub id: Uuid,
    pub event_id: Uuid,
    pub slug: String,
    pub signup_enabled: bool,
    pub signup_open: bool,
    pub slot_limit: Option<i32>,
    pub show_title: bool,
    pub show_datetime: bool,
    pub show_location: bool,
    pub show_description: bool,
    pub show_remaining: bool,
}

/// Invite configuration the owner supplies (create or full replace on update).
#[derive(Debug, Clone)]
pub struct InviteConfig {
    pub signup_enabled: bool,
    pub signup_open: bool,
    pub slot_limit: Option<i32>,
    pub show_title: bool,
    pub show_datetime: bool,
    pub show_location: bool,
    pub show_description: bool,
    pub show_remaining: bool,
}

/// The public invite page: the invite config + going count + the (always-fetched)
/// event fields. The handler applies the `show_*` toggles when projecting to the
/// wire so hidden fields never leave the server.
#[derive(Debug, Clone)]
pub struct InvitePage {
    pub invite_id: Uuid,
    pub event_id: Uuid,
    pub slug: String,
    pub signup_enabled: bool,
    pub signup_open: bool,
    pub slot_limit: Option<i32>,
    pub going_count: i64,
    pub show_title: bool,
    pub show_datetime: bool,
    pub show_location: bool,
    pub show_description: bool,
    pub show_remaining: bool,
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: Option<DateTime<Utc>>,
    pub all_day: bool,
}

/// A sign-up row, as the owner sees it in `ListSignups`.
#[derive(Debug, Clone)]
pub struct SignupRow {
    pub id: Uuid,
    pub kind: SignupKind,
    pub user_id: Option<Uuid>,
    pub guest_name: Option<String>,
    pub guest_email: Option<String>,
    pub status: SignupStatus,
    pub created_at: DateTime<Utc>,
}

#[repository]
pub struct InviteRepo<P = ()>;

#[impl_repository(InviteRepo)]
impl<P: Has<EventsRead>> InviteRepo<P> {
    /// The invite configured for an event the caller can read, if any.
    pub async fn get_for_event(&self, event_id: Uuid) -> Result<Option<Invite>, RepoError> {
        let viewer = self.user().map(|u| u.id.0);
        let row = sqlx::query!(
            r#"
            SELECT i.id, i.event_id, i.slug, i.signup_enabled, i.signup_open, i.slot_limit,
                   i.show_title, i.show_datetime, i.show_location, i.show_description,
                   i.show_remaining
            FROM events.invite i
            JOIN events.event e ON e.id = i.event_id
            WHERE i.event_id = $1
              AND (e.visibility = 'public'
                   OR platform.user_can_access('events:event', e.id, $2, 'events:read'))
            "#,
            event_id,
            viewer
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|r| Invite {
            id: r.id,
            event_id: r.event_id,
            slug: r.slug,
            signup_enabled: r.signup_enabled,
            signup_open: r.signup_open,
            slot_limit: r.slot_limit,
            show_title: r.show_title,
            show_datetime: r.show_datetime,
            show_location: r.show_location,
            show_description: r.show_description,
            show_remaining: r.show_remaining,
        }))
    }

    /// All sign-ups for an event the caller can read (owner-side roster).
    pub async fn list_signups(&self, event_id: Uuid) -> Result<Vec<SignupRow>, RepoError> {
        let viewer = self.user().map(|u| u.id.0);
        let rows = sqlx::query!(
            r#"
            SELECT s.id, s.kind AS "kind: SignupKind", s.user_id, s.guest_name, s.guest_email,
                   s.status AS "status: SignupStatus", s.created_at
            FROM events.signup s
            JOIN events.invite i ON i.id = s.invite_id
            JOIN events.event e ON e.id = i.event_id
            WHERE e.id = $1
              AND (e.visibility = 'public'
                   OR platform.user_can_access('events:event', e.id, $2, 'events:read'))
            ORDER BY s.created_at, s.id
            "#,
            event_id,
            viewer
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| SignupRow {
                id: r.id,
                kind: r.kind,
                user_id: r.user_id,
                guest_name: r.guest_name,
                guest_email: r.guest_email,
                status: r.status,
                created_at: r.created_at,
            })
            .collect())
    }
}

#[impl_repository(InviteRepo)]
impl<P: Has<EventsRead> + Has<EventsWrite>> InviteRepo<P> {
    /// Create the invite for an event the caller can write. `slug` is a
    /// pre-generated unguessable token. A unique violation surfaces as
    /// [`RepoError::Db`] (one invite per event); the handler maps it to
    /// `already_exists`.
    pub async fn create(
        &self,
        event_id: Uuid,
        slug: &str,
        config: &InviteConfig,
    ) -> Result<Invite, RepoError> {
        let viewer = self.user().map(|u| u.id.0);
        let can_write: bool = sqlx::query_scalar!(
            r#"SELECT platform.user_can_access('events:event', $1, $2, 'events:write') AS "ok!""#,
            event_id,
            viewer
        )
        .fetch_one(self.pool())
        .await?;
        if !can_write {
            return Err(RepoError::NotFound);
        }
        let r = sqlx::query!(
            r#"
            INSERT INTO events.invite
              (event_id, slug, signup_enabled, signup_open, slot_limit,
               show_title, show_datetime, show_location, show_description, show_remaining)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING id, event_id, slug, signup_enabled, signup_open, slot_limit,
                      show_title, show_datetime, show_location, show_description, show_remaining
            "#,
            event_id,
            slug,
            config.signup_enabled,
            config.signup_open,
            config.slot_limit,
            config.show_title,
            config.show_datetime,
            config.show_location,
            config.show_description,
            config.show_remaining,
        )
        .fetch_one(self.pool())
        .await?;
        self.audit()
            .emit(
                "events:invite.create",
                self.user().map(|u| u.id),
                "events:event",
                Some(event_id),
                serde_json::json!({ "invite_id": r.id }),
            )
            .await?;
        Ok(Invite {
            id: r.id,
            event_id: r.event_id,
            slug: r.slug,
            signup_enabled: r.signup_enabled,
            signup_open: r.signup_open,
            slot_limit: r.slot_limit,
            show_title: r.show_title,
            show_datetime: r.show_datetime,
            show_location: r.show_location,
            show_description: r.show_description,
            show_remaining: r.show_remaining,
        })
    }

    /// Replace the configuration of an event's invite (caller needs write access
    /// to the event). `NotFound` if the event has no invite or isn't writable.
    pub async fn update(&self, event_id: Uuid, config: &InviteConfig) -> Result<Invite, RepoError> {
        let viewer = self.user().map(|u| u.id.0);
        let r = sqlx::query!(
            r#"
            UPDATE events.invite i SET
              signup_enabled = $2, signup_open = $3, slot_limit = $4,
              show_title = $5, show_datetime = $6, show_location = $7,
              show_description = $8, show_remaining = $9, updated_at = now()
            FROM events.event e
            WHERE i.event_id = $1 AND e.id = i.event_id
              AND platform.user_can_access('events:event', e.id, $10, 'events:write')
            RETURNING i.id, i.event_id, i.slug, i.signup_enabled, i.signup_open, i.slot_limit,
                      i.show_title, i.show_datetime, i.show_location, i.show_description,
                      i.show_remaining
            "#,
            event_id,
            config.signup_enabled,
            config.signup_open,
            config.slot_limit,
            config.show_title,
            config.show_datetime,
            config.show_location,
            config.show_description,
            config.show_remaining,
            viewer,
        )
        .fetch_optional(self.pool())
        .await?
        .ok_or(RepoError::NotFound)?;
        self.audit()
            .emit(
                "events:invite.update",
                self.user().map(|u| u.id),
                "events:event",
                Some(event_id),
                serde_json::json!({ "invite_id": r.id }),
            )
            .await?;
        Ok(Invite {
            id: r.id,
            event_id: r.event_id,
            slug: r.slug,
            signup_enabled: r.signup_enabled,
            signup_open: r.signup_open,
            slot_limit: r.slot_limit,
            show_title: r.show_title,
            show_datetime: r.show_datetime,
            show_location: r.show_location,
            show_description: r.show_description,
            show_remaining: r.show_remaining,
        })
    }
}

impl<P> InviteRepo<P> {
    /// **Ungated public read**: resolve an invite by slug for the viewer surface.
    /// Honours the event ACL — public events are open to anyone (including the
    /// caller-less anonymous case, `self.user() == None`); a private event needs
    /// a caller with `events:read` access, else `NotFound` (no existence leak).
    /// Available on `InviteRepo<()>` precisely because the public invite page
    /// carries no permission witness.
    pub async fn get_page_by_slug(&self, slug: &str) -> Result<InvitePage, RepoError> {
        let viewer = self.user().map(|u| u.id.0);
        let r = sqlx::query!(
            r#"
            SELECT i.id AS invite_id, i.event_id, i.slug, i.signup_enabled, i.signup_open,
                   i.slot_limit, i.show_title, i.show_datetime, i.show_location,
                   i.show_description, i.show_remaining,
                   e.title, e.description, e.location, e.starts_at, e.ends_at, e.all_day,
                   (SELECT count(*) FROM events.signup s
                    WHERE s.invite_id = i.id AND s.status = 'going') AS "going_count!"
            FROM events.invite i
            JOIN events.event e ON e.id = i.event_id
            WHERE i.slug = $1
              AND (e.visibility = 'public'
                   OR platform.user_can_access('events:event', e.id, $2, 'events:read'))
            "#,
            slug,
            viewer
        )
        .fetch_optional(self.pool())
        .await?
        .ok_or(RepoError::NotFound)?;
        Ok(InvitePage {
            invite_id: r.invite_id,
            event_id: r.event_id,
            slug: r.slug,
            signup_enabled: r.signup_enabled,
            signup_open: r.signup_open,
            slot_limit: r.slot_limit,
            going_count: r.going_count,
            show_title: r.show_title,
            show_datetime: r.show_datetime,
            show_location: r.show_location,
            show_description: r.show_description,
            show_remaining: r.show_remaining,
            title: r.title,
            description: r.description,
            location: r.location,
            starts_at: r.starts_at,
            ends_at: r.ends_at,
            all_day: r.all_day,
        })
    }
}
