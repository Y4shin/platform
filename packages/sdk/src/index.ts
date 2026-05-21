export type { AuthProviderProps } from './auth/AuthProvider.js';

export { AuthProvider, useAuth } from './auth/AuthProvider.js';
export { useIsAuthenticated } from './auth/useIsAuthenticated.js';
export { useCurrentUser, useUser } from './auth/useUser.js';
export { requirePermissions } from './permissions/requirePermissions.js';

export {
  useHasAllPermissions,
  useHasAnyPermission,
  useHasPermission,
} from './permissions/useHasPermission.js';
export type {
  ComponentRegistry,
  ComponentRegistryProviderProps,
} from './registry/ComponentRegistryProvider.js';

export {
  ComponentRegistryProvider,
  useComponentRegistry,
} from './registry/ComponentRegistryProvider.js';
export { useComponent } from './registry/getComponent.js';
export type { RouterContext, User, UserId } from './types.js';
