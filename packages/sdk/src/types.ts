/**
 * Shared types used by `@junius/sdk` consumers.
 *
 * At M04 these are intentionally lean — only the fields the dev-shell auth
 * stub exposes. Membership / per-group permissions arrive in M06.
 */

export type UserId = string;

export interface User {
  id: UserId;
  email: string;
  displayName: string;
  /** Empty in M04; populated by the real `/api/me` payload in M06. */
  memberships: ReadonlyArray<unknown>;
  /** Flat capability-permission set across all memberships. Empty in M04. */
  permissions: ReadonlySet<string>;
}

export interface RouterContext {
  user: User | null;
}
