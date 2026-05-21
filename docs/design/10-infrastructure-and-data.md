# 10. Infrastructure & Data

The host (`platform/`) provides a small set of shared infrastructure capabilities. Plugins consume them through typed handles on `PluginContext` (exposed via `junius-sdk`), gated by `[requires.capabilities]` in the plugin's manifest.

## 10.1 Provided infrastructure

| Capability | Handle | v1 backing | Notes |
|---|---|---|---|
| Database | `Db` | PostgreSQL (single instance per deployment) | Per-plugin schema + per-plugin Postgres role (§10.2 / §10.4). |
| Object storage | `Storage` | S3-compatible (MinIO locally; S3/R2/etc. in prod) | Per-plugin bucket prefix. |
| Background jobs | `Jobs` | Postgres-backed queue (e.g. apalis) | No Redis dependency in v1. |
| Email | `Email` | Pluggable transactional provider (SES / Resend / Postmark) | Backend chosen per deployment. |
| Config / Secrets | `Config` | Typed access to deployment config + env-var secrets | See [05-repository-and-deployment-layout.md](05-repository-and-deployment-layout.md) §5.5 deployment block. |
| Telemetry | `Telemetry` | OpenTelemetry (tracer, meter, logger) | Standard across PLAI codebases. |

**Deferred to v2**: cache (Redis), search (Meilisearch/Elasticsearch), realtime/WebSockets, formal outbound-HTTP capability.

## 10.2 Database

- **PostgreSQL**, single instance per deployment.
- **sqlx** for access: compile-time-checked queries (`query!` / `query_as!`), async, native connection pool. No ORM — plain SQL is the contract.
- Each plugin commits its `.sqlx/` prepared-query cache; `cargo sqlx prepare --check` runs in `junius check`.
- **One Postgres schema per plugin**, named after the plugin (`speakers.*`, `events.*`). The host owns `platform.*` (users, sessions, RBAC) and `meta.*` (migration bookkeeping).
- **Connection pools owned by the host.** Plugins never instantiate `PgPool` directly. The host runs one pool per Postgres role (§10.4); `PluginContext.db()` returns the plugin's scoped pool.
- Capability gates: `db.read` grants a read-only handle; `db.write` grants a full handle.

## 10.3 Cross-plugin data access

Always-apply migrations (§10.5) mean every plugin's schema exists in every deployment. This makes cross-plugin data access uniform whether the dep is required or optional.

**Declaring the surface**:

```toml
# plugins/speakers/plugin.toml — owner declares public tables
[exposes.tables.speaker]
schema = "speakers"
description = "Speaker records — stable public schema."

# plugins/events/plugin.toml — consumer declares what it uses
[dependencies.speakers]
optional = false
tables = ["speaker"]
```

**Reads (SELECT / JOIN)**: allowed against any table in `B.[exposes.tables]` that the consumer declared in its `tables = [...]` list.

**Writes (INSERT / UPDATE / DELETE)**: allowed against the same surface. Plugin authors are trusted to use SQL directly. If B needs to enforce invariants regardless of who writes, B defines **Postgres triggers** on its own tables — triggers fire for every writer.

**FKs**:
- Across required deps: `NOT NULL` permitted.
- Across optional deps: **must be nullable** — the dep's code may not be running and won't be creating rows to reference. `junius check` rejects `NOT NULL` FKs targeting an optional-dep schema.

**Forbidden**:
- `ON DELETE CASCADE` / `ON UPDATE CASCADE` on cross-plugin FKs — silently mutates other plugins' tables and bypasses coordination. `junius check` rejects them.
- Any SQL reference to a non-public table in another plugin's schema. Enforced at runtime by Postgres roles (§10.4) and at PR time by `junius check`.

## 10.4 Postgres role enforcement

Each plugin runs its queries as its own Postgres role. Grants are computed from manifests and applied by the migration runner — convention becomes database-enforced.

For a plugin `events` that declares `[dependencies.speakers].tables = ["speaker"]`:

```sql
CREATE ROLE role_events NOINHERIT;

-- Own schema: full access
GRANT USAGE, CREATE ON SCHEMA events TO role_events;
GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA events TO role_events;
ALTER DEFAULT PRIVILEGES IN SCHEMA events GRANT ALL ON TABLES TO role_events;

-- Declared cross-plugin deps: full DML on declared exposed tables
GRANT USAGE ON SCHEMA speakers TO role_events;
GRANT SELECT, INSERT, UPDATE, DELETE ON speakers.speaker TO role_events;

-- Host public surfaces: read-only
GRANT USAGE ON SCHEMA platform TO role_events;
GRANT SELECT ON platform.user TO role_events;
```

