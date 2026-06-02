import { Code, ConnectError, type Transport } from '@connectrpc/connect';
import { TransportProvider } from '@connectrpc/connect-query';
import {
  AuthProvider,
  type CatalogLoader,
  ComponentRegistryProvider,
  type ComponentRegistryValue,
  type ForbiddenState,
  goToLoginUnlessPublic,
  I18nProvider,
  useAuth,
} from '@junius/sdk';
import { QueryCache, QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { type AnyRouter, type HistoryState, RouterProvider } from '@tanstack/react-router';
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
        const connectError = ConnectError.from(error);
        if (connectError.code === Code.Unauthenticated) {
          // No session → hand off to the host login (full redirect).
          goToLoginUnlessPublic(PUBLIC_ROUTE_PREFIXES);
        } else if (connectError.code === Code.PermissionDenied) {
          // Authenticated but under-permissioned → the real 403, naming the
          // missing permission when the denial message carries it.
          const state = {
            missing: missingPermissionsFromError(connectError),
          } satisfies ForbiddenState as HistoryState;
          void router.navigate({ to: '/403', state });
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
                <RouterWithUser router={router} />
              </ComponentRegistryProvider>
            </TransportProvider>
          </QueryClientProvider>
        </I18nProvider>
      </AuthProvider>
    </StrictMode>
  );
}

// Injects the live viewer into the router context once `<AuthProvider>` has
// resolved `/api/me`, so `requirePermissions` guards in `beforeLoad` enforce
// against the real permission set rather than the `null` seed from `main.tsx`.
function RouterWithUser({ router }: { router: AnyRouter }): ReactElement {
  const { user } = useAuth();
  return <RouterProvider router={router} context={{ user }} />;
}

// The host's `permission_denied` errors carry "missing permission: <perm>"
// (see ApiError::forbidden); surface that name on `/403` when present.
function missingPermissionsFromError(error: ConnectError): string[] {
  const match = /missing permission:\s*(.+)/i.exec(error.rawMessage);
  return match?.[1] ? [match[1].trim()] : [];
}
