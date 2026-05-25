import { expect, test } from '@playwright/test';

// The cross-plugin page in a real browser. `greetings` consumes `widgets`'
// VenuePicker as an *optional* dependency: when widgets is enabled the venue
// field is its `<select>`; when widgets is disabled (a separate build with it
// dropped from platform.toml) the page degrades to a free-text `<input>`.
//
// Gated behind JUNIUS_E2E because it needs the dev stack + an authenticated
// session (the deterministic version of this assertion lives in the greetings
// frontend's vitest `venueField.test.tsx`).
test.describe('greetings cross-plugin page', () => {
  test.skip(
    !process.env.JUNIUS_E2E,
    'set JUNIUS_E2E=1 against a running, logged-in dev stack (see docs/impl/10-M09-cross-plugin.md)',
  );

  test('renders widgets VenuePicker when widgets is enabled', async ({ page }) => {
    await page.goto('/p/greetings');
    await expect(page.getByRole('combobox')).toBeVisible();
  });

  test('falls back to a free-text venue input when widgets is disabled', async ({ page }) => {
    test.skip(
      process.env.JUNIUS_E2E_WIDGETS !== 'disabled',
      'run against a build with widgets removed from platform.toml',
    );
    await page.goto('/p/greetings');
    await expect(page.getByPlaceholder(/free text/i)).toBeVisible();
  });
});
