import { SUPPORTED_LOCALES, useLocale } from '@junius/sdk';

/**
 * Minimal locale picker for the host shell. Persists the choice via the
 * `useLocale` setter (which POSTs to `/api/me/locale`); a fuller, Lingui-aware
 * label set arrives in Stage E.
 */
const LABELS: Record<string, string> = {
  en: 'English',
  de: 'Deutsch',
  pseudo: 'Pseudo',
};

export function LocaleSwitcher() {
  const { locale, setLocale } = useLocale();
  return (
    <label className="text-fg-2 text-sm">
      <span className="sr-only">Locale</span>
      <select
        className="bg-surface-1 border-border rounded border px-2 py-1"
        value={locale}
        onChange={(e) => {
          void setLocale(e.target.value);
        }}
      >
        {SUPPORTED_LOCALES.map((code) => (
          <option key={code} value={code}>
            {LABELS[code] ?? code}
          </option>
        ))}
      </select>
    </label>
  );
}
