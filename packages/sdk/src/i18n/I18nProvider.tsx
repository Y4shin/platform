/**
 * Lingui-backed i18n seam for `@junius/sdk` consumers.
 *
 * The Provider:
 * - picks an initial locale (User.locale → navigator.language → defaultLocale),
 * - dynamically imports the active locale's compiled catalogs (one per
 *   participating package — supplied via the `catalogs` prop),
 * - calls `i18n.load(locale, msgs)` + `i18n.activate(locale)` from `@lingui/core`,
 * - wraps children in Lingui's own `<I18nProvider>` so descendants of any plugin
 *   tree can use `t` / `<Trans>` from `@lingui/macro`.
 *
 * `useLocale().setLocale(...)` persists the choice via `POST /api/me/locale` and
 * re-activates Lingui against the new catalog set.
 */

import { i18n } from '@lingui/core';
import { I18nProvider as LinguiI18nProvider } from '@lingui/react';
import { createContext, type ReactNode, useCallback, useContext, useEffect, useState } from 'react';

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
   * Persist a new locale preference on the server and re-activate Lingui in
   * the active document. Awaiting the promise lets callers refresh queries
   * that embed locale-dependent strings.
   */
  setLocale: (code: LocaleCode) => Promise<void>;
}

const I18nContext = createContext<I18nContextValue | null>(null);

/**
 * A loader produces the compiled Lingui catalog for one locale, on demand.
 * Each participating package (host + each plugin) supplies one entry. The
 * Provider awaits every loader for the active locale and merges the
 * resulting message maps into a single Lingui `messages` object.
 *
 * Compiled catalogs come from `pnpm exec lingui compile` (run by
 * `junius i18n extract`), which writes `i18n/<locale>.js` next to the `.po`
 * source. Loaders are typically `() => import('./i18n/de.po')` — Lingui's
 * Vite plugin rewrites the `.po` import to the compiled module.
 */
export type CatalogLoader = (locale: LocaleCode) => Promise<{ messages: Record<string, string> }>;

export interface I18nProviderProps {
  /**
   * Deployment-wide default locale code. Used when the user has no stored
   * preference and `navigator.language` isn't one we ship. Should match the
   * host config's `default_locale`.
   */
  defaultLocale?: LocaleCode;
  /**
   * One catalog loader per participating package. The host typically passes
   * its own loader plus one for each plugin frontend; tests can pass an
   * empty array.
   */
  catalogs?: readonly CatalogLoader[];
  /**
   * Override the active locale (tests). When set, `setLocale` becomes a
   * client-side no-op past the in-memory state update + Lingui re-activation.
   */
  testOverride?: LocaleCode;
  children: ReactNode;
}

export function I18nProvider({
  defaultLocale = 'en',
  catalogs = [],
  testOverride,
  children,
}: I18nProviderProps) {
  const user = useUser();
  const initial: LocaleCode =
    testOverride ??
    normalise(user?.locale ?? null) ??
    normalise(typeof navigator !== 'undefined' ? navigator.language : null) ??
    defaultLocale;
  const [locale, setLocaleState] = useState<LocaleCode>(initial);
  // Tracks whether Lingui has finished loading the active locale at least once.
  // Until the first activation completes, the Lingui Provider would render
  // raw msgids; we render `null` during that flicker.
  const [ready, setReady] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const merged: Record<string, string> = {};
      for (const load of catalogs) {
        try {
          const cat = await load(locale);
          Object.assign(merged, cat.messages);
        } catch (err) {
          // A missing catalog (locale file not yet committed) is non-fatal;
          // Lingui's fallback path will render the source msgid.
          // eslint-disable-next-line no-console
          console.warn('[i18n] catalog load failed', { locale, err });
        }
      }
      if (cancelled) {
        return;
      }
      i18n.load(locale, merged);
      i18n.activate(locale);
      setReady(true);
    })();
    return () => {
      cancelled = true;
    };
  }, [locale, catalogs]);

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

  if (!ready) {
    return null;
  }

  return (
    <I18nContext.Provider value={{ locale, setLocale }}>
      <LinguiI18nProvider i18n={i18n}>{children}</LinguiI18nProvider>
    </I18nContext.Provider>
  );
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
