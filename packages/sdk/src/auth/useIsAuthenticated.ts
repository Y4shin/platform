import { useAuth } from './AuthProvider.js';

export function useIsAuthenticated(): boolean {
  return useAuth().isAuthenticated;
}
