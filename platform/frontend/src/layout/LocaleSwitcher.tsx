import { SUPPORTED_LOCALES, useLocale } from '@junius/sdk';
import { useLingui } from '@lingui/react/macro';

/**
 * Minimal locale picker for the host shell. Persists the choice via the
 * `useLocale` setter (which POSTs to `/api/me/locale`); option labels go
 * through Lingui so the picker itself translates.
 */
export function LocaleSwitcher() {
  const { locale, setLocale } = useLocale();
  const { t } = useLingui();
  const labels: Record<string, string> = {
    en: t`English`,
    de: t`German`,
    pseudo: t`Pseudo`,
  };
  return (
    <label className="text-fg-2 text-sm">
      <span className="sr-only">{t`Locale`}</span>
      <select
        className="bg-surface-1 border-border rounded border px-2 py-1"
        value={locale}
        onChange={(e) => {
          void setLocale(e.target.value);
        }}
      >
        {SUPPORTED_LOCALES.map((code) => (
          <option key={code} value={code}>
            {labels[code] ?? code}
          </option>
        ))}
      </select>
    </label>
  );
}
