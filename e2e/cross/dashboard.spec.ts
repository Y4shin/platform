import { expect, grantPermissions, insertSession, test, upsertUser } from '@junius/e2e';

// The dashboard at `/` end-to-end in a real browser. A user who holds any
// access lands on the greeting; a brand-new zero-permission account lands on
// the no-access empty state, which names the deployment's `admin_contact_email`
// (set to `ops@junius.local` in dev/platform.toml).
//
// Gated behind JUNIUS_E2E because it needs the dev stack; sessions are
// fixture-seeded (no Authentik round-trip). The deterministic backend
// counterpart is platform/tests/auth_pg.rs (`/api/me` surfaces the contact
// email) and the frontend-unit counterpart is DashboardPage.test.tsx (the
// greeting/empty-state branch).
test.describe('dashboard at /', () => {
  test.skip(!process.env.JUNIUS_E2E, 'set JUNIUS_E2E=1 against a running dev stack');

  async function seedAndLogin(
    name: string,
    { context, baseURL }: { context: import('@playwright/test').BrowserContext; baseURL?: string },
  ) {
    const user = await upsertUser(name);
    const session = await insertSession(user.id, { userAgent: 'Chrome / Linux' });
    const cookieUrl = baseURL ?? process.env.JUNIUS_E2E_BASE_URL ?? 'http://localhost:5173';
    await context.addCookies([
      { name: 'session', value: session, url: cookieUrl, httpOnly: true },
    ]);
    return user;
  }

  test('a user with access lands on the greeting, not the empty state', async ({
    page,
    context,
    baseURL,
  }) => {
    // A spec-private user granted one permission ⇒ one group membership ⇒ access.
    const user = await seedAndLogin('dashboard-access', { context, baseURL });
    await grantPermissions(user.id, ['events:read']);

    await page.goto('/');
    const main = page.getByRole('main');
    await expect(main.getByRole('heading', { name: /Welcome back/i })).toBeVisible();
    await expect(main.getByText(user.displayName, { exact: false })).toBeVisible();
    // No empty-state copy for a user who has access.
    await expect(main.getByText(/have access yet/i)).toHaveCount(0);
  });

  test('a zero-permission user lands on the empty state naming the admin contact', async ({
    page,
    context,
    baseURL,
  }) => {
    // Deliberately grant nothing: no memberships, no user-roles ⇒ empty state.
    await seedAndLogin('dashboard-noaccess', { context, baseURL });

    await page.goto('/');
    const main = page.getByRole('main');
    await expect(main.getByRole('heading', { name: /have access yet/i })).toBeVisible();
    // `admin_contact_email` from dev/platform.toml is named on the card.
    await expect(main.getByText('ops@junius.local')).toBeVisible();
    // The greeting is not shown.
    await expect(main.getByRole('heading', { name: /Welcome back/i })).toHaveCount(0);
  });
});
