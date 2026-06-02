/// <reference types="vite/client" />
/**
 * Lingui catalog loader for the events plugin. `import.meta.glob` produces one
 * importer per `i18n/*.po` file; the `@lingui/vite-plugin` rewrites the import
 * to the compiled JS module on demand. The host shell's
 * `<I18nProvider catalogs={...}>` awaits this loader for the active locale.
 */

import type { CatalogLoader } from '@junius/sdk';

const catalogs = import.meta.glob<{ messages: Record<string, string> }>('../i18n/*.po');

export const loadI18n: CatalogLoader = async (locale) => {
  const importer = catalogs[`../i18n/${locale}.po`];
  if (!importer) {
    return { messages: {} };
  }
  return importer();
};
