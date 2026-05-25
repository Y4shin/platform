//! `EventRepo<P>` — CRUD over `events.event`.
//!
//! Reads return public events (world-readable) ∪ private events the caller can
//! access, each tagged with the viewer's server-computed `viewer_can_*` flags
//! (from `platform.user_can_access`). `create` records ownership for a **user or
//! group** principal atomically with the INSERT; `update`/`delete` are gated in
//! SQL on the caller's `events:write` access to the row (a row the caller can't
//! edit is indistinguishable from a missing one — `NotFound`, no existence leak).

use chrono::{DateTime, Utc};
use junius_sdk::permissions::Has;
use junius_sdk::{Authz, PluginError, Principal, RepoError, impl_repository, repository};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{EventId, OwnerKind, Visibility};
use crate::permissions::{EventsRead, EventsWrite};

/// A persisted event plus the viewer's server-computed capabilities. The
/// `viewer_can_*` flags come from `platform.user_can_access`; the FE must never
/// recompute them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventView {
    pub id: EventId,
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: Option<DateTime<Utc>>,
    pub all_day: bool,
    pub visibility: Visibility,
    pub owner_kind: OwnerKind,
    pub owning_user_id: Option<Uuid>,
    pub owning_group_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub viewer_can_edit: bool,
    pub viewer_can_share: bool,
}

/// Input for creating an event. The owner is supplied separately as a
/// [`Principal`] (verified by the caller — see `EventService::create_event`).
#[derive(Debug, Clone)]
pub struct NewEvent {
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: Option<DateTime<Utc>>,
    pub all_day: bool,
    pub visibility: Visibility,
}

/// Input for editing an event. Ownership is immutable; visibility *is* editable
/// (the publish private→public transition).
#[derive(Debug, Clone)]
pub struct EventUpdate {
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: Option<DateTime<Utc>>,
    pub all_day: bool,
    pub visibility: Visibility,
}

#[repository]
pub struct EventRepo<P = ()>;

#[impl_repository(EventRepo)]
impl<P: Has<EventsRead>> EventRepo<P> {
    /// Public events (world-readable) ∪ private events the caller can access,
    /// oldest start first, each tagged with the viewer's edit/share capability.
    pub async fn list(&self) -> Result<Vec<EventView>, RepoError> {
        let viewer = self.user().map(|u| u.id.0);
        let rows = sqlx::query!(
            r#"
            SELECT
              e.id, e.title, e.description, e.location, e.starts_at, e.ends_at, e.all_day,
              e.visibility      AS "visibility: Visibility",
              e.owner_kind      AS "owner_kind: OwnerKind",
              e.owning_user_id, e.owning_group_id, e.created_at, e.updated_at,
              platform.user_can_access('events:event', e.id, $1, 'events:write') AS "viewer_can_edit!",
              platform.user_can_access('events:event', e.id, $1, 'events:share') AS "viewer_can_share!"
            FROM events.event e
            WHERE e.visibility = 'public'
               OR platform.user_can_access('events:event', e.id, $1, 'events:read')
            ORDER BY e.starts_at, e.id
            "#,
            viewer
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| EventView {
                id: EventId(r.id),
                title: r.title,
                description: r.description,
                location: r.location,
                starts_at: r.starts_at,
                ends_at: r.ends_at,
                all_day: r.all_day,
                visibility: r.visibility,
                owner_kind: r.owner_kind,
                owning_user_id: r.owning_user_id,
                owning_group_id: r.owning_group_id,
                created_at: r.created_at,
                updated_at: r.updated_at,
                viewer_can_edit: r.viewer_can_edit,
                viewer_can_share: r.viewer_can_share,
            })
            .collect())
    }

    /// A single event by id, if it is public or the caller can read it —
    /// `NotFound` otherwise (a hidden event is indistinguishable from a missing
    /// one).
    pub async fn get(&self, id: EventId) -> Result<EventView, RepoError> {
        let viewer = self.user().map(|u| u.id.0);
        let r = sqlx::query!(
            r#"
            SELECT
              e.id, e.title, e.description, e.location, e.starts_at, e.ends_at, e.all_day,
              e.visibility      AS "visibility: Visibility",
              e.owner_kind      AS "owner_kind: OwnerKind",
              e.owning_user_id, e.owning_group_id, e.created_at, e.updated_at,
              platform.user_can_access('events:event', e.id, $2, 'events:write') AS "viewer_can_edit!",
              platform.user_can_access('events:event', e.id, $2, 'events:share') AS "viewer_can_share!"
            FROM events.event e
            WHERE e.id = $1
              AND (e.visibility = 'public'
                   OR platform.user_can_access('events:event', e.id, $2, 'events:read'))
            "#,
            id.0,
            viewer
        )
        .fetch_optional(self.pool())
        .await?
        .ok_or(RepoError::NotFound)?;
        Ok(EventView {
            id: EventId(r.id),
            title: r.title,
            description: r.description,
            location: r.location,
            starts_at: r.starts_at,
            ends_at: r.ends_at,
            all_day: r.all_day,
            visibility: r.visibility,
            owner_kind: r.owner_kind,
            owning_user_id: r.owning_user_id,
            owning_group_id: r.owning_group_id,
            created_at: r.created_at,
            updated_at: r.updated_at,
            viewer_can_edit: r.viewer_can_edit,
            viewer_can_share: r.viewer_can_share,
        })
    }
}

