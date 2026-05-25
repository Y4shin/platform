//! The hello plugin's data layer. `HelloRepo<P>` is the only path to the
//! `hello.greeting` table: read methods require `Has<HelloRead>`, the write
//! method additionally requires `Has<HelloWrite>`, so a context proven to hold
//! only `hello:read` cannot even *name* `create` (compile-time gate). Writes
//! record an audit event.

use chrono::{DateTime, Utc};
use junius_sdk::permissions::Has;
use junius_sdk::{Authz, PluginError, Principal, RepoError, impl_repository, repository};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::permissions::{HelloRead, HelloWrite};

/// Identifier for a greeting row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GreetingId(pub Uuid);

/// A persisted greeting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Greeting {
    pub id: GreetingId,
    pub name: String,
    pub body: String,
    pub created: DateTime<Utc>,
}

/// Input for creating a greeting.
#[derive(Debug, Clone)]
pub struct NewGreeting {
    pub name: String,
    pub body: String,
}

#[repository]
pub struct HelloRepo<P = ()>;

#[impl_repository(HelloRepo)]
impl<P: Has<HelloRead>> HelloRepo<P> {
    /// All greetings, oldest first.
    pub async fn list(&self) -> Result<Vec<Greeting>, RepoError> {
        let rows =
            sqlx::query!("SELECT id, name, body, created FROM hello.greeting ORDER BY created")
                .fetch_all(self.pool())
                .await?;
        Ok(rows
            .into_iter()
            .map(|r| Greeting {
                id: GreetingId(r.id),
                name: r.name,
                body: r.body,
                created: r.created,
            })
            .collect())
    }

    /// A single greeting by id, or [`RepoError::NotFound`].
    pub async fn get(&self, id: GreetingId) -> Result<Greeting, RepoError> {
        let row = sqlx::query!(
            "SELECT id, name, body, created FROM hello.greeting WHERE id = $1",
            id.0
        )
        .fetch_optional(self.pool())
        .await?
        .ok_or(RepoError::NotFound)?;
        Ok(Greeting {
            id: GreetingId(row.id),
            name: row.name,
            body: row.body,
            created: row.created,
        })
    }
}

#[impl_repository(HelloRepo)]
impl<P: Has<HelloRead> + Has<HelloWrite>> HelloRepo<P> {
    /// Insert a greeting and record an audit event. Requires `hello:write`.
    pub async fn create(&self, input: NewGreeting) -> Result<Greeting, RepoError> {
        let row = sqlx::query!(
            "INSERT INTO hello.greeting (name, body) VALUES ($1, $2) \
             RETURNING id, name, body, created",
            input.name,
            input.body
        )
        .fetch_one(self.pool())
        .await?;
        let greeting = Greeting {
            id: GreetingId(row.id),
            name: row.name,
            body: row.body,
            created: row.created,
        };
        self.audit()
            .emit(
                "hello:greeting.create",
                self.user().map(|u| u.id),
                "hello:greeting",
                Some(greeting.id.0),
                serde_json::json!({}),
            )
            .await?;
        Ok(greeting)
    }
}

// --- notes: per-instance access-controlled resource (M08) ---------------------

/// Identifier for a note row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteId(pub Uuid);

/// A note plus the viewer's server-computed capabilities. The `viewer_can_*`
/// flags come from `platform.user_can_access`; the FE must never recompute them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteView {
    pub id: NoteId,
    pub title: String,
    pub body: String,
    pub created: DateTime<Utc>,
    pub viewer_can_edit: bool,
    pub viewer_can_share: bool,
}

/// Input for creating a note.
#[derive(Debug, Clone)]
pub struct NewNote {
    pub title: String,
    pub body: String,
}

#[repository]
pub struct NoteRepo<P = ()>;

#[impl_repository(NoteRepo)]
impl<P: Has<HelloRead>> NoteRepo<P> {
    /// Notes the caller can read, each tagged with their edit/share capability.
    pub async fn list(&self) -> Result<Vec<NoteView>, RepoError> {
        let viewer = self.user().map(|u| u.id.0);
        let rows = sqlx::query!(
            r#"
            SELECT n.id, n.title, n.body, n.created,
              platform.user_can_access('hello:note', n.id, $1, 'hello:write') AS "viewer_can_edit!",
              platform.user_can_access('hello:note', n.id, $1, 'hello:share') AS "viewer_can_share!"
            FROM hello.note n
            WHERE platform.user_can_access('hello:note', n.id, $1, 'hello:read')
            ORDER BY n.created
            "#,
            viewer
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| NoteView {
                id: NoteId(r.id),
                title: r.title,
                body: r.body,
                created: r.created,
                viewer_can_edit: r.viewer_can_edit,
                viewer_can_share: r.viewer_can_share,
            })
            .collect())
    }

    /// A note by id, if the caller can read it — `NotFound` otherwise (a note the
    /// caller can't access is indistinguishable from a missing one).
    pub async fn get(&self, id: NoteId) -> Result<NoteView, RepoError> {
        let viewer = self.user().map(|u| u.id.0);
        let row = sqlx::query!(
            r#"
            SELECT n.id, n.title, n.body, n.created,
              platform.user_can_access('hello:note', n.id, $2, 'hello:write') AS "viewer_can_edit!",
              platform.user_can_access('hello:note', n.id, $2, 'hello:share') AS "viewer_can_share!"
            FROM hello.note n
            WHERE n.id = $1
              AND platform.user_can_access('hello:note', n.id, $2, 'hello:read')
            "#,
            id.0,
            viewer
        )
        .fetch_optional(self.pool())
        .await?
        .ok_or(RepoError::NotFound)?;
        Ok(NoteView {
            id: NoteId(row.id),
            title: row.title,
            body: row.body,
            created: row.created,
            viewer_can_edit: row.viewer_can_edit,
            viewer_can_share: row.viewer_can_share,
        })
    }
}

#[impl_repository(NoteRepo)]
impl<P: Has<HelloRead> + Has<HelloWrite>> NoteRepo<P> {
    /// Create a note owned by the caller, recording ownership atomically.
    pub async fn create(&self, input: NewNote, authz: &Authz) -> Result<NoteView, RepoError> {
        let Some(owner) = self.user().map(|u| u.id) else {
            return Err(RepoError::Plugin(PluginError::PermissionDenied(
                "authentication required".to_string(),
            )));
        };
        let mut tx = self.pool().begin().await?;
        let row = sqlx::query!(
            "INSERT INTO hello.note (title, body) VALUES ($1, $2) \
             RETURNING id, title, body, created",
            input.title,
            input.body
        )
        .fetch_one(&mut *tx)
        .await?;
        authz
            .record_owner(&mut tx, "hello:note", row.id, Principal::User(owner))
            .await?;
        tx.commit().await?;
        Ok(NoteView {
            id: NoteId(row.id),
            title: row.title,
            body: row.body,
            created: row.created,
            viewer_can_edit: true,
            viewer_can_share: true,
        })
    }
}
