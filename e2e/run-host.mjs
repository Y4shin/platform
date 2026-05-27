// Boots the pre-built juniusd binary against the ephemeral testcontainers
// stack. Playwright's webServer invokes this as a single command; the env
// values come from the state file globalSetup wrote.
import { spawn } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const stateFile = process.env.JUNIUS_E2E_STATE ?? 'e2e/.playwright-state/state.json';
const state = JSON.parse(readFileSync(stateFile, 'utf8'));

const binary = process.env.JUNIUSD_BINARY ?? resolve('target/release/juniusd');

// The host config template inlines every secret as a literal — none of the
// `env:` indirections from dev/platform.toml apply here — so the binary only
// needs JUNIUS_CONFIG (and DATABASE_URL for sqlx, which the host re-reads
// from the resolved config anyway, but kept for parity with Task targets).
const child = spawn(binary, [], {
  stdio: 'inherit',
  env: {
    ...process.env,
    JUNIUS_CONFIG: state.configPath,
    DATABASE_URL: state.databaseUrl,
  },
});

child.on('exit', (code) => process.exit(code ?? 0));
process.on('SIGINT', () => child.kill('SIGINT'));
process.on('SIGTERM', () => child.kill('SIGTERM'));
