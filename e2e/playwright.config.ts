import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from '@playwright/test';

const HERE = fileURLToPath(new URL('.', import.meta.url));
// Must match HOST_PORT in global-setup.ts; the bind_addr in
// e2e/host-config.toml.tmpl is also pinned to the same port.
const HOST_PORT = 18888;
const BASE_URL = process.env.JUNIUS_E2E_BASE_URL ?? `http://127.0.0.1:${HOST_PORT}`;

// Per-plugin Playwright discovery (M17).
// - Plugin-owned specs live under `plugins/<name>/frontend/e2e/**`.
// - Cross-plugin / host-level journeys live under `e2e/cross/**`.
// `testDir: '..'` re-bases the matcher at the repo root so both globs resolve.
//
// `globalSetup` starts an ephemeral testcontainers stack (Postgres + RabbitMQ
// + MinIO + mailpit), generates a host config pointing at it, and runs the
// platform migrations. `webServer` then launches the pre-built juniusd
// binary against that config. `globalTeardown` removes the containers.
export default defineConfig({
  testDir: '..',
  testMatch: ['plugins/*/frontend/e2e/**/*.spec.ts', 'e2e/cross/**/*.spec.ts'],
  fullyParallel: true,
  retries: process.env.CI ? 2 : 0,
  globalSetup: resolve(HERE, 'global-setup.ts'),
  globalTeardown: resolve(HERE, 'global-teardown.ts'),
  webServer: {
    command: 'node e2e/run-host.mjs',
    url: `${BASE_URL}/api/me`,
    reuseExistingServer: false,
    timeout: 120_000,
    stdout: 'pipe',
    stderr: 'pipe',
  },
  use: {
    baseURL: BASE_URL,
    trace: 'on-first-retry',
  },
});
