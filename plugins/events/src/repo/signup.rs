//! `SignupRepo<P>` — sign-up / opt-out (ungated, public) + group pre-sign-up
//! (gated, owner-side).
//!
//! [`signup`](SignupRepo::signup) and [`opt_out`](SignupRepo::opt_out) are
//! ungated (plain `impl<P>`, callable on `SignupRepo<()>`): they are the public
//! attendee surface, with the event ACL enforced at runtime, not a compile-time
//! witness. Slot enforcement runs inside a transaction with `FOR UPDATE` on the
//! invite row so concurrent sign-ups serialise on the count.
//! [`presign_users`](SignupRepo::presign_users) is the owner-side group
//! pre-sign-up and *is* permission-gated.

use junius_sdk::permissions::Has;
use junius_sdk::{RepoError, UserId, impl_repository, repository};
use uuid::Uuid;

use crate::domain::SignupStatus;
use crate::permissions::{EventsRead, EventsWrite};

/// Why a public sign-up / opt-out was refused. Distinct from [`RepoError`] so the
/// RPC handler can map each outcome to a precise Connect code instead of a flat
/// 500/404.
#[derive(Debug, thiserror::Error)]
pub enum SignupError {
    /// No such invite, or the caller can't read the (private) event.
    #[error("not found")]
    NotFound,
    /// Sign-up is disabled or manually closed.
    #[error("sign-up is not open")]
    Closed,
    /// The slot limit is reached.
    #[error("no slots remaining")]
    Full,
    /// The logged-in caller already has an active sign-up.
    #[error("already signed up")]
    AlreadySignedUp,
    /// A user-scoped action (opt-out) with no authenticated caller.
    #[error("authentication required")]
    Unauthenticated,
    /// A guest sign-up with a missing/invalid name or email.
    #[error("invalid guest details: {0}")]
    InvalidGuest(String),
    /// Underlying SQL failure.
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

impl From<SignupError> for connectrpc::ConnectError {
    fn from(e: SignupError) -> Self {
        match e {
            SignupError::NotFound => Self::not_found("not found"),
            SignupError::Closed => Self::failed_precondition("sign-up is not open"),
            SignupError::Full => Self::resource_exhausted("no slots remaining"),
            SignupError::AlreadySignedUp => Self::already_exists("already signed up"),
            SignupError::Unauthenticated => Self::unauthenticated("authentication required"),
            SignupError::InvalidGuest(m) => Self::invalid_argument(m),
            SignupError::Db(err) => Self::internal(err.to_string()),
        }
    }
}

/// The result of a successful sign-up: the new (or reactivated) row plus the
/// event id + title, so the caller can enqueue a confirmation email without a
/// second query.
#[derive(Debug, Clone)]
pub struct SignupOutcome {
    pub id: Uuid,
    pub status: SignupStatus,
    pub event_id: Uuid,
    pub event_title: String,
}

/// Fail with [`SignupError::Full`] when the invite's slot limit is reached.
/// Called inside the sign-up transaction (under the invite row lock), only on the
/// paths that add a going row.
async fn ensure_slot(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    invite_id: Uuid,
    slot_limit: Option<i32>,
) -> Result<(), SignupError> {
    if let Some(limit) = slot_limit {
        let going: i64 = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM events.signup
               WHERE invite_id = $1 AND status = 'going'"#,
            invite_id
        )
        .fetch_one(&mut **tx)
        .await?;
        if going >= i64::from(limit) {
            return Err(SignupError::Full);
        }
    }
    Ok(())
}

#[repository]
pub struct SignupRepo<P = ()>;

