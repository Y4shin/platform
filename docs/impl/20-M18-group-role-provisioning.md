# 20. M18 — Group & Role Provisioning

> **Status:** ⏳ in progress. Stages 1–4 implemented (the core of the
> milestone — user-roles + admin override, admin plugin with CRUD over
> groups / group-roles / user-roles / OIDC mappings, OIDC group
> reconciliation with REST refresh endpoint, declarative TOML
> provisioning with hash-guarded apply). Two integration paths are
> deferred to follow-up: `lock_managed` runtime enforcement at the
> `PlatformAdminApi` seam (the flag is parsed + persisted, but the
> SDK's mutators don't yet reject writes against `managed_by='config'`
> rows); and `auto_apply_on_boot = true` host-side startup integration
> (parsed but `server::run` doesn't yet invoke `provision apply` after
> `migrate up`). Both are small, isolated changes against well-defined
> seams; punting them lets each commit stay reviewable.
>
> Picks up the carve-out M16 explicitly deferred: *"a group/role
> management UI (assigning permissions without raw SQL) — a much larger
> platform feature surfaced separately during M13 testing"*
> ([M16 §Out of scope](18-M16-authoring-ergonomics.md#out-of-scope)).

One-line goal: end the era where the only way to create a group, add a role, or
grant a permission is `psql`. After M18, an **admin** logs into a UI to do all
of that; OIDC group memberships drive `platform.group_membership` automatically;
and a deployment can ship its initial groups/roles/permissions as **TOML
configuration** rather than a hand-run `dev-seed.sql`.

## Why this milestone exists

The platform shipped its authorization model in [M06](07-M06-db-auth.md) (groups,
per-group roles, role→permission, memberships) and grew per-resource ownership /
sharing in [M08](09-M08-resource-access.md). But the **provisioning** side — how
those rows get *into* `platform.group*` in the first place — has never been
addressed:

- The dev environment uses [`dev/dev-seed.sql`](../../dev/dev-seed.sql): a
  hand-written SQL script that creates the `Organisers` group, an `organiser`
  role, the permission set, and the memberships. It is documented as
  *"run this AFTER logging in once as each user"* — i.e. **a manual step every
  new contributor must remember**.
- There is **no admin UI**. Creating a group in a real deployment means SSH'ing
  to the box and running SQL. Adding a permission to an existing role means
  the same.
- There is **no notion of a global "admin"**. Every permission is per-(group,
  role); there is no shape for "this user can do everything everywhere", which
  is precisely what someone tasked with managing the platform needs.
- There is **no OIDC group integration**. Authentik already models groups
  (organiser teams, member rosters), but the platform ignores the `groups`
  claim — every Junius membership is hand-maintained, redundantly with what the
  IdP already knows.
- There is **no declarative provisioning**. A deployment can't say *"these are
  my groups, roles, and permissions; reconcile the DB to match"* — every change
  is imperative and ad-hoc.

This milestone closes those four gaps. Concretely it adds:

1. **User-roles** (a global-scope counterpart to `group_role`), including a
   built-in `admin` role that grants every permission, in every group, on every
   resource.
2. An **`admin` plugin** with pages for creating groups, editing
   role↔permission tables for both group-roles and user-roles, and assigning
   user-roles.
3. **OIDC group → Junius group/role mapping** evaluated on every login.
4. **Config-based provisioning** (`junius provision apply`), with the dev
   environment migrated off `dev-seed.sql`.

## Outcome / acceptance

- A fresh deployment with one OIDC user designated **admin** can use the
  browser, end-to-end, to create a group, create roles, edit their permissions,
  add members, and define an OIDC-group mapping — **without touching SQL**.
- A user holding the built-in **`admin` user-role** passes every permission
  check and every resource ACL (`platform.user_can_access`) regardless of
  group membership.
- A user assigned to an Authentik group that maps to a Junius `(group, role)`
  pair has the corresponding `platform.group_membership` row after their next
  login; removing them from the Authentik group removes the membership on the
  subsequent login. Manual memberships are unaffected.
- `dev/dev-seed.sql` is deleted in favour of a checked-in
  `dev/provisioning.toml`; `junius provision apply --config dev/platform.toml`
  produces an identical DB state, idempotently.
- `task ci` includes a focused Postgres integration test plus a Playwright
  spec under `plugins/admin/frontend/e2e/`.

## Design

The four concerns are independent in their domain logic but **stack** in
dependency order: the user-role schema (A) is what the admin UI (B) edits and
what the admin permission check honours; the OIDC mapping (C) and config-based
provisioning (D) both write to the same group/role/permission tables and reuse
A's `managed_by` audit column.

### A — User-roles + the built-in `admin` role

**Problem.** `platform.group_role` is *intrinsically* per-group: every role
exists inside one group, and `role_permission` only matters in conjunction with
a `group_membership` row. There is no shape for "this user can do X everywhere"
— short of inserting them into every group with a fully-permissioned role,
which doesn't scale and breaks the moment a new group is created.

**Fix.** Introduce a parallel **user-role** axis: roles scoped to the user, not
the group.

New migration `platform/migrations/0011_user_roles.up.sql`:

```sql
CREATE TABLE platform.user_role (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name            TEXT NOT NULL UNIQUE,                 -- 'admin', 'support', …
    description     TEXT,
    is_builtin      BOOLEAN NOT NULL DEFAULT false,       -- 'admin' is builtin; protects it from delete
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE platform.user_role_permission (
    role_id         UUID NOT NULL REFERENCES platform.user_role(id) ON DELETE CASCADE,
    permission      TEXT NOT NULL,                        -- '<plugin>:<perm>' or '*' (admin wildcard)
    PRIMARY KEY (role_id, permission)
);

CREATE TABLE platform.user_role_assignment (
    user_id         UUID NOT NULL REFERENCES platform.user(id) ON DELETE CASCADE,
    role_id         UUID NOT NULL REFERENCES platform.user_role(id) ON DELETE CASCADE,
    granted_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    granted_by      UUID REFERENCES platform.user(id),    -- the admin who granted (NULL = system/config)
    PRIMARY KEY (user_id, role_id)
);

-- Builtin admin role: the one allowed wildcard. Re-inserted idempotently.
INSERT INTO platform.user_role (name, description, is_builtin)
VALUES ('admin', 'Full access to every permission in every group.', true)
ON CONFLICT (name) DO NOTHING;

INSERT INTO platform.user_role_permission (role_id, permission)
SELECT id, '*' FROM platform.user_role WHERE name = 'admin'
ON CONFLICT DO NOTHING;
```

**Permission resolution** ([`crates/junius-sdk/src/auth.rs`](../../crates/junius-sdk/src/auth.rs)).
`User` grows a `user_roles: Vec<UserRoleGrant>` field carrying the resolved
permission set; the session middleware populates it alongside `memberships`:

```rust
pub struct UserRoleGrant {
    pub role_id:    RoleId,
    pub role_name:  String,
    pub permissions: HashSet<String>,   // '*' if this is the admin role
}

impl User {
    pub fn is_admin(&self) -> bool {
        self.user_roles.iter().any(|r| r.permissions.contains("*"))
    }

    pub fn has_permission(&self, p: &str) -> bool {
        self.is_admin()
            || self.user_roles.iter().any(|r| r.permissions.contains(p))
            || self.memberships.iter().any(|m| m.permissions.contains(p))
    }

    pub fn has_permission_in_group(&self, g: GroupId, p: &str) -> bool {
        // The M16 helper, with admin override layered on.
        self.is_admin()
            || self.memberships.iter().any(|m| m.group_id == g && m.permissions.contains(p))
    }
}
```

**`platform.user_can_access` fast-path** ([`0008_authz_functions.up.sql`](../../platform/migrations/0008_authz_functions.up.sql)).
A new top-level clause: if the caller holds any user-role grant containing
`'*'`, return `true` before touching `resource_principal`/`resource_share`. This
keeps a plugin's ACL queries (`WHERE … OR user_can_access(...)`) automatically
correct for admins without per-plugin changes.

**`managed_by` audit column** on `group_membership` (and `user_role_assignment`):
a small `TEXT` discriminator (`'manual'`/`'oidc'`/`'config'`) plus an optional
`managed_source` (the OIDC group name or config key). Lets stages C and D
reconcile *their* rows without clobbering manual ones, and lets the admin UI
display provenance.

```sql
ALTER TABLE platform.group_membership
  ADD COLUMN managed_by      TEXT NOT NULL DEFAULT 'manual'
    CHECK (managed_by IN ('manual', 'oidc', 'config')),
  ADD COLUMN managed_source  TEXT;
```

### B — The `admin` plugin

**Decision: a first-party plugin, not host UI.** The platform's design is that
the host has no UI of its own (every page is a plugin's). Modelling admin as a
plugin preserves that and proves the plugin contract is rich enough to host the
platform's own management surface. It also dogfoods the new user-role gating:
the admin plugin's pages are reachable iff the caller's user-role grants the
relevant `admin:*` permission (which the builtin `admin` role does, via `*`).

```
plugins/admin/
├── plugin.toml
├── Cargo.toml
├── build.rs
├── proto/admin/v1/
│   ├── groups.proto                   # GroupAdminService
│   ├── roles.proto                    # UserRoleAdminService
│   ├── users.proto                    # UserAdminService (assign user-roles)
│   ├── permissions.proto              # PermissionCatalogService (read-only)
│   └── oidc.proto                     # OidcMappingService
├── src/
│   ├── lib.rs
│   ├── repo/{group,role,user,oidc}.rs
│   └── service/{groups,roles,users,permissions,oidc}.rs
└── frontend/
    └── src/
        ├── routes/
        │   ├── index.ts
        │   └── pages/
        │       ├── GroupsListPage.tsx
        │       ├── GroupDetailPage.tsx       # name/desc + roles + memberships
        │       ├── UserRolesPage.tsx         # list & edit user-roles + their permissions
        │       ├── UsersPage.tsx             # assign/revoke user-role; see effective perms
        │       └── OidcMappingsPage.tsx
        └── lib/PermissionPicker.tsx          # exposed (cross-plugin reuse)
```

`plugin.toml`:

```toml
[plugin]
name = "admin"
display_name = "Admin"
description = "Manage groups, roles, permissions, and OIDC mappings."

[permissions]
"admin:groups.read"    = "List groups and their roles/members."
"admin:groups.write"   = "Create/edit/delete groups and group roles."
"admin:user_roles.read"  = "List user-roles and their permissions."
"admin:user_roles.write" = "Create/edit user-roles and assign them to users."
"admin:oidc.read"   = "List OIDC group mappings."
"admin:oidc.write"  = "Create/edit OIDC group mappings."

[requires]
capabilities = ["db.read", "db.write", "audit.emit", "platform.admin"]
```

**`platform.admin` is a new capability.** It's the gate the host uses to grant
the admin plugin (and only the admin plugin, in v1) the cross-schema reads/
writes on `platform.group*` / `platform.user_role*` it needs. The capability is
**listed in the trusted-capabilities allowlist** the host checks at boot; any
other plugin declaring `platform.admin` fails `junius check` with a clear
error (extends the M12 ruleset).

**SDK accessor.** Rather than emitting cross-schema SQL grants and letting the
plugin query `platform.*` directly, the host exposes a typed
`PlatformAdminApi` on `PluginResources` (gated by the capability). The plugin
calls methods like `admin.create_group(name, desc)` / `admin.set_role_permissions(role_id, perms)`
/ `admin.assign_user_role(user_id, role_id)`; the API runs inside the host's
pool and emits `platform.audit_event` rows for every mutation. This keeps the
attack surface narrow (a typed API), keeps the role grants small (no new
plugin role with `platform.user_role` write), and gives the audit log a free
"who changed which role-permission, when" trail.

**Permission catalogue.** `PermissionCatalogService.List()` reads the host's
compiled-in plugin metadata (each plugin's `[permissions]` table) and returns
`{ plugin, permission, description }` triples. The admin UI uses this to render
multi-select pickers — no hardcoded permission strings, no risk of a typo
silently creating a never-checked permission. This needs a small host change
(expose `Vec<PermissionDecl>` on the existing plugin registry); the
declarations themselves already exist as of M06.

**Page sketch (`GroupDetailPage`).** Three sections: the group itself (rename/
edit description); a table of roles in the group with their permission set
(each row editable via `PermissionPicker`); a table of members showing
`(user, role, managed_by)` with **manual** rows editable and **oidc**/**config**
rows read-only (badge: "Managed by OIDC group `<x>`" / "Managed by config").
The provenance display is the UX rule that keeps stages C/D from looking like
mysterious overrides.

### C — OIDC group → Junius (group, role) mapping

**Problem.** Authentik already models groups (`organisers`, `members`, …) and
emits them as a `groups: [..]` claim when the application's OAuth provider
includes the `groups` scope mapping. The platform currently ignores it.

**Fix.** Add a mapping table + reconcile on every login.

```sql
-- platform/migrations/0012_oidc_group_mapping.up.sql
CREATE TABLE platform.oidc_group_mapping (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    oidc_group_name  TEXT NOT NULL,
    group_id         UUID NOT NULL REFERENCES platform.group(id)      ON DELETE CASCADE,
    role_id          UUID NOT NULL REFERENCES platform.group_role(id) ON DELETE CASCADE,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (oidc_group_name, group_id)                   -- one mapping per (oidc_group, target_group)
);
CREATE INDEX oidc_group_mapping_name_idx ON platform.oidc_group_mapping (oidc_group_name);
```

**Reconciliation** ([`platform/src/auth/oidc.rs`](../../platform/src/auth/oidc.rs)).
After `upsert_user`, take `claims.additional_claims().get("groups")` (a
`Vec<String>` parsed via `openidconnect`'s `AdditionalClaims` extension) and:

1. Resolve mappings: `SELECT group_id, role_id, oidc_group_name FROM platform.oidc_group_mapping WHERE oidc_group_name = ANY($1)`.
2. **Upsert** memberships: for every mapping the user matches, insert
   `group_membership (user_id, group_id, role_id, managed_by='oidc', managed_source='<oidc_group_name>')`
   (`ON CONFLICT (user_id, group_id) DO UPDATE` only when the existing row is
   already `managed_by='oidc'`; a manual override wins).
3. **Reap stale OIDC memberships**: `DELETE FROM platform.group_membership WHERE user_id = $1 AND managed_by = 'oidc' AND managed_source NOT IN ($current_oidc_groups)`.

Manual memberships are untouched. The reconciler is a single helper
`reconcile_oidc_memberships(user_id, oidc_groups)` so the four trigger points
share one implementation.

**Trigger points (four total):**

1. **Login** — the OIDC callback runs the reconciler with the claim's `groups`.
2. **On-demand REST** — `POST /api/me/refresh-groups`, authenticated. The
   handler refreshes the access token from the session's stored OIDC tokens,
   calls the IdP's userinfo endpoint (cheaper than the admin API; uses
   what the user already has), reconciles, returns the updated `User` JSON.
3. **On-demand Connect-RPC** — `UserService.RefreshOidcGroups` on a new host
   `user.v1` proto. Same body as the REST endpoint; reuses session auth.
4. **CLI sweep** — `junius oidc resync [--all | <user>]` for ops who need to
   force a refresh without a session (e.g. fixing drift batch-style).

Use case for the on-demand endpoints: Authentik fires a webhook on group
membership change → n8n / similar receives it → calls the REST endpoint *as
the affected user* (Authentik holds that user's refresh token via a service
flow). Junius itself doesn't ship the webhook listener; the endpoint surface
is the seam.

> **Caveat (out of scope for v1).** Authenticating an automation client *as*
> an arbitrary user requires either user API tokens (a real feature, not
> yet built) or Authentik service-account impersonation. v1 ships the
> self-refresh endpoint + the CLI sweep; admin-side
> `POST /api/admin/users/<id>/refresh-groups` is left for a follow-up
> milestone alongside user API tokens.

**Authentik blueprint update.** `dev/authentik/blueprints/junius.yaml` gains:
- the `groups` scope mapping in the provider's `property_mappings`;
- two seeded Authentik groups (`junius-organisers`, `junius-members`) with
  Alice/Bob as members, so the dev environment exercises the mapping path
  out-of-the-box.

The `User` struct serialisation gains an `oidcGroups: Vec<String>` field
(populated from the session if available) so the admin UI can show *"this
user's last login carried OIDC groups: …"* — purely advisory; the
authoritative state is `group_membership.managed_source`.

### D — Config-based provisioning

**Problem.** A new deployment needs a starting set of groups, roles, and
permissions before any admin can log in (chicken-and-egg: even the first
`admin` user-role assignment needs an admin to make it, unless it's in config).
Today: `dev-seed.sql` for dev, hand-run SQL for prod.

**Fix.** A declarative `[provisioning]` block — either inline in the deployment
TOML or in a separate file referenced from it — plus a `junius provision`
CLI that applies it.

```toml
# dev/platform.toml (excerpt)
[provisioning]
file = "provisioning.toml"            # optional; otherwise the [[provisioning.groups]] etc. tables go here

# dev/provisioning.toml
[[groups]]
name        = "Organisers"
description = "Dev test group (all events permissions)"

[[groups.roles]]
name        = "organiser"
permissions = [
  "events:read", "events:write", "events:share",
  "greetings:read", "greetings:write",
  "hello:read", "hello:write", "hello:share",
  "widgets:read",
]

[[user_roles]]
name        = "admin"
description = "Full platform admin (managed by config — admin user-role)."
permissions = ["*"]
builtin     = true                    # ensures the row exists; same idempotent INSERT as the migration

[[user_role_assignments]]
oidc_sub    = "<alice's sub>"         # or `email = "alice@example.com"` — see Library choices
user_role   = "admin"

[[oidc_mappings]]
oidc_group  = "junius-organisers"
group       = "Organisers"
role        = "organiser"
```

**CLI.** Two subcommands on `junius`:

- **`junius provision diff --config dev/platform.toml`** — prints the planned
  changes (groups to create / update / delete, roles, permissions,
  assignments, OIDC mappings) against the current DB, **without** applying
  them. The format mirrors `junius migrate up --dry-run`'s tone.
- **`junius provision apply --config dev/platform.toml`** — applies the diff in
  one transaction. All config-owned rows get `managed_by='config'` and a
  `managed_source` of the config entity's name. Re-running is idempotent.

**Reconciliation semantics.** Same shape as OIDC (C):
- `managed_by='config'` rows are owned by the config — drift is reconciled
  *toward* the config (extra perms removed, missing perms added).
- `managed_by='manual'` rows are left alone; the admin UI added them, the
  config doesn't know.
- **Removal** of an entity from the config: in v1, `apply` *warns* and asks for
  `--allow-delete` to actually drop config-managed rows whose entity is no
  longer present. This is the destructive-action guardrail mirroring how the
  M11 deployment workflow gates `cache prune`.

**Boot-time auto-apply (default on; cheap when nothing changed).** The host
runs `provision apply` automatically at start, after migrations. To keep
restarts from spamming the audit log when nothing has changed:

- The provisioning block (groups + roles + permissions + assignments +
  OIDC mappings) is hashed (blake3) at apply time. The hash + the
  applied-at timestamp persist in a new `platform.provisioning_state` row.
- On boot, the host computes the current config's hash; if it matches the
  stored hash, `apply` short-circuits and emits a single
  `provisioning.skipped_unchanged` audit event. No diff, no writes.
- A drift sweep against actual DB state can be forced via
  `junius provision apply --force` (re-applies regardless of hash) or
  `junius provision diff` (read-only, always inspects state).

This makes the boot path cheap on every restart while keeping the
`junius provision apply` CLI semantically explicit when ops need it.
`[provisioning] auto_apply_on_boot = false` opts a deployment out of the
boot-time pass entirely (kept for advanced cases — e.g. an operator who
wants the audit trail anchored at a human-triggered command, not at every
process restart).

**Lock-on-managed (forbids manual edits to config-owned rows).** A new
`[provisioning] lock_managed = true` (default) enforces that:

- Groups / roles / role-permissions / memberships / user-roles /
  user-role-assignments / OIDC mappings with `managed_by='config'` are
  **read-only** through every mutation seam: the `PlatformAdminApi`
  accessor rejects writes with a typed `ManagedByConfig` error, the
  admin UI hides edit affordances and shows a "locked by provisioning"
  badge, and the host's session-level group-membership mutations refuse
  too. Removal is only possible by removing the entity from the config
  + `junius provision apply --allow-delete`.
- A `managed_by='manual'` row that someone wants to bring under config
  control is moved by adding it to the config; `apply` upserts (with the
  unique-key conflict path flipping `managed_by` to `'config'`) and
  audit-emits the takeover.

`lock_managed = false` is the escape hatch: `managed_by='config'` becomes
purely informational (the original v1 plan), and admin UI can edit any
row regardless of provenance. Useful for deployments using the TOML as a
seed rather than a source-of-truth; not the default.

**Dev migration.** `dev/dev-seed.sql` is **deleted** and replaced by
`dev/provisioning.toml`; the dev README points contributors to
`junius provision apply --config dev/platform.toml`. The admin user-role
assignment for `alice@example.com` is in the file so a fresh `task dev:up` ends
with Alice already admin.

## Stages

Repo norm: one **unsigned** commit per stage, `task ci` green per stage,
sign+push the batch at the end. Stages are dependency-ordered (A precedes B
because B reads the user-role schema; C precedes D as written because D writes
OIDC mappings via the same reconciler shape, but C and D can swap if it's
cleaner during the build).

- **Stage 1 — A: user-role schema + admin override.** Migrations `0011`/the
  `managed_by` column; the `user_roles` field on `User`; the session-middleware
  query update; `is_admin` + `has_permission`/`has_permission_in_group` admin
  fall-through; the `platform.user_can_access` fast-path. *Verify:* hand-insert
  an `admin` assignment for alice in psql; alice's `/api/me` lists the user-role;
  alice can read a private event owned by bob (whose group she's not in) — pre-stage
  this would 404. Unit test over a constructed `User` covers the matrix.

- **Stage 2 — B: the `admin` plugin (CRUD + permission catalogue).** Scaffold
  with `junius new plugin admin` (M13 Stage-4 scaffolder + the M16-A
  `sync` auto-wiring once it lands; otherwise the M13 manual buf/pnpm steps).
  Host changes: `platform.admin` capability + allowlist, `PlatformAdminApi`
  accessor, permission-catalogue endpoint on the plugin registry, host frontend
  workspace dep. RPC + frontend pages for groups, group-roles, user-roles,
  user-role assignments, and the permission catalogue picker (no OIDC yet —
  Stage 3). *Verify:* alice (admin user-role from Stage 1) opens `/p/admin`,
  creates a group, adds a role, picks permissions, adds bob with that role;
  bob's next `/api/me` reflects the new group + permissions; the audit log
  shows one event per mutation.

- **Stage 3 — C: OIDC group provisioning.** Migration `0012`; the Authentik
  blueprint update (`groups` scope mapping + two seeded Authentik groups); the
  reconciler in `oidc::callback`; the `OidcMappingService` + admin UI page; the
  `junius oidc resync` CLI. *Verify:* in the Authentik admin UI, add bob to
  `junius-organisers`; bob logs in; bob now appears in the Junius `Organisers`
  group with role `organiser`, `managed_by='oidc'`; the admin UI shows
  bob's membership row as managed-by-OIDC and non-editable. Remove bob from
  the Authentik group, re-login → row disappears. Add a *manual* extra
  membership for bob and confirm the next login doesn't clobber it.

- **Stage 4 — D: config-based provisioning.** TOML schema + `junius provision
  diff`/`apply`; `--allow-delete` guardrail; the optional
  `auto_apply_on_boot`; rewrite `dev/dev-seed.sql` to `dev/provisioning.toml`;
  update [`dev/authentik/README.md`](../../dev/authentik/README.md) and the
  root README dev-bootstrap step. *Verify:* on a wiped DB, `junius migrate up`
  + `junius provision apply --config dev/platform.toml` produces a DB state
  identical (modulo timestamps) to the old `dev-seed.sql` output, and alice is
  admin without any browser action. Re-running `apply` is a no-op. Removing the
  `Organisers` group from config + `apply --allow-delete` removes it (and
  warns if it had manual memberships).

- **Stage 5 — E2E + docs.** Playwright spec under
  `plugins/admin/frontend/e2e/admin-flow.spec.ts` walking Stage-2's flow via
  the M17 fixtures (session-seed an admin, create group, add role, assign
  permission, add member, assert via `/api/me`). Authoring-guide section on
  "the platform admin plugin: managing groups & roles" (read-only orientation
  — most plugin authors don't ship admin pages, but they *do* declare
  permissions that show up in the picker). Decision-log entry.

## Out of scope

- **SCIM / Just-in-Time directory sync beyond OIDC `groups` claim.** Authentik
  is the source of identity; ingesting from a separate IdP-of-IdPs is a much
  larger feature.
- **Bulk import/export of groups & permissions** (CSV/JSON round-trip beyond
  the TOML config). The TOML *is* the canonical form.
- **Per-tenant admin separation.** The platform is single-tenant by design
  (locked in M06); admin is admin-of-the-deployment.
- **Row-level security (RLS) in Postgres.** The capability-gated SDK
  (`PlatformAdminApi`) is the gate; per-row PG policies stay an optional
  hardening pass for a later milestone.
- **Two-factor / WebAuthn step-up for admin actions.** The auth flow stays
  what M06 built; an admin's session cookie is sufficient. A future hardening
  milestone can add re-auth on sensitive actions.
- **Approval workflows / four-eyes admin actions.** Admin mutations land
  immediately; the audit log is the after-the-fact trail.
- **Nested groups.** A Junius group is flat; OIDC group membership is also
  flat (`groups: [...]` is a list, not a tree). A user can hold roles in many
  groups, but groups do not contain groups.
- **Removing the legacy per-group `role_permission` table.** Group-roles stay
  exactly as they are — this milestone *adds* user-roles alongside, it does
  not replace anything.

## Library choices — confirm with user before starting

| Choice | Proposed default | Rationale | Notes |
|---|---|---|---|
| **Wildcard permission encoding** | A literal `'*'` row in `user_role_permission`, plus a fast-path in `has_permission` / `user_can_access` | Simple, greppable; one builtin role inserts it once | Alternative: a `is_admin` boolean column on `user_role`. Rejected — a flag and a wildcard are two ways to say the same thing; pick one. |
| **Admin plugin name** | `admin` (path `/p/admin`) | Mirrors design conventions; short, unambiguous | Alternative: `platform-admin`. Rejected — `platform` is the host's name, prefixing implies a special tier. |
| **Admin SQL access** | A typed `PlatformAdminApi` accessor on `PluginResources`, gated by the new `platform.admin` capability + host allowlist | Keeps cross-schema writes off plugin pools; audit-emits for free; one place to enforce checks | Alternative: emit cross-schema `GRANT`s on `platform.group*` to `role_admin`. Rejected — every mutation would then need restating the audit/auth checks. |
| **Permission catalogue source** | Host's compiled-in plugin metadata via the existing registry | The declarations already exist; no second source of truth | — |
| **Permission picker UI** | A custom `<PermissionPicker>` in `plugins/admin/frontend/src/lib`, grouped by plugin | One screen-friendly multi-select; exposed for cross-plugin reuse | Forms use the M13 `react-hook-form` + `zod` default. |
| **`managed_by` discriminator** | A `TEXT` column with a `CHECK ... IN ('manual','oidc','config')` constraint | Trivially extensible; readable in psql | Alternative: a Postgres enum. Rejected — small set, doesn't justify a migration when a value is added. |
| **OIDC groups claim key** | `groups` (Authentik default) | Matches the Authentik blueprint update in Stage 3 | Configurable in `[config]` (`oidc_groups_claim = "groups"`) for IdPs that name it differently. |
| **OIDC reconciliation triggers** | (1) login, (2) `POST /api/me/refresh-groups`, (3) `UserService.RefreshOidcGroups` Connect-RPC, (4) `junius oidc resync` CLI — single shared reconciler | Login is the minimum correct point; the two on-demand endpoints (REST + RPC) let user-side automation (Authentik webhook → n8n → endpoint) push the refresh without re-login; the CLI is the admin sweep | All four call the same `reconcile_oidc_memberships(user_id, groups)` helper. Admin-side "refresh arbitrary user" stays out of scope (needs user API tokens — future milestone). |
| **Config user identity** | `oidc_sub` preferred, `email` accepted with a warning | `sub` is stable; email can change in the IdP | The `apply` step resolves email → `oidc_sub` via the upserted `platform.user` row, errors if the user has never logged in. |
| **Config file format** | TOML, inline or `[provisioning] file = "…"` pointer | Existing host config is TOML; consistent | — |
| **Boot-time auto-apply** | **On** by default, **hash-guarded** so an unchanged config is a no-op (emits one `provisioning.skipped_unchanged` audit event); opt out via `[provisioning] auto_apply_on_boot = false` | Cheap on every restart; declarative state stays converged automatically; the audit log isn't spammed because the no-diff path doesn't run | `junius provision apply --force` re-applies regardless of hash. State stored in a new `platform.provisioning_state` table (one row per deployment, holds the last hash + apply timestamp). |
| **Lock on `managed_by='config'`** | `[provisioning] lock_managed = true` (default) — config-owned rows are read-only at every mutation seam (`PlatformAdminApi`, admin UI, session reconciler); takeover only via the config | The user's hard guarantee: a deployment that wants the TOML to *be* the truth doesn't have to police drift through the UI; admins literally cannot edit a config-managed group without first removing it from the file | `lock_managed = false` makes `managed_by='config'` purely informational. A separate, typed `ManagedByConfig` error is what the SDK / UI raise. |
| **Destructive provisioning** | Removed entities `WARN` by default; `--allow-delete` required to drop | Mirrors the M11 `cache prune` guardrail; protects against a copy-paste deleting a real group | — |

## Open questions resolved

- **Where does the admin UI live?** — As a first-party plugin (`plugins/admin/`),
  gated by a new `platform.admin` capability + host allowlist. Preserves the
  "host has no UI" architecture and dogfoods the plugin contract.
- **What does a permission catalogue look like?** — A read-only RPC that
  surfaces every plugin's `[permissions]` block from the host's compiled-in
  registry. No second source of truth; no risk of catalogue/manifest drift.
- **How do admins log in for the first time?** — Config-based provisioning
  (Stage 4) assigns the `admin` user-role declaratively, so a fresh deployment
  with one declared admin in TOML doesn't need a SQL bootstrap.
- **What happens to the dev-seed?** — Stage 4 replaces it with
  `dev/provisioning.toml`; the file becomes a worked example of the format.

## Downstream doc updates

- [`README.md`](README.md) milestone index — flip M18 from "needs fleshing out"
  to the full description; update the *"What lands"* / *"Verify by"* cells to
  match this doc's outcome/verification.
- [`../plugin-authoring-guide.md`](../plugin-authoring-guide.md) — note that
  permissions declared in `plugin.toml` automatically appear in the admin
  permission picker; describe how the `admin` user-role bypasses both static
  RPC `requires` gates and per-resource ACLs.
- [`../design/14-decision-log.md`](../design/14-decision-log.md) — an M18 entry
  covering: user-roles + `admin` wildcard; admin-as-plugin with a typed
  capability-gated SDK accessor; OIDC group reconciliation on login;
  config-based provisioning replacing `dev-seed.sql`.
- [`18-M16-authoring-ergonomics.md`](18-M16-authoring-ergonomics.md#out-of-scope)
  — note in the "Out of scope" carve-out that the group/role management UI is
  delivered by M18.
- [`dev/authentik/README.md`](../../dev/authentik/README.md) — update the dev
  bootstrap to (a) call out the new `groups` scope mapping and (b) replace
  the `psql < dev-seed.sql` step with `junius provision apply`.

## Verification

```bash
# 0. Wipe + migrate + provision.
docker compose -f dev/docker-compose.yml down -v && docker compose -f dev/docker-compose.yml up -d
target/release/junius migrate up --config dev/platform.toml
target/release/junius provision diff  --config dev/platform.toml   # prints the planned changes
target/release/junius provision apply --config dev/platform.toml   # applies them

# Alice logs in once (so platform.user exists, then her sub is resolved by Stage 4).
# Re-run `apply` to attach her admin user-role:
target/release/junius provision apply --config dev/platform.toml

# 1. Stage A — admin override.
target/release/junius dev --config dev/platform.toml &
#  - Log in as alice@example.com → /api/me shows userRoles: [{ name: 'admin', permissions: ['*'] }].
#  - Create a *private* event as bob (in a group alice is NOT a member of).
#  - As alice, GET /h/events/ics/e/<bob's private event id> → 200 (admin override).
#  - Without the admin role, the same request would be 404 (M13 verification step 13).

# 2. Stage B — admin UI flow.
#  - As alice, open /p/admin → /p/admin/groups visible (admin:groups.read).
#  - Create group "Marketing", add role "editor", pick permissions: events:read, events:write.
#  - Add bob as 'editor'. bob re-logs → /api/me lists Marketing/editor.
#  - psql: SELECT event_kind, resource_kind FROM platform.audit_event ORDER BY occurred_at DESC LIMIT 5;
#    → admin:group.create, admin:role.create, admin:role.set_permissions, admin:membership.add (etc).

# 3. Stage C — OIDC mapping.
#  - In Authentik admin UI, add bob to group `junius-organisers`.
#  - Define mapping: junius-organisers → Junius "Organisers" → role "organiser" (via /p/admin/oidc).
#  - bob re-logs → /api/me shows Organisers/organiser, managed_by='oidc'.
#  - In /p/admin, bob's Organisers row is non-editable, badged "Managed by OIDC group junius-organisers".
#  - Remove bob from junius-organisers in Authentik, re-log → row disappears.
#  - Manually add bob to Organisers via /p/admin/groups/<id> → managed_by='manual'.
#    Re-log with no OIDC group → row is preserved (manual wins).

# 4. Stage D — config-based provisioning.
#  - Edit dev/provisioning.toml, remove the Marketing group → `provision diff` shows it red.
#  - `provision apply` errors: "config-managed group 'Marketing' would be deleted; re-run with --allow-delete".
#  - `provision apply --allow-delete` removes it; psql confirms; the audit log carries the events.

# 5. Role isolation & negative checks.
#  - As bob (not admin, not in any admin:* permission), GET /p/admin → frontend 403; RPCs → permission denied.
#  - PGUSER=role_admin psql -c "SELECT * FROM platform.user;" → OK (capability-gated grant).
#  - PGUSER=role_events psql -c "SELECT * FROM platform.user_role;" → ERROR (no grant).

# 6. CI gate.
target/release/junius check
cargo test --workspace
pnpm exec playwright test
```

End of M18. The platform is no longer admin-by-`psql`: groups and roles are
created in the browser, OIDC group memberships flow in from Authentik
automatically, and a deployment's initial state ships as checked-in TOML.
Future milestones (third-party plugins, RLS, step-up auth) build on the
`admin` user-role + `platform.admin` capability the host now exposes.
