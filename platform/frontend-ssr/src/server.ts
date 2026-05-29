/**
 * M23 SSR FE host — Node HTTP server.
 *
 * Three responsibilities:
 *
 *   1. **Proxy** the BE-served prefixes (`/api`, `/h`, `/rpc`) to the
 *      backend container. Streaming-safe; forwards `Cookie` /
 *      `Set-Cookie` end-to-end so login round-trips through the FE origin.
 *   2. **Static assets** from Vite's build output (or via Vite middleware
 *      in dev mode for HMR).
 *   3. **SSR** every remaining URL. The inbound `Cookie` + `Accept-Language`
 *      are placed on an `AsyncLocalStorage` store the SSR Connect transport
 *      reads on each BE call, so the rendered HTML reflects the requesting
 *      user.
 *
 * Two modes, gated on `NODE_ENV`:
 *
 *   - **Development** (`NODE_ENV !== 'production'`): Vite in middleware mode
 *     handles module/asset requests and HMR; `ssrLoadModule` re-imports
 *     `entry-server.tsx` on change.
 *   - **Production** (`NODE_ENV === 'production'`): serves built static assets
 *     from `dist/client/` and `import()`s the SSR bundle from
 *     `dist/server/entry-server.js` once.
 */

import {
  createServer as createHttpServer,
  request as httpRequest,
  type IncomingMessage,
  type ServerResponse,
} from 'node:http';
import { Transform } from 'node:stream';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

import { requestContext } from './als.js';

const __dirname = dirname(fileURLToPath(import.meta.url));
const IS_PROD = process.env['NODE_ENV'] === 'production';
// In dev mode (tsx runs `src/server.ts` directly), `__dirname` is the
// package's `src/` and ROOT = the package root. The Vite-mode branch
// below reads `index.html` from there; the prod-mode branch reads
// pre-built artifacts from `<ROOT>/dist/*`. After the M24
// `build:node-server` step bundles us into `dist/node-server/server.js`,
// `__dirname` becomes `<pkg>/dist/node-server` instead — so `ROOT`
// would be `<pkg>/dist` and the `dist/client/...` lookups would resolve
// to `<pkg>/dist/dist/...` (broken). Detect mode here and step the
// right number of levels.
const ROOT = IS_PROD ? resolve(__dirname, '..', '..') : resolve(__dirname, '..');
const PORT = Number(process.env['PORT'] ?? 3000);

/** Headers the FE proxy is allowed to forward from inbound → BE. An explicit
 * allowlist avoids leaking internal-only headers between tiers. */
const FORWARD_REQUEST_HEADERS = new Set([
  'accept',
  'accept-encoding',
  'accept-language',
  'content-length',
  'content-type',
  'cookie',
  'user-agent',
]);

/** Headers the FE proxy passes back to the browser from the BE response.
 * `Set-Cookie` is the load-bearing one (login + session lifetime). */
const FORWARD_RESPONSE_HEADERS = new Set([
  'cache-control',
  'content-encoding',
  'content-language',
  'content-length',
  'content-type',
  'etag',
  'expires',
  'last-modified',
  'location',
  'set-cookie',
  'vary',
]);

const BACKEND_PREFIXES = ['/api/', '/h/', '/rpc/'];

type RenderFn = (typeof import('./entry-server.js'))['render'];

interface ViteDevServer {
  middlewares: (req: IncomingMessage, res: ServerResponse, next: (err?: unknown) => void) => void;
  transformIndexHtml: (url: string, html: string) => Promise<string>;
  ssrLoadModule: <T = unknown>(id: string) => Promise<T>;
  ssrFixStacktrace: (err: Error) => void;
}

interface ResolvedServer {
  vite: ViteDevServer | null;
  getTemplate: (url: string) => Promise<string>;
  getRender: () => Promise<RenderFn>;
  beInternalUrl: string;
}

async function buildServer(): Promise<ResolvedServer> {
  const beInternalUrl = process.env['JUNIUS_BE_INTERNAL_URL'];
  if (!beInternalUrl) {
    throw new Error('JUNIUS_BE_INTERNAL_URL is required (split-mode BE address)');
  }

  if (IS_PROD) {
    const { readFileSync } = await import('node:fs');
    const template = readFileSync(resolve(ROOT, 'dist/client/index.html'), 'utf-8');
    // Static import: lets Vite's `build:node-server` bundle the whole
    // SSR tree (React + plugin frontends + entry-server) into the same
    // single-file server.js. Resolved at module-load time; getRender
    // just returns the already-imported `render` function.
    const { render } = await import('./entry-server.js');
    return {
      vite: null,
      getTemplate: () => Promise.resolve(template),
      getRender: () => Promise.resolve(render),
      beInternalUrl,
    };
  }
  const vite = (await (await import('vite')).createServer({
    root: ROOT,
    server: { middlewareMode: true },
    appType: 'custom',
  })) as unknown as ViteDevServer;
  const { readFileSync } = await import('node:fs');
  const rawTemplate = readFileSync(resolve(ROOT, 'index.html'), 'utf-8');
  return {
    vite,
    getTemplate: (url) => vite.transformIndexHtml(url, rawTemplate),
    getRender: async () => {
      const mod = await vite.ssrLoadModule<typeof import('./entry-server.js')>(
        '/src/entry-server.tsx',
      );
      return mod.render;
    },
    beInternalUrl,
  };
}

