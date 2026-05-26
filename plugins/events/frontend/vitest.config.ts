import { lingui } from '@lingui/vite-plugin';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  // The Lingui Vite plugin rewrites `.po` imports to the compiled JS
  // catalogs; loading it here lets vitest exercise `loadI18n('pseudo')`
  // the same way production code does (the same plugin is configured in
  // `platform/frontend/vite.config.ts`).
  plugins: [lingui()],
  test: {
    environment: 'jsdom',
    globals: true,
  },
});