The host maintains one `PgPool` per role; `PluginContext.db()` returns the plugin's pool. A plugin trying to query outside its grants gets a Postgres permission error.

**Migration runner role**: a privileged role (e.g. `platform_migrator`) that owns all schemas and can issue GRANT statements. The only role with broad schema-modification rights. Used exclusively by `junius migrate`.

## 10.5 Migrations

**Layout**: each plugin has `plugins/<name>/migrations/<ts>_<name>.up.sql`. Down migrations are optional and discouraged — forward-fix is the recommended discipline.

**Bookkeeping** lives in `meta.migrations`:

```sql
CREATE SCHEMA meta;

CREATE TABLE meta.migrations (
    id              BIGSERIAL    PRIMARY KEY,            -- apply order across deployment lifetime
    plugin          TEXT         NOT NULL,               -- 'platform' for host migrations
    migration_name  TEXT         NOT NULL,               -- e.g. '0007_create_speaker'
    checksum        TEXT         NOT NULL,               -- SHA-256 of .up.sql contents
    applied_at      TIMESTAMPTZ  NOT NULL DEFAULT now(),
    UNIQUE (plugin, migration_name)
);
```

`id BIGSERIAL` gives true apply-order; `checksum` lets `junius` detect post-apply file edits.

**Always-apply**: every deployment runs the full migration set from its pinned source revision regardless of which plugins are *enabled*. A disabled plugin still has its schema and Postgres role; only its router / RPC / code paths are absent. Enable/disable is purely a code concern.

**Ordering**:

1. **Host migrations first.** All migrations under `platform/migrations/` apply before any plugin migration.
2. **Plugin migrations** then apply as a topo sort over a DAG with edges from:
   - **Within a plugin**: migration N → migration N+1 (implicit, from filename order).
   - **`@requires` declarations**: explicit edges between any two migrations.

   Plugin-level manifest dependencies **do not** infer migration ordering. Migrations declare their cross-plugin order directly in the SQL.

**`@requires` syntax** — SQL header comment, parsed before any executable SQL:

```sql
-- @requires speakers:0007_create_speaker
-- @requires platform:0003_users

CREATE TABLE events.event (
  id           UUID PRIMARY KEY,
  speaker_id   UUID NULL REFERENCES speakers.speaker(id),
  organizer_id UUID NOT NULL REFERENCES platform.user(id),
  ...
);
```

Format: `-- @requires <plugin>:<migration_name>` where `<migration_name>` is the filename without `.up.sql`. The `@` prefix is reserved for `junius` directives; future additions (`@breaking-change`, etc.) go here.

**Edges flow in either direction.** Cleanup migrations naturally reverse the dep arrow:

```sql
-- plugins/events/migrations/0050_drop_speaker_email_cache.up.sql
ALTER TABLE events.event DROP COLUMN cached_speaker_email;
```

```sql
-- plugins/speakers/migrations/0051_drop_speaker_email.up.sql
-- @requires events:0050_drop_speaker_email_cache
ALTER TABLE speakers.speaker DROP COLUMN email;
```

Topo sort puts `events:0050` before `speakers:0051`. This is exactly why we don't infer ordering from manifest deps — cleanup flows against them.

**`junius check` enforces**:
- Every `@requires` reference resolves to a real migration.
- The DAG has no cycles.
- Every cross-plugin SQL reference (FK or schema-qualified table reference) has a matching `@requires` for the target migration.
- No `CASCADE` clauses on cross-plugin FKs.
- `checksum` matches `meta.migrations` for already-applied migrations.
- `cargo sqlx prepare --check` is clean for every plugin.

## 10.6 Schema evolution & compatibility

The cross-plugin compatibility surface is `[exposes.tables]`. Rules mirror `buf breaking` for protos, applied to SQL:

| Change to an exposed table | Compatibility |
|---|---|
| Add column (nullable or with default) | Compatible |
| Add table to exposed set | Compatible |
| Drop `NOT NULL` | Compatible |
| Widening type change (`varchar(50)` → `text`, `int` → `bigint`) | Compatible |
| Add index | Compatible |
| Drop column | **Breaking** |
| Rename column | **Breaking** |
| Drop table or remove from exposed set | **Breaking** |
| Narrowing type change | **Breaking** |
| Add `NOT NULL` (when existing rows could violate) | **Breaking** |
| Change PK / drop unique constraint targeted by FK | **Breaking** |

Private tables (anything not in `[exposes.tables]`) can change freely — no consumer dependencies.

**Coordinated breaking changes**: a breaking change to an exposed table must ship in the same source revision (same PR) as matching migrations and code updates in every consumer plugin.

