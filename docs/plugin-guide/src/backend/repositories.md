# Repositories

A repository is the **only** sanctioned path to your plugin's tables. Your
handlers never call `sqlx` directly; they call repository methods. The
repository macros inject the database handle, the caller and the audit emitter,
and — crucially — attach the `Has<P>` permission bounds that make the
compile-time gate work.

The worked example is `plugins/events/src/repo/event.rs`.

## Declaring a repository

```rust
use junius_sdk::{impl_repository, repository, Authz, Has, Principal, RepoError};
use crate::permissions::{EventsRead, EventsWrite};

#[repository]
pub struct EventRepo<P = ()>;
```

`#[repository]` generates the struct's hidden fields (a sealed, plugin-scoped DB
handle, the optional caller, the audit emitter) and a `new(db: &PluginDb) -> Self`
constructor. The `P` type parameter is the permission witness — defaulting to
`()`, the no-permission witness, for caller-less contexts.

## Implementing methods, gated by permission

You write methods in `#[impl_repository]` blocks. The bound on the `impl`
decides which witnesses can call the methods inside it:

```rust
#[impl_repository(EventRepo)]
impl<P: Has<EventsRead>> EventRepo<P> {
    pub async fn list(&self) -> Result<Vec<EventView>, RepoError> {
        let caller = self.user().map(|u| u.id.0);
        let rows = sqlx::query_as!(
            EventView,
            r#"
            SELECT
              e.id,
              e.title,
              e.visibility AS "visibility: Visibility",
              platform.user_can_access('events:event', e.id, $1, 'events:write')
                AS "viewer_can_edit!",
              platform.user_can_access('events:event', e.id, $1, 'events:share')
                AS "viewer_can_share!"
            FROM events.event e
            WHERE e.visibility = 'public'
               OR platform.user_can_access('events:event', e.id, $1, 'events:read')
            ORDER BY e.created_at DESC
            "#,
            caller,
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }
}

#[impl_repository(EventRepo)]
impl<P: Has<EventsRead> + Has<EventsWrite>> EventRepo<P> {
    pub async fn create(
        &self,
        input: NewEvent,
        owner: Principal,
        authz: &Authz,
    ) -> Result<EventView, RepoError> {
        let mut tx = self.pool().begin().await?;
        let row = sqlx::query!(
            r#"INSERT INTO events.event (title, visibility, owner_kind, owning_user_id, owning_group_id)
               VALUES ($1, $2::events.visibility, $3::events.owner_kind, $4, $5)
               RETURNING id"#,
            input.title,
            input.visibility as Visibility,
            /* owner_kind, owning_user_id, owning_group_id from `owner` */
        )
        .fetch_one(&mut *tx)
        .await?;

        authz.record_owner(&mut tx, "events:event", row.id, owner).await?;
        tx.commit().await?;
        // re-read through the access-filtered query so the returned view carries
        // the viewer_can_* flags:
        self.get(row.id).await
    }
}
```

The two blocks are the whole idea:

- `list` lives in an `impl<P: Has<EventsRead>>` block — callable from any context
  that proved read access.
- `create` lives in an `impl<P: Has<EventsRead> + Has<EventsWrite>>` block — a
  read-only context can't name it. The compiler is your access reviewer.

## What the macros give you

Inside an `#[impl_repository]` method you have three accessors, all injected by
`#[repository]`:

| Accessor | Returns | Use for |
| --- | --- | --- |
| `self.pool()` | the sealed `&PgPool` for your plugin's role | running queries |
| `self.user()` | `Option<&User>` — the caller | the `$1` caller id in ACL predicates |
| `self.audit()` | the `AuditEmitter` | writing audit events for mutations |

`self.pool()` is **sealed**: only generated repository code can reach the
executor. There is no way to run a query against your tables that bypasses a
repository — which is what makes the `Has<P>` discipline airtight.

## Compile-time vs. runtime SQL

You'll use both `sqlx` flavours, deliberately:

- **Compile-time checked** (`sqlx::query!` / `sqlx::query_as!`) for your **own**
  tables. These are verified at build time against the committed `.sqlx/`
  offline cache. This is the default — prefer it.
- **Runtime** (`sqlx::query(...)`) for host-schema reads that skip the cache.
  This is what the SDK's own `Groups`/`Users` directory accessors use internally;
  you'll rarely need it directly.

### Querying enum columns

With `sqlx::query!`/`query_as!` against an enum column, remember the three rules
from [Migrations and the database](./migrations.md):

```sql
SELECT e.visibility AS "visibility: Visibility"   -- annotate the column
FROM events.event e
WHERE e.visibility = $1::events.visibility         -- cast the bind
```

`SELECT e.*` will **not** carry the `: Visibility` override — name every column
explicitly when any of them is an enum.

### The `!` in `"viewer_can_edit!"`

`platform.user_can_access(...)` is a function call, so sqlx infers its result as
*nullable*. The trailing `!` (`AS "viewer_can_edit!"`) tells sqlx "this is
non-null" and gives you a `bool` instead of `Option<bool>`.

## Regenerating the `.sqlx` cache

Whenever you **add or change** a compile-time query, regenerate the offline
cache and commit it:

```bash
# with a Postgres up and host + plugin migrations applied:
DATABASE_URL=… SQLX_OFFLINE=false cargo sqlx prepare --workspace
git add .sqlx
```

CI verifies the cache is fresh (`task ci:integration`). A stale `.sqlx` is one of
the most common red-CI surprises — see
[Validation and the CI gate](../quality/ci-gate.md).

## Repositories without a permission witness

Public, unauthenticated handlers build a `Repo<()>` — a repository whose witness
proves nothing — and put their methods in a **plain `impl<P>` block** (no
`Has<>` bound) so they're callable on `Repo<()>`. The ACL in the SQL still does
the access control. This is the pattern behind invite/sign-up pages; see
[Public and unauthenticated surfaces](./public-surfaces.md).

Next: exposing these repository methods as an RPC service.
