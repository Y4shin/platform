---
kind: feature
title: "Dashboard skeleton + zero-permissions empty state"
slug: dashboard-empty-state
issue: 4
prd: ../prd.md
mode: hitl
---

# Slice #4 — Dashboard skeleton + zero-permissions empty state

Full design: [`21-M19-platform-ui.md` §Tier 1.D](../../../impl/21-M19-platform-ui.md#tier-1d--real-dashboard-at-).

## What to build

End-to-end: a user lands on `/` and sees a greeting; a brand-new user with zero permissions
sees a clear empty state naming who to contact instead of a blank pane.

- `<DashboardPage>` replaces the `null` `/` route. v1 renders a greeting
  ("Welcome back, &lt;name&gt;") only.
- Zero-permission case → `<NoAccessEmptyState>`: a card explaining the account has no access
  yet and naming the admin contact.
- New **optional** `[config] admin_contact_email` read from `platform.toml`; surfaced on the
  empty-state card (and reused on the slice-#5 403 page). Degrades gracefully if absent.
- **No tile grid** — the permission-filtered tile grid (and the manifest-driven nav it shares
  a data source with) is deferred to M20 Stage 4 once `[[plugin.apps]]` exists. The header
  nav stays the hardcoded M04 `NavLinks`.

## Acceptance criteria

- [ ] A user with permissions (e.g. alice) lands on the greeting only — no tiles.
- [ ] A fresh OIDC user with zero permissions lands on the empty-state card naming
      `admin_contact_email` (and degrading gracefully when it is unset).
- [ ] The header nav remains the hardcoded M04 `NavLinks` (no auto-derivation in M19).
- [ ] All new strings ship through Lingui.
- [ ] Playwright spec under `e2e/cross/` covers both the greeting and the empty-state path.

## Blocked by

- None — can start immediately.

## Test plan

**Test type:** frontend-unit (primary, CI-enforced) + e2e (full path) + rust-integration (config→API seam)
**Reasoning:** The slice has three behaviours that fail differently — a pure render branch (vitest, fast CI loop), the full login→config→`/api/me`→render path (mandated Playwright, `JUNIUS_E2E`-gated), and the *new* backend behaviour of surfacing `admin_contact_email` to the SPA (rust-integration, the part most likely to silently break and not covered by the CI-skipped e2e).

### Assertions

**frontend-unit — `<DashboardPage>` render branch** (mock `/api/me`):
- Viewer with ≥1 membership *or* ≥1 user-role → renders the greeting ("Welcome back, <name>"); no empty-state card, no tiles.
- Viewer with `memberships.length === 0 && userRoles.length === 0` → renders `<NoAccessEmptyState>` naming the `admin_contact_email`.
- Same zero-permission viewer with `admin_contact_email` **unset/null** → empty state still renders and degrades gracefully (no broken "contact " fragment, no crash).
- All visible strings are Lingui-wrapped (no raw literals).

**rust-integration — config surfacing through `/api/me`:**
- With `admin_contact_email` set in config, the `/api/me` response carries it (e.g. `adminContactEmail`).
- With the field absent from config, the response omits it / returns null (the FE degrade path has a real source).

**e2e — full path (`e2e/cross/`, `JUNIUS_E2E`-gated):**
- A user with permissions lands on `/` and sees the greeting only — no tiles, header nav is the hardcoded M04 `NavLinks`.
- A fresh OIDC user with zero permissions lands on the empty-state card naming the configured `admin_contact_email`.

### Test file

- `platform/frontend/src/pages/DashboardPage.test.tsx` (new — first vitest under the host frontend `src/`; follows the `packages/` vitest pattern)
- `platform/tests/auth_pg.rs` (extend the existing `/api/me` integration coverage with the config-surfacing assertions; testcontainers Postgres)
- `e2e/cross/dashboard.spec.ts` (new — mirrors [`me-profile.spec.ts`](../../../../e2e/cross/me-profile.spec.ts); `test.skip(!process.env.JUNIUS_E2E, …)`)

### Run command

`task test:js` (vitest, primary fast loop) · `task test:rust` (rust-integration) · `task test:e2e` (Playwright, needs Docker)
