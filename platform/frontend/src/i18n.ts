/// <reference types="vite/client" />
/**
 * Lingui catalog loader for the host shell. Glob-imports `i18n/*.po` so the
 * Vite plugin compiles each on demand; `<I18nProvider catalogs={...}>` calls
 * this with the active locale.
 */

import type { CatalogLoader } from '@junius/sdk';

const catalogs = import.meta.glob<{ messages: Record<string, string> }>('../i18n/*.po');

export const loadShellI18n: CatalogLoader = async (locale) => {
  const importer = catalogs[`../i18n/${locale}.po`];
  if (!importer) {
    return { messages: {} };
  }
  return importer();
};
