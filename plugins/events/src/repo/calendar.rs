//! `CalendarRepo<P>` — iCalendar feed queries + feed-token management.
//!
//! The feed *reads* (`event_for_ics`, `personal_feed`, `group_feed`,
//! `lookup_token`, `group_is_public`) are **ungated** (plain `impl<P>`, callable
//! on `CalendarRepo<()>`): they back the unauthenticated `.ics` HTTP endpoints,
//! resolving against the **subject's** live entitlements (a token's owner / a
//! group), never the anonymous caller. Token management (`mint_token`,
//! `list_tokens`, `revoke_token`, `set_group_public`) is permission-gated for the
//! `CalendarService` RPCs.

use chrono::{DateTime, Utc};
use junius_sdk::permissions::Has;
use junius_sdk::{RepoError, impl_repository, repository};
use uuid::Uuid;

use crate::domain::FeedKind;
use crate::ics::IcsEvent;
use crate::permissions::{EventsRead, EventsWrite};

/// What a (valid, unrevoked) feed token resolves to.
#[derive(Debug, Clone)]
pub struct TokenSubject {
    pub kind: FeedKind,
    pub subject_user_id: Option<Uuid>,
    pub subject_group_id: Option<Uuid>,
}

/// A feed token as listed for its creator (the secret key is never recoverable).
#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub id: Uuid,
    pub kind: FeedKind,
    pub subject_group_id: Option<Uuid>,
    pub label: Option<String>,
    pub created_at: DateTime<Utc>,
    pub revoked: bool,
}

#[repository]
pub struct CalendarRepo<P = ()>;

impl<P> CalendarRepo<P> {
    /// A single event for `.ics` export, honouring the event ACL (public open;
    /// private needs `viewer` with read access). `None` if absent/forbidden.
    pub async fn event_for_ics(
        &self,
        event_id: Uuid,
        viewer: Option<Uuid>,
    ) -> Result<Option<IcsEvent>, RepoError> {
        let row = sqlx::query!(
            r#"
            SELECT e.id, e.title, e.description, e.location, e.starts_at, e.ends_at,
                   e.all_day, e.updated_at
            FROM events.event e
            WHERE e.id = $1
              AND (e.visibility = 'public'
                   OR platform.user_can_access('events:event', e.id, $2, 'events:read'))
            "#,
            event_id,
            viewer
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|r| IcsEvent {
            id: r.id,
            title: r.title,
            description: r.description,
            location: r.location,
            starts_at: r.starts_at,
            ends_at: r.ends_at,
            all_day: r.all_day,
            updated_at: r.updated_at,
        }))
    }

    /// The subject's personal feed: every event they can access (owned / via a
    /// group role) **or** are signed up for (still going) — resolved live, so
    /// leaving a group or opting out drops events on the next poll.
    pub async fn personal_feed(&self, subject: Uuid) -> Result<Vec<IcsEvent>, RepoError> {
        let rows = sqlx::query!(
            r#"
            SELECT e.id, e.title, e.description, e.location, e.starts_at, e.ends_at,
                   e.all_day, e.updated_at
            FROM events.event e
            WHERE platform.user_can_access('events:event', e.id, $1, 'events:read')
               OR EXISTS (
                 SELECT 1 FROM events.invite i
                 JOIN events.signup s ON s.invite_id = i.id
                 WHERE i.event_id = e.id AND s.user_id = $1 AND s.status = 'going')
            ORDER BY e.starts_at, e.id
            "#,
            subject
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| IcsEvent {
                id: r.id,
                title: r.title,
                description: r.description,
                location: r.location,
                starts_at: r.starts_at,
                ends_at: r.ends_at,
                all_day: r.all_day,
                updated_at: r.updated_at,
            })
            .collect())
    }

    /// All events owned by a group (including private group events) — the group
    /// feed. The caller authorisation is the feed credential (public toggle or
    /// key), resolved by the handler before calling this.
    pub async fn group_feed(&self, group_id: Uuid) -> Result<Vec<IcsEvent>, RepoError> {
        let rows = sqlx::query!(
            r#"
            SELECT e.id, e.title, e.description, e.location, e.starts_at, e.ends_at,
                   e.all_day, e.updated_at
            FROM events.event e
            WHERE e.owner_kind = 'group' AND e.owning_group_id = $1
            ORDER BY e.starts_at, e.id
            "#,
            group_id
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| IcsEvent {
                id: r.id,
                title: r.title,
                description: r.description,
                location: r.location,
                starts_at: r.starts_at,
                ends_at: r.ends_at,
                all_day: r.all_day,
                updated_at: r.updated_at,
            })
            .collect())
    }

    /// Resolve an active (unrevoked) feed token by its hash.
    pub async fn lookup_token(&self, token_hash: &str) -> Result<Option<TokenSubject>, RepoError> {
        let row = sqlx::query!(
            r#"
            SELECT kind AS "kind: FeedKind", subject_user_id, subject_group_id
            FROM events.calendar_token
            WHERE token_hash = $1 AND revoked_at IS NULL
            "#,
            token_hash
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|r| TokenSubject {
            kind: r.kind,
            subject_user_id: r.subject_user_id,
            subject_group_id: r.subject_group_id,
        }))
    }

    /// Whether a group's calendar is published publicly (keyless feed).
    pub async fn group_is_public(&self, group_id: Uuid) -> Result<bool, RepoError> {
        let public: bool = sqlx::query_scalar!(
            r#"SELECT COALESCE(
                 (SELECT public FROM events.group_calendar WHERE group_id = $1), false
               ) AS "public!""#,
            group_id
        )
        .fetch_one(self.pool())
        .await?;
        Ok(public)
    }
}

