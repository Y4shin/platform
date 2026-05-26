# 21. M19 — Core platform UIs

> **Status:** 🚧 planned. Sequenced **after [M18](20-M18-group-role-provisioning.md)**:
> the Tier-2 admin pages in this milestone extend the M18 `admin` plugin and
> consume its `platform.admin` capability + permission catalogue. Independent
> of M14/M15/M16/M17 otherwise.
>
> **Scope handoff to [M20](22-M20-app-navigation.md):** the nav redesign
> (auto-derived, permission-filtered plugin nav + the dashboard's
> per-plugin tile grid) **moved to M20**, which models the nav unit as a
> plugin-declared *app* (not a plugin) and lands a left-rail picker + sub-nav
> rail. M19 therefore keeps the hand-edited M04 `NavLinks` in place and
> ships the dashboard as a greeting + zero-permissions empty state only —
> tiles arrive in M20 once `[[plugin.apps]]` exists.

One-line goal: the host frontend is no longer a chrome around plugin pages —
it gains the **user-facing surfaces every web app needs** (a real dashboard,
a logout control, a profile page, permission-filtered navigation,
permission-denied/error pages) and the **operator surfaces a real deployment
needs** (audit log, job runs, plugin manager, system health).

## Why this milestone exists

The host frontend has been an *intentional* skeleton through M04–M18:
every feature milestone added what it needed and nothing more. That was the
right call — but it leaves several glaring gaps the M13 build and the M18
plan surfaced together:

