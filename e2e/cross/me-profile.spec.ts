import { assignUserRole, expect, insertSession, test } from '@junius/e2e';

// The /me profile page end-to-end in a real browser. A signed-in user opens
// `/me` and sees their identity, group memberships, user-roles, and active
// sessions; they switch language (persisted via POST /api/me/locale) and revoke
// an older session — which forces a browser carrying that session cookie to
// re-login on its next navigation.
//
// Gated behind JUNIUS_E2E because it needs the dev stack. Sessions/roles are
// fixture-seeded (no Authentik round-trip). The deterministic backend
// counterpart is platform/tests/sessions_pg.rs (the auth gates on
// GET /api/me + DELETE /api/sessions/<id>).
test.describe('/me profile page', () => {
  test.skip(!process.env.JUNIUS_E2E, 'set JUNIUS_E2E=1 against a running dev stack');

  test('shows identity + memberships + roles + sessions, switches locale, revokes a session', async ({
    page,
    context,
    baseURL,
  }) => {
    // A spec-private user (not the shared `alice`): specs run fully-parallel
    // against one DB and seed `alice` repeatedly, so her membership/role counts
    // are non-deterministic. A unique subject isolates this spec's exact
    // expectations (one manual membership, one user-role, two sessions).
    const { upsertUser, grantPermissions, pool } = await import('@junius/e2e');
    const user = await upsertUser('me-profile-user');
    await grantPermissions(user.id, ['events:read', 'events:write']);
    const currentSession = await insertSession(user.id, { userAgent: 'Chrome / Linux (current)' });
    // A second, older session the user should be able to revoke.
    const olderSession = await insertSession(user.id, { userAgent: 'Firefox / macOS (older)' });
    // A user-role assignment so the User-roles section has a row.
    await assignUserRole(user.id, 'admin', ['*']);

    const cookieUrl = baseURL ?? process.env.JUNIUS_E2E_BASE_URL ?? 'http://localhost:5173';
    await context.addCookies([
      { name: 'session', value: currentSession, url: cookieUrl, httpOnly: true },
    ]);

    await page.goto('/me');

    // Scope identity assertions to the page body — the header user menu also
    // shows the display name, so an unscoped getByText(displayName) is ambiguous.
    const main = page.getByRole('main');

    // Identity (read-only): display name, email, OIDC subject.
    await expect(page.getByRole('heading', { name: /Profile|Profil/ })).toBeVisible();
    await expect(main.getByText(user.displayName, { exact: true })).toBeVisible();
    await expect(main.getByText(user.email, { exact: true })).toBeVisible();
    await expect(main.getByText('e2e:me-profile-user', { exact: true })).toBeVisible();

    // Group memberships: the seeded group with its permission count + a
    // managed_by badge (the granted membership is `manual`).
    await expect(main.getByText('2 permissions', { exact: false })).toBeVisible();
    await expect(main.getByText(/^Manual$/)).toBeVisible();

    // User-roles: the seeded `admin` assignment (the helper suffixes the role
    // name with a UUID for per-run isolation, so match on the prefix).
    await expect(main.getByText(/^admin-/)).toBeVisible();

    // Sessions: the current device row has NO Revoke; the older one HAS one.
    const olderRow = page.getByRole('row', { name: /Firefox \/ macOS/ });
    const currentRow = page.getByRole('row', { name: /Chrome \/ Linux/ });
    await expect(currentRow.getByRole('button', { name: /Revoke|Widerrufen/ })).toHaveCount(0);
    const revoke = olderRow.getByRole('button', { name: /Revoke|Widerrufen/ });
    await expect(revoke).toBeVisible();

    // Locale switch persists `user.locale`: pick German; visible strings flip
    // immediately (assert a distinctly-German section heading, Sessions →
    // Sitzungen, so the check can't pass on the English source).
    await page.getByRole('combobox', { name: /Locale|Sprache/ }).selectOption('de');
    await expect(main.getByRole('heading', { name: 'Sitzungen' })).toBeVisible();
    // …and it is recorded server-side (POST /api/me/locale).
    await expect
      .poll(async () => {
        const r = await pool().query<{ locale: string | null }>(
          'SELECT locale FROM platform."user" WHERE id = $1',
          [user.id],
        );
        return r.rows[0]?.locale;
      })
      .toBe('de');

    // Revoke the older session → its row disappears.
    await olderRow.getByRole('button', { name: /Revoke|Widerrufen/ }).click();
    await expect(page.getByRole('row', { name: /Firefox \/ macOS/ })).toHaveCount(0);

    // A browser carrying the revoked session cookie is bounced to login on its
    // next navigation (the row deletion invalidated that session server-side).
    const ghost = await context.browser()?.newContext();
    if (ghost) {
      await ghost.addCookies([
        { name: 'session', value: olderSession, url: cookieUrl, httpOnly: true },
      ]);
      const ghostPage = await ghost.newPage();
      await ghostPage.goto('/me');
      await ghostPage.waitForURL(/\/api\/auth\/login|\/if\/flow|authentik/i);
      await ghost.close();
    }
  });
});
