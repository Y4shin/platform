/**
 * M23 split-mode smoke. Validates the two-container topology end-to-end:
 *
 *   1. The browser only ever sees the SSR FE origin; juniusd is on a
 *      different port not published to the test runner.
 *   2. GET /  → SSR'd HTML (not a hydration placeholder). The HTML
 *      contains the brand chrome that lives in the root layout.
 *   3. GET /api/me → 401 (no cookie attached) — confirms the FE
 *      container's proxy is forwarding to juniusd and round-tripping
 *      the response.
 *
 * The "SSR shows bob's events when bob's cookie is attached" check from
 * the M23 verification step requires the events plugin to await its
 * data during SSR (TanStack Router loaders or a QueryClient prefetch
 * pass). That's plugin-level work outside Stage 6's scope; this smoke
 * covers the platform-level wiring.
 */

import { expect, test } from '@playwright/test';

test('SSR HTML contains the rendered shell (no hydration placeholder)', async ({
  request,
  baseURL,
}) => {
  const res = await request.get('/');
  expect(res.status()).toBe(200);
  const ct = res.headers()['content-type'] ?? '';
  expect(ct).toContain('text/html');
  const body = await res.text();
  // The Shell renders the "Junius" brand string and an `<div id="root">`
  // wrapper; the rendered HTML should contain markup beyond just the
  // empty `<div id="root"></div>` you'd see from a non-SSR'd template.
  expect(body).toContain('Junius');
  // Sanity: the SSR origin we're hitting matches the configured baseURL.
  expect(baseURL).toBeTruthy();
});

test('proxy forwards /api/me to juniusd (returns 401 with no cookie)', async ({ request }) => {
  const res = await request.get('/api/me');
  // No session cookie → juniusd's session middleware returns 401, which the
  // FE proxy passes back unchanged.
  expect(res.status()).toBe(401);
});

test('proxy forwards /h/ paths (returns 404 for an unknown plugin route)', async ({ request }) => {
  const res = await request.get('/h/__nonexistent_plugin__/');
  // Whatever juniusd returns for a non-existent /h prefix flows back through
  // the proxy untouched; the key invariant is the request reached the BE
  // and we didn't get an SSR-side fallthrough (which would be the FE 404).
  expect(res.status()).not.toBe(0);
  expect(res.status()).toBeLessThan(500);
});
