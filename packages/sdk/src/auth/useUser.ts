import type { User } from '../types.js';
import { useAuth } from './AuthProvider.js';

export function useUser(): User | null {
  return useAuth().user;
}

/** Throws if the user isn't authenticated. Use only inside auth-required routes. */
export function useCurrentUser(): User {
  const user = useAuth().user;
  if (user === null) {
    throw new Error(
      'useCurrentUser called while unauthenticated; gate the route with requirePermissions',
    );
  }
  return user;
}
