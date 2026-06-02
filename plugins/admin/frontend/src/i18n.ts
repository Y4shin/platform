/// <reference types="vite/client" />

import type { CatalogLoader } from '@junius/sdk';

const catalogs = import.meta.glob<{ messages: Record<string, string> }>('./i18n/*.po');

export const loadI18n: CatalogLoader = async (locale) => {
  const importer = catalogs[`./i18n/${locale}.po`];
  if (!importer) {
    return { messages: {} };
  }
  return importer();
};
