import { defineConfig } from '@playwright/test';

// Browser E2E for the cross-plugin demo. These run against a *running* dev stack
// (authentik + juniusd + Vite) with an authenticated session — they are not part
// of the default CI gate. Run them with `JUNIUS_E2E=1 task test:e2e` after
// `docker compose -f dev/docker-compose.yml up -d` + `task dev` + a login.
// See docs/impl/10-M09-cross-plugin.md §Verification.
export default defineConfig({
  testDir: '.',
  fullyParallel: true,
  use: {
    baseURL: process.env.JUNIUS_E2E_BASE_URL ?? 'http://localhost:5173',
  },
});
