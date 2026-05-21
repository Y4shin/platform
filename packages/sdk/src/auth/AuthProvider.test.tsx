import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { AuthProvider, useAuth } from './AuthProvider.js';

function UserName() {
  const { user, isAuthenticated } = useAuth();
  return (
    <>
      <span data-testid="name">{user?.displayName ?? 'none'}</span>
      <span data-testid="auth">{isAuthenticated ? 'yes' : 'no'}</span>
    </>
  );
}

describe('AuthProvider', () => {
  it('exposes the dev-user stub by default', () => {
    render(
      <AuthProvider>
        <UserName />
      </AuthProvider>,
    );
    expect(screen.getByTestId('name').textContent).toBe('Dev User');
    expect(screen.getByTestId('auth').textContent).toBe('yes');
  });

  it('reports anonymous when explicitly given null', () => {
    render(
      <AuthProvider user={null}>
        <UserName />
      </AuthProvider>,
    );
    expect(screen.getByTestId('name').textContent).toBe('none');
    expect(screen.getByTestId('auth').textContent).toBe('no');
  });

  it('throws when useAuth is used outside the provider', () => {
    // Suppress the React error-boundary noise.
    const spy = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    expect(() => render(<UserName />)).toThrow(/AuthProvider/);
    spy.mockRestore();
  });
});
