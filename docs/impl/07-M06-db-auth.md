# M06 — Postgres + Migrations + Roles + OIDC + Sessions

## Goal

Postgres becomes the platform's persistence layer. Host migrations create `platform.*` (users, sessions, groups, roles, memberships, resource_principal, resource_share, `user_can_access`) and `meta.migrations`. Each plugin gets a Postgres role with grants derived from its manifest. OIDC login against Authentik produces server-side sessions; `GET /api/me` returns the authenticated user. `junius migrate up` applies host + plugin migrations in topo order with `-- @requires` edges.

## Why now

Every later capability — repositories (M07), resource ownership (M08), cross-plugin tables (M09), jobs (M10) — sits on persistence and identity. Landing both together makes sense because the identity schema is *part* of the host migrations, and Postgres roles connect identity to data access through grants.

## Scope (in)

### Dev infrastructure

`dev/docker-compose.yml`:
```yaml
services:
  postgres:
    image: postgres:17
    environment:
      POSTGRES_USER: platform_migrator
      POSTGRES_PASSWORD: dev-only
      POSTGRES_DB: platform
    ports: ["5432:5432"]
    volumes: [postgres-data:/var/lib/postgresql/data]

  authentik:
    image: ghcr.io/goauthentik/server:latest
    # Seeded with a test app + test users via docker-entrypoint init scripts.
    # See dev/authentik/ for the bootstrap.

volumes:
  postgres-data:
```

`dev/authentik/` contains init scripts that seed:
- A test application named `junius` with OIDC enabled.
- Two test users: `alice@local` and `bob@local`, passwords printed at startup.

### `crates/junius-sdk` additions

`PluginResources` gains a `db` field — but the type is the opaque `PluginDb` from the design (§11.3); no query API exposed yet:

```rust
pub struct PluginResources {
    pub config: PluginConfig,
    pub telemetry: Telemetry,
    pub(crate) db: PluginDb,           // opaque; queries land in M07
    pub auth: Auth,                    // platform identity primitives
    pub users: Users,                  // user lookup helper
    pub audit: AuditEmitter,           // audit logging hook
}
```

`Auth` exposes the current request's `User` and methods like `lookup_user(id)`. `Users` is a host-provided user-directory handle (used by plugins that need to display "user N's name" without joining `platform.user` directly).

`AuditEmitter`:

```rust
impl AuditEmitter {
    pub async fn emit(
        &self,
        event_kind: &str,                                // 'speakers:speaker.share' etc.
        actor_user_id: Option<UserId>,
        resource_kind: &str,
        resource_id: Option<Uuid>,
        details: serde_json::Value,
    ) -> Result<(), PluginError>;
}
```

Audit events go into `platform.audit_event` (schema below). Plugins gain the `audit.emit` capability to use this.

### Host migrations

```
platform/migrations/
├── 0001_users.up.sql
├── 0002_sessions.up.sql
├── 0003_groups_roles_memberships.up.sql
├── 0004_resource_principal_share.up.sql
├── 0005_user_can_access.up.sql
├── 0006_meta_migrations.up.sql                 # actually runs first (bootstrap)
└── 0007_audit_event.up.sql
```

Schemas mirror [../design/10-infrastructure-and-data.md](../design/10-infrastructure-and-data.md) §10.7.2–§10.7.5 exactly. `0007_audit_event.up.sql`:

```sql
CREATE TABLE platform.audit_event (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_kind      TEXT NOT NULL,                   -- '<plugin>:<resource>.<verb>'
    actor_user_id   UUID REFERENCES platform.user(id),
    resource_kind   TEXT,                            -- '<plugin>:<table>'
    resource_id     UUID,
    details         JSONB NOT NULL DEFAULT '{}'::jsonb,
    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX audit_event_actor_idx      ON platform.audit_event(actor_user_id);
CREATE INDEX audit_event_resource_idx   ON platform.audit_event(resource_kind, resource_id);
CREATE INDEX audit_event_occurred_idx   ON platform.audit_event(occurred_at DESC);
```

Retention: deliberately not enforced at the DB level in v1. A nightly job (M10) deletes events older than the deployment-configured retention (default 1 year).

### `junius migrate up` implementation

Algorithm (per [../design/10-infrastructure-and-data.md](../design/10-infrastructure-and-data.md) §10.5):

1. Connect as `platform_migrator`.
2. Ensure `meta` schema + `meta.migrations` exist (bootstrap 0006 always runs first if absent).
3. Discover all `*.up.sql` files: `platform/migrations/*.up.sql` + every enabled plugin's `plugins/<name>/migrations/*.up.sql`.
4. Parse the header of each file for `-- @requires <plugin>:<migration_name>` directives.
5. Build a DAG:
   - Within-plugin: filename order → implicit edges.
   - Cross-plugin: explicit `@requires` edges.
   - Host migrations: all prepended (host always before plugins).
