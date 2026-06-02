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
export type UserRoleId = string;

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

/**
 * A user-role assignment (M18). Global-scope — not per-group. A grant whose
 * `permissions` contains `'*'` makes the user a platform admin: every
 * permission check + every resource ACL fast-paths to true.
 */
export interface UserRoleGrant {
  roleId: UserRoleId;
  roleName: string;
  permissions: ReadonlySet<string>;
}

/** The wildcard permission an admin user-role holds. */
export const ADMIN_WILDCARD = '*';

export interface User {
  id: UserId;
  email: string;
  displayName: string;
  /**
   * Persisted locale preference (e.g. `"en"`, `"de"`). `null` means the host
   * falls back to `Accept-Language` and then the deployment default. Updated
   * via `useLocale().setLocale(...)` from `@junius/sdk`.
   */
  locale: string | null;
  memberships: ReadonlyArray<Membership>;
  /** Global-scope role assignments (M18). Empty for non-admin users. */
  userRoles: ReadonlyArray<UserRoleGrant>;
}

export interface RouterContext {
  user: User | null;
}

/**
 * Router history-state carried to `/403`: the permission names the viewer is
 * missing. Set by `requirePermissions` (route guard) and the `PermissionDenied`
 * query-error branch; read by `<ForbiddenPage>`. TanStack's `HistoryState` is an
 * empty interface by default, so this is the shared shape both ends agree on.
 */
export interface ForbiddenState {
  missing?: string[];
}
