import { test } from '@junius/e2e';

// M13 walk #1 — private → published visibility transition. Marked `.fixme`
// pending UI-selector verification against the live host; the journey shape
// is captured below so a follow-up can flip these to live with confidence.
test.describe('event visibility', () => {
  test.fixme('private user event is invisible to others until alice publishes it', async ({
    page,
    loginAs,
  }) => {
    // 1. loginAs('alice', { permissions: ['events:read', 'events:write'] })
    // 2. Navigate /p/events/new; fill title/starts_at; owner=alice; visibility=private; submit.
    // 3. Capture the new event id from the URL after redirect to /p/events/$id.
    // 4. New context: loginAs('bob', { permissions: ['events:read'] }).
    //    GET /p/events — assert alice's private event title is NOT visible.
    //    GET /p/events/$id — assert 404 (or redirect-to-list).
    // 5. Back in alice's context: open /p/events/$id/edit; flip visibility=public; save.
    // 6. Bob's context: reload /p/events — assert title IS visible.
    //    GET /p/events/$id — assert title visible; no "Edit" affordance.
    await page.goto('/p/events');
    void loginAs;
  });
});
