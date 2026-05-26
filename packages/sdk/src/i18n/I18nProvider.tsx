/**
 * Library-agnostic i18n seam for `@junius/sdk` consumers.
 *
 * Plugins call `useLocale()` from this module to read the active locale and
 * persist a user-driven change. The Provider lives high in the host tree
 * (between `AuthProvider` and `TransportProvider`); it picks an initial
 * locale from the `User.locale` preference, falling back to `navigator.language`,
 * then to `defaultLocale`.
 *
 * Stage C ships this seam; **Stage E plugs Lingui into it** (a `<Trans>` wrapper
 * + `t()` re-export will be added in the same module alongside `useLocale`).
 * Plugins thus depend only on `@junius/sdk`, not on the underlying i18n library —
 * a future swap stays internal.
 */

import { createContext, type ReactNode, useCallback, useContext, useMemo, useState } from 'react';

import { useUser } from '../auth/useUser.js';

/** A locale code (e.g. `"en"`, `"de"`). The backend rejects unknown codes. */
export type LocaleCode = string;

/** Locales the platform ships catalogs for. Mirrors `junius_sdk::Locale`. */
export const SUPPORTED_LOCALES: readonly LocaleCode[] = ['en', 'de', 'pseudo'] as const;

/** Mark a string as a known locale code (or `null` for "let backend decide"). */
function normalise(code: string | null | undefined): LocaleCode | null {
  if (!code) {
    return null;
  }
  const head = code.split(/[-_]/)[0]?.toLowerCase();
  if (head && SUPPORTED_LOCALES.includes(head)) {
    return head;
  }
  return null;
}

interface I18nContextValue {
  locale: LocaleCode;
  /**
   * Persist a new locale preference on the server and update the active
   * locale in-memory. Awaiting the promise lets callers refresh queries that
   * embed locale-dependent strings.
   */
  setLocale: (code: LocaleCode) => Promise<void>;
}

const I18nContext = createContext<I18nContextValue | null>(null);

export interface I18nProviderProps {
  /**
   * Deployment-wide default locale code. Used when the user has no stored
   * preference and `navigator.language` isn't one we ship. The host config's
   * `default_locale` should match this for consistency between SSR-less FE
   * resolution and backend resolution.
   */
  defaultLocale?: LocaleCode;
  /**
   * Override the active locale (tests). When set, `setLocale` becomes a
   * client-side no-op past the in-memory state update.
   */
  testOverride?: LocaleCode;
  children: ReactNode;
}

export function I18nProvider({ defaultLocale = 'en', testOverride, children }: I18nProviderProps) {
  const user = useUser();
  const initial: LocaleCode =
    testOverride ??
    normalise(user?.locale ?? null) ??
    normalise(typeof navigator !== 'undefined' ? navigator.language : null) ??
    defaultLocale;
  const [locale, setLocaleState] = useState<LocaleCode>(initial);

  const setLocale = useCallback<I18nContextValue['setLocale']>(
    async (code: LocaleCode) => {
      const next = normalise(code) ?? defaultLocale;
      if (!testOverride) {
        await persistLocale(next);
      }
      setLocaleState(next);
    },
    [defaultLocale, testOverride],
  );

  const value = useMemo<I18nContextValue>(() => ({ locale, setLocale }), [locale, setLocale]);
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

/** Read the active locale + a setter that persists user preference. */
export function useLocale(): I18nContextValue {
  const ctx = useContext(I18nContext);
  if (ctx === null) {
    throw new Error('useLocale must be called inside an <I18nProvider>');
  }
  return ctx;
}

async function persistLocale(code: LocaleCode): Promise<void> {
  const res = await fetch('/api/me/locale', {
    method: 'POST',
    credentials: 'include',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ locale: code }),
  });
  if (!res.ok) {
    throw new Error(`failed to persist locale: ${res.status}`);
  }
}
