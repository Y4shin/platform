import { lingui } from '@lingui/vite-plugin';
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

/**
 * Shared Vite config for the M23 SSR FE host. Two builds run off it:
 *
 *   - `pnpm build:client` — bundles `src/entry-client.tsx` (driven by
 *     `index.html`) into `dist/client/`, producing the asset manifest the
 *     server reads to emit `<script>`/`<link>` tags per page.
 *   - `pnpm build:server` — `vite build --ssr src/entry-server.tsx` writes
 *     the SSR bundle to `dist/server/`. Node's `server.ts` imports the
 *     exported `render(...)` from that bundle.
 *
 * Stage 4 will plug Vite's middleware mode into `server.ts` for dev/HMR.
 */
export default defineConfig({
  plugins: [
    lingui(),
    react({
      babel: {
        plugins: ['@lingui/babel-plugin-lingui-macro'],
      },
    }),
    tailwindcss(),
  ],
  ssr: {
    // The bundle must be pure ESM (Node 22 runs it as such); externalising
    // node_modules keeps it small + lets Node's resolver find them at
    // runtime. Workspace packages stay bundled so the SSR build picks up
    // every plugin's source.
    noExternal: [/@junius\//],
  },
  build: {
    emptyOutDir: true,
  },
});
