import { isRedirect } from '@tanstack/react-router';
import { describe, expect, it } from 'vitest';

import type { Membership, User, UserRoleGrant } from '../types.js';
import { missingPermissions, requirePermissions, userHasPermission } from './requirePermissions.js';

// Deterministic counterpart to the `/403` e2e: it proves the guard's
// permission-matching + the redirect it throws, in isolation, without driving a
// browser. Derived from slice #5 AC1 ("requirePermissions on a route the viewer
// can't write blocks navigation and shows the missing permission").

function membership(...permissions: string[]): Membership {
  return {
    groupId: 'g1',
    groupName: 'Group',
    role: { id: 'r1', name: 'role' },
    permissions: new Set(permissions),
  };
}

function userRole(...permissions: string[]): UserRoleGrant {
  return { roleId: 'ur1', roleName: 'role', permissions: new Set(permissions) };
}

function user(opts: { memberships?: Membership[]; userRoles?: UserRoleGrant[] } = {}): User {
  return {
    id: 'u1',
    email: 'u@example.com',
    displayName: 'U',
    locale: null,
    memberships: opts.memberships ?? [],
    userRoles: opts.userRoles ?? [],
  };
}

describe('userHasPermission', () => {
  it('is true when a membership grants the exact permission', () => {
    expect(
      userHasPermission(user({ memberships: [membership('events:read')] }), 'events:read'),
    ).toBe(true);
  });

  it('is false when no membership/user-role grants it', () => {
    expect(
      userHasPermission(user({ memberships: [membership('events:read')] }), 'events:write'),
    ).toBe(false);
  });

  it('is false for an unauthenticated (null) user', () => {
    expect(userHasPermission(null, 'events:read')).toBe(false);
  });

  it('treats the admin wildcard on a user-role as holding every permission', () => {
    const admin = user({ userRoles: [userRole('*')] });
    expect(userHasPermission(admin, 'events:write')).toBe(true);
    expect(userHasPermission(admin, 'admin:audit.read')).toBe(true);
  });

  it('honours the wildcard on a membership too', () => {
    expect(userHasPermission(user({ memberships: [membership('*')] }), 'anything:goes')).toBe(true);
  });
});

describe('missingPermissions', () => {
  it('returns only the permissions the user lacks, preserving order', () => {
    const u = user({ memberships: [membership('events:read')] });
    expect(missingPermissions(u, ['events:read', 'events:write', 'admin:jobs.read'])).toEqual([
      'events:write',
      'admin:jobs.read',
    ]);
  });

  it('returns an empty array when all permissions are held', () => {
    const u = user({ memberships: [membership('events:read', 'events:write')] });
    expect(missingPermissions(u, ['events:read', 'events:write'])).toEqual([]);
  });
});

describe('requirePermissions', () => {
  it('throws a redirect to /403 carrying the missing permission(s) when the viewer lacks one', () => {
    const ctx = { user: user({ memberships: [membership('events:read')] }) };
    let thrown: unknown;
    try {
      requirePermissions(ctx, ['events:write']);
    } catch (e) {
      thrown = e;
    }
    expect(isRedirect(thrown)).toBe(true);
    const redirect = thrown as { options: { to: string; state: { missing: string[] } } };
    expect(redirect.options.to).toBe('/403');
    expect(redirect.options.state.missing).toEqual(['events:write']);
  });

  it('does not throw when the viewer holds every required permission', () => {
    const ctx = { user: user({ memberships: [membership('events:read', 'events:write')] }) };
    expect(() => requirePermissions(ctx, ['events:read', 'events:write'])).not.toThrow();
  });

  it('blocks an unauthenticated viewer with the full required set as missing', () => {
    let thrown: unknown;
    try {
      requirePermissions({ user: null }, ['events:read']);
    } catch (e) {
      thrown = e;
    }
    expect(isRedirect(thrown)).toBe(true);
    expect((thrown as { options: { state: { missing: string[] } } }).options.state.missing).toEqual(
      ['events:read'],
    );
  });
});
