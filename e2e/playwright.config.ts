import { defineConfig } from '@playwright/test';

// Per-plugin Playwright discovery (M17).
// - Plugin-owned specs live under `plugins/<name>/frontend/e2e/**`.
// - Cross-plugin / host-level journeys live under `e2e/cross/**`.
// `testDir: '..'` re-bases the matcher at the repo root so both globs resolve.
//
// Stage 2 still requires a hand-started dev stack (`JUNIUS_E2E=1`) — the
// session is fixture-seeded by `@junius/e2e`'s `loginAs`. Stage 3 will add a
// `globalSetup`/`webServer` pair that starts an ephemeral testcontainers
// stack so the whole thing cold-starts unattended.
export default defineConfig({
  testDir: '..',
  testMatch: ['plugins/*/frontend/e2e/**/*.spec.ts', 'e2e/cross/**/*.spec.ts'],
  fullyParallel: true,
  retries: process.env.CI ? 2 : 0,
  use: {
    baseURL: process.env.JUNIUS_E2E_BASE_URL ?? 'http://localhost:5173',
    trace: 'on-first-retry',
  },
});
