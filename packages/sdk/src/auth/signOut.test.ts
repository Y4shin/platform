import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { signOut } from './AuthProvider.js';

// Deterministic counterpart to the e2e user-menu round-trip: it proves the
// request the e2e can't cheaply assert on (method + credentials) and the
// post-logout redirect, in isolation. Mocks `fetch` + `window.location`.

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

describe('signOut', () => {
  it('POSTs /api/auth/logout with credentials, then redirects to login', async () => {
    const fetchMock = vi.fn(async () => ({ ok: true, status: 204 }));
    vi.stubGlobal('fetch', fetchMock);

    await signOut();

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock).toHaveBeenCalledWith('/api/auth/logout', {
      method: 'POST',
      credentials: 'include',
    });
    // A fresh session starts at the plain login entry (no return_to).
    expect(assign).toHaveBeenCalledWith('/api/auth/login');
  });

  it('still redirects to login when the logout request fails', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => {
        throw new Error('network down');
      }),
    );

    // A failed logout must not strand the user in a half-authenticated SPA.
    await expect(signOut()).resolves.toBeUndefined();
    expect(assign).toHaveBeenCalledWith('/api/auth/login');
  });
});
