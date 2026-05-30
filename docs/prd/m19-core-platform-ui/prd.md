---
kind: feature
title: Core platform UIs
slug: m19-core-platform-ui
epic: platform-ux-foundations
milestone: M19
prd_issue: 1
slices: [2, 3, 4, 5, 6, 7, 8, 9]
status: issues-created
---

# Core platform UIs

> Migrated from the M19 milestone doc
> [`docs/impl/21-M19-platform-ui.md`](../../impl/21-M19-platform-ui.md), which holds the
> **full design** (per-section ASCII mockups, proto sketches, endpoint lists). This PRD is
> the planning surface; it links back rather than duplicating. Sequenced **after M18** — the
> Tier-2 admin pages extend the M18 `admin` plugin and consume its `platform.admin`
> capability + permission catalogue.

## Problem / why

The host frontend has been an *intentional* skeleton through M04–M18: every feature
milestone added what it needed and nothing more. That leaves glaring gaps in the surfaces
**every web app needs** and the operator surfaces **a real deployment needs**:

- `/` renders literal `null` — a freshly-logged-in user lands on an empty page.
- There is **no logout control** in the UI; `POST /api/auth/logout` exists but only `curl`
  reaches it.
- There is **no profile page** — a user can't see their groups, permissions, locale, or the
  bound OIDC subject, all of which `/api/me` already serves raw.
- There is **no real 403 page** — a permission denial redirects to login; a frontend route
  guard (`requirePermissions`) is an M04 no-op.
- A **new OIDC user with zero permissions** lands on an unusable, unexplained empty app.
- The platform has **no operator UI**: `platform.audit_event`, `platform.job_run` (M10), and
  the compiled-in plugin registry are all invisible in a browser. Debugging a failed job
  means `psql`.

M19 closes the user surface and the operator surface in one milestone because they share the
same chrome (host `Shell`, page templates) and because the operator pages dovetail into the
M18 `admin` plugin — extending it is cheaper than a parallel surface.

**Scope handoff to M20:** the nav redesign (auto-derived, permission-filtered plugin nav +
the dashboard tile grid) **moved to M20**, which models the nav unit as a plugin-declared
*app*. M19 keeps the hand-edited M04 `NavLinks` and ships the dashboard as greeting +
zero-permissions empty state only.

## User stories

**Tier 1 — every user**

- As a signed-in user I land on a **dashboard** at `/`: a greeting, plus (when I hold zero
  plugin permissions) a clear empty state naming the configured admin contact.
- As a user I have a header **user menu** showing my display name + email, a link to `/me`,
  and a **Sign out** action that ends my session and returns me to login.
- As a user I have a **profile page** at `/me`: identity (display name, email, OIDC sub —
  read-only), an editable locale preference, my group memberships (group, role, permission
  count, `managed_by` badge), my user-role assignments, and my active sessions with a revoke
  control.
- As a user who hits a permission-denied or an unexpected error, I see a real **403 / 500
  page** (the 403 names the missing permission(s) where known) instead of a redirect loop or
  white screen.

**Tier 2 — admins** (`admin` user-role from M18, or `admin:*.read`)

- As an admin I browse the **audit log** at `/p/admin/audit` with filters (actor, resource
  kind, event kind, date range), pagination, and a JSON details view.
- As an admin I browse **job runs** at `/p/admin/jobs` with status filters, per-run attempt
  history, and the captured error (read-only — no retry/cancel in v1).
- As an admin I see a read-only **plugin inventory** at `/p/admin/plugins`: name, version,
  declared permissions (with per-permission holder counts), required capabilities,
  exposed/private tables, frontend-present flag — sourced from the host's compiled-in
  registry, not on-disk manifests.
- As an admin I see live **system health** at `/p/admin/health`: one green/amber/red row per
  dependency (Postgres, OIDC issuer, job worker, email transport, storage).

## End-to-end behaviour

