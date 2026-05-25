//! The hello plugin's data layer. `HelloRepo<P>` is the only path to the
//! `hello.greeting` table: read methods require `Has<HelloRead>`, the write
//! method additionally requires `Has<HelloWrite>`, so a context proven to hold
//! only `hello:read` cannot even *name* `create` (compile-time gate). Writes
//! record an audit event.

use chrono::{DateTime, Utc};
use junius_sdk::permissions::Has;
use junius_sdk::{RepoError, impl_repository, repository};
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