#[impl_repository(CalendarRepo)]
impl<P: Has<EventsRead>> CalendarRepo<P> {
    /// Mint a feed token, storing only its hash. Returns the new token id.
    pub async fn mint_token(
        &self,
        kind: FeedKind,
        subject_user_id: Option<Uuid>,
        subject_group_id: Option<Uuid>,
        created_by: Uuid,
        label: Option<&str>,
        token_hash: &str,
    ) -> Result<Uuid, RepoError> {
        let id = sqlx::query_scalar!(
            r#"
            INSERT INTO events.calendar_token
              (token_hash, kind, subject_user_id, subject_group_id, created_by, label)
            VALUES ($1, $2::events.feed_kind, $3, $4, $5, $6)
            RETURNING id
            "#,
            token_hash,
            kind as FeedKind,
            subject_user_id,
            subject_group_id,
            created_by,
            label,
        )
        .fetch_one(self.pool())
        .await?;
        Ok(id)
    }

    /// The caller's own feed tokens (most recent first).
    pub async fn list_tokens(&self, created_by: Uuid) -> Result<Vec<TokenInfo>, RepoError> {
        let rows = sqlx::query!(
            r#"
            SELECT id, kind AS "kind: FeedKind", subject_group_id, label, created_at,
                   (revoked_at IS NOT NULL) AS "revoked!"
            FROM events.calendar_token
            WHERE created_by = $1
            ORDER BY created_at DESC, id
            "#,
            created_by
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| TokenInfo {
                id: r.id,
                kind: r.kind,
                subject_group_id: r.subject_group_id,
                label: r.label,
                created_at: r.created_at,
                revoked: r.revoked,
            })
            .collect())
    }

    /// Revoke a token the caller created. `NotFound` if it isn't theirs or is
    /// already revoked.
    pub async fn revoke_token(&self, token_id: Uuid, created_by: Uuid) -> Result<(), RepoError> {
        let revoked = sqlx::query_scalar!(
            r#"
            UPDATE events.calendar_token SET revoked_at = now()
            WHERE id = $1 AND created_by = $2 AND revoked_at IS NULL
            RETURNING id
            "#,
            token_id,
            created_by
        )
        .fetch_optional(self.pool())
        .await?;
        if revoked.is_none() {
            return Err(RepoError::NotFound);
        }
        Ok(())
    }
}

#[impl_repository(CalendarRepo)]
impl<P: Has<EventsRead> + Has<EventsWrite>> CalendarRepo<P> {
    /// Set a group's calendar public flag (upsert). The handler verifies the
    /// caller holds `events:write` within the group first.
    pub async fn set_group_public(
        &self,
        group_id: Uuid,
        public: bool,
        updated_by: Uuid,
    ) -> Result<(), RepoError> {
        sqlx::query!(
            r#"
            INSERT INTO events.group_calendar (group_id, public, updated_by)
            VALUES ($1, $2, $3)
            ON CONFLICT (group_id)
            DO UPDATE SET public = $2, updated_by = $3, updated_at = now()
            "#,
            group_id,
            public,
            updated_by
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }
}
