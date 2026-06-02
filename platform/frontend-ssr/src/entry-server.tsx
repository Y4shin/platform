/**
 * M23 SSR entry. Vite builds this into `dist/server/entry-server.js`; the
 * Node server (`server.ts`) imports `render(url)` per request.
 *
 * Each call builds a per-request router (memory history at the inbound URL)
 * and a per-request Connect transport that forwards the inbound cookie /
 * Accept-Language headers to the BE via the `requestContext` ALS store
 * the HTTP handler populates. The transport's `baseUrl` is the BE's
 * internal address (`JUNIUS_BE_INTERNAL_URL`); the browser-side hydration
 * entry uses a relative `/rpc` URL instead so cookies attach automatically.
 */

import type { Interceptor, Transport } from '@connectrpc/connect';
import { createConnectTransport } from '@connectrpc/connect-web';
import { AppShell } from '@junius/shell/app-shell';
import { routeTree } from '@junius/shell/routes';
import { createMemoryHistory, createRouter } from '@tanstack/react-router';
import { type PipeableStream, renderToPipeableStream } from 'react-dom/server';

import { requestContext } from './als.js';

/**
 * Connect interceptor that copies the inbound request's `Cookie` and
 * `Accept-Language` headers onto every outbound BE call within the same
 * async context. Calling code (TanStack Query, Connect-Query) doesn't have
 * to know about the SSR seam.
 */
const forwardInboundHeaders: Interceptor = (next) => async (req) => {
  const ctx = requestContext.getStore();
  if (ctx) {
    if (ctx.cookie) {
      req.header.set('Cookie', ctx.cookie);
    }
    if (ctx.acceptLanguage) {
      req.header.set('Accept-Language', ctx.acceptLanguage);
    }
  }
  return next(req);
};

function ssrTransport(): Transport {
  const base = process.env['JUNIUS_BE_INTERNAL_URL'];
  if (!base) {
    throw new Error(
      'JUNIUS_BE_INTERNAL_URL is required in split mode (set it to the BE container, e.g. http://backend:18080)',
    );
  }
  return createConnectTransport({
    baseUrl: `${base.replace(/\/$/, '')}/rpc`,
    interceptors: [forwardInboundHeaders],
  });
}

export interface RenderOpts {
  onShellError?: (err: unknown) => void;
  onError?: (err: unknown) => void;
}

export interface RenderResult {
  stream: PipeableStream;
}

export function render(url: string, opts: RenderOpts = {}): RenderResult {
  const history = createMemoryHistory({ initialEntries: [url] });
  // Seed the router context with a null viewer; AppShell injects the live user
  // once `/api/me` resolves (see platform/frontend/src/AppShell.tsx).
  const router = createRouter({ routeTree, history, context: { user: null } });
  const transport = ssrTransport();
  const stream = renderToPipeableStream(<AppShell router={router} transport={transport} />, {
    onShellError: opts.onShellError,
    onError: opts.onError,
  });
  return { stream };
}
