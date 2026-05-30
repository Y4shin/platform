import { expect, test } from '@junius/e2e';

// The header user menu + logout round-trip in a real browser. A signed-in user
// opens a dropdown carrying their name, sees their email + a Profile link, and
// signs out — which clears the server session and bounces them out of the authed
// SPA. We assert the session is gone (a follow-up navigation leaves the app),
// NOT that Authentik's own login DOM rendered: the session-seed fixtures exist
// precisely to avoid the external IdP, and asserting on it would reintroduce
// that flakiness.
//
// Gated behind JUNIUS_E2E because it needs the dev stack. The session is
// fixture-seeded (no Authentik round-trip). The deterministic counterpart is
// the SDK's `signOut.test.ts`, which proves the logout request shape + redirect.
test.describe('user menu + logout', () => {
  test.skip(!process.env.JUNIUS_E2E, 'set JUNIUS_E2E=1 against a running dev stack');

  test('opens the menu, shows identity, and signs out of the app', async ({ page, loginAs }) => {
    await loginAs('alice');
    await page.goto('/');

    // The header trigger carries the display name (+ a chevron).
    const trigger = page.getByRole('button', { name: /User menu|Benutzermenü/i });
    await expect(trigger).toBeVisible();

    // The LocaleSwitcher no longer renders in the header (it re-homes to /me).
    await expect(
      page.locator('header').getByRole('combobox', { name: /Locale|Sprache/i }),
    ).toHaveCount(0);

    // Opening it reveals the email, a Profile link to /me, and Sign out.
    await trigger.click();
    const menu = page.getByRole('menu');
    await expect(menu).toBeVisible();
    await expect(menu).toContainText('@');
    await expect(page.getByRole('menuitem', { name: /Profile|Profil/i })).toHaveAttribute(
      'href',
      '/me',
    );
    const signOut = page.getByRole('menuitem', { name: /Sign out|Abmelden/i });
    await expect(signOut).toBeVisible();

    // Sign out ends the session: a follow-up navigation is bounced to login
    // instead of rendering the authed app.
    await signOut.click();
    await page.waitForURL(/\/api\/auth\/login|\/if\/flow|authentik/i);
  });
});
