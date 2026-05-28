/**
 * M23 hydration entry. The SSR bundle delivers HTML; this script attaches
 * React's reconciler to the existing DOM via `hydrateRoot`, then the page
 * continues as a normal SPA.
 *
 * Browser-side, the Connect transport is relative (`/rpc`) and the request
 * goes through the FE container's proxy to the BE (cookies attached
 * automatically because origin matches).
 */

import { createConnectTransport } from '@connectrpc/connect-web';
import { AppShell } from '@junius/shell/app-shell';
import { routeTree } from '@junius/shell/routes';
import { createRouter } from '@tanstack/react-router';
import { hydrateRoot } from 'react-dom/client';

import '@junius/shell/styles.css';

const router = createRouter({ routeTree });

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router;
  }
}

// Browser transport: relative `/rpc`. The FE container's proxy (Stage 3)
// forwards to the BE. Stage 3 documents the exact split-mode wiring.
const transport = createConnectTransport({ baseUrl: '/rpc' });

const container = document.getElementById('root');
if (!container) {
  throw new Error('root element missing from index.html');
}
hydrateRoot(container, <AppShell router={router} transport={transport} />);