6. Topological sort. Cycle → fail with the cycle reported.
7. For each migration not in `meta.migrations`:
   - Compute SHA-256 of the file.
   - Apply in a transaction together with the `INSERT INTO meta.migrations`.
   - On error, rollback and exit non-zero.
8. After applying, compute per-plugin Postgres-role grants from manifests and emit `CREATE ROLE` / `GRANT` statements (idempotently). Role passwords are derived from a host-secret + role-name HMAC, so the host can re-derive them at boot without storing them.

### Postgres role grant computation

Per [../design/10-infrastructure-and-data.md](../design/10-infrastructure-and-data.md) §10.4. The migration runner emits:

```sql
DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'role_hello') THEN
    CREATE ROLE role_hello NOINHERIT LOGIN PASSWORD '<derived>';
  END IF;
END$$;

GRANT USAGE, CREATE ON SCHEMA hello TO role_hello;
GRANT ALL ON ALL TABLES IN SCHEMA hello TO role_hello;
ALTER DEFAULT PRIVILEGES IN SCHEMA hello GRANT ALL ON TABLES TO role_hello;

GRANT USAGE ON SCHEMA platform TO role_hello;
GRANT SELECT ON platform.user TO role_hello;

-- Cross-plugin grants from manifest [dependencies.<dep>].tables = [...]
-- (none for hello yet; this section is empty for M06)
```

### `platform/db/` host module

```rust
// platform/src/db/mod.rs
pub struct DbBootstrap {
    migrator_pool: PgPool,            // platform_migrator role
}

pub struct PluginPools {
    pools: HashMap<&'static str, PgPool>,    // one per plugin role
}

impl DbBootstrap {
    pub async fn run_migrations(&self, plugins: &[&'static PluginMetadata]) -> Result<()> { ... }
    pub async fn build_plugin_pools(&self, plugins: &[&'static PluginMetadata]) -> Result<PluginPools> { ... }
}
```

The host calls these during boot, after `junius migrate up` has been run. (In dev, `junius dev` calls `junius migrate up` first; in prod, ops run it as a separate step.)

`PluginDb` (opaque type returned from `PluginPools::get`):

```rust
pub struct PluginDb {
    pool: PgPool,                       // not pub
    plugin_name: &'static str,
}
// No public methods that return PgPool or executors.
// Real query API lands in M07 with #[derive(Repository)].
```

### `platform/auth/` host module

OIDC flow using the `openidconnect` crate:

- `GET /api/auth/login` → constructs an OIDC auth URL with `state` and `nonce`, redirects.
- `GET /api/auth/callback?code=...&state=...` → exchanges code for tokens, fetches userinfo, upserts `platform.user`, inserts `platform.session`, sets session cookie, redirects to `/`.
- `POST /api/auth/logout` → deletes session row, clears cookie, redirects to Authentik logout.

Session middleware on every request:
- Reads `session` cookie.
- Looks up `platform.session` row.
- Loads the `User` (with memberships + permissions) via a single query that joins `group_membership` + `group_role` + `role_permission`.
- Attaches `Option<User>` to the request via Axum's `Extensions`.
- Unauthenticated routes (`/api/auth/*`, `/login`, static assets) bypass; everything else returns 401 if no user.

`User`:

```rust
pub struct User {
    pub id: UserId,
    pub email: String,
    pub display_name: String,
    pub memberships: Vec<Membership>,
}
pub struct Membership {
    pub group_id: GroupId,
    pub group_name: String,
    pub role: Role,
    pub permissions: HashSet<String>,
}
```

`GET /api/me` returns the current user as JSON, used by the FE `AuthProvider`.

### Frontend `AuthProvider` (real version)

Replaces the stub from M04:
- On mount, `fetch('/api/me')`.
- 200 → set user; render children.
- 401 → redirect to `/api/auth/login?return_to=<current path>`.
- Refresh strategy: on every TanStack Query 401 response, re-attempt `/api/me`; if still 401, redirect to login.

### `junius new migration <plugin> <name>`

Implemented in this milestone. Generates `plugins/<plugin>/migrations/<ts>_<name>.up.sql` with a header template:

```sql
-- @requires platform:0007_audit_event
-- Add additional @requires for any cross-plugin tables this migration references.

-- Migration body goes here.
```

## Scope (out)

- No `#[derive(Repository)]`. Plugins have a `PluginDb` but can't query yet. Adding query helpers without the permission system would create a footgun. M07 covers both.
- No down migrations. Forward-fix per design §10.5.
- No retention-cleanup job for audit events — the queries work; the daily prune is a job that lands in M10.
- No password-based auth. OIDC-only.
- No multi-tenancy. Single-tenant locked. Every table is per-deployment; no `org_id` columns.

## Library choices — confirm with user before starting