A user signs in via Authentik and lands on the dashboard greeting. The header carries their
name in a dropdown; opening it reveals their email, a Profile link, and Sign out. They open
`/me`, change their language (persisted via `POST /api/me/locale`), see their group
memberships and user-roles, and revoke an old session — that browser is forced to re-login on
its next request. They sign out from the header and land back on the Authentik login page. A
brand-new OIDC user with no group sees the empty-state dashboard naming the admin contact
rather than a blank pane. Navigating directly to a page they lack permission for lands them
on `/403` naming the missing permission.

An admin opens `/p/admin/audit` and sees every mutation (create group, assign role, add
member) with actor + resource; filters narrow the table and a row drawer shows the JSON
`details`. `/p/admin/jobs` shows a confirmation-email job as *succeeded*; killing mailpit and
retrying surfaces a *failed*/*retrying* run with the captured SMTP error. `/p/admin/plugins`
lists every enabled plugin with accurate versions and permission-holder counts.
`/p/admin/health` shows all dependencies green, flips email to *down* within 30s of mailpit
stopping, and back to *ok* on the next poll after it returns.

## Layers touched

Full design per section in
[`21-M19-platform-ui.md` §Design](../../impl/21-M19-platform-ui.md#design).

**Tier 1 — host frontend + auth (`platform/frontend/`, `platform/src/auth/`, `packages/`)**

- `@junius/design/Menu` — new Radix-backed (`@radix-ui/react-dropdown-menu`) dropdown
  primitive; `lucide-react` icons.
- `<UserMenu>` in the host `Shell`; `signOut` helper in `@junius/sdk/auth`; `LocaleSwitcher`
  moves out of the header onto `/me`.
- New `/me` host route; `/api/me` extended with `oidcSub` + a `sessions` array;
  `DELETE /api/sessions/<id>` (current session protected from revoke).
- `<DashboardPage>` replaces the null `/` route; `<NoAccessEmptyState>` reading a new
  optional `[config] admin_contact_email` from `platform.toml`.
- `<ForbiddenPage>` at `/403` (takes missing-permission list); `<RouteErrorBoundary>` around
  the authed layout with a correlation id; `requirePermissions` becomes a real route guard;
  `queryClient.onError` routes `PermissionDenied` → `/403` (vs `Unauthenticated` → login).

**Tier 2 — admin plugin extension (`plugins/admin/`)**

- New proto services + repos + service impls: `AuditService`, `JobsAdminService`,
  `PluginInventoryService`, `HealthService`.
- New frontend pages: `AuditPage`, `JobsPage`, `PluginsPage`, `HealthPage`.
- New permissions `admin:audit.read` / `admin:jobs.read` / `admin:plugins.read` /
  `admin:health.read`, granted by the builtin `admin` user-role's `'*'`.
- All four read data the host or M10 already write — **no new tables**.

**Both tiers**

- All new strings ship through the M14 i18n seam (Lingui).
- All new pages have Playwright specs under the M17 harness (host pages in `e2e/cross/`,
  admin pages in `plugins/admin/frontend/e2e/`).

## Out of scope

- **Nav redesign / app launcher / dashboard tile grid** — moved to M20 (`[[plugin.apps]]` +
  left-rail picker). M19 leaves the M04 hardcoded `NavLinks` in place on purpose.
- **Onboarding wizard** — deployment setup is M11; M18 seeds the initial admin.
- **In-app notifications / activity feed** — needs notification infra not yet built.
- **Cross-plugin search** — needs a per-plugin search RPC + host aggregator.
- **Theme toggle (light/dark)** — tokens exist; a runtime toggle + flicker dance is Tier 3.
- **Branded login landing page** — immediate Authentik redirect is fine for v1.
- **Job retry / cancel from the UI** — the jobs viewer is read-only; a hands-on admin needs
  its own safety story.
- **About / host-version page** — `junius plugin info` + the inventory page cover it.
- **Avatar images** — initials only until a user-storage capability exists.
- **"Sign out everywhere"** — deferred; per-row revoke of non-current sessions covers it.

## Decisions

Library choices carried from the milestone's
[*Library choices*](../../impl/21-M19-platform-ui.md#library-choices--confirm-with-user-before-starting)
table — **confirm before starting**:

- **Menu primitive:** `@radix-ui/react-dropdown-menu` wrapped as `@junius/design/Menu`
  (already a transitive dep; a11y-correct). Reject hand-rolled.
- **Icons:** `lucide-react` (host frontend dep) for the user menu + operator status
  indicators.
- **Profile sessions:** list + per-row Revoke for **non-current** sessions only; the current
  session signs out via the user menu (avoids self-lockout).
- **403 rendering:** the page receives the missing-permission names and renders them when
  present, falling back to generic copy when the caller (e.g. an RPC denial) doesn't know.
- **Error-boundary correlation id:** pulled from the M10 tracing context, shown to the user,
  and logged via `tracing::error!`.
- **Audit/Jobs pagination:** cursor-based, opaque cursor = `(occurred_at, id)` — not
  `OFFSET`/`LIMIT` over a busy table.
- **Health cadence:** on mount + 30s polling while visible (`document.visibilityState`); no
  SSE/WebSocket. Cheap pings (Postgres `SELECT 1`, S3 `HeadBucket`) run each call; OIDC uses
  the cached last-success timestamp.
- **`admin_contact_email`:** new optional `platform.toml [config]` field; surfaced on the
  empty-state dashboard and the no-access 403 page; degrades gracefully if absent.
- **Plugin holder-counts source:** live SQL `COUNT(DISTINCT user_id)` per page load (admin
  page, infrequent); materialize into a view only if it gets slow.

## Open questions

Resolved at the design level (see
[§Open questions resolved](../../impl/21-M19-platform-ui.md#open-questions-resolved)):

- *Does the host get a UI?* — Yes, but only chrome + cross-cutting surfaces; domain UI stays
  in plugins, admin/operator UI lives in the M18 `admin` plugin.
- *Where does the audit log live?* — In the admin plugin (same `platform.admin` gate), not a
  new plugin and not the host.
- *Should plugin nav be auto-derived?* — Yes, but in **M20**, not M19. The M04 hardcoded
  `NavLinks` stays through M19.
- *Personalized dashboard content?* — No; equal-weight tiles arrive in M20, iterated on real
  usage.

No open blockers for slicing. The library-choice confirmations above are the only gate.

## Implementation notes
<!-- appended by implement-issue as slices land; empty for now -->

- **Slice #2 (shell + user menu + logout)** — PR #43. Added `@junius/design/Menu`
  (Radix `react-dropdown-menu` wrapper, first host menu primitive), `signOut` in
  `@junius/sdk/auth`, and `<UserMenu>` in the host `Shell`. Deviations from the spec:
  the Profile link uses a plain `<a href="/me">` rather than a router `Link` because
  the `/me` route lands in slice #3 (avoids depending on an unregistered route);
  `signOut` redirects to `/api/auth/login` **unconditionally** (even when the logout
  fetch throws) so a failed logout can't strand a half-authenticated SPA. New deps:
  `@radix-ui/react-dropdown-menu` (design), `lucide-react` (host frontend). The
  `LocaleSwitcher` strings stay in the host catalog — the component still exists and
  re-homes onto `/me` in slice #3.
- **Slice #3 (`/me` profile page)** — `GET /api/me` now returns a flattened
  `MeResponse` (user + `oidcSub` + `sessions[]`), built by a dedicated host query so the
  per-request `User` in extensions stays lean; `DELETE /api/sessions/{id}` revokes a
  session (403 on the current one, 404 on non-owned/unknown to avoid leakage). Migration
  `0016` adds `user_agent` + `last_seen` to `platform.session`; the session middleware
  stamps `last_seen`, the OIDC callback records the login User-Agent. Decision: surfaced
  `managed_by` by adding it to the **SDK `Membership`** (intrinsic to a membership; the
  standard `load_memberships` path now carries it) rather than a parallel /me-only query.
  `/me` is host-owned, so the `junius sync` route generator emits the route
  unconditionally (the index `/` still renders `null` until slice #4's dashboard). The
  Playwright spec follows the repo convention for `e2e/cross/**` browser specs —
  `JUNIUS_E2E`-gated (skipped in the automated `run-suite`, like `user-menu`/`locale`);
  the CI-enforced behavioural coverage for the auth gates is `platform/tests/sessions_pg.rs`.