**Enforcement in v1** uses two layers:

**Layer 1 — `junius check` (static, fast)**:
- Parses migrations; flags `ALTER` / `DROP` / `RENAME` on tables listed in any plugin's `[exposes.tables]` as "touches public schema."
- For known-breaking patterns (`DROP COLUMN`, `RENAME COLUMN`, `DROP TABLE`): verifies that every consumer plugin (declared via `[dependencies.b].tables`) either drops the table from its declared deps or has a matching migration in this PR.
- Rejects `CASCADE` on cross-plugin FKs.

**Layer 2 — CI integration test**:
- Ephemeral Postgres (testcontainers).
- `junius migrate up` against a representative deployment configuration.
- Run all plugin test suites against the migrated DB.
- Catches anything Layer 1 missed: actual FK violations on apply, query failures, type errors, runtime test failures.

**Layer 3 — schema snapshot diff (deferred)**: per-plugin `exposed-schema.sql` snapshot diffed by `junius`, with breaking changes requiring an `@breaking-change` annotation. Adopt when manual coordination starts missing things at scale.

## 10.7 Authentication & Authorization

The host owns identity, sessions, groups, roles, and resource-level access checks. Plugins consume these via `Auth`, `Users`, and authorization helpers on `PluginResources` ([11-backend-plugin-interface.md](11-backend-plugin-interface.md) §11.3); they never roll their own auth.

### 10.7.1 Identity: OIDC + Authentik, server sessions

- Authentication is via **OIDC against Authentik** (configurable per deployment). The platform never manages passwords.
- After a successful OIDC flow, the platform creates a **server-side session** in `platform.session` (Postgres). Sessions are identified by an HTTP-only, Secure, SameSite=Lax cookie carrying only the session ID.
- OIDC access/refresh tokens are stored encrypted in the session for downstream calls if needed (proxying to other Authentik-protected services); otherwise unused.
- The host's auth middleware exchanges the cookie for a `User` on every request before handler logic runs.

Token-in-localStorage is explicitly not supported (XSS exposure).

### 10.7.2 Identity schema

```sql
CREATE TABLE platform.user (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    oidc_sub      TEXT NOT NULL UNIQUE,         -- Authentik subject
    email         TEXT NOT NULL,
    display_name  TEXT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE platform.session (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id       UUID NOT NULL REFERENCES platform.user(id) ON DELETE CASCADE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at    TIMESTAMPTZ NOT NULL,
    oidc_tokens   BYTEA                          -- encrypted, optional
);
CREATE INDEX session_user_idx    ON platform.session(user_id);
CREATE INDEX session_expires_idx ON platform.session(expires_at);
```

### 10.7.3 Groups, roles, memberships

- A **group** is a real-world organizational unit (committee, caucus, working group, campaign team).
- A **role** is defined *within* a group; the same role name in two groups is two distinct rows. ("Chair of Committee X" is a different role from "Chair of Caucus Y".)
- A user has **at most one role per group**. Multiple roles are modeled as a promotion (replace the existing membership row), not as a multi-role membership.
- Roles own the **capability** permissions — the strings declared in plugin manifests' `[permissions]` blocks ([06-plugin-shape.md](06-plugin-shape.md) §6.1).
- **Groups have no relations among themselves.** No hierarchy, no parent/child, no inheritance. A user is either a member of a group or they aren't.

```sql
CREATE TABLE platform.group (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name          TEXT NOT NULL,
    description   TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE platform.group_role (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    group_id      UUID NOT NULL REFERENCES platform.group(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    UNIQUE (group_id, name)
);

CREATE TABLE platform.role_permission (
    role_id       UUID NOT NULL REFERENCES platform.group_role(id) ON DELETE CASCADE,
    permission    TEXT NOT NULL,                 -- e.g. 'speakers:read'
    PRIMARY KEY (role_id, permission)
);

CREATE TABLE platform.group_membership (
    user_id       UUID NOT NULL REFERENCES platform.user(id) ON DELETE CASCADE,
    group_id      UUID NOT NULL REFERENCES platform.group(id) ON DELETE CASCADE,
    role_id       UUID NOT NULL REFERENCES platform.group_role(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, group_id)              -- one role per (user, group)
);
```

### 10.7.4 Resource ownership and sharing

Capability permissions (`speakers:read`) declare *what kind* of action. **Scope** — which specific resources a user may act on — is resolved separately via ownership and explicit shares.

