/**
 * Per-request `AsyncLocalStorage` carrying the inbound headers the SSR
 * transport forwards to the BE on each Connect RPC. The Node HTTP handler
 * calls `requestContext.run({...}, render)` for every request; the
 * Connect interceptor inside `entry-server.tsx` reads the store on each
 * outbound BE call.
 *
 * Keeping the headers in async-context rather than threading a `ctx`
 * argument through every React loader lets plugin code stay
 * transport-agnostic: `useQuery(eventsService.list)` works the same in
 * SPA mode and in SSR mode.
 */

import { AsyncLocalStorage } from 'node:async_hooks';

export interface SsrRequestContext {
  /** Verbatim `Cookie` header from the inbound request (may be ''). */
  cookie: string;
  /** Inbound `Accept-Language`, forwarded so the BE can localize. */
  acceptLanguage?: string;
}

export const requestContext = new AsyncLocalStorage<SsrRequestContext>();
