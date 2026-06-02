import { type HistoryState, redirect } from '@tanstack/react-router';

import { ADMIN_WILDCARD, type ForbiddenState, type RouterContext, type User } from '../types.js';

/**
 * Whether `user` holds `perm` through any of their memberships or global
 * user-roles. A grant of the admin wildcard (`'*'`) satisfies every check.
 * A `null` (unauthenticated) user holds nothing.
 */
export function userHasPermission(user: User | null, perm: string): boolean {
  if (user === null) {
    return false;
  }
  const grants = (set: ReadonlySet<string>) => set.has(perm) || set.has(ADMIN_WILDCARD);
  return (
    user.memberships.some((m) => grants(m.permissions)) ||
    user.userRoles.some((r) => grants(r.permissions))
  );
}

/** The subset of `required` that `user` does **not** hold, in the given order. */
export function missingPermissions(user: User | null, required: readonly string[]): string[] {
  return required.filter((perm) => !userHasPermission(user, perm));
}

/**
 * Route-guard for TanStack Router's `beforeLoad`. Real enforcement (M19 slice
 * #5; closes the M13 friction no-op): if the viewer lacks any of `perms`, throw
 * a redirect to `/403` carrying the missing permission names in location state,
 * so `<ForbiddenPage>` can name them. A satisfied check returns without throwing
 * and the page renders.
 */
export function requirePermissions<P extends string>(
  ctx: RouterContext,
  perms: readonly P[],
): void {
  const missing = missingPermissions(ctx.user, perms);
  if (missing.length > 0) {
    // `HistoryState` is an empty interface; assert our shared `ForbiddenState`
    // shape into it so `<ForbiddenPage>` can read `state.missing`.
    throw redirect({ to: '/403', state: { missing } satisfies ForbiddenState as HistoryState });
  }
}
