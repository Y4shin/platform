import { expect, test } from '@junius/e2e';

// `.ics` calendar export: unauthenticated HTTP, gated by event/feed ACL.
// Covers the M13 walk step 6 (event + personal feed fetch + revoke).
//
// Asserts:
// - Unknown event id → 404.
// - Unknown personal feed key → 404.
// - Unknown group name → 404 (also covers "group exists but not public").
//
// The end-to-end happy path (mint personal feed → fetch → assert event present
// → revoke → 404) needs the CalendarService RPC client (see
// plugins/events/proto/events/v1/calendar.proto). It's left as test.fixme until
// the RPC client wiring lands in a follow-up; the negative cases below already
// exercise the http.rs router + the testcontainers stack end-to-end.
test.describe('events ICS endpoints', () => {
  test('rejects an unknown event id with 404', async ({ page }) => {
    const response = await page.request.get(
      '/h/events/ics/e/00000000-0000-0000-0000-000000000000',
      {
        failOnStatusCode: false,
      },
    );
    expect(response.status()).toBe(404);
  });

  test('rejects an unknown personal feed key with 404', async ({ page }) => {
    const response = await page.request.get('/h/events/ics/u/this-key-was-never-minted', {
      failOnStatusCode: false,
    });
    expect(response.status()).toBe(404);
  });

  test('rejects an unknown group name with 404', async ({ page }) => {
    const response = await page.request.get('/h/events/ics/g/no-such-group', {
      failOnStatusCode: false,
    });
    expect(response.status()).toBe(404);
  });

  test.fixme('personal feed round-trip: mint → fetch → revoke → 404', async ({ page, loginAs }) => {
    await loginAs('alice', { permissions: ['events:read'] });
    // 1. Call CalendarService.GetPersonalFeed() to mint a key.
    // 2. GET /h/events/ics/u/<key> → expect 200, content-type text/calendar,
    //    body starts with BEGIN:VCALENDAR.
    // 3. Call CalendarService.RevokeFeed(id).
    // 4. GET /h/events/ics/u/<key> → expect 404.
    await page.goto('/p/events');
  });
});
