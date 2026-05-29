import { defineConfig } from 'vite';

import baseConfig from './vite.config';

/**
 * Vite config for bundling the Node SSR entry (`src/server.ts`) into a
 * single self-contained JS file at `dist/node-server/server.js`. M24
 * frontend image runs `node dist/node-server/server.js` against this
 * bundle — no tsx, no source files, no workspace symlinks at runtime,
 * which keeps the image small.
 *
 * `noExternal: true` inlines every workspace + npm dep into the bundle
 * (otherwise the runtime would still need a populated `node_modules`
 * tree). `vite` itself is marked external because the dev-mode branch
 * lazily imports it — production never reaches that import, but
 * rollup needs the marker to avoid bundling Vite into the production
 * artifact.
 */
export default defineConfig({
  ...baseConfig,
  ssr: {
    ...baseConfig.ssr,
    noExternal: true,
    // `vite` is only imported in the lazy dev-mode branch and stays
    // external; the production runtime image won't have it installed
    // but never reaches that code path.
    external: ['vite'],
  },
  build: {
    ...baseConfig.build,
    ssr: 'src/server.ts',
    outDir: 'dist/node-server',
    rollupOptions: {
      external: ['vite'],
    },
    // Node 22 — no transpilation back to older targets.
    target: 'node22',
  },
});
