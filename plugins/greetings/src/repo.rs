//! The greetings plugin's data layer. `GreetingRepo<P>` reads its own
//! `greetings.greeting` table **joined with hello's `hello.greeting_template`** —
//! the cross-plugin SQL the M09 grants make possible (`role_greetings` holds
//! SELECT on `hello.greeting_template`, but *not* on `hello.greeting`). Reads require
//! `Has<GreetingsRead>`; create additionally requires `Has<GreetingsWrite>`.

use chrono::{DateTime, Utc};
use junius_sdk::permissions::Has;
use junius_sdk::{RepoError, impl_repository, repository};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::permissions::{GreetingsRead, GreetingsWrite};

/// Identifier for a greetings row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GreetingId(pub Uuid);

/// A persisted greeting, resolved against its hello template (cross-plugin).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Greeting {
    pub id: GreetingId,
    pub template_id: Uuid,
    pub template_name: String,
    pub recipient: String,
    pub venue: Option<String>,
    /// The template body with `{name}` rendered for `recipient`.
    pub message: String,
    pub created: DateTime<Utc>,
}

/// Input for creating a greeting. `template_name` is resolved to a
/// `hello.greeting_template` row server-side (cross-plugin read).
#[derive(Debug, Clone)]
pub struct NewGreeting {
    pub template_name: String,
    pub recipient: String,
    pub venue: Option<String>,
}

/// Render a template body (`"Hello, {name}!"`) for a recipient.
fn render(body: &str, recipient: &str) -> String {
    body.replace("{name}", recipient)
}

#[repository]
pub struct GreetingRepo<P = ()>;

#[impl_repository(GreetingRepo)]
impl<P: Has<GreetingsRead>> GreetingRepo<P> {
    /// All greetings, oldest first, each resolved against its hello template.
    /// The `JOIN hello.greeting_template` is the cross-plugin read.
    pub async fn list(&self) -> Result<Vec<Greeting>, RepoError> {
        let rows = sqlx::query!(
            r#"
            SELECT g.id, g.template_id, g.recipient, g.venue, g.created,
                   t.name AS template_name, t.body AS template_body
            FROM greetings.greeting g
            JOIN hello.greeting_template t ON t.id = g.template_id
            ORDER BY g.created
            "#,
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| Greeting {
                id: GreetingId(r.id),
                template_id: r.template_id,
                template_name: r.template_name,
                message: render(&r.template_body, &r.recipient),
                recipient: r.recipient,
                venue: r.venue,
                created: r.created,
            })
            .collect())
    }
}

#[impl_repository(GreetingRepo)]
impl<P: Has<GreetingsRead> + Has<GreetingsWrite>> GreetingRepo<P> {
    /// Resolve `template_name` against hello's table (cross-plugin read), insert a
    /// greeting referencing it via the NOT NULL FK, and return it resolved against
    /// the template. `NotFound` if the template name is unknown. Requires
    /// `greetings:write`.
    pub async fn create(&self, input: NewGreeting) -> Result<Greeting, RepoError> {
        let row = sqlx::query!(
            r#"
            WITH tpl AS (
                SELECT id, name, body FROM hello.greeting_template WHERE name = $1
            ),
            inserted AS (
                INSERT INTO greetings.greeting (template_id, recipient, venue)
                SELECT id, $2, $3 FROM tpl
                RETURNING id, template_id, recipient, venue, created
            )
            SELECT i.id AS "id!", i.template_id AS "template_id!",
                   i.recipient AS "recipient!", i.venue, i.created AS "created!",
                   tpl.name AS "template_name!", tpl.body AS "template_body!"
            FROM inserted i
            CROSS JOIN tpl
            "#,
            input.template_name,
            input.recipient,
            input.venue,
        )
        .fetch_optional(self.pool())
        .await?
        .ok_or(RepoError::NotFound)?;
        let greeting = Greeting {
            id: GreetingId(row.id),
            template_id: row.template_id,
            template_name: row.template_name,
            message: render(&row.template_body, &row.recipient),
            recipient: row.recipient,
            venue: row.venue,
            created: row.created,
        };
        self.audit()
            .emit(
                "greetings:greeting.create",
                self.user().map(|u| u.id),
                "greetings:greeting",
                Some(greeting.id.0),
                serde_json::json!({ "template_id": greeting.template_id }),
            )
            .await?;
        Ok(greeting)
    }
}
