import { test } from '@junius/e2e';

// M13 walk #4–5 — public invite works logged-out; private invite is 404
// logged-out (no existence leak) but accessible after login.
test.describe('public invite page', () => {
  test.fixme('public event invite is reachable logged-out', async ({ page, loginAs }) => {
    // 1. alice creates a *public* event with an invite; capture its slug.
    // 2. New browser context (no session): GET /i/events/<slug> →
    //    expect 200, title visible, sign-up form present.
    await page.goto('/p/events');
    void loginAs;
  });

  test.fixme('private event invite returns 404 logged-out and is accessible after login', async ({
    page,
    loginAs,
  }) => {
    // 1. alice creates a *private* event with an invite; capture its slug.
    // 2. New context (no session): /i/events/<slug> → expect 404.
    // 3. Same context, loginAs('bob', { permissions: ['events:read'] }) +
    //    grant bob access to the event (resource_share) → reload → 200 with title.
    await page.goto('/p/events');
    void loginAs;
  });
});
