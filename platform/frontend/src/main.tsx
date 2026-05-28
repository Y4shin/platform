import { createConnectTransport } from '@connectrpc/connect-web';
import { createRouter } from '@tanstack/react-router';
import { createRoot } from 'react-dom/client';

import { AppShell } from './AppShell.js';
import { routeTree } from './generated/routes.js';

import './styles.css';

const router = createRouter({ routeTree });

// Single Connect-Web transport for the whole app. Backend RPC routes are
// mounted under /rpc (see platform/src/server.rs); Connect-Web computes
// `<baseUrl>/<service.typeName>/<method>`, so the proto package name
// (e.g. `hello.v1.HelloService`) does the plugin scoping.
const transport = createConnectTransport({ baseUrl: '/rpc' });

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router;
  }
}

const container = document.getElementById('root');
if (!container) {
  throw new Error('root element missing from index.html');
}

createRoot(container).render(<AppShell router={router} transport={transport} />);
