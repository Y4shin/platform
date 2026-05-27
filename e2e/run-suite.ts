// `task test:e2e`'s orchestrator. Sits *above* Playwright (not as
// globalSetup/webServer — Playwright starts webServer before globalSetup,
// so any state shared between them would race). Lifecycle: bring up the
// testcontainers stack → render config → migrate → boot juniusd →
// `playwright test` → teardown (always). A `junius-e2e-run=<uuid>` docker
// label backs a defensive sweep so a kill-9 still cleans up.
import type { ChildProcess } from 'node:child_process';
import { execFileSync, spawn } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { closePool, type StackHandles, startStack, stopStack, writeState } from '@junius/e2e';

const HERE = fileURLToPath(new URL('.', import.meta.url));
const REPO_ROOT = resolve(HERE, '..');
const HOST_PORT = 18888;
const BASE_URL = `http://127.0.0.1:${HOST_PORT}`;

let stack: StackHandles | undefined;
let juniusd: ChildProcess | undefined;
let tornDown = false;

async function teardown() {
  if (tornDown) return;
  tornDown = true;
  const proc = juniusd;
  if (proc && proc.exitCode === null) {
    proc.kill('SIGTERM');
    await new Promise((res) => proc.once('exit', res));
  }
  await closePool().catch(() => {});
  if (stack) await stopStack(stack).catch(() => {});
  // Defensive label sweep — covers a testcontainers miss or a kill-9 on us.
  if (stack?.runId) {
    try {
      const ids = execFileSync(
        'docker',
        ['ps', '-aq', '--filter', `label=junius-e2e-run=${stack.runId}`],
        { stdio: ['ignore', 'pipe', 'ignore'] },
      )
        .toString()
        .split(/\s+/)
        .filter(Boolean);
      if (ids.length > 0) {
        execFileSync('docker', ['rm', '-f', ...ids], { stdio: 'ignore' });
      }
    } catch {
      /* best-effort */
    }
  }
}

for (const sig of ['SIGINT', 'SIGTERM']) {
  process.on(sig, async () => {
    await teardown();
    process.exit(sig === 'SIGINT' ? 130 : 143);
  });
}

function renderTemplate(template, vars) {
  return template.replace(/\$\{(\w+)\}/g, (_match, key) => {
    const v = vars[key];
    if (v === undefined) throw new Error(`unknown template token \${${key}}`);
    return v;
  });
}

async function waitForReady(url, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(url, { signal: AbortSignal.timeout(2000) });
      // 200 means user is resolved (won't happen pre-login); 401/200 both mean alive.
      if (res.status < 500) return;
    } catch {
      /* retry */
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`juniusd not ready at ${url} within ${timeoutMs}ms`);
}

try {
  console.log('e2e: starting testcontainers stack…');
  stack = await startStack();

  console.log('e2e: rendering host config…');
  const templatePath = resolve(REPO_ROOT, 'e2e/host-config.toml.tmpl');
  const configPath = resolve(REPO_ROOT, 'e2e/.playwright-state/host.toml');
  mkdirSync(dirname(configPath), { recursive: true });
  const rendered = renderTemplate(readFileSync(templatePath, 'utf8'), {
    DATABASE_URL: stack.databaseUrl,
    AMQP_URL: stack.amqpUrl,
    S3_ENDPOINT: stack.s3Endpoint,
    S3_ACCESS_KEY: stack.s3AccessKey,
    S3_SECRET_KEY: stack.s3SecretKey,
    SMTP_HOST: stack.smtpHost,
    SMTP_PORT: String(stack.smtpPort),
  });
  writeFileSync(configPath, rendered);
  writeState({
    runId: stack.runId,
    databaseUrl: stack.databaseUrl,
    amqpUrl: stack.amqpUrl,
    s3Endpoint: stack.s3Endpoint,
    s3AccessKey: stack.s3AccessKey,
    s3SecretKey: stack.s3SecretKey,
    smtpHost: stack.smtpHost,
    smtpPort: stack.smtpPort,
    mailpitHttpUrl: stack.mailpitHttpUrl,
    hostPort: HOST_PORT,
    configPath,
  });

  console.log('e2e: applying migrations…');
  execFileSync(
    'cargo',
    ['run', '-q', '-p', 'junius', '--', 'migrate', 'up', '--config', configPath],
    {
      cwd: REPO_ROOT,
      stdio: 'inherit',
      env: { ...process.env, DATABASE_URL: stack.databaseUrl },
    },
  );

  console.log('e2e: starting juniusd…');
  const juniusdLog = resolve(REPO_ROOT, 'e2e/.playwright-state/juniusd.log');
  const { openSync } = await import('node:fs');
  const logFd = openSync(juniusdLog, 'w');
  juniusd = spawn(resolve(REPO_ROOT, 'target/release/juniusd'), [], {
    cwd: REPO_ROOT,
    stdio: ['ignore', logFd, logFd],
    env: {
      ...process.env,
      JUNIUS_CONFIG: configPath,
      DATABASE_URL: stack.databaseUrl,
      RUST_LOG: process.env.RUST_LOG ?? 'info',
    },
  });
  console.log(`e2e: juniusd stdio → ${juniusdLog}`);
  juniusd.on('exit', (code) => {
    if (!tornDown) {
      console.error(`e2e: juniusd exited unexpectedly (code=${code})`);
      process.exitCode = 1;
    }
  });

  await waitForReady(`${BASE_URL}/api/me`, 180_000);

  console.log('e2e: running playwright…');
  const exitCode = await new Promise((res) => {
    const pw = spawn(
      'pnpm',
      ['exec', 'playwright', 'test', '--config', 'e2e/playwright.config.ts'],
      {
        cwd: REPO_ROOT,
        stdio: 'inherit',
        env: {
          ...process.env,
          JUNIUS_E2E_BASE_URL: BASE_URL,
          // Spec workers seed sessions via `@junius/e2e`'s `loginAs`, which
          // connects directly to the same Postgres juniusd uses.
          DATABASE_URL: stack!.databaseUrl,
        },
      },
    );
    pw.on('exit', (c) => res(c ?? 1));
  });
  process.exitCode = exitCode;
} catch (err) {
  console.error('e2e: orchestration failed:', err);
  process.exitCode = 1;
  if (process.env.JUNIUS_E2E_DEBUG_HOLD) {
    console.error(`e2e: JUNIUS_E2E_DEBUG_HOLD set — holding stack up; press Ctrl-C to tear down.`);
    await new Promise(() => {});
  }
} finally {
  await teardown();
}
