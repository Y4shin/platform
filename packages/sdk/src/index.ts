export type { AuthProviderProps } from './auth/AuthProvider.js';

export {
  AuthProvider,
  goToLogin,
  goToLoginUnlessPublic,
  isPublicPath,
  useAuth,
} from './auth/AuthProvider.js';
export { useIsAuthenticated } from './auth/useIsAuthenticated.js';
export { useCurrentUser, useUser } from './auth/useUser.js';
export { requirePermissions } from './permissions/requirePermissions.js';

export {
  useHasAllPermissions,
  useHasAnyPermission,
  useHasPermission,
} from './permissions/useHasPermission.js';
export type {
  ComponentRegistryProviderProps,
  ComponentRegistryValue,
} from './registry/ComponentRegistryProvider.js';

export {
  ComponentRegistryProvider,
  useComponentRegistry,
} from './registry/ComponentRegistryProvider.js';
export { useComponent } from './registry/getComponent.js';
export type {
  GroupId,
  Membership,
  Role,
  RoleId,
  RouterContext,
  User,
  UserId,
} from './types.js';
