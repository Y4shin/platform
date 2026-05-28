# Example deployment — split (SSR FE + headless BE)

The M23 two-container topology. Pair this directory with the embedded
example (`../example-deployment/`) to compare:

|                | Embedded (`example-deployment/`)        | Split (this one)                       |
|----------------|------------------------------------------|----------------------------------------|
| Process count  | 1 (`juniusd`)                            | 2 (`juniusd` + `frontend-ssr`)         |
| FE delivery    | Embedded SPA via `rust-embed`            | Node SSR + hydration                   |
| Browser sees   | `juniusd` directly                       | `frontend-ssr` only (proxies `/api`)   |
| `[build]`      | `frontend = "embedded"` (default)        | `frontend = "none"`                    |
| Best for       | Single-binary self-contained deployments | SSR for first-paint / SEO, CDN-fronted FE, independent FE/BE scaling |

## When to pick split

Reach for split when at least one of these matters:

- **SSR** for public, SEO-sensitive routes (events, signup) or
  measurably better first-paint on slow links.
- **Operational separation**: independently scale the FE container, sit
  a CDN in front of it, keep the BE on a private subnet.
- **FE runtime evolution**: future swap to edge-runtime, per-plugin FE
  bundles, etc. without touching the BE.

Otherwise, embedded is simpler — one image, one health check, no Node
runtime to operate.

## What's in this directory

```
platform.toml                    # split-mode config ([build] frontend = "none")
docker-compose.yml               # juniusd + frontend-ssr + postgres + rabbitmq + minio
Dockerfile.juniusd               # (write per your prod base image) — M24 will publish one
Dockerfile.frontend-ssr          # (write per your prod base image) — M24 will publish one
README.md                        # this file
```

Once M24 ships the precompiled images on `ghcr.io`, drop the
`Dockerfile.*` files and flip the `build:` keys to `image:` tags.

## Run

```sh
# Export the secrets the config references (matches dev/docker-compose.yml's set).
export OIDC_CLIENT_SECRET=… SESSION_KEY=… ROLE_PW_SECRET=…
export STORAGE_TOKEN_SECRET=… MINIO_USER=minioadmin MINIO_PASS=minioadmin

docker compose up --build
```

Browse to `http://localhost:3000` — `view-source:` will show the
server-rendered HTML, not a hydration placeholder.

## Wiring notes

- The `frontend-ssr` container reaches `juniusd` over the compose
  network at `JUNIUS_BE_INTERNAL_URL=http://juniusd:18080`. Don't
  expose juniusd to the public internet — only `:3000` is published.
- `oidc_redirect_url` in `platform.toml` points at the FE origin
  (`http://localhost:3000/api/auth/callback`). The FE container's
  proxy forwards `/api/auth/*` to juniusd, so the callback round-trips
  correctly. Update this for your real domain.
- Cookie forwarding is automatic: the SSR layer threads inbound
  `Cookie` through an `AsyncLocalStorage` to the SSR Connect
  transport's per-request interceptor; the `/api` proxy passes
  cookies + `Set-Cookie` end-to-end.

## SSR considerations for plugin authors

Plugin frontends are double-bundled (SSR server + browser hydration).
A few rules of thumb keep them SSR-safe:

- **No top-level `window` / `document`.** Read them inside `useEffect`
  or behind `typeof window !== 'undefined'` checks. Top-level access
  throws during SSR.
- **No browser-only APIs in route loaders.** Loaders run on both
  sides; `localStorage` / `window.matchMedia` / `IntersectionObserver`
  belong in components.
- **Idempotent rendering.** A render with the same router + props
  should produce the same HTML server-side and client-side, or
  hydration mismatches.
- **`useId` is your friend.** Generated IDs match across server and
  client — don't use `Math.random()` in render.

Most React-19-idiomatic code is already SSR-safe; these are the
common gotchas.