function shouldProxy(url: string): boolean {
  return BACKEND_PREFIXES.some((p) => url === p.slice(0, -1) || url.startsWith(p));
}

/** Forward an inbound request to the BE; stream the response back. */
function proxy(req: IncomingMessage, res: ServerResponse, beInternalUrl: string): void {
  const target = new URL(req.url ?? '/', beInternalUrl);
  const outHeaders: Record<string, string | string[]> = {};
  for (const [name, value] of Object.entries(req.headers)) {
    if (value === undefined) continue;
    if (FORWARD_REQUEST_HEADERS.has(name.toLowerCase())) {
      outHeaders[name] = value;
    }
  }
  // `Host` must reflect the BE for routing/virtual-host correctness; the
  // BE's session validator also keys on cookies, not Host, so this is safe.
  outHeaders['host'] = target.host;

  const proxyReq = httpRequest(
    {
      method: req.method,
      protocol: target.protocol,
      hostname: target.hostname,
      port: target.port,
      path: `${target.pathname}${target.search}`,
      headers: outHeaders,
    },
    (proxyRes) => {
      const respHeaders: Record<string, string | string[]> = {};
      for (const [name, value] of Object.entries(proxyRes.headers)) {
        if (value === undefined) continue;
        if (FORWARD_RESPONSE_HEADERS.has(name.toLowerCase())) {
          respHeaders[name] = value;
        }
      }
      res.writeHead(proxyRes.statusCode ?? 502, respHeaders);
      proxyRes.pipe(res);
    },
  );
  proxyReq.on('error', (err: Error) => {
    console.error('[proxy] upstream error', err);
    if (!res.headersSent) {
      res.writeHead(502, { 'Content-Type': 'text/plain' });
    }
    res.end('Bad Gateway');
  });
  req.pipe(proxyReq);
}

function inboundContext(req: IncomingMessage): {
  cookie: string;
  acceptLanguage: string | undefined;
} {
  const cookie = typeof req.headers.cookie === 'string' ? req.headers.cookie : '';
  const al = req.headers['accept-language'];
  const acceptLanguage = typeof al === 'string' ? al : Array.isArray(al) ? al[0] : undefined;
  return { cookie, acceptLanguage };
}

async function ssr(
  req: IncomingMessage,
  res: ServerResponse,
  { vite, getTemplate, getRender }: ResolvedServer,
): Promise<void> {
  const url = req.url ?? '/';
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
    // React's `PipeableStream` calls `end()` on the writable when done; piping
    // through this Transform lets us append the closing tags afterwards.
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
      onError(err: unknown) {
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
}

async function main(): Promise<void> {
  const ctx = await buildServer();

  const server = createHttpServer((req, res) => {
    const url = req.url ?? '/';
    // M24: container-orchestrator healthcheck. Returns 200 without touching
    // the BE so an SSR<>BE network partition doesn't mark the FE unhealthy.
    if (url === '/healthz') {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end('{"status":"ok"}');
      return;
    }
    if (shouldProxy(url)) {
      proxy(req, res, ctx.beInternalUrl);
      return;
    }
    const handle = (): void => {
      // Run SSR (and any nested Vite middleware) inside the per-request ALS
      // so the Connect transport's interceptor sees the inbound cookies.
      requestContext.run(inboundContext(req), () => {
        // Vite handles module/static requests in dev; falls through to SSR
        // for HTML routes.
        if (ctx.vite) {
          ctx.vite.middlewares(req, res, (err) => {
            if (err) {
              console.error('[vite] middleware error', err);
            }
            if (res.headersSent || res.writableEnded) return;
            void ssr(req, res, ctx);
          });
        } else {
          void ssr(req, res, ctx);
        }
      });
    };
    handle();
  });

  server.listen(PORT, () => {
    console.log(`[ssr] listening on http://localhost:${PORT} (${IS_PROD ? 'prod' : 'dev'})`);
  });
}

main().catch((err: unknown) => {
  console.error('[ssr] boot failed', err);
  process.exit(1);
});
