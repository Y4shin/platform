import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { startStack, writeState } from '@junius/e2e';
import { rememberStack } from './_stack.js';

const HERE = fileURLToPath(new URL('.', import.meta.url));

/** Fixed local port for the embedded juniusd webServer. Single-machine
 * concurrent E2E runs would collide here — the same constraint as any
 * localhost dev server — but the *containers* stay UUID-labeled so DB/queue
 * state is still per-run isolated. */
export const HOST_PORT = 18888;

function renderTemplate(template: string, vars: Record<string, string>): string {
  return template.replace(/\$\{(\w+)\}/g, (_match, key: string) => {
    const v = vars[key];
    if (v === undefined) throw new Error(`unknown template token \${${key}}`);
    return v;
  });
}

export default async function globalSetup(): Promise<void> {
  const repoRoot = resolve(HERE, '..');
  const stack = await startStack();
  rememberStack(stack);
  process.env.JUNIUS_E2E_RUN_ID = stack.runId;

  const templatePath = resolve(repoRoot, 'e2e/host-config.toml.tmpl');
  const configPath = resolve(repoRoot, 'e2e/.playwright-state/host.toml');
  const template = readFileSync(templatePath, 'utf8');
  const rendered = renderTemplate(template, {
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

  // Apply host + plugin migrations against the ephemeral DB.
  execFileSync(
    'cargo',
    ['run', '-q', '-p', 'junius', '--', 'migrate', 'up', '--config', configPath],
    {
      cwd: repoRoot,
      stdio: 'inherit',
      env: { ...process.env, DATABASE_URL: stack.databaseUrl },
    },
  );
}
