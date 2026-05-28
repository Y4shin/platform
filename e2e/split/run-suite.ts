// M23 split-mode E2E orchestrator. Sits above Playwright (mirrors
// `e2e/run-suite.ts`). Lifecycle:
//   1. Bring up the testcontainers stack (Postgres + RabbitMQ + MinIO + mailpit).
//   2. Render the host config.
//   3. Migrate the DB.
//   4. Build juniusd WITHOUT `--features embed-frontend` (headless mode).
//   5. Build the SSR FE bundle (`pnpm --filter @junius/shell-ssr build`).
//   6. Boot juniusd, then the SSR Node server pointing at it.
//   7. Run Playwright with the split-mode config.
//   8. Teardown (always).
import type { ChildProcess } from 'node:child_process';
import { execFileSync, spawn } from 'node:child_process';
import { mkdirSync, openSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { closePool, type StackHandles, startStack, stopStack, writeState } from '@junius/e2e';

const HERE = fileURLToPath(new URL('.', import.meta.url));
const REPO_ROOT = resolve(HERE, '..', '..');
const HOST_PORT = 18888; // juniusd
const SSR_PORT = 3001; // SSR FE Node server (3000 reserved for `junius dev`)
const SSR_BASE_URL = `http://127.0.0.1:${SSR_PORT}`;
const JUNIUSD_BASE_URL = `http://127.0.0.1:${HOST_PORT}`;

let stack: StackHandles | undefined;
let juniusd: ChildProcess | undefined;
let ssr: ChildProcess | undefined;
let tornDown = false;

async function teardown(): Promise<void> {
  if (tornDown) return;
  tornDown = true;
  for (const proc of [ssr, juniusd]) {
    if (proc && proc.exitCode === null) {
      proc.kill('SIGTERM');
      await new Promise((res) => proc.once('exit', res));
    }
  }
  await closePool().catch(() => {});
  if (stack) await stopStack(stack).catch(() => {});
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

for (const sig of ['SIGINT', 'SIGTERM'] as const) {
  process.on(sig, async () => {
    await teardown();
    process.exit(sig === 'SIGINT' ? 130 : 143);
  });
}

function renderTemplate(template: string, vars: Record<string, string>): string {
  return template.replace(/\$\{(\w+)\}/g, (_match, key) => {
    const v = vars[key];
    if (v === undefined) throw new Error(`unknown template token \${${key}}`);
    return v;
  });
}

async function waitForReady(url: string, timeoutMs: number, label: string): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(url, { signal: AbortSignal.timeout(2000) });
      if (res.status < 500) return;
    } catch {
      /* retry */
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`${label} not ready at ${url} within ${timeoutMs}ms`);
}

try {
  console.log('e2e-split: starting testcontainers stack…');
  stack = await startStack();

  console.log('e2e-split: rendering host config…');
  const templatePath = resolve(REPO_ROOT, 'e2e/host-config.toml.tmpl');
  const configPath = resolve(REPO_ROOT, 'e2e/.playwright-state-split/host.toml');
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

  console.log('e2e-split: applying migrations…');
  execFileSync(
    'cargo',
    ['run', '-q', '-p', 'junius', '--', 'migrate', 'up', '--config', configPath],
    {
      cwd: REPO_ROOT,
      stdio: 'inherit',
      env: { ...process.env, DATABASE_URL: stack.databaseUrl },
    },
  );

  console.log('e2e-split: building headless juniusd…');
  execFileSync('cargo', ['build', '--release', '-p', 'platform'], {
    cwd: REPO_ROOT,
    stdio: 'inherit',
  });

  console.log('e2e-split: building SSR FE bundle…');
  execFileSync('pnpm', ['--filter', '@junius/shell-ssr', 'build'], {
    cwd: REPO_ROOT,
    stdio: 'inherit',
  });

  console.log('e2e-split: starting juniusd (headless)…');
  const stateDir = resolve(REPO_ROOT, 'e2e/.playwright-state-split');
  const juniusdLog = resolve(stateDir, 'juniusd.log');
  const juniusdFd = openSync(juniusdLog, 'w');
  juniusd = spawn(resolve(REPO_ROOT, 'target/release/juniusd'), [], {
    cwd: REPO_ROOT,
    stdio: ['ignore', juniusdFd, juniusdFd],
    env: {
      ...process.env,
      JUNIUS_CONFIG: configPath,
      DATABASE_URL: stack.databaseUrl,
      RUST_LOG: process.env.RUST_LOG ?? 'info',
    },
  });
  console.log(`e2e-split: juniusd stdio → ${juniusdLog}`);
  juniusd.on('exit', (code) => {
    if (!tornDown) {
      console.error(`e2e-split: juniusd exited unexpectedly (code=${code})`);
      process.exitCode = 1;
    }
  });
  await waitForReady(`${JUNIUSD_BASE_URL}/api/me`, 180_000, 'juniusd');

  console.log('e2e-split: starting SSR Node server…');
  const ssrLog = resolve(stateDir, 'ssr.log');
  const ssrFd = openSync(ssrLog, 'w');
  ssr = spawn('pnpm', ['--filter', '@junius/shell-ssr', 'start'], {
    cwd: REPO_ROOT,
    stdio: ['ignore', ssrFd, ssrFd],
    env: {
      ...process.env,
      NODE_ENV: 'production',
      PORT: String(SSR_PORT),
      JUNIUS_BE_INTERNAL_URL: JUNIUSD_BASE_URL,
    },
  });
  console.log(`e2e-split: SSR stdio → ${ssrLog}`);
  ssr.on('exit', (code) => {
    if (!tornDown) {
      console.error(`e2e-split: SSR exited unexpectedly (code=${code})`);
      process.exitCode = 1;
    }
  });
  await waitForReady(SSR_BASE_URL, 60_000, 'ssr');

  console.log('e2e-split: running playwright…');
  const exitCode = await new Promise<number>((res) => {
    const pw = spawn(
      'pnpm',
      ['exec', 'playwright', 'test', '--config', 'e2e/split/playwright.config.ts'],
      {
        cwd: REPO_ROOT,
        stdio: 'inherit',
        env: {
          ...process.env,
          JUNIUS_E2E_SPLIT_BASE_URL: SSR_BASE_URL,
          DATABASE_URL: stack.databaseUrl,
        },
      },
    );
    pw.on('exit', (c) => res(c ?? 1));
  });
  process.exitCode = exitCode;
} catch (err) {
  console.error('e2e-split: orchestration failed:', err);
  process.exitCode = 1;
  if (process.env['JUNIUS_E2E_DEBUG_HOLD']) {
    console.error('e2e-split: JUNIUS_E2E_DEBUG_HOLD set — holding stack up; Ctrl-C to tear down.');
    await new Promise(() => {
      // hold indefinitely
    });
  }
} finally {
  await teardown();
}
