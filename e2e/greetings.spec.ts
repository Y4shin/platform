import { expect, test } from '@junius/e2e';

// The cross-plugin page in a real browser. `greetings` consumes `widgets`'
// VenuePicker as an *optional* dependency: when widgets is enabled the venue
// field is its `<select>`; when widgets is disabled (a separate build with it
// dropped from platform.toml) the page degrades to a free-text `<input>`.
//
// Gated behind JUNIUS_E2E because it needs the dev stack (greetings sourced).
// The session is fixture-seeded — no Authentik round-trip, no manual login.
// The deterministic counterpart lives in the greetings frontend's vitest
// `venueField.test.tsx`.
test.describe('greetings cross-plugin page', () => {
  test.skip(
    !process.env.JUNIUS_E2E,
    'set JUNIUS_E2E=1 against a running dev stack with `greetings` sourced',
  );

  test('renders widgets VenuePicker when widgets is enabled', async ({ page, loginAs }) => {
    await loginAs('alice');
    await page.goto('/p/greetings');
    await expect(page.getByRole('combobox')).toBeVisible();
  });

  test('falls back to a free-text venue input when widgets is disabled', async ({ page, loginAs }) => {
    test.skip(
      process.env.JUNIUS_E2E_WIDGETS !== 'disabled',
      'run against a build with widgets removed from platform.toml',
    );
    await loginAs('alice');
    await page.goto('/p/greetings');
    await expect(page.getByPlaceholder(/free text/i)).toBeVisible();
  });
});
