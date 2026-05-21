import type { RouterContext } from '../types.js';

/**
 * Route-guard helper for TanStack Router's `beforeLoad`. At M04 it's a no-op;
 * M07 adds the real check (throws / redirects if the user lacks any of the
 * requested permissions).
 */
export function requirePermissions<P extends string>(
  _ctx: RouterContext,
  _perms: readonly P[],
): void {
  // M04 stub.
}
