import { Code, ConnectError } from '@connectrpc/connect';
import { TransportProvider } from '@connectrpc/connect-query';
import { createConnectTransport } from '@connectrpc/connect-web';
import { AuthProvider, ComponentRegistryProvider, goToLogin, isPublicPath } from '@junius/sdk';
import { QueryCache, QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createRouter, RouterProvider } from '@tanstack/react-router';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { componentRegistry } from './generated/component-registry.js';
import { PUBLIC_ROUTE_PREFIXES, routeTree } from './generated/routes.js';

import './styles.css';

// When any query fails with an unauthenticated RPC error (expired/absent
// session), send the browser to login — except on a public (login-optional)
// path, where an Unauthenticated RPC is expected and must not redirect.
// AuthProvider applies the same allowlist to the initial /api/me check.
const queryClient = new QueryClient({
  queryCache: new QueryCache({
    onError: (error) => {
      if (
        ConnectError.from(error).code === Code.Unauthenticated &&
        !isPublicPath(PUBLIC_ROUTE_PREFIXES)
      ) {
        goToLogin();
      }
    },
  }),
});
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

createRoot(container).render(
  <StrictMode>
    <AuthProvider publicPaths={PUBLIC_ROUTE_PREFIXES}>
      <QueryClientProvider client={queryClient}>
        <TransportProvider transport={transport}>
          <ComponentRegistryProvider registry={componentRegistry}>
            <RouterProvider router={router} />
          </ComponentRegistryProvider>
        </TransportProvider>
      </QueryClientProvider>
    </AuthProvider>
  </StrictMode>,
);
