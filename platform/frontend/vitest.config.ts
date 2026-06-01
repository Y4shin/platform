import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

// Mirrors the Lingui macro transform from vite.config.ts so `<Trans>` / `t`
// macros compile in unit tests exactly as they do in the app build.
export default defineConfig({
  plugins: [
    react({
      babel: {
        plugins: ['@lingui/babel-plugin-lingui-macro'],
      },
    }),
  ],
  test: {
    environment: 'jsdom',
    globals: true,
  },
});