#[impl_repository(EventRepo)]
impl<P: Has<EventsRead> + Has<EventsWrite>> EventRepo<P> {
    /// Insert an event owned by `owner` (a user or a group), recording ownership
    /// atomically via the host's `record_owner` definer function. The caller is
    /// responsible for verifying it may own on `owner`'s behalf (the service
    /// checks group membership). The creator is the owner, so the returned view's
    /// `viewer_can_*` flags are `true`.
    pub async fn create(
        &self,
        input: NewEvent,
        owner: Principal,
        authz: &Authz,
    ) -> Result<EventView, RepoError> {
        let (owner_kind, owning_user_id, owning_group_id) = match owner {
            Principal::User(u) => (OwnerKind::User, Some(u.0), None),
            Principal::Group(g) => (OwnerKind::Group, None, Some(g.0)),
            Principal::Public => {
                return Err(RepoError::Plugin(PluginError::PermissionDenied(
                    "an event cannot be owned by the public".to_string(),
                )));
            }
        };
        let mut tx = self.pool().begin().await?;
        let r = sqlx::query!(
            r#"
            INSERT INTO events.event
              (title, description, location, starts_at, ends_at, all_day, visibility,
               owner_kind, owning_user_id, owning_group_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7::events.visibility,
                    $8::events.owner_kind, $9, $10)
            RETURNING
              id, title, description, location, starts_at, ends_at, all_day,
              visibility   AS "visibility: Visibility",
              owner_kind   AS "owner_kind: OwnerKind",
              owning_user_id, owning_group_id, created_at, updated_at
            "#,
            input.title,
            input.description,
            input.location,
            input.starts_at,
            input.ends_at,
            input.all_day,
            input.visibility as Visibility,
            owner_kind as OwnerKind,
            owning_user_id,
            owning_group_id,
        )
        .fetch_one(&mut *tx)
        .await?;
        authz
            .record_owner(&mut tx, "events:event", r.id, owner)
            .await?;
        tx.commit().await?;
        self.audit()
            .emit(
                "events:event.create",
                self.user().map(|u| u.id),
                "events:event",
                Some(r.id),
                serde_json::json!({ "owner_kind": owner_kind }),
            )
            .await?;
        Ok(EventView {
            id: EventId(r.id),
            title: r.title,
            description: r.description,
            location: r.location,
            starts_at: r.starts_at,
            ends_at: r.ends_at,
            all_day: r.all_day,
            visibility: r.visibility,
            owner_kind: r.owner_kind,
            owning_user_id: r.owning_user_id,
            owning_group_id: r.owning_group_id,
            created_at: r.created_at,
            updated_at: r.updated_at,
            viewer_can_edit: true,
            viewer_can_share: true,
        })
    }

    /// Edit an event the caller can write (`events:write` access to the row),
    /// including the publish (private→public) transition. `NotFound` if the row
    /// is absent or the caller can't edit it.
    pub async fn update(&self, id: EventId, input: EventUpdate) -> Result<EventView, RepoError> {
        let viewer = self.user().map(|u| u.id.0);
        let r = sqlx::query!(
            r#"
            UPDATE events.event e SET
              title = $2, description = $3, location = $4, starts_at = $5,
              ends_at = $6, all_day = $7, visibility = $8::events.visibility,
              updated_at = now()
            WHERE e.id = $1
              AND platform.user_can_access('events:event', e.id, $9, 'events:write')
            RETURNING
              e.id, e.title, e.description, e.location, e.starts_at, e.ends_at, e.all_day,
              e.visibility   AS "visibility: Visibility",
              e.owner_kind   AS "owner_kind: OwnerKind",
              e.owning_user_id, e.owning_group_id, e.created_at, e.updated_at,
              platform.user_can_access('events:event', e.id, $9, 'events:write') AS "viewer_can_edit!",
              platform.user_can_access('events:event', e.id, $9, 'events:share') AS "viewer_can_share!"
            "#,
            id.0,
            input.title,
            input.description,
            input.location,
            input.starts_at,
            input.ends_at,
            input.all_day,
            input.visibility as Visibility,
            viewer,
        )
        .fetch_optional(self.pool())
        .await?
        .ok_or(RepoError::NotFound)?;
        self.audit()
            .emit(
                "events:event.update",
                self.user().map(|u| u.id),
                "events:event",
                Some(r.id),
                serde_json::json!({}),
            )
            .await?;
        Ok(EventView {
            id: EventId(r.id),
            title: r.title,
            description: r.description,
            location: r.location,
            starts_at: r.starts_at,
            ends_at: r.ends_at,
            all_day: r.all_day,
            visibility: r.visibility,
            owner_kind: r.owner_kind,
            owning_user_id: r.owning_user_id,
            owning_group_id: r.owning_group_id,
            created_at: r.created_at,
            updated_at: r.updated_at,
            viewer_can_edit: r.viewer_can_edit,
            viewer_can_share: r.viewer_can_share,
        })
    }

    /// Delete an event the caller can write. `NotFound` if absent or not editable.
    pub async fn delete(&self, id: EventId) -> Result<(), RepoError> {
        let viewer = self.user().map(|u| u.id.0);
        let deleted = sqlx::query_scalar!(
            r#"
            DELETE FROM events.event e
            WHERE e.id = $1
              AND platform.user_can_access('events:event', e.id, $2, 'events:write')
            RETURNING e.id
            "#,
            id.0,
            viewer,
        )
        .fetch_optional(self.pool())
        .await?;
        if deleted.is_none() {
            return Err(RepoError::NotFound);
        }
        self.audit()
            .emit(
                "events:event.delete",
                self.user().map(|u| u.id),
                "events:event",
                Some(id.0),
                serde_json::json!({}),
            )
            .await?;
        Ok(())
    }
}
