/**
 * Authentication provider.
 *
 * On mount it fetches `/api/me`: a 200 yields the current user; a 401 redirects
 * the browser to `/api/auth/login` (preserving the current path as `return_to`).
 * Tests (and any caller that already has a user) can pass the `user` prop to
 * skip the fetch. The `useUser` / `useIsAuthenticated` hook contract is
 * unchanged.
 */

import { createContext, type ReactNode, useContext, useEffect, useMemo, useState } from 'react';

import type { Membership, User } from '../types.js';

type AuthStatus = 'loading' | 'authed' | 'unauth';

interface AuthContextValue {
  user: User | null;
  isAuthenticated: boolean;
  status: AuthStatus;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export interface AuthProviderProps {
  /**
   * Override the current user instead of fetching `/api/me`. `undefined` (the
   * default) fetches; an explicit `User` or `null` is used as-is without
   * fetching or redirecting (tests, storybook, SSR).
   */
  user?: User | null;
  /**
   * Path prefixes that are login-optional: still attempt `/api/me` (so a
   * logged-in visitor is upgraded), but on a 401 stay unauthenticated and
   * render children instead of redirecting to login. Used for public pages
   * like a plugin's `/i/<name>` invite surface.
   */
  publicPaths?: string[];
  children: ReactNode;
}

export function AuthProvider({ user: override, publicPaths, children }: AuthProviderProps) {
  const hasOverride = override !== undefined;
  const [user, setUser] = useState<User | null>(hasOverride ? override : null);
  const [status, setStatus] = useState<AuthStatus>(
    hasOverride ? (override ? 'authed' : 'unauth') : 'loading',
  );

  useEffect(() => {
    if (hasOverride) {
      return;
    }
    let cancelled = false;
    fetchMe()
      .then((fetched) => {
        if (cancelled) {
          return;
        }
        if (fetched) {
          setUser(fetched);
          setStatus('authed');
        } else {
          setStatus('unauth');
          goToLoginUnlessPublic(publicPaths);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setStatus('unauth');
          goToLoginUnlessPublic(publicPaths);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [hasOverride, publicPaths]);

  const value = useMemo<AuthContextValue>(
    () => ({ user, isAuthenticated: user !== null, status }),
    [user, status],
  );

  // Hold rendering until the first /api/me resolves, so children never observe a
  // transient null user mid-fetch.
  if (status === 'loading') {
    return null;
  }

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (ctx === null) {
    throw new Error('useAuth must be called inside an <AuthProvider>');
  }
  return ctx;
}

/** Whether the current path is under one of the login-optional prefixes. */
export function isPublicPath(publicPaths?: string[]): boolean {
  return (publicPaths ?? []).some((p) => window.location.pathname.startsWith(p));
}

/** Redirect the browser to the host login, preserving the current path. */
export function goToLogin(): void {
  const returnTo = encodeURIComponent(window.location.pathname + window.location.search);
  window.location.assign(`/api/auth/login?return_to=${returnTo}`);
}

/**
 * End the current session and return to login for a fresh one. POSTs
 * `/api/auth/logout` (with credentials so the session cookie is sent), then
 * redirects to `/api/auth/login`. The redirect is unconditional: a failed
 * logout request must never strand the user in a half-authenticated SPA, so we
 * swallow the error and still send them out to re-authenticate.
 */
export async function signOut(): Promise<void> {
  try {
    await fetch('/api/auth/logout', { method: 'POST', credentials: 'include' });
  } catch {
    // Ignore: redirect to login regardless (see doc comment).
  }
  window.location.assign('/api/auth/login');
}

/**
 * The single "should an unauthenticated state send us to login?" decision,
 * consulted by *every* redirect path (AuthProvider's initial `/api/me`, and the
 * app's `queryClient.onError` for `Unauthenticated` RPCs). It redirects unless
 * the current path is login-optional; centralizing it means a new public-route
 * surface can't be allow-listed in one place and forgotten in the other.
 *
 * Returns whether it redirected (handy for tests / short-circuiting).
 */
export function goToLoginUnlessPublic(publicPaths?: string[]): boolean {
  if (isPublicPath(publicPaths)) {
    return false;
  }
  goToLogin();
  return true;
}

interface WireRole {
  id: string;
  name: string;
}
interface WireMembership {
  groupId: string;
  groupName: string;
  role: WireRole;
  permissions: string[];
}
interface WireUserRoleGrant {
  roleId: string;
  roleName: string;
  permissions: string[];
}
interface WireUser {
  id: string;
  email: string;
  displayName: string;
  locale: string | null;
  memberships: WireMembership[];
  userRoles?: WireUserRoleGrant[];
}

/** Fetch the current user, or `null` on 401 / any non-200. */
async function fetchMe(): Promise<User | null> {
  const res = await fetch('/api/me', { credentials: 'include' });
  if (res.status !== 200) {
    return null;
  }
  const raw = (await res.json()) as WireUser;
  return {
    id: raw.id,
    email: raw.email,
    displayName: raw.displayName,
    locale: raw.locale ?? null,
    memberships: (raw.memberships ?? []).map(
      (m): Membership => ({
        groupId: m.groupId,
        groupName: m.groupName,
        role: { id: m.role.id, name: m.role.name },
        permissions: new Set(m.permissions ?? []),
      }),
    ),
    userRoles: (raw.userRoles ?? []).map((r) => ({
      roleId: r.roleId,
      roleName: r.roleName,
      permissions: new Set(r.permissions ?? []),
    })),
  };
}
