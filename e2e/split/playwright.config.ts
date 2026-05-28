import { defineConfig } from '@playwright/test';

// M23 split-mode Playwright config. Runs through the SSR FE container's
// origin (default :3001), not juniusd directly. `e2e/split/run-suite.ts`
// boots both processes before launching playwright.
const BASE_URL = process.env.JUNIUS_E2E_SPLIT_BASE_URL ?? 'http://127.0.0.1:3001';

export default defineConfig({
  testDir: '.',
  testMatch: ['specs/**/*.spec.ts'],
  fullyParallel: false, // SSR coalesces a single Node process; keep it serial.
  retries: process.env.CI ? 1 : 0,
  use: {
    baseURL: BASE_URL,
    trace: 'on-first-retry',
  },
});
