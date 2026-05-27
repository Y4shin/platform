import { expect, test } from '@junius/e2e';

// Locale-switch round-trip in a real browser. Picking a non-default locale in
// the header `<LocaleSwitcher>` should:
//   1. POST /api/me/locale and persist `platform.user.locale`,
//   2. re-activate Lingui so visible strings flip immediately,
//   3. survive a hard reload (the persisted preference re-applies).
//
// Gated behind JUNIUS_E2E because it needs the dev stack with `hello` sourced.
// The session is fixture-seeded (no Authentik round-trip). The deterministic
// counterpart is the SDK's I18nProvider vitest
// (`packages/sdk/src/i18n/I18nProvider.test.tsx`), which covers the same
// state machine in isolation.
test.describe('locale switch', () => {
  test.skip(!process.env.JUNIUS_E2E, 'set JUNIUS_E2E=1 against a running dev stack');

  test('persists the choice across reload and translates host + plugin strings', async ({
    page,
    loginAs,
  }) => {
    await loginAs('alice');
    // Start on a page that mixes host shell strings (the header's "Anonymous"
    // fallback isn't visible if we're logged in; the locale-switcher labels
    // always are) and a plugin string (HelloPage's greeting).
    await page.goto('/p/hello');

    // Default locale renders the English source string from
    // `plugins/hello/frontend/i18n/en.po`.
    await expect(page.getByRole('heading', { name: /^Hello,/ })).toBeVisible();

    // Pick German. The select element is labelled "Locale" (visually hidden).
    await page.getByRole('combobox', { name: /Locale|Sprache/ }).selectOption('de');

    // German source string from `plugins/hello/frontend/i18n/de.po`.
    await expect(page.getByRole('heading', { name: /^Hallo,/ })).toBeVisible();
    // Host shell label flipped (LocaleSwitcher labels itself translate too).
    await expect(page.getByRole('combobox', { name: /Sprache/ })).toBeVisible();

    // Reload — the persisted preference re-applies; no manual re-selection.
    await page.reload();
    await expect(page.getByRole('heading', { name: /^Hallo,/ })).toBeVisible();
  });
});
