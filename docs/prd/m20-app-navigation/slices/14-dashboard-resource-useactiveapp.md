# Slice #14 — Dashboard re-source to `APPS` + `useActiveApp`

**PRD:** ../prd.md · **kind:** feature · **mode:** afk

Full design: [`22-M20-app-navigation.md` §D–E](../../../impl/22-M20-app-navigation.md#d--dashboard-re-source).

## What to build

Re-source the M19 dashboard tiles from apps, and expose the active-app context to plugin code.

- Switch the M19 `<DashboardPage>` tile source from `PLUGIN_NAV` (one tile per plugin) to
  `APPS` (one tile per app): `APPS.filter(a => userHasAny(user, a.visibleWith))`, ordered by
  `SECTION_ORDER` then declaration order. The M19 empty-state (zero visible apps →
  `<NoAccessEmptyState>` with `admin_contact_email`) is unchanged.
- Ship `useActiveApp(): ActiveApp | null` from `packages/sdk/src/registry/`, backed by the
  `ActiveAppContext` from slice #12 (key, displayName, path, nav).

## Acceptance criteria

- [ ] Alice sees the full grid of app tiles on `/` (a two-app plugin yields two tiles).
- [ ] Bob sees two tiles (Events, Calendar feeds).
- [ ] A zero-permission user still sees the empty-state card.
- [ ] A vitest covers `useActiveApp()` returning the right entry inside a route under the
      app's path (and `null` on `/`).
- [ ] All new strings ship through Lingui.

## Blocked by

- #11 — needs the generated `APPS`.
- #13 — tiles are meaningful only once plugins declare apps.
