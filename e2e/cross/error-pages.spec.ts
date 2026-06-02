import { expect, test } from '@junius/e2e';

// The 403 / 500 / error-boundary surfaces end-to-end (M19 slice #5).
//
// Gated behind JUNIUS_E2E because it needs the dev stack (skipped in the
// automated run-suite, like the other `e2e/cross/**` browser specs). The
// deterministic, CI-enforced coverage lives in unit tests:
//   - the `requirePermissions` guard matching + redirect:
//       packages/sdk/src/permissions/requirePermissions.test.ts
//   - the error-boundary correlation-id extraction:
//       platform/frontend/src/router/RouteErrorBoundary.test.tsx
//   - the trace-id-on-error-response + matching log line:
//       platform/src/correlation.rs (`cargo test -p platform --lib correlation`)
// This spec proves the one thing those can't: the guard actually blocks a real
// browser navigation and lands the user on `/403`.
test.describe('403 / error boundary', () => {
  test.skip(!process.env.JUNIUS_E2E, 'set JUNIUS_E2E=1 against a running dev stack');

  test('a viewer without events:write is redirected from /p/events/new to /403 naming the permission', async ({
    page,
    loginAs,
  }) => {
    // events:read only — the create form's `requirePermissions(['events:write'])`
    // guard must block before the page renders.
    await loginAs('forbidden-spec-user', { permissions: ['events:read'] });

    await page.goto('/p/events/new');

    // The guard throws a redirect to /403 in beforeLoad, so the browser ends up
    // there rather than on the edit form.
    await page.waitForURL(/\/403/);
    const main = page.getByRole('main');
    await expect(main.getByRole('heading', { name: /Access denied|Zugriff verweigert/ })).toBeVisible();
    // …and the page names the exact missing permission carried in router state.
    await expect(main.getByText('events:write')).toBeVisible();
  });

  // AC2 — a PermissionDenied RPC (not intercepted by a route guard) routes to
  // /403 via `queryClient.onError`. Needs a page that issues a write RPC the
  // viewer is denied *without* a `requirePermissions` guard in front (the
  // events create/edit routes are now guarded, so they redirect before the
  // RPC). Left fixme until such a fixture page exists; the onError branch is
  // wired in platform/frontend/src/AppShell.tsx.
  test.fixme('a PermissionDenied RPC routes to /403 with the same shape', async () => {});

  // AC3 — throwing inside a route component renders <RouteErrorBoundary> (a
  // friendly card + correlation id) instead of a white screen. Needs a
  // fault-injecting route (test-only) to throw at render. Left fixme; the
  // boundary's correlation-id extraction is unit-tested and the prod card is
  // exercised by RouteErrorBoundary.test.tsx.
  test.fixme('a throwing route renders the error boundary with a correlation id', async () => {});
});
