---
kind: feature
title: "403 / 500 / error boundary"
slug: error-pages-403-500
issue: 5
prd: ../prd.md
mode: hitl
---

# Slice #5 — 403 / 500 / error boundary

Full design: [`21-M19-platform-ui.md` §Tier 1.E](../../../impl/21-M19-platform-ui.md#tier-1e--403-500-and-error-boundary).

## What to build

End-to-end: a permission denial lands on a real 403 page naming the missing permission, and
an uncaught render error shows a friendly card with a correlation id instead of a white
screen.

- `<ForbiddenPage>` at `/403`, receiving a missing-permission list via
  `navigate({ to: '/403', state: { missing: [...] } })`; renders the names when present,
  falls back to generic copy otherwise. Reuses `admin_contact_email` from slice #4.
- `queryClient.onError` gains a branch: `Code.PermissionDenied` → `/403` (alongside the
  existing `Code.Unauthenticated` → login via `goToLoginUnlessPublic`).
- `requirePermissions` becomes a **real** route guard (no longer the M04 no-op): if the
  viewer lacks the listed permissions, redirect to `/403` before the page renders. Closes the
  M13 friction row #36.
- `<RouteErrorBoundary>` wrapping the authed layout's `<Outlet />`: in dev, show the stack; in
  prod, a friendly card with a `correlation_id` pulled from the M10 tracing context; also call
  `tracing::error!` so it lands in the logs.
- Keep the existing 404 card; route to it explicitly where the design's existence-leak rule
  applies (reserving 403 for permission-denied on *known* resources).

## Acceptance criteria

- [ ] `requirePermissions(['events:write'])` on a route the viewer can't write blocks
      navigation and shows the missing permission.
- [ ] An RPC that returns `PermissionDenied` routes to `/403` with the same shape.
- [ ] Throwing inside a route component renders the error boundary with a correlation id that
      matches the `juniusd` log — not a white screen.
- [ ] All new strings ship through Lingui.
- [ ] Playwright spec under `e2e/cross/`.

## Blocked by

- None — can start immediately.

## Test plan

**Test type:** rust-integration + frontend-unit + e2e (three layers)
**Reasoning:** the correlation id rides on the Connect **error metadata** as the active
OTel trace ID, so "the card's id matches the `juniusd` log" is a deterministic backend
assertion (Rust integration) rather than a flaky log-grep; the branchy `requirePermissions`
matching is cheapest as a pure unit; the cross-surface wiring is proven once end-to-end in a
browser — mirroring slice #3 (gated Playwright + a CI-enforced Rust counterpart).

### Assertions

**rust-integration — `platform/tests/error_correlation_pg.rs`** (CI-enforced core)
- A failing RPC returns error metadata carrying the **current OTel trace ID** as the
  correlation id, and the same trace ID appears in the emitted (`tracing`) log line.
- `Code.PermissionDenied` is returned for a viewer lacking the permission, with the
  correlation-id metadata present.
- Graceful degradation: no active span ⇒ no/empty correlation-id metadata (the FE then
  falls back to generic copy — no white screen).

**frontend-unit — `packages/sdk/src/permissions/requirePermissions.test.ts`**
- Viewer holding all listed permissions ⇒ guard passes (no redirect).
- Viewer missing one ⇒ redirect to `/403` carrying the **missing** permission name(s) in
  router state.
- Wildcard admin user-role (`'*'`) ⇒ passes every check.
- Multi-permission lists: passes only when *all* are held; missing-set extraction is exact.

**e2e — `e2e/cross/error-pages.spec.ts`** (`JUNIUS_E2E`-gated browser proof of the wiring)
- Direct-URL nav to a route the viewer can't access blocks the page and lands on `/403`
  naming the missing permission(s).
- An RPC returning `PermissionDenied` routes to `/403` with the same shape.
- Throwing inside a route component renders the `<RouteErrorBoundary>` (friendly card,
  correlation id of the expected shape) instead of a white screen.
- All asserted strings resolve through Lingui (assert a German variant flips, per the
  slice-#3 locale convention).

### Test files
- `platform/tests/error_correlation_pg.rs`
- `packages/sdk/src/permissions/requirePermissions.test.ts`
- `e2e/cross/error-pages.spec.ts`

### Run command
`task test:rust` (integration) · `task test:js` (frontend-unit) · `task test:e2e` (Playwright)