**Ownership is per-resource-type AND per-instance.** Some resource types are *conceptually* group-owned: a meeting in a speakers-list app belongs to the committee that scheduled it. Others are *conceptually* user-owned: a personal draft document, a vote ballot cast by an individual. The platform supports both via a single `resource_principal` table where exactly one of `owner_user_id` or `owner_group_id` is set; **the plugin decides at creation time** which principal kind is appropriate for the resource being created. There is no manifest-level constraint forcing all instances of a resource type to one ownership kind — though plugins are encouraged to be consistent within a resource type for predictability.

**Owners have implicit full access** to their resources, regardless of role permissions. Otherwise users could create resources they can't subsequently see — a footgun.

**Sharing in v1**: only owners may share their resources. A finer-grained "who can share what" model is deferred. Explicit shares grant a specific permission to a principal (another user, a group, or the platform-wide public).

**Public resources**: opt-in per resource via a `resource_share` row with `principal_kind = 'public'`. Useful for things like committee-wide announcements or organization-wide documents.

```sql
-- Ownership: exactly one of user_id / group_id is set.
CREATE TABLE platform.resource_principal (
    resource_kind   TEXT NOT NULL,               -- '<plugin>:<table>', e.g. 'speakers:speaker'
    resource_id     UUID NOT NULL,
    owner_user_id   UUID REFERENCES platform.user(id)  ON DELETE CASCADE,
    owner_group_id  UUID REFERENCES platform.group(id) ON DELETE CASCADE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (resource_kind, resource_id),
    CHECK ((owner_user_id IS NULL) <> (owner_group_id IS NULL))
);

-- Explicit per-resource grants.
CREATE TABLE platform.resource_share (
    resource_kind         TEXT NOT NULL,
    resource_id           UUID NOT NULL,
    principal_kind        TEXT NOT NULL,         -- 'user' | 'group' | 'public'
    principal_user_id     UUID REFERENCES platform.user(id)  ON DELETE CASCADE,
    principal_group_id    UUID REFERENCES platform.group(id) ON DELETE CASCADE,
    permission            TEXT NOT NULL,
    granted_by_user_id    UUID NOT NULL REFERENCES platform.user(id),
    granted_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at            TIMESTAMPTZ,
    CHECK (
        (principal_kind = 'user'   AND principal_user_id  IS NOT NULL AND principal_group_id IS NULL) OR
        (principal_kind = 'group'  AND principal_group_id IS NOT NULL AND principal_user_id  IS NULL) OR
        (principal_kind = 'public' AND principal_user_id  IS NULL     AND principal_group_id IS NULL)
    )
);
```

`resource_kind` uses the **`<plugin>:<table>`** format (e.g. `speakers:speaker`, `events:meeting`). Plugins document their resource kinds in the authoring guide.

### 10.7.5 The access-check function

```sql
CREATE OR REPLACE FUNCTION platform.user_can_access(
    p_resource_kind TEXT,
    p_resource_id   UUID,
    p_user_id       UUID,
    p_permission    TEXT
) RETURNS BOOLEAN
LANGUAGE plpgsql STABLE
AS $$
DECLARE
    v_owner_user_id  UUID;
    v_owner_group_id UUID;
BEGIN
    -- 0. Resolve ownership.
    SELECT owner_user_id, owner_group_id
    INTO v_owner_user_id, v_owner_group_id
    FROM platform.resource_principal
    WHERE resource_kind = p_resource_kind AND resource_id = p_resource_id;

    -- 1. Owner has implicit full access.
    IF v_owner_user_id = p_user_id THEN RETURN true; END IF;

    -- 2. Owned by a group where the user's role grants p_permission.
    IF v_owner_group_id IS NOT NULL AND EXISTS (
        SELECT 1
        FROM platform.group_membership gm
        JOIN platform.role_permission rp ON rp.role_id = gm.role_id
        WHERE gm.user_id    = p_user_id
          AND gm.group_id   = v_owner_group_id
          AND rp.permission = p_permission
    ) THEN RETURN true; END IF;

    -- 3. Explicit share to this user (active).
    IF EXISTS (
        SELECT 1 FROM platform.resource_share rs
        WHERE rs.resource_kind = p_resource_kind
          AND rs.resource_id   = p_resource_id
          AND rs.permission    = p_permission
          AND rs.principal_user_id = p_user_id
          AND (rs.expires_at IS NULL OR rs.expires_at > now())
    ) THEN RETURN true; END IF;

    -- 4. Explicit share to a group the user is in (active).
    IF EXISTS (
        SELECT 1
        FROM platform.resource_share rs
        JOIN platform.group_membership gm ON gm.group_id = rs.principal_group_id
        WHERE rs.resource_kind = p_resource_kind
          AND rs.resource_id   = p_resource_id
          AND rs.permission    = p_permission
          AND gm.user_id       = p_user_id
          AND (rs.expires_at IS NULL OR rs.expires_at > now())
    ) THEN RETURN true; END IF;

    -- 5. Public share (active).
    IF EXISTS (
        SELECT 1 FROM platform.resource_share rs
        WHERE rs.resource_kind = p_resource_kind
          AND rs.resource_id   = p_resource_id
          AND rs.permission    = p_permission
          AND rs.principal_kind = 'public'
          AND (rs.expires_at IS NULL OR rs.expires_at > now())
    ) THEN RETURN true; END IF;

    RETURN false;
END;
$$;
```

