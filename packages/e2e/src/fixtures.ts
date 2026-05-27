import { test as base } from '@playwright/test';
import {
  closePool,
  type DbPool,
  grantPermissions,
  insertSession,
  pool,
  type SeededUser,
  upsertUser,
} from './seed.js';

export interface LoginOptions {
  /** Permissions to grant the user via an ephemeral group + role. */
  permissions?: readonly string[];
}

export type LoginAs = (name: string, options?: LoginOptions) => Promise<SeededUser>;

export interface JuniusFixtures {
  /** Log in as a seeded user. Returns the seeded user (id, email, displayName). */
  loginAs: LoginAs;
  /** Raw pg pool for seed/cleanup helpers inside specs. */
  db: DbPool;
}

/**
 * Playwright `test` extended with the @junius/e2e fixtures. Import this in
 * specs instead of `@playwright/test`:
 *
 *   import { test, expect } from '@junius/e2e';
 *
 *   test('alice publishes an event', async ({ page, loginAs }) => {
 *     await loginAs('alice', { permissions: ['events:read', 'events:write'] });
 *     await page.goto('/p/events');
 *   });
 */
export const test = base.extend<JuniusFixtures>({
  loginAs: async ({ context, baseURL }, use) => {
    const login: LoginAs = async (name, options) => {
      const user = await upsertUser(name);
      if (options?.permissions && options.permissions.length > 0) {
        await grantPermissions(user.id, options.permissions);
      }
      const sessionId = await insertSession(user.id);
      const cookieUrl = baseURL ?? process.env.JUNIUS_E2E_BASE_URL ?? 'http://localhost:5173';
      await context.addCookies([
        {
          name: 'session',
          value: sessionId,
          url: cookieUrl,
          httpOnly: true,
          // sameSite/secure deliberately left default — the host issues the cookie
          // without HMAC; the middleware only parses it as a UUID.
        },
      ]);
      return user;
    };
    await use(login);
  },

  // biome-ignore lint/correctness/noEmptyPattern: Playwright fixture API requires destructuring of dependencies
  db: async ({}, use) => {
    await use(pool());
  },
});

export { expect } from '@playwright/test';
export { closePool };
