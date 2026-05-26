// Lingui runtime re-exports. The `t` / `<Trans>` / `<Plural>` macros are NOT
// re-exported: Lingui's babel plugin only transforms calls whose import source
// it recognises (`@lingui/react/macro`, `@lingui/core/macro`), so plugin code
// imports those directly. Everything that's pure runtime — the `useLingui`
// hook, the `i18n` singleton — goes through the SDK so the Provider stays
// one composition point.
export { i18n } from '@lingui/core';
export { useLingui } from '@lingui/react';

export type { CatalogLoader, I18nProviderProps, LocaleCode } from './I18nProvider.js';
export { I18nProvider, SUPPORTED_LOCALES, useLocale } from './I18nProvider.js';
