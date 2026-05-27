import { expect, test } from '@junius/e2e';

// Proves the per-plugin Playwright harness end-to-end: a seeded session
// (no Authentik round-trip) on a per-plugin spec dir gets a real authed page
// against the embedded juniusd. This is the "harness works" smoke; the rich
// events journeys live in the other specs in this directory.
test('session-seeded loginAs reaches /api/me', async ({ page, loginAs }) => {
  const user = await loginAs('alice', { permissions: ['events:read'] });
  const response = await page.request.get('/api/me');
  expect(response.status()).toBe(200);
  const body = (await response.json()) as { id: string; email: string };
  expect(body.id).toBe(user.id);
  expect(body.email).toBe(user.email);
});
