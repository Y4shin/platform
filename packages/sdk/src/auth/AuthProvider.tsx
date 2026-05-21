/**
 * Dev-mode authentication provider stub.
 *
 * Always returns a hard-coded local-dev user. Real OIDC + session lookup via
 * `/api/me` arrives in M06; the same `useUser` / `useIsAuthenticated` hook
 * contract carries through unchanged so consumer code doesn't need to move.
 */

import { createContext, type ReactNode, useContext, useMemo } from 'react';

import type { User } from '../types.js';

const DEV_USER: User = {
  id: 'dev',
  email: 'dev@local',
  displayName: 'Dev User',
  memberships: [],
  permissions: new Set<string>(),
};

interface AuthContextValue {
  user: User | null;
  isAuthenticated: boolean;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export interface AuthProviderProps {
  /** Optional override; tests inject a specific user. */
  user?: User | null;
  children: ReactNode;
}

export function AuthProvider({ user = DEV_USER, children }: AuthProviderProps) {
  const value = useMemo(() => ({ user, isAuthenticated: user !== null }), [user]);
  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (ctx === null) {
    throw new Error('useAuth must be called inside an <AuthProvider>');
  }
  return ctx;
}