impl<P> SignupRepo<P> {
    /// **Ungated public write**: sign up for an invite by slug. A logged-in caller
    /// signs up as themselves (deduped; a prior opt-out is reactivated); an
    /// anonymous caller signs up as a guest (name + email required). Enforces the
    /// event ACL, `signup_enabled`/`signup_open`, and the slot limit atomically
    /// under a row lock on the invite.
    pub async fn signup(
        &self,
        slug: &str,
        guest_name: Option<&str>,
        guest_email: Option<&str>,
    ) -> Result<SignupOutcome, SignupError> {
        let viewer = self.user().map(|u| u.id.0);
        let mut tx = self.pool().begin().await?;
        // Resolve + lock the invite, enforcing event read-access. The event id +
        // title come along for the confirmation email the caller enqueues.
        let inv = sqlx::query!(
            r#"
            SELECT i.id, i.signup_enabled, i.signup_open, i.slot_limit,
                   e.id AS event_id, e.title AS event_title
            FROM events.invite i
            JOIN events.event e ON e.id = i.event_id
            WHERE i.slug = $1
              AND (e.visibility = 'public'
                   OR platform.user_can_access('events:event', e.id, $2, 'events:read'))
            FOR UPDATE OF i
            "#,
            slug,
            viewer
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(SignupError::NotFound)?;

        if !inv.signup_enabled || !inv.signup_open {
            return Err(SignupError::Closed);
        }

        // The slot check runs only on paths that actually *add* a going row — a
        // duplicate active sign-up (already counted) must report AlreadySignedUp,
        // not Full.
        let id = if let Some(uid) = viewer {
            // Logged-in: one sign-up per user. Reactivate a prior opt-out;
            // refuse a duplicate active sign-up.
            let existing = sqlx::query!(
                r#"SELECT id, status AS "status: SignupStatus" FROM events.signup
                   WHERE invite_id = $1 AND user_id = $2 AND kind = 'user'"#,
                inv.id,
                uid
            )
            .fetch_optional(&mut *tx)
            .await?;
            match existing {
                Some(row) if matches!(row.status, SignupStatus::Going) => {
                    return Err(SignupError::AlreadySignedUp);
                }
                Some(row) => {
                    ensure_slot(&mut tx, inv.id, inv.slot_limit).await?;
                    sqlx::query!(
                        "UPDATE events.signup SET status = 'going' WHERE id = $1",
                        row.id
                    )
                    .execute(&mut *tx)
                    .await?;
                    row.id
                }
                None => {
                    ensure_slot(&mut tx, inv.id, inv.slot_limit).await?;
                    sqlx::query_scalar!(
                        r#"INSERT INTO events.signup (invite_id, kind, user_id, status)
                           VALUES ($1, 'user', $2, 'going') RETURNING id"#,
                        inv.id,
                        uid
                    )
                    .fetch_one(&mut *tx)
                    .await?
                }
            }
        } else {
            // Anonymous guest: name + email required.
            let name = guest_name
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| SignupError::InvalidGuest("name is required".to_string()))?;
            let email = guest_email
                .map(str::trim)
                .filter(|s| s.contains('@'))
                .ok_or_else(|| {
                    SignupError::InvalidGuest("a valid email is required".to_string())
                })?;
            ensure_slot(&mut tx, inv.id, inv.slot_limit).await?;
            sqlx::query_scalar!(
                r#"INSERT INTO events.signup (invite_id, kind, guest_name, guest_email)
                   VALUES ($1, 'guest', $2, $3) RETURNING id"#,
                inv.id,
                name,
                email
            )
            .fetch_one(&mut *tx)
            .await?
        };
        tx.commit().await?;
        Ok(SignupOutcome {
            id,
            status: SignupStatus::Going,
            event_id: inv.event_id,
            event_title: inv.event_title,
        })
    }

    /// **Ungated public write**: the logged-in caller opts out of (cancels) their
    /// sign-up for an invite they can read. Idempotent target aside, `NotFound`
    /// when the caller has no user sign-up there.
    pub async fn opt_out(&self, slug: &str) -> Result<(), SignupError> {
        let uid = self
            .user()
            .map(|u| u.id.0)
            .ok_or(SignupError::Unauthenticated)?;
        let updated = sqlx::query_scalar!(
            r#"
            UPDATE events.signup s SET status = 'opted_out'
            FROM events.invite i
            JOIN events.event e ON e.id = i.event_id
            WHERE s.invite_id = i.id AND i.slug = $1 AND s.user_id = $2 AND s.kind = 'user'
              AND (e.visibility = 'public'
                   OR platform.user_can_access('events:event', e.id, $2, 'events:read'))
            RETURNING s.id
            "#,
            slug,
            uid
        )
        .fetch_optional(self.pool())
        .await?;
        if updated.is_none() {
            return Err(SignupError::NotFound);
        }
        Ok(())
    }
}

#[impl_repository(SignupRepo)]
impl<P: Has<EventsRead> + Has<EventsWrite>> SignupRepo<P> {
    /// Group pre-sign-up: snapshot `user_ids` as `going` user sign-ups for an
    /// invite (idempotent — existing rows are left untouched). Returns the number
    /// of rows inserted. Caller authority is the owner-side write witness; the
    /// caller has already been verified to own/write the group event.
    pub async fn presign_users(
        &self,
        invite_id: Uuid,
        user_ids: &[UserId],
    ) -> Result<u64, RepoError> {
        if user_ids.is_empty() {
            return Ok(0);
        }
        let ids: Vec<Uuid> = user_ids.iter().map(|u| u.0).collect();
        let result = sqlx::query!(
            r#"
            INSERT INTO events.signup (invite_id, kind, user_id, status)
            SELECT $1, 'user'::events.signup_kind, uid, 'going'::events.signup_status
            FROM unnest($2::uuid[]) AS t(uid)
            ON CONFLICT (invite_id, user_id) WHERE kind = 'user' DO NOTHING
            "#,
            invite_id,
            &ids
        )
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected())
    }
}
