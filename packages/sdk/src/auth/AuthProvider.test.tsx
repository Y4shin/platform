import { render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { User } from '../types.js';
import { AuthProvider, goToLoginUnlessPublic, useAuth } from './AuthProvider.js';

function UserView() {
  const { user, isAuthenticated } = useAuth();
  return (
    <>
      <span data-testid="name">{user?.displayName ?? 'none'}</span>
      <span data-testid="auth">{isAuthenticated ? 'yes' : 'no'}</span>
    </>
  );
}

const SAMPLE_ME = {
  id: 'u1',
  email: 'alice@local',
  displayName: 'Alice',
  locale: null,
  memberships: [
    {
      groupId: 'g1',
      groupName: 'Committee',
      role: { id: 'r1', name: 'chair' },
      permissions: ['speakers:read'],
    },
  ],
};

const OVERRIDE_USER: User = {
  id: 'u2',
  email: 'bob@local',
  displayName: 'Bob',
  locale: null,
  memberships: [],
  userRoles: [],
};

let assign: ReturnType<typeof vi.fn>;
let originalLocation: Location;

beforeEach(() => {
  originalLocation = window.location;
  assign = vi.fn();
  Object.defineProperty(window, 'location', {
    configurable: true,
    value: { pathname: '/p/hello', search: '', assign },
  });
});

afterEach(() => {
  Object.defineProperty(window, 'location', { configurable: true, value: originalLocation });
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('AuthProvider', () => {
  it('renders the user from /api/me on success', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => ({ status: 200, json: async () => SAMPLE_ME })),
    );
    render(
      <AuthProvider>
        <UserView />
      </AuthProvider>,
    );
    const name = await screen.findByTestId('name');
    expect(name.textContent).toBe('Alice');
    expect(screen.getByTestId('auth').textContent).toBe('yes');
    expect(assign).not.toHaveBeenCalled();
  });

  it('redirects to login on 401', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => ({ status: 401, json: async () => ({}) })),
    );
    render(
      <AuthProvider>
        <UserView />
      </AuthProvider>,
    );
    await waitFor(() => expect(assign).toHaveBeenCalledTimes(1));
    expect(String(assign.mock.calls[0]?.[0])).toContain('/api/auth/login?return_to=');
  });

  it('does not redirect on a 401 when the path is login-optional', async () => {
    Object.defineProperty(window, 'location', {
      configurable: true,
      value: { pathname: '/i/events/abc123', search: '', assign },
    });
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => ({ status: 401, json: async () => ({}) })),
    );
    render(
      <AuthProvider publicPaths={['/i/events']}>
        <UserView />
      </AuthProvider>,
    );
    // Renders children as anonymous; never redirects to login.
    const auth = await screen.findByTestId('auth');
    expect(auth.textContent).toBe('no');
    expect(screen.getByTestId('name').textContent).toBe('none');
    expect(assign).not.toHaveBeenCalled();
  });

  it('uses an injected user without fetching', () => {
    const fetchSpy = vi.fn();
    vi.stubGlobal('fetch', fetchSpy);
    render(
      <AuthProvider user={OVERRIDE_USER}>
        <UserView />
      </AuthProvider>,
    );
    expect(screen.getByTestId('name').textContent).toBe('Bob');
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('treats an explicit null override as anonymous without redirecting', () => {
    const fetchSpy = vi.fn();
    vi.stubGlobal('fetch', fetchSpy);
    render(
      <AuthProvider user={null}>
        <UserView />
      </AuthProvider>,
    );
    expect(screen.getByTestId('name').textContent).toBe('none');
    expect(screen.getByTestId('auth').textContent).toBe('no');
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(assign).not.toHaveBeenCalled();
  });

  it('throws when useAuth is used outside the provider', () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    expect(() => render(<UserView />)).toThrow(/AuthProvider/);
    spy.mockRestore();
  });
});

describe('goToLoginUnlessPublic', () => {
  it('redirects (and reports it) on a private path', () => {
    // window.location.pathname is '/p/hello' from the outer beforeEach.
    expect(goToLoginUnlessPublic(['/i/events'])).toBe(true);
    expect(assign).toHaveBeenCalledTimes(1);
    expect(String(assign.mock.calls[0]?.[0])).toContain('/api/auth/login?return_to=');
  });

  it('does not redirect on a login-optional path', () => {
    Object.defineProperty(window, 'location', {
      configurable: true,
      value: { pathname: '/i/events/abc123', search: '', assign },
    });
    expect(goToLoginUnlessPublic(['/i/events'])).toBe(false);
    expect(assign).not.toHaveBeenCalled();
  });

  it('redirects when no public prefixes are configured', () => {
    expect(goToLoginUnlessPublic()).toBe(true);
    expect(assign).toHaveBeenCalledTimes(1);
  });
});