| Choice | Proposed default | Rationale | Downstream milestones to update if changed |
|---|---|---|---|
| **DB driver** | `sqlx` with `postgres` + `runtime-tokio-rustls` features | Locked by design ([../design/10-infrastructure-and-data.md](../design/10-infrastructure-and-data.md) §10.2) | M07, M08, M09, M12, M13 |
| **Commit `.sqlx/`?** | Yes — committed, `cargo sqlx prepare --check` runs in CI | Locked by design | M07 (extends CI step), M12 (enforces in `junius check`) |
| **Migration runner** | Custom inside `junius` (not `sqlx-cli`) | sqlx-cli can't model cross-plugin DAG | M11 (deployment), M12 (Layer 1 SQL diff rules) |
| **Postgres version** | 17 (latest stable at impl time) | Modern features (we use `gen_random_uuid`, JSONB indices, plpgsql) | — |
| **Cookies** | `tower-cookies` | Standard Axum-aligned crate; in-tree session storage means we don't need signed cookies | If session storage changes to client-side |
| **Session-token encryption** | `aes-gcm` (RustCrypto) for `oidc_tokens BYTEA` only | Symmetric encryption suffices for tokens stored server-side; key from env via secret-loading | If we move to KMS-managed keys |
| **OIDC client** | `openidconnect` crate | Active, OIDC-compliant; works with Authentik out of the box | If Authentik drops standard OIDC flows |
| **JWT validation** | `jsonwebtoken` (only if Authentik's userinfo endpoint isn't used) | The OIDC crate does ID-token validation; raw JWT only needed for niche flows | — |
| **Testcontainers** | `testcontainers-modules` with `postgres` feature | Ephemeral Postgres for integration tests | M12 (CI schema test) |
| **Secret loading** | Custom: `env:VAR` indirection in `platform.toml` `[config]`; `vault:PATH` deferred | Keeps the deployment config the single source for resolving secrets | M11 (deployment workflow) |
| **Authentik image** | `ghcr.io/goauthentik/server:latest` | Easy to run; production deployments pin a specific tag | — |

## Open questions resolved

- **Multi-tenancy** — locked: **single-tenant**. Every org runs its own deployment.
- **Trust model** — locked: **first-party plugins only in v1**. All plugin code lives in the source monorepo (in-tree or via submodule), code-reviewed. Capability enforcement remains audit-only. Third-party plugin support is a deliberate future change with its own design.
- **Audit logging** — schema decided here (`platform.audit_event`), API decided (`PluginResources.audit.emit`), retention default 1 year (configurable per deployment, enforced by a nightly job in M10).

## Verification

```bash
# Bring up dev infrastructure
docker compose -f dev/docker-compose.yml up -d
# Wait for postgres + authentik to be healthy.

# Configure dev/platform.toml [config] with:
#   database_url   = "postgres://platform_migrator:dev-only@localhost:5432/platform"
#   oidc_issuer    = "http://localhost:9000/application/o/junius/"
#   oidc_client_id = "platform"
#   oidc_client_secret = "env:OIDC_CLIENT_SECRET"
#   session_encryption_key = "env:SESSION_KEY"

# Run migrations
target/release/junius migrate up --config dev/platform.toml

# Verify host schemas exist
psql platform -c "\dn"             # → meta, platform, hello
psql platform -c "\dt platform.*"  # → user, session, group, group_role, role_permission, group_membership, resource_principal, resource_share, audit_event

# Verify role grants
psql platform -c "\du role_hello"
psql platform -c "SELECT has_schema_privilege('role_hello', 'hello', 'USAGE');"     # → t
psql platform -c "SELECT has_table_privilege('role_hello', 'platform.user', 'SELECT');"  # → t
psql platform -c "SELECT has_table_privilege('role_hello', 'platform.session', 'SELECT');"  # → f

# Idempotency
target/release/junius migrate up --config dev/platform.toml
# → "No pending migrations." exits 0

# Boot the platform + log in
target/release/junius dev --config dev/platform.toml &
# Browser:
#   1. Open http://localhost:5173 → redirects to Authentik login.
#   2. Log in as alice@local.
#   3. Redirected back to /. AuthProvider fetches /api/me → 200 with Alice's details.
#   4. Navigate to /p/hello → page renders.

# Direct API check
curl -fsS -b cookies.txt http://127.0.0.1:8080/api/me
# → 200 with the alice user object

curl -fsS -b cookies.txt -X POST http://127.0.0.1:8080/api/auth/logout
# Then:
curl -i http://127.0.0.1:8080/api/me
# → 401

# Role isolation test (asserts Postgres rejects an out-of-grant query)
PGUSER=role_hello psql -c "SELECT * FROM platform.session;"
# → ERROR: permission denied for table session
```

End of M06: the platform has persistence, identity, sessions, and per-plugin Postgres roles. M07 unlocks data access for plugins via the repository pattern.
