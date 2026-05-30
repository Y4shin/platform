# Slice #5 — 403 / 500 / error boundary

**PRD:** ../prd.md · **kind:** feature · **mode:** hitl

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
