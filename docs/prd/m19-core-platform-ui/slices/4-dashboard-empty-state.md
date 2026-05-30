# Slice #4 — Dashboard skeleton + zero-permissions empty state

**PRD:** ../prd.md · **kind:** feature · **mode:** hitl

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
