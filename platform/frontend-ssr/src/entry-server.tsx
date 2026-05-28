/**
 * M23 SSR entry. Vite builds this into `dist/server/entry-server.js`; the
 * Node server (`server.ts`) imports `render(url, opts)` per request.
 *
 * Each call builds a per-request router (memory history at the inbound URL)
 * and a per-request Connect transport, then streams the rendered React tree
 * via `react-dom/server`'s `renderToPipeableStream`.
 */

import { type Transport } from '@connectrpc/connect';
import { AppShell } from '@junius/shell/app-shell';
import { routeTree } from '@junius/shell/routes';
import { createMemoryHistory, createRouter } from '@tanstack/react-router';
import { type PipeableStream, renderToPipeableStream } from 'react-dom/server';

export interface RenderOpts {
  /** Connect-Web transport configured with the inbound cookie (Stage 3). */
  transport: Transport;
  /** Called by React when streaming runs into a recoverable error. */
  onShellError?: (err: unknown) => void;
  /** Called for errors after the shell has been committed. */
  onError?: (err: unknown) => void;
}

export interface RenderResult {
  stream: PipeableStream;
}

export function render(url: string, opts: RenderOpts): RenderResult {
  const history = createMemoryHistory({ initialEntries: [url] });
  const router = createRouter({ routeTree, history });
  const stream = renderToPipeableStream(
    <AppShell router={router} transport={opts.transport} />,
    {
      onShellError: opts.onShellError,
      onError: opts.onError,
    },
  );
  return { stream };
}
