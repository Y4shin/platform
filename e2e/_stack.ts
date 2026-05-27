import type { StackHandles } from '@junius/e2e';

// Module-scoped slot shared between globalSetup and globalTeardown — both run
// in the same Node process before/after the worker fork. Storing the handles
// here (rather than in the state file) keeps the StartedTestContainer
// references alive so testcontainers' graceful stop has a control handle.
let slot: StackHandles | undefined;

export function rememberStack(stack: StackHandles): void {
  slot = stack;
}

export function takeStack(): StackHandles | undefined {
  const current = slot;
  slot = undefined;
  return current;
}
