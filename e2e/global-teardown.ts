import { execFileSync } from 'node:child_process';
import { closePool, stopStack } from '@junius/e2e';
import { takeStack } from './_stack.js';

export default async function globalTeardown(): Promise<void> {
  await closePool();
  const stack = takeStack();
  if (stack) {
    await stopStack(stack);
  }
  // Defensive sweep: even if testcontainers missed something, the run-id
  // label gets it. `docker rm -f` is a no-op on an empty arg list, so the
  // dry-run check via `docker ps -q --filter` decides whether to invoke it.
  const runId = process.env.JUNIUS_E2E_RUN_ID;
  if (runId) {
    try {
      const ids = execFileSync(
        'docker',
        ['ps', '-aq', '--filter', `label=junius-e2e-run=${runId}`],
        { stdio: ['ignore', 'pipe', 'ignore'] },
      )
        .toString()
        .split(/\s+/)
        .filter(Boolean);
      if (ids.length > 0) {
        execFileSync('docker', ['rm', '-f', ...ids], { stdio: 'ignore' });
      }
    } catch {
      // Best-effort.
    }
  }
}
