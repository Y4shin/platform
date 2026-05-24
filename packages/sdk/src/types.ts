/**
 * Shared types used by `@junius/sdk` consumers.
 *
 * The `User` shape mirrors the host's `/api/me` payload (camelCase; see
 * `crates/junius-sdk/src/auth.rs`). Per-group permissions live on each
 * membership; the permission hooks (M07) read across them.
 */

export type UserId = string;
export type GroupId = string;
export type RoleId = string;

export interface Role {
  id: RoleId;
  name: string;
}

export interface Membership {
  groupId: GroupId;
  groupName: string;
  role: Role;
  /** Permission keys this membership's role grants (`<plugin>:<perm>`). */
  permissions: ReadonlySet<string>;
}

export interface User {
  id: UserId;
  email: string;
  displayName: string;
  memberships: ReadonlyArray<Membership>;
}

export interface RouterContext {
  user: User | null;
}
