# Migrations and the database

Each plugin owns its **own Postgres schema**. The host runs as a privileged
migrator; your plugin's *runtime* connects as a narrow role (`role_<name>`) that
has DML only on its own tables (plus any tables other plugins explicitly
expose). This isolation is enforced at the database level — a bug in one plugin
can't corrupt another's data.

## Where migrations live

```text
plugins/events/migrations/
├── 0001_event.up.sql
├── 0001_event.down.sql
├── 0002_invite.up.sql
├── 0002_invite.down.sql
└── …
```

Each migration is a numbered pair: `NNNN_<name>.up.sql` applies it,
`NNNN_<name>.down.sql` reverses it. Numbers are zero-padded and strictly
increasing.

Scaffold a new one rather than hand-creating the files:

```bash
cargo run -p junius -- new migration events add_reminder
# → creates plugins/events/migrations/000N_add_reminder.up.sql (+ .down.sql)
#   with the next free number
```

## Anatomy of a migration

```sql
-- plugins/events/migrations/0001_event.up.sql
-- @requires platform:0001_users
-- @requires platform:0003_groups_roles_memberships

CREATE SCHEMA IF NOT EXISTS events;

CREATE TYPE events.visibility AS ENUM ('private', 'public');
CREATE TYPE events.owner_kind AS ENUM ('user', 'group');

CREATE TABLE events.event (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title           TEXT NOT NULL CHECK (length(btrim(title)) > 0),
    visibility      events.visibility NOT NULL DEFAULT 'private',
    owner_kind      events.owner_kind NOT NULL,
    owning_user_id  UUID REFERENCES platform.user(id),
    owning_group_id UUID REFERENCES platform.group(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT event_owner_matches_kind CHECK (
        (owner_kind = 'user'  AND owning_user_id  IS NOT NULL AND owning_group_id IS NULL)
     OR (owner_kind = 'group' AND owning_group_id IS NOT NULL AND owning_user_id  IS NULL)
    )
);
```

Three things to notice:

1. **`@requires` directives.** The comment header declares ordering
   dependencies on *host* migrations. The migration runner uses them to
   sequence everything correctly — your `event` table can reference
   `platform.user(id)` because you required `platform:0001_users` first.

2. **Your own schema.** Create and work inside `events.*`. Don't create objects
   in `platform.*` or another plugin's schema.

3. **Cross-schema foreign keys are nullable and non-cascading.** A FK from your
   table into `platform.user` (or another plugin's exposed table) must **not**
   use `ON DELETE CASCADE` — `junius check`'s `FK.CROSS.CASCADE` rule rejects
   it. Keep cascades *inside* your own schema; make cross-schema FKs nullable so
   a deleted host row doesn't break your invariants.

## Postgres enums map to Rust

Define the enum in SQL, then mirror it as a Rust type your repositories use:

```rust
#[derive(sqlx::Type)]
#[sqlx(type_name = "events.visibility", rename_all = "lowercase")]
pub enum Visibility {
    Private,
    Public,
}
```

When you query an enum column with the compile-time `sqlx::query!` macro you
must:

- **name every column** (`SELECT e.*` won't carry the type override), and
- **annotate the enum column**: `visibility AS "visibility: Visibility"`, and
- **cast enum binds**: `$7::events.visibility`.

This trips everyone up once. The [Repositories](./repositories.md) chapter shows
it in context.

## Running migrations

```bash
task migrate
# = cargo run -p junius -- migrate up --config dev/platform.toml

# inspect state:
cargo run -p junius -- migrate status --config dev/platform.toml
```

`migrate up` applies all pending host + plugin migrations **and** emits the
per-plugin Postgres role grants (the DML your `role_<name>` gets on its own
tables, plus any exposed tables you depend on).

## A note on `.sqlx`

Because repositories use *compile-time-checked* SQL, the repo commits an offline
query cache under `.sqlx/`. When you add or change a query you must regenerate
it — covered at the end of [Repositories](./repositories.md) and in
[Validation and the CI gate](../quality/ci-gate.md). Migrations themselves don't
need it, but the queries that read your new tables do.

Next: the permission model your queries and handlers gate on.
