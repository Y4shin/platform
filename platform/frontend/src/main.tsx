import { Code, ConnectError } from '@connectrpc/connect';
import { TransportProvider } from '@connectrpc/connect-query';
import { createConnectTransport } from '@connectrpc/connect-web';
import {
  AuthProvider,
  type CatalogLoader,
  ComponentRegistryProvider,
  goToLoginUnlessPublic,
  I18nProvider,
} from '@junius/sdk';
import { QueryCache, QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createRouter, RouterProvider } from '@tanstack/react-router';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { componentRegistry } from './generated/component-registry.js';
import { PLUGIN_I18N_CATALOGS } from './generated/i18n-catalogs.js';
import { PUBLIC_ROUTE_PREFIXES, routeTree } from './generated/routes.js';
import { loadShellI18n } from './i18n.js';

// Host shell catalog + plugin catalogs (codegen'd by `junius sync` against
// the deployment's plugin list — same source-of-truth as routes + the
// component registry).
const I18N_CATALOGS: readonly CatalogLoader[] = [loadShellI18n, ...PLUGIN_I18N_CATALOGS];

import './styles.css';

// When any query fails with an unauthenticated RPC error (expired/absent
// session), send the browser to login — except on a public (login-optional)
// path, where an Unauthenticated RPC is expected and must not redirect. The
// redirect decision is the shared goToLoginUnlessPublic helper, the same one
// AuthProvider's initial /api/me check uses, so both honour one allowlist.
const queryClient = new QueryClient({
  queryCache: new QueryCache({
    onError: (error) => {
      if (ConnectError.from(error).code === Code.Unauthenticated) {
        goToLoginUnlessPublic(PUBLIC_ROUTE_PREFIXES);
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
      <I18nProvider catalogs={I18N_CATALOGS}>
        <QueryClientProvider client={queryClient}>
          <TransportProvider transport={transport}>
            <ComponentRegistryProvider registry={componentRegistry}>
              <RouterProvider router={router} />
            </ComponentRegistryProvider>
          </TransportProvider>
        </QueryClientProvider>
      </I18nProvider>
    </AuthProvider>
  </StrictMode>,
);
