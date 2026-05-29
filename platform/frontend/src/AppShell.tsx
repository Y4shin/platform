import { Code, ConnectError, type Transport } from '@connectrpc/connect';
import { TransportProvider } from '@connectrpc/connect-query';
import {
  AuthProvider,
  type CatalogLoader,
  ComponentRegistryProvider,
  type ComponentRegistryValue,
  goToLoginUnlessPublic,
  I18nProvider,
} from '@junius/sdk';
import { QueryCache, QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { type AnyRouter, RouterProvider } from '@tanstack/react-router';
import { type ReactElement, StrictMode } from 'react';

import { componentRegistry } from './generated/component-registry.js';
import { PLUGIN_I18N_CATALOGS } from './generated/i18n-catalogs.js';
import { PUBLIC_ROUTE_PREFIXES } from './generated/routes.js';
import { loadShellI18n } from './i18n.js';

// Host shell catalog + plugin catalogs, in the same order both the embedded SPA
// and the M23 SSR variant pass to `<I18nProvider>`. Re-exported so the SSR
// entry can share one source of truth.
export const I18N_CATALOGS: readonly CatalogLoader[] = [loadShellI18n, ...PLUGIN_I18N_CATALOGS];

export type { ComponentRegistryValue };
export { componentRegistry, PUBLIC_ROUTE_PREFIXES };

export interface AppShellProps {
  /** Per-mount router (memory history on the server, browser history on
   * the client). The SPA entry creates one global router; the SSR pipeline
   * builds a fresh one per request. */
  router: AnyRouter;
  /** Connect-Web transport. Browser-side: relative `/rpc`. Server-side:
   * absolute internal URL + cookie forwarding (Stage 3). */
  transport: Transport;
}

/**
 * The provider tree shared by every host-frontend entry point: the SPA's
 * `main.tsx`, the M23 SSR entry, and the M23 hydration entry. Anything that
 * varies per topology (router instance, transport) is passed in; everything
 * else (i18n catalogs, component registry, the query-client error handler)
 * is stable and lives here.
 */
export function AppShell({ router, transport }: AppShellProps): ReactElement {
  // Built per render so server requests don't share a QueryClient. Returning a
  // fresh one each call also keeps StrictMode's double-mount honest in dev.
  const queryClient = new QueryClient({
    queryCache: new QueryCache({
      onError: (error) => {
        if (ConnectError.from(error).code === Code.Unauthenticated) {
          goToLoginUnlessPublic(PUBLIC_ROUTE_PREFIXES);
        }
      },
    }),
  });
  return (
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
    </StrictMode>
  );
}
