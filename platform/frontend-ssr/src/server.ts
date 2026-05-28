/**
 * M23 SSR FE host — Node HTTP server.
 *
 * Two modes, gated on `NODE_ENV`:
 *
 *   - **Development** (`NODE_ENV !== 'production'`): boots Vite in middleware
 *     mode so HMR works for both the client bundle and the SSR module.
 *     `ssrLoadModule` re-imports `entry-server.tsx` on change.
 *   - **Production** (`NODE_ENV === 'production'`): serves built static assets
 *     from `dist/client/` and `import()`s the SSR bundle from
 *     `dist/server/entry-server.js` once.
 *
 * Stage 3 will plug in the `/api` + `/h` + `/rpc` proxy and per-request cookie
 * forwarding. For Stage 2 the server SSRs every URL and returns the rendered
 * stream; BE calls during SSR run without forwarded cookies, so the rendered
 * HTML reflects the logged-out state and the post-hydration client re-fetches
 * with cookies attached. Stage 3 fixes that flash.
 */

import {
  createServer as createHttpServer,
  type IncomingMessage,
  type ServerResponse,
} from 'node:http';
import { Transform } from 'node:stream';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

import { createConnectTransport } from '@connectrpc/connect-web';
import type { Transport } from '@connectrpc/connect';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');
const IS_PROD = process.env['NODE_ENV'] === 'production';
const PORT = Number(process.env['PORT'] ?? 3000);

/** Resolved at boot in prod; lazily in dev via Vite's `ssrLoadModule`. */
type RenderFn = (typeof import('./entry-server.js'))['render'];

interface ViteDevServer {
  middlewares: (req: IncomingMessage, res: ServerResponse, next: (err?: unknown) => void) => void;
  transformIndexHtml: (url: string, html: string) => Promise<string>;
  ssrLoadModule: <T = unknown>(id: string) => Promise<T>;
  ssrFixStacktrace: (err: Error) => void;
}

async function buildServer(): Promise<{
  vite: ViteDevServer | null;
  getTemplate: (url: string) => Promise<string>;
  getRender: () => Promise<RenderFn>;
}> {
  if (IS_PROD) {
    const { readFileSync } = await import('node:fs');
    const template = readFileSync(resolve(ROOT, 'dist/client/index.html'), 'utf-8');
    const mod = (await import(resolve(ROOT, 'dist/server/entry-server.js'))) as typeof import(
      './entry-server.js'
    );
    return {
      vite: null,
      getTemplate: () => Promise.resolve(template),
      getRender: () => Promise.resolve(mod.render),
    };
  }
  // Dev: Vite middleware mode. `createServer` is dynamic so the prod bundle
  // doesn't pull Vite as a runtime dependency.
  const vite = (await import('vite')).createServer({
    root: ROOT,
    server: { middlewareMode: true },
    appType: 'custom',
  });
  const resolved = (await vite) as unknown as ViteDevServer;
  const { readFileSync } = await import('node:fs');
  const rawTemplate = readFileSync(resolve(ROOT, 'index.html'), 'utf-8');
  return {
    vite: resolved,
    getTemplate: (url) => resolved.transformIndexHtml(url, rawTemplate),
    getRender: async () => {
      const mod = await resolved.ssrLoadModule<typeof import('./entry-server.js')>(
        '/src/entry-server.tsx',
      );
      return mod.render;
    },
  };
}

/**
 * Connect transport used during SSR. Stage 3 will wrap this in a per-request
 * variant that forwards the inbound `Cookie` header; for Stage 2 it's a bare
 * absolute-URL transport pointing at the BE container's internal address.
 */
function ssrTransport(): Transport {
  const base = process.env['JUNIUS_BE_INTERNAL_URL'];
  if (!base) {
    throw new Error(
      'JUNIUS_BE_INTERNAL_URL is required in split mode (set it to the BE container, e.g. http://backend:18080)',
    );
  }
  return createConnectTransport({ baseUrl: `${base.replace(/\/$/, '')}/rpc` });
}

async function main(): Promise<void> {
  const { vite, getTemplate, getRender } = await buildServer();

  const server = createHttpServer(async (req, res) => {
    const url = req.url ?? '/';
    if (vite) {
      // Let Vite handle module requests, assets, HMR; fall through to our SSR
      // handler for anything else.
      await new Promise<void>((resolveOnce) => {
        vite.middlewares(req, res, () => resolveOnce());
      });
      if (res.headersSent || res.writableEnded) return;
    }
    try {
      const template = await getTemplate(url);
      const render = await getRender();
      const [head, tail] = template.split('<!--ssr-outlet-->');
      if (!head || !tail) {
        res.writeHead(500, { 'Content-Type': 'text/plain' });
        res.end('SSR template missing <!--ssr-outlet--> marker');
        return;
      }
      res.writeHead(200, { 'Content-Type': 'text/html' });
      res.write(head);
      // React's `PipeableStream` calls `end()` on the writable when done,
      // which would close the response before we can append the closing tags.
      // Pipe through a Transform that lets us flush the tail after React
      // finishes streaming.
      const tailWriter = new Transform({
        transform(chunk: Buffer, _enc, cb) {
          cb(null, chunk);
        },
        flush(cb) {
          cb(null, tail);
        },
      });
      tailWriter.pipe(res);
      const { stream } = render(url, {
        transport: ssrTransport(),
        onError(err: unknown) {
          // Logged but not surfaced to the client mid-stream; React continues
          // with an empty placeholder for the failing subtree.
          console.error('[ssr] render error', err);
        },
      });
      stream.pipe(tailWriter);
    } catch (err) {
      if (vite && err instanceof Error) {
        vite.ssrFixStacktrace(err);
      }
      console.error('[ssr] handler error', err);
      if (!res.headersSent) {
        res.writeHead(500, { 'Content-Type': 'text/plain' });
      }
      res.end('Internal Server Error');
    }
  });

  server.listen(PORT, () => {
    console.log(`[ssr] listening on http://localhost:${PORT} (${IS_PROD ? 'prod' : 'dev'})`);
  });
}

main().catch((err) => {
  console.error('[ssr] boot failed', err);
  process.exit(1);
});
