/// <reference types="vite/client" />
/**
 * Lingui catalog loader for the hello plugin.
 *
 * `import.meta.glob` with `eager: false` produces one importer function per
 * `i18n/*.po` file. The Lingui Vite plugin rewrites the `.po` import to the
 * compiled JS module ({@link https://lingui.dev/ref/vite-plugin}); at runtime
 * we look up the active locale and call the importer.
 *
 * Exposed from the plugin's `index.ts` as `loadI18n`; the host shell's
 * `<I18nProvider catalogs={...}>` array contains one entry per participating
 * plugin so every package's catalog activates against the same locale.
 */

import type { CatalogLoader } from '@junius/sdk';

const catalogs = import.meta.glob<{ messages: Record<string, string> }>('./i18n/*.po');

export const loadI18n: CatalogLoader = async (locale) => {
  const importer = catalogs[`./i18n/${locale}.po`];
  if (!importer) {
    return { messages: {} };
  }
  return importer();
};
