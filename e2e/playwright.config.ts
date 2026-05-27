import { defineConfig } from '@playwright/test';

// Per-plugin Playwright discovery (M17).
// - Plugin-owned specs live under `plugins/<name>/frontend/e2e/**`.
// - Cross-plugin / host-level journeys live under `e2e/cross/**`.
// `testDir: '..'` re-bases the matcher at the repo root so both globs resolve.
//
// Stack + host lifecycle is owned by `e2e/run-suite.mjs` (invoked by
// `task test:e2e`), not by Playwright's `webServer`/`globalSetup` — those
// race because Playwright starts webServer before globalSetup, so any state
// shared between them is unsynchronised. The orchestrator brings the stack
// up + boots juniusd before `playwright test` is invoked, so by the time
// this config loads, `JUNIUS_E2E_BASE_URL` is already set and the host is
// healthy.
const BASE_URL = process.env.JUNIUS_E2E_BASE_URL ?? 'http://127.0.0.1:18888';

export default defineConfig({
  testDir: '..',
  testMatch: ['plugins/*/frontend/e2e/**/*.spec.ts', 'e2e/cross/**/*.spec.ts'],
  fullyParallel: true,
  retries: process.env.CI ? 2 : 0,
  use: {
    baseURL: BASE_URL,
    trace: 'on-first-retry',
  },
});
