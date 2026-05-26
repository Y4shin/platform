import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { AuthProvider } from '../auth/AuthProvider.js';
import type { User } from '../types.js';
import { I18nProvider, useLocale } from './I18nProvider.js';

function Probe() {
  const { locale, setLocale } = useLocale();
  return (
    <div>
      <span data-testid="locale">{locale}</span>
      <button
        type="button"
        onClick={() => {
          void setLocale('de');
        }}
      >
        de
      </button>
    </div>
  );
}

const USER_NO_PREF: User = {
  id: 'u1',
  email: 'a@local',
  displayName: 'A',
  locale: null,
  memberships: [],
};

const USER_DE: User = { ...USER_NO_PREF, locale: 'de' };

let fetchSpy: ReturnType<typeof vi.fn>;

beforeEach(() => {
  fetchSpy = vi.fn().mockResolvedValue(new Response(null, { status: 204 }));
  vi.stubGlobal('fetch', fetchSpy);
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('I18nProvider', () => {
  it('seeds the active locale from User.locale', () => {
    render(
      <AuthProvider user={USER_DE}>
        <I18nProvider>
          <Probe />
        </I18nProvider>
      </AuthProvider>,
    );
    expect(screen.getByTestId('locale').textContent).toBe('de');
  });

  it('falls back to the explicit default when User.locale is null', () => {
    render(
      <AuthProvider user={USER_NO_PREF}>
        <I18nProvider defaultLocale="en">
          <Probe />
        </I18nProvider>
      </AuthProvider>,
    );
    expect(screen.getByTestId('locale').textContent).toBe('en');
  });

  it('persists locale changes via POST /api/me/locale', async () => {
    render(
      <AuthProvider user={USER_NO_PREF}>
        <I18nProvider>
          <Probe />
        </I18nProvider>
      </AuthProvider>,
    );
    await act(async () => {
      screen.getByRole('button', { name: 'de' }).click();
    });
    expect(screen.getByTestId('locale').textContent).toBe('de');
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    const [url, init] = fetchSpy.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('/api/me/locale');
    expect(init.method).toBe('POST');
    expect(JSON.parse(init.body as string)).toEqual({ locale: 'de' });
  });

  it('ignores unknown locale codes and falls back to the default', async () => {
    function BadProbe() {
      const { locale, setLocale } = useLocale();
      return (
        <div>
          <span data-testid="locale">{locale}</span>
          <button
            type="button"
            onClick={() => {
              void setLocale('fr');
            }}
          >
            fr
          </button>
        </div>
      );
    }
    render(
      <AuthProvider user={USER_NO_PREF}>
        <I18nProvider defaultLocale="en">
          <BadProbe />
        </I18nProvider>
      </AuthProvider>,
    );
    await act(async () => {
      screen.getByRole('button', { name: 'fr' }).click();
    });
    // Falls back to default and the persist call carries 'en'.
    expect(screen.getByTestId('locale').textContent).toBe('en');
    const [, init] = fetchSpy.mock.calls[0] as [string, RequestInit];
    expect(JSON.parse(init.body as string)).toEqual({ locale: 'en' });
  });
});