- **`/` renders literal `null`** ([routes.ts:16](../../platform/frontend/src/generated/routes.ts#L16)).
  A freshly-logged-in user lands on an empty page.
- **There is no logout control.** `POST /api/auth/logout` exists, but nothing
  in the UI calls it. Logging out requires `curl`.
- **There is no profile page.** A user can't see which groups they're a member
  of, which permissions they hold, what locale is set, or which OIDC subject
  is bound to their account — *all of which the host already has*, served
  raw by `/api/me`.
- **Navigation is hand-edited.** [`NavLinks.tsx`](../../platform/frontend/src/layout/NavLinks.tsx)
  hardcodes `Home` and `Events`. Every new plugin needs a manual edit, and
  links to plugins the viewer has no permissions for are still rendered (M13
  friction row #36 already flagged that `requirePermissions` is an M04
  no-op).
- **There is no real 403 page.** A permission denial in an RPC redirects to
  login (because the redirect path is the only one wired); a frontend route
  guard does nothing.
- **A new OIDC user lands on an unusable app.** No permissions ⇒ no plugins
  in the nav ⇒ empty page ⇒ no explanation. Onboarding is "ask the admin to
  SQL you a membership."
- **The platform has no operator UI.** `platform.audit_event` is filled by
  every plugin mutation. `platform.job_run` (M10) tracks every job. The
  host's compiled-in plugin registry knows every plugin's version, declared
  permissions, required capabilities, and exposed tables. **None of this is
  visible in a browser.** An operator who wants to debug a failed job runs
  `psql`.

M19 closes both surfaces in one milestone because they share the same
chrome (the host `Shell`, the auto-derived nav, the page templates) and
because the operator pages dovetail into the M18 `admin` plugin that just
shipped — extending it is cheaper than building a parallel surface.

## Outcome / acceptance

**Tier 1 — every user.**

- The `/` route renders a **dashboard**: a greeting and — when the viewer
  holds zero plugin permissions — a clear empty state with the configured
  admin contact. (The permission-filtered **tile grid** lands in
  [M20 Stage 4](22-M20-app-navigation.md#stages) once `[[plugin.apps]]` is
  the source of truth.)
- The header carries a **user menu** with the viewer's display name + email,
  a link to `/me`, and a **Sign out** action that calls
  `POST /api/auth/logout` and routes to login.
- `/me` is the **profile page**: identity (display name, email, OIDC sub —
  read-only), locale preference (editable; replaces the header switcher),
  a table of group memberships (group, role, permissions count,
  `managed_by` badge), a table of user-role assignments (after M18),
  and active sessions with a revoke control.
- **403 and 500 pages exist** and are reached when an RPC returns
  `PermissionDenied` or any unexpected error escapes the route tree; the
  403 page names the missing permission(s) where known.
- A user with **zero permissions** sees the empty-state dashboard (no
  broken links, no redirect loops) and an instructional message rather than
  an empty pane.

**Tier 2 — admins (`admin` user-role from M18, or `admin:*.read`).**

- `/p/admin/audit` browses `platform.audit_event` with filters (actor,
  resource kind, event kind, date range), pagination, and a JSON details
  view.
- `/p/admin/jobs` browses `platform.job_run` with status filters
  (queued / running / succeeded / failed / retrying), per-run attempt
  history, and the captured error.
- `/p/admin/plugins` is a **read-only plugin inventory**: name, version,
  declared permissions (each with a count of users holding it), required
  capabilities, exposed/private tables, frontend present? Pulled from the
  host's compiled-in registry — *not* the manifest files on disk.
- `/p/admin/health` shows live status of Postgres (reachable, migrations
  ahead/behind, pool stats), the OIDC issuer (last successful discovery),
  the job worker (alive, queue depth, recent failure rate), the email
  transport (configured + last successful send), and storage (bucket
  reachable). One row per dependency, green/amber/red.

**Both tiers.**

- All new strings ship through the M14 i18n seam (Lingui).
- All new pages have a Playwright spec under the M17 harness (host pages in
  `e2e/cross/`, admin pages in `plugins/admin/frontend/e2e/`).

## Design

### Tier 1.A — `Shell` chrome: user menu + logout

[`Shell.tsx`](../../platform/frontend/src/layout/Shell.tsx) currently renders
the display name as a static `<span>`. M19 replaces it with a
`<UserMenu>` dropdown:

```tsx
// platform/frontend/src/layout/UserMenu.tsx (new)
<DropdownMenu>
  <DropdownMenuTrigger>
    {user.displayName} <ChevronDown />
  </DropdownMenuTrigger>
  <DropdownMenuContent>
    <DropdownMenuLabel>{user.email}</DropdownMenuLabel>
    <DropdownMenuItem asChild><Link to="/me">Profile</Link></DropdownMenuItem>
    <DropdownMenuSeparator />
    <DropdownMenuItem onSelect={signOut}>Sign out</DropdownMenuItem>
  </DropdownMenuContent>
</DropdownMenu>
```

`signOut` is a small SDK helper next to `goToLogin`:

```ts
// packages/sdk/src/auth/signOut.ts
export async function signOut(): Promise<void> {
  await fetch('/api/auth/logout', { method: 'POST', credentials: 'include' });
  window.location.assign('/api/auth/login');   // back to a fresh session
}
```

The `LocaleSwitcher` moves out of the header and into the profile page
(Tier 1.C); the header becomes brand + nav + user menu.

A new `Menu` / `DropdownMenu` component lands in `@junius/design`, wrapping
`@radix-ui/react-dropdown-menu` (already a transitive dep of the design
package — see `packages/design/node_modules/@radix-ui/`). This is the first
non-trivial host-side menu primitive; reusable for any plugin's row-action
menus later.

### Tier 1.B — Nav (deferred to [M20](22-M20-app-navigation.md))

Originally this milestone was going to ship a permission-filtered,
manifest-driven header `NavLinks` keyed on **plugin** (one entry per
plugin). On reflection — see the M20 *Why this milestone exists* section
— the unit is wrong: the M18 admin plugin has five operator surfaces
that should be peers in the picker, and the events plugin has at least
two (events + calendar feeds). M20 lands a **left-rail picker** keyed on
**app** (one entry per `[[plugin.apps]]` declaration) with sub-nav
underneath.

M19 therefore **leaves the M04 hardcoded `NavLinks` in place**. That is
deliberate: shipping a permission-filtered header here and then
replacing it in M20 is throwaway work. The hand edit (adding a `/p/admin`
link when M18 lands) is a one-time tax.

### Tier 1.C — `/me` profile page

A new host route `/me` (under `authedLayoutRoute`):

```
┌─────────────────────────────────────────────────────────────┐
│ Profile                                                     │
├─────────────────────────────────────────────────────────────┤
│  Identity                                                   │
│    Display name   Alice Example         (from Authentik)    │
│    Email          alice@example.com     (from Authentik)    │
│    OIDC subject   abc123…               (read-only)         │
│                                                             │
│  Preferences                                                │
│    Language       [English ▾]           (saves on change)   │
│                                                             │
│  Group memberships                                          │
│    Organisers     organiser    manual    7 permissions      │
│    Marketing      editor       OIDC      2 permissions      │
│                                                             │
│  User-roles                            (visible after M18)  │
│    admin          * (wildcard)          assigned 2026-03-…  │
│                                                             │
│  Sessions                                                   │
│    Current        Chrome / Linux        last seen 12:01     │
│    Older          Firefox / macOS       [Revoke]            │
└─────────────────────────────────────────────────────────────┘
```

Host endpoints needed (extending `platform/src/auth/`):

- `GET /api/me` already returns identity + memberships + user-roles (after
  M18). Add `oidcSub` + a `sessions` array (id, user_agent, last_seen).
- `POST /api/me/locale` exists (M14).
- `DELETE /api/sessions/<id>` revokes a session; the current session is
  protected from being revoked except via the dedicated *Sign out*
  affordance (avoids "I just locked myself out by clicking the wrong row").

No plugin-side code is needed — this lives entirely in the host frontend
because it queries `/api/me` and `/api/sessions/*`.

### Tier 1.D — Real dashboard at `/`

The `/` route currently renders `null`. Replace it with `<DashboardPage>`:

```
┌─────────────────────────────────────────────────────────────┐
│ Welcome back, Alice                                         │
├─────────────────────────────────────────────────────────────┤
│  (the app-tile grid lands in M20 once [[plugin.apps]] is    │
│   the source of truth; until then, just the greeting and    │
│   the empty-state path below)                               │
└─────────────────────────────────────────────────────────────┘
```

- **Tile grid** → owned by [M20 Stage 4](22-M20-app-navigation.md#stages);
  the data source is `APPS` (one tile per app), so a plugin with two
  apps gets two tiles. M19 ships only the **greeting** and the
  **empty-state** path.
- Empty-state ⇒ render `<NoAccessEmptyState>`:
  > Your account doesn't have access to anything yet. Contact your
  > administrator (`config.admin_contact_email`) to be added to a group.
- A new `[config] admin_contact_email` is read from `platform.toml`;
  optional but recommended. (Also surfaced on the M19 403 page.)

The dashboard is intentionally simple in v1 — *every* app has equal
weight; no "recent activity" / "starred" / "pinned" features. Those follow
real usage data, per [00-approach.md §0.6](00-approach.md#06-what-this-plan-does-not-cover).

### Tier 1.E — 403, 500, and error-boundary

**403 (permission denied).** Add `/403` host route rendering `<ForbiddenPage>`
that, when navigated via `useNavigate({ to: '/403', state: { missing: [...] } })`,
names the missing permissions in the body. Three callers:

1. `queryClient.onError` ([main.tsx](../../platform/frontend/src/main.tsx))
   gains a branch: on `Code.PermissionDenied`, redirect to `/403` (currently
   only `Code.Unauthenticated` is handled, via `goToLoginUnlessPublic`).
2. The M13-friction `requirePermissions` route guard becomes a real check
   (no longer a no-op): if the viewer lacks the listed permissions, it
   throws a `notFound`/redirect to `/403` *before* the page renders.
3. The dashboard / nav already filters by permission, so the common case
   (a user clicking a link they don't have access to) doesn't occur — 403
   is reserved for direct-URL navigation and stale links.

**500 (uncaught error).** A new top-level `<RouteErrorBoundary>` wraps the
authed layout's `<Outlet />`. It catches render-time errors and (in dev)
shows the stack; in prod, a friendly card with a `correlation_id` (pulled
from the tracing context — M10) for ops to grep the logs.

**404.** Already exists ([notFound.tsx](../../platform/frontend/src/router/notFound.tsx));
keep the existing card but route to it explicitly from `requirePermissions`
when the design says "404 to avoid existence leaks" (e.g. M08's
private-resource rule), reserving 403 for permission-denied on *known*
resources.

### Tier 2 — admin pages on the M18 `admin` plugin

The M18 milestone introduces `plugins/admin/` for groups/roles/users/
OIDC mappings. M19 *extends* it with four read-mostly operator pages.
They live in the same plugin (not a sibling) because they share the
`platform.admin` capability and the host accessor it gates.

```
plugins/admin/
├── proto/admin/v1/
│   ├── …                              # existing (groups/roles/users/oidc)
│   ├── audit.proto                    # NEW: AuditService
│   ├── jobs.proto                     # NEW: JobsAdminService
│   ├── plugins.proto                  # NEW: PluginInventoryService
│   └── health.proto                   # NEW: HealthService
├── src/
│   ├── repo/{audit,job,plugin_inv,health}.rs  # NEW
│   └── service/{audit,jobs,plugins,health}.rs # NEW
└── frontend/src/routes/pages/
    ├── …                              # existing M18 pages
    ├── AuditPage.tsx                  # NEW
    ├── JobsPage.tsx                   # NEW
    ├── PluginsPage.tsx                # NEW
    └── HealthPage.tsx                 # NEW
```

All four read from data the host or M10 already write; no new tables.

#### Tier 2.A — Audit log viewer (`/p/admin/audit`)

A paginated table over `platform.audit_event`. RPC:

```proto
service AuditService {
  rpc List (ListAuditRequest) returns (ListAuditResponse)
    { option (platform.requires) = "admin:audit.read"; }
}
message ListAuditRequest {
  optional string actor_user_id  = 1;
  optional string resource_kind  = 2;   // e.g. 'events:event'
  optional string event_kind     = 3;   // e.g. 'events:event.create'
  optional string starts_at      = 4;   // RFC3339
  optional string ends_at        = 5;
  optional string cursor         = 6;   // opaque (created_at + id)
  int32           limit          = 7;   // default 50, max 200
}
```

UI: filter bar + table (when, actor, event_kind, resource_kind →
`resource_id`); click into a row to open a drawer with the full JSON
`details`. New permission `admin:audit.read` (added to M18's permission
list and granted by the builtin `admin` user-role's `'*'`).

#### Tier 2.B — Job-run inspector (`/p/admin/jobs`)

Reads `platform.job_run` (M10). Same shape as the audit page: filter +
table + per-row drawer. Drawer shows attempts (M10 records `attempt_count`,
`last_error`), enqueued/started/finished timestamps, and the serialized
job args. **No retry/cancel control in v1** — read-only viewer.

New permission `admin:jobs.read`. (`admin:jobs.write` is reserved for a
future hands-on operator UI; out of scope here.)

#### Tier 2.C — Plugin inventory (`/p/admin/plugins`)

A read-only table of every enabled plugin, sourced from the host's
compiled-in registry (the same data M18's `PermissionCatalogService` reads).
Columns:

- Name + display name
- Version (from the plugin's `Cargo.toml` — already exposed by the
  registry to power `junius plugin info`)
- Declared permissions + a per-permission **holder count** (join through
  `role_permission`/`user_role_permission` to count distinct users)
- Required capabilities (`db.read`, `audit.emit`, `platform.admin`, …)
- Exposed tables / private tables
- Frontend present? (boolean — does the plugin's package.json exist?)

RPC `PluginInventoryService.List(Empty) → ListPluginsResponse`. Permission
`admin:plugins.read`.

**Explicit non-goal:** no enable/disable toggle. Toggling a plugin is a
deploy-time concern owned by [`junius sync`](../../tools/junius/src/commands/sync.rs)
+ a `platform.toml` edit (M11); surfacing it as a button would create a
divergence between the file and the running state. The page links to the
file path for the operator to edit.

#### Tier 2.D — System health (`/p/admin/health`)

A live status panel polling once on mount + every 30s. RPC
`HealthService.Check(Empty) → HealthReport`:

```proto
message HealthReport {
  Dep postgres = 1;
  Dep oidc     = 2;
  Dep jobs     = 3;
  Dep email    = 4;
  Dep storage  = 5;
}
message Dep {
  string status = 1;        // 'ok' | 'degraded' | 'down' | 'not_configured'
  string detail = 2;        // human-readable: "12/12 migrations applied", "last poll 3s ago"
  optional string error = 3;
}
```

The host already does most of these checks at boot (per M06: OIDC discovery
is "best-effort"; per M10: job worker, S3 client, email transport). M19
factors them into a `HostHealth` service queryable on demand. Where a real
ping is cheap (Postgres `SELECT 1`, S3 `HeadBucket`) it runs each call;
where it's not (OIDC discovery), the cached last-success timestamp is used.

Permission `admin:health.read`.

## Stages

Repo norm: one **unsigned** commit per stage, `task ci` green per stage,
sign+push the batch at the end. Stages 1–4 are Tier 1 (host); stages 5–6
are Tier 2 (admin plugin extension); stage 7 wraps up.

- **Stage 1 — Shell + user menu + logout.** `@junius/design/Menu`
  (Radix-backed); `<UserMenu>` in the host shell; `signOut` helper in
  `@junius/sdk/auth`; move `LocaleSwitcher` out of the header (it'll
  reappear on `/me` in Stage 2). *Verify:* the header shows a dropdown
  with the user's name; "Sign out" terminates the session and lands on
  the Authentik login page.

- **Stage 2 — `/me` profile page.** New host route; `/api/me` extended to
  return `oidcSub` + sessions; `DELETE /api/sessions/<id>` endpoint;
  re-home `LocaleSwitcher` as a profile field. *Verify:* `/me` renders
  identity (read-only), the locale switcher updates user.locale, the
  memberships table lists the user's groups with `managed_by` badges
  (post-M18), revoking a non-current session forces that browser to
  re-login on its next request.

- **Stage 3 — Dashboard skeleton + zero-permissions empty state.**
  `<DashboardPage>` replaces the null `/` route; the page renders a
  greeting and routes the zero-permission case to `<NoAccessEmptyState>`
  reading `[config] admin_contact_email`. **No tile grid here** — the
  permission-filtered tile grid (and the manifest-driven nav it shares a
  data source with) lands in [M20 Stage 4](22-M20-app-navigation.md#stages).
  *Verify:* a fresh OIDC user lands on the empty state with the
  configured contact email; alice lands on the greeting (no tiles yet).

- **Stage 4 — 403 / 500 / error boundary.** `<ForbiddenPage>` at `/403`
  (takes a list of missing permissions); `<RouteErrorBoundary>` wrapping
  the authed layout; `requirePermissions` becomes a real route guard;
  `queryClient.onError` routes `PermissionDenied` → `/403` (vs.
  `Unauthenticated` → login). *Verify:* `requirePermissions(['events:write'])`
  on a route the viewer can't write blocks navigation and shows the
  missing permission; throwing inside a component renders the boundary
  with a correlation id, not a white screen.

- **Stage 5 — Audit log viewer.** Admin plugin: `AuditService` +
  `AuditPage`; new permission `admin:audit.read` granted by `admin`
  user-role. *Verify:* every action from M18's Stage 2 verification flow
  (create group, add role, assign permission, add member) appears in
  `/p/admin/audit` with the actor + resource; filters narrow it; the
  drawer shows the JSON details.

- **Stage 6 — Jobs + plugin inventory + health.** The three remaining
  admin pages; permissions `admin:jobs.read` / `admin:plugins.read` /
  `admin:health.read`. *Verify:* failing a job (e.g. by killing the SMTP
  container) surfaces in `/p/admin/jobs` with the captured error;
  `/p/admin/plugins` lists every enabled plugin with accurate permission
  holder counts; `/p/admin/health` flips email to *down* when mailpit
  stops, back to *ok* when it returns.

- **Stage 7 — E2E + docs.** Playwright specs under `e2e/cross/host-ui/`
  (Tier 1: login → dashboard → /me → sign-out) and
  `plugins/admin/frontend/e2e/operator/` (Tier 2: audit + jobs +
  health). Authoring-guide section "what the host already gives you for
  free" so plugin authors don't accidentally re-implement profile/nav
  surfaces. Decision-log entry.

## Out of scope

- **An onboarding wizard.** Setting up Authentik / a first deployment is
  a separate concern; M11 owns the deployment workflow and M18's config
  provisioning seeds the initial admin.
- **In-app notifications / activity feed.** Requires a notification
  infrastructure the platform hasn't built (channels, read/unread state,
  real-time push). Defer until at least one plugin has an in-domain
  driver for it.
- **Search across plugins.** A global search bar requires every plugin to
  expose a search RPC; the host has no aggregator. Deferred.
- **Theme toggle (light/dark).** Design tokens exist (`packages/design/src/theme.css`)
  but a runtime toggle adds a persisted preference + the SSR-equivalent
  flicker dance. Tier 3 — not this milestone.
- **Branded login landing page.** The current immediate redirect to
  Authentik is fine for v1. A branded interstitial ("Sign in to Junius")
  is Tier 3.
- **Nav redesign / app launcher.** The hand-edited `NavLinks` and the
  dashboard tile grid both **move to [M20](22-M20-app-navigation.md)**,
  which lands a manifest-declared `[[plugin.apps]]` model + a left-rail
  picker + sub-nav rail. M19 leaves the M04 nav alone on purpose — see
  the Tier 1.B section above.
- **Job retry / cancel from the UI.** The viewer is read-only — operators
  should re-enqueue via code paths that own the invariants. A hands-on
  jobs admin is a later milestone with a clear safety story.
- **About / version page.** `junius plugin info` covers it from the CLI;
  the inventory page (Stage 6) shows per-plugin versions. A separate
  page for the host's own version is polish; if needed, surface it in
  the user-menu footer.
- **Avatar images.** Initials only. Once a `users` plugin (or a
  user-storage capability) exists, this becomes a manifest-driven add.

## Library choices — confirm with user before starting

| Choice | Proposed default | Rationale | Notes |
|---|---|---|---|
| **Menu primitive** | `@radix-ui/react-dropdown-menu` wrapped as `@junius/design/Menu` | Already a transitive dep; unstyled + a11y-correct; one wrapper is reusable | Alternative: hand-rolled. Rejected — focus management + ARIA roles are work no one wants to redo. |
| **Icons** | `lucide-react` (host frontend dep) | Used by the user menu (chevron, sign-out icon) and the operator pages (status indicators in `/p/admin/health`, etc.); M20 reuses it for app icons | Alternative: inline SVGs only. Rejected — keeps spreading. |
| **Profile-page sessions** | List + per-row Revoke (non-current sessions only); current session signs out via the user menu | Avoids accidental self-lockout while still letting "log me out everywhere" work in two clicks | A "Sign out everywhere" button (revoke all but current) is a nice add but deferred until requested. |
| **403 missing-permission rendering** | The page receives the missing permission names; renders them when present, falls back to a generic copy otherwise | Actionable for the user; works even when the caller doesn't know which permission tripped (e.g. an RPC layer denial) | — |
| **Error boundary correlation id** | Pulled from the existing tracing context (M10) and shown to the user; the page also calls `tracing::error!` so it lands in logs | Matches the design's tracing-first posture | — |
| **Audit/Jobs pagination** | Cursor-based (opaque cursor = `(occurred_at, id)`) | `OFFSET`/`LIMIT` over a busy audit table is the textbook footgun | — |
| **Health endpoint cadence** | On mount + 30s polling while the page is in view (`document.visibilityState`); no SSE/WebSocket | Simple; matches the "no live infra" v1 posture | A websocket push channel is a later add if/when it has more than one consumer. |
| **`admin_contact_email`** | New optional field in `platform.toml [config]`; surfaced on the empty-state dashboard and the no-access 403 page | Lets a deployment tell users *who* to ping; degrades gracefully if absent | — |
| **Plugin holder-counts source** | Live SQL `COUNT(DISTINCT user_id)` joins per page load | Inventory page is admin-only and infrequently visited; not worth caching | If it gets slow, materialize per-permission counts in a view. |

## Open questions resolved

- **Does the host get a UI?** — Yes, but only the **chrome and the
  cross-cutting surfaces** (shell, dashboard, profile, nav, error pages).
  Domain UI stays in plugins; admin/operator UI lives in the M18 `admin`
  plugin.
- **Where does the audit log live?** — In the admin plugin, extending the
  M18 surface. *Not* in a new `audit` plugin and *not* in the host —
  same capability (`platform.admin`), same gate.
- **Should plugin nav be auto-derived?** — Yes, but **not in M19**.
  Auto-derivation + the left-rail picker + sub-nav are owned by
  [M20](22-M20-app-navigation.md), which models the nav unit as a
  manifest-declared *app* rather than a plugin. The M04 hardcoded
  `NavLinks` stays in place through M19.
- **Does the dashboard show "recent activity" / personalized content?** —
  No, not in v1. Equal-weight tiles (added in M20); iterate based on
  real usage.

## Downstream doc updates

- [`README.md`](README.md) milestone index — add the M19 row; flip
  "Verify by" to the dashboard / 403 / audit-view checks below.
- [`14-M13-events-plugin.friction.md`](14-M13-events-plugin.friction.md) —
  close the `requirePermissions` row (#36): "fixed in M19 Stage 4 — real
  route guard, routes to `/403` with missing permissions."
- [`20-M18-group-role-provisioning.md`](20-M18-group-role-provisioning.md) —
  add the four new admin permissions (`admin:audit.read`,
  `admin:jobs.read`, `admin:plugins.read`, `admin:health.read`) to the
  `admin` plugin's manifest table; note in the open-questions section that
  M19 fills out the admin plugin with operator pages.
- [`../plugin-authoring-guide.md`](../plugin-authoring-guide.md) — new
  section: *"what the host provides for free"* (profile, sign-out, nav,
  error pages) so plugin authors don't re-implement; reference the
  `[nav]` manifest fields.
- [`../design/14-decision-log.md`](../design/14-decision-log.md) — M19
  entry covering: dashboard at `/`, `/me` profile, manifest-driven nav,
  the 403/500 surfaces, the four admin operator pages.

## Verification

```bash
# Bootstrap (after M18 has shipped).
docker compose -f dev/docker-compose.yml up -d
target/release/junius migrate up    --config dev/platform.toml
target/release/junius provision apply --config dev/platform.toml   # M18

# Run.
target/release/junius dev --config dev/platform.toml &

# ─── Tier 1 ──────────────────────────────────────────────────────────────

# 1. Header user menu + logout.
#  - Log in as alice@example.com (admin user-role from provisioning).
#  - Header shows "Alice Example ▾"; opening reveals email + Profile + Sign out.
#  - Click Sign out → /api/auth/logout fires → lands on Authentik login.

# 2. /me profile page.
#  - Log in again; navigate to /me.
#  - Identity section: display name, email, OIDC sub — all read-only.
#  - Locale switcher updates user.locale (POST /api/me/locale, visible in /api/me).
#  - Memberships table lists Organisers / Marketing with managed_by badges
#    (manual / OIDC) per Stage-C of M18.
#  - User-roles section lists 'admin' (wildcard).
#  - Sessions table lists the current session (no Revoke); open a 2nd browser,
#    log in, refresh /me → second session listed; Revoke → that browser
#    redirects to login on its next request.

# 3. Dashboard skeleton + zero-permissions empty state.
#  - Header nav remains the hardcoded M04 NavLinks (Home + Events); a hand
#    edit adds a /p/admin link when M18 is enabled. Auto-derivation lands
#    in M20.
#  - As alice, / renders the greeting "Welcome back, Alice" only (no tile
#    grid — that's M20 Stage 4).
#  - Log in as a fresh OIDC user with zero permissions (create in Authentik,
#    log in once, do NOT add to a group): / renders the empty-state card
#    naming admin_contact_email.

# 4. 403 / 500 / error boundary.
#  - As bob, navigate directly to /p/admin → /403 page names "admin:groups.read"
#    (or the first missing permission) and offers a link Home.
#  - As bob, trigger an RPC the server denies (e.g. ShareEvent on someone
#    else's event) → /403 again, same shape.
#  - Force an uncaught exception in a route component (dev-only test page)
#    → error boundary renders with a correlation id; the same id is in the
#    juniusd log.

# ─── Tier 2 (admin) ──────────────────────────────────────────────────────

# 5. Audit log viewer.
#  - As alice, /p/admin/audit lists every M18-Stage-2 mutation done so far.
#  - Filter event_kind = 'admin:membership.add' → only those rows.
#  - Click a row → drawer shows JSON details (target user, role, etc.).

# 6. Job-run inspector.
#  - Sign someone up via the events invite page → a SendSignupConfirmation
#    job runs; /p/admin/jobs lists it as succeeded.
#  - Stop mailpit; sign up again → the new job appears as 'failed' or
#    'retrying'; drawer shows the captured SMTP error and attempt count.

# 7. Plugin inventory.
#  - /p/admin/plugins lists events, hello, greetings, widgets, admin (and
#    any others enabled) with the right versions, declared permissions, and
#    holder counts (alice = 1 for admin:*; the events perms count = members
#    of the Organisers group).

# 8. System health.
#  - /p/admin/health → Postgres OK, OIDC OK, Jobs OK, Email OK, Storage OK.
#  - Stop mailpit → Email flips to 'down' within 30s.
#  - Restart mailpit → Email returns to 'ok' on the next poll.

# ─── CI gate ─────────────────────────────────────────────────────────────

target/release/junius check
cargo test --workspace
pnpm exec playwright test          # exercises host UI + admin operator pages
```

End of M19. The host frontend is a real product surface: a user signs in,
sees a dashboard, finds their work, manages their profile, and signs out —
and an operator has the read-only window into the platform's running state
that used to require `psql`. Future polish (theme toggle, app launcher,
notifications) builds on this chrome rather than fighting it.
