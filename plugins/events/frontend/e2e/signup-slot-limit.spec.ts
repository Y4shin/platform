import { test } from '@junius/e2e';

// M13 walk #3 — invite + slot-limit signup refusal + manual close.
test.describe('invite sign-up slot enforcement', () => {
  test.fixme('slot_limit=2 admits two signups and refuses the third; close hides the button', async ({
    page,
    loginAs,
  }) => {
    // 1. alice (events:read+write) creates a public event with invite enabled,
    //    signup_open=true, slot_limit=2, show_description=false.
    //    Capture the invite slug (visible on /p/events/$id/invite as
    //    `/i/events/<slug>`).
    // 2. bob (events:read) opens /i/events/<slug>; signs up → expect "going".
    // 3. New guest context (no loginAs): opens /i/events/<slug>; submits
    //    name/email signup → expect "going".
    // 4. New guest context: opens /i/events/<slug>; submits another signup
    //    → expect refusal copy ("Sorry, this event is full" or similar) and
    //    no new signup row.
    // 5. alice opens /p/events/$id/invite; toggles signup_open=false; saves.
    // 6. Reload /i/events/<slug> as a guest → expect sign-up button gone.
    await page.goto('/p/events');
    void loginAs;
  });
});
