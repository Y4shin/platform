import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';

/**
 * Cross-process handoff for the ephemeral stack URLs Playwright globalSetup
 * starts. globalSetup writes; the webServer command and per-worker fixtures
 * read. We don't ship the `containers[]` handles through here — those only
 * live in globalSetup's memory and are torn down by globalTeardown.
 */
export interface PersistedState {
  runId: string;
  databaseUrl: string;
  amqpUrl: string;
  s3Endpoint: string;
  s3AccessKey: string;
  s3SecretKey: string;
  smtpHost: string;
  smtpPort: number;
  mailpitHttpUrl: string;
  hostPort: number;
  configPath: string;
}

export function defaultStatePath(): string {
  return process.env.JUNIUS_E2E_STATE ?? 'e2e/.playwright-state/state.json';
}

export function writeState(state: PersistedState, path = defaultStatePath()): void {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, JSON.stringify(state, null, 2));
}

export function readState(path = defaultStatePath()): PersistedState {
  return JSON.parse(readFileSync(path, 'utf8')) as PersistedState;
}
