import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// Backend default bind address is 127.0.0.1:18080 (see platform/src/config.rs).
// We proxy every backend-served prefix here so the dev server can serve the
// SPA at :5173 and forward server-side calls to juniusd transparently.
const BACKEND = 'http://127.0.0.1:18080';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 5173,
    proxy: {
      '/h': BACKEND, // non-RPC plugin HTTP routes
      '/rpc': BACKEND, // Connect-RPC (lands in M05)
      '/api': BACKEND, // platform-internal HTTP (auth, /api/me — lands in M06)
    },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
  },
});