### 10.7.6 Two-layer authorization in plugin code

Both layers apply on every authenticated request:

**Layer 1 — capability gate at the handler / repo method** (already specified in [11-backend-plugin-interface.md](11-backend-plugin-interface.md) §11.5, §11.6). The `Has<X>` bound ensures the user has the capability *somewhere* in their group memberships. Coarse, fast, compile-time enforced.

**Layer 2 — resource-scoped filter at the query.** Every repository method that returns or operates on resources joins through `platform.user_can_access(...)`:

```rust
#[impl_repository(SpeakerRepo)]
impl<P: Has<SpeakersRead>> SpeakerRepo<P> {
    pub async fn list(&self) -> Result<Vec<Speaker>, RepoError> {
        sqlx::query_as!(Speaker, "
            SELECT s.*
            FROM speakers.speaker s
            WHERE platform.user_can_access('speakers:speaker', s.id, $1, 'speakers:read')
        ", self.user_id)
        .fetch_all(self.pool())
        .await
        .map_err(Into::into)
    }

    pub async fn get(&self, id: SpeakerId) -> Result<Option<Speaker>, RepoError> {
        sqlx::query_as!(Speaker, "
            SELECT s.*
            FROM speakers.speaker s
            WHERE s.id = $1
              AND platform.user_can_access('speakers:speaker', s.id, $2, 'speakers:read')
        ", id, self.user_id)
        .fetch_optional(self.pool())
        .await
        .map_err(Into::into)
    }
}
```

Rows the user can't access simply don't appear in results — no "permission denied" mid-query, no leaked existence.

### 10.7.7 Plugin integration: recording ownership and sharing

**On create**: plugins record ownership atomically with the insert, via an `authz` helper on `PluginResources` (or via a derive that generates the boilerplate):

```rust
#[impl_repository(SpeakerRepo)]
impl<P: Has<SpeakersWrite>> SpeakerRepo<P> {
    pub async fn create(&self, input: NewSpeaker) -> Result<Speaker, RepoError> {
        let mut tx = self.pool().begin().await?;

        let s = sqlx::query_as!(Speaker,
            "INSERT INTO speakers.speaker (...) VALUES (...) RETURNING *",
            /* ... */
        ).fetch_one(&mut *tx).await?;

        // Plugin chooses ownership kind per resource type:
        //   - Speakers (personal contacts):     Principal::User(self.user_id)
        //   - Meetings (committee-scheduled):   Principal::Group(meeting.committee_id)
        //   - Ballots (cast by a user):         Principal::User(self.user_id)
        self.authz
            .record_owner(&mut tx, "speakers:speaker", s.id, Principal::User(self.user_id))
            .await?;

        tx.commit().await?;
        Ok(s)
    }
}
```

A `#[owned_by(user)]` / `#[owned_by(group_from = "...")]` attribute on the repo method could automate this; deferred to implementation.

**Sharing**: plugins expose mutation RPCs (e.g. `SpeakerService.ShareSpeaker`) that delegate to the host's `authz.share(...)` API. The host enforces "only the current owner can share" and writes to `platform.resource_share`.

### 10.7.8 Frontend `User` and per-resource flags

The frontend `User` object (from `@junius/sdk`) carries memberships and their capability permissions:

```ts
interface User {
  id: UserId;
  email: string;
  displayName: string;
  memberships: ReadonlyArray<{
    groupId:     GroupId;
    groupName:   string;
    role:        { id: RoleId; name: string };
    permissions: ReadonlySet<string>;
  }>;
}
```

`useHasPermission('speakers:read')` returns true if **any** of the user's memberships grants it. This is the FE analog of Layer 1 — gates "Create" buttons, navigation items, etc.

For per-resource decisions ("can I edit *this* speaker?"), the server includes computed flags on each returned resource — e.g. `Speaker { ..., viewerCanEdit: bool, viewerCanShare: bool }`. The FE reads these flags rather than recomputing access. **The server is the source of truth**; the client just reflects.
