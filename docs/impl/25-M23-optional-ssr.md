# 25. M23 — Optional SSR mode (split FE/BE deployment)

> **Status:** 🚧 planned. **★ Top priority** — together with [M24](26-M24-precompiled-containers.md),
> this milestone jumps the queue ahead of M18–M22. M24 depends on this
> (the `frontend` and `backend` images it ships are the split topology
> introduced here).
>
> No plugin-API change. The `Plugin` trait, `junius-sdk`, `plugin.toml`,
> the per-plugin `frontend/` shape — all unchanged. The only thing that
> moves is the *delivery* of the FE bundle (still embedded by default;
> optionally hosted by a separate Node process that also does SSR).

One-line goal: add a second supported deployment topology where the
frontend runs in its own container as an SSR server and the backend
(`juniusd`) ships **without** the embedded SPA. Both topologies remain
first-class — the embedded-SPA mode is unchanged and stays the default.

## Why this milestone exists

Today `juniusd` is the only process the browser talks to: it serves the
SPA bundle (via the `embed-frontend` cargo feature wired up in
[platform/src/server.rs:215](../../platform/src/server.rs#L215) and the
`rust-embed` integration from M04), the API, and the static assets. That
is the right default for a self-contained single-binary deployment.

It is the wrong default for deployments that want:

- **SSR** for first-paint latency, SEO of public pages (events, public
  group pages, signup), or fewer hydration round-trips on slow links.
- **Operational separation** between the FE and the API tier — e.g. CDN
  in front of the FE container, BE on a private subnet, or independent
  horizontal scaling.
- **A path to swapping FE infrastructure** (e.g. edge runtime, future
  per-plugin FE bundles) without touching the BE binary.

Adding SSR cleanly requires a JS runtime to execute React. The
embed-in-Rust route (`deno_core`, `rquickjs`) was ruled out as too much
complexity for the payoff. The Node-sidecar route is the standard
pattern and is what this milestone adopts — but only as an **opt-in
second topology**, not as a replacement.

## Outcome / acceptance

Two deployment topologies, both green:

**A. Embedded (today, unchanged).** `juniusd` built with
`--features embed-frontend` serves the SPA from one binary. A real
deployment built with `junius build` and the existing
[examples/example-deployment](../../examples/example-deployment)
continues to work bit-identically. Verified by the existing E2E suite.

**B. Split (new).** Two containers, configured with the same
`platform.toml`:
- `juniusd` built without `embed-frontend`. Serves the API only. Returns
  404 on `/` and on all FE-asset paths (no more silent fallback).
- A Node FE server (TanStack Start, see §0.2 default) that
  - SSRs every plugin route the user's permissions allow,
  - hydrates on the client and continues as a normal SPA from there,
  - proxies `/api/*` (and `/h/*`, the platform helper namespace) to the
    BE container,
  - forwards the inbound session cookie on every server-side BE call so
    SSR runs **as the requesting user**, not as a service principal.

  Verified by:
  - A new `e2e/split-mode/` Playwright sub-suite that boots both
    containers against the testcontainers stack and runs a curated slice
    of the existing M17 suite (login, events list, event create, audit
    log) end-to-end through the FE container.
  - `curl -I http://fe/events` returns server-rendered HTML containing
    the event list (no `<div id="root"></div>` placeholder).
  - Cookie forwarding: the SSR'd event list shows the bob-specific
    events when bob's cookie is on the request and the alice-specific
    events when alice's is.

Either topology is a fully supported v1 — the docs, the deployment
example folder, and the precompiled images from M24 cover both.

## Design

### Stage 1 — Make the headless BE a first-class build mode

`embed-frontend` already exists as a cargo feature
([platform/Cargo.toml:21](../../platform/Cargo.toml#L21)) but the
no-embed path is effectively a fallback ([server.rs:210-213](../../platform/src/server.rs#L210-L213)
returns a placeholder route). Promote the no-embed path to a real mode:

- When `embed-frontend` is off, `base_app()` returns a `Router` that
  does **not** mount `/`, does **not** handle FE asset paths, and
  returns a 404 with a structured error body so misrouted requests are
  obvious.
- `junius build` learns `[build] frontend = "embedded" | "none"` in
  `platform.toml` (default: `"embedded"`, matches today). `"none"`
  drops the `--features embed-frontend` flag and skips the `pnpm
  --filter @junius/shell build` step ([tools/junius/src/commands/build.rs:78-98](../../tools/junius/src/commands/build.rs#L78-L98)).
- `junius dev` is **unaffected**: it always runs Vite + juniusd
  side-by-side already. The split-mode dev story is its own stage (4).

Stage 1 verification: `junius build` with `[build] frontend = "none"`
produces a binary that boots, serves `/api/me` correctly, and returns
404 on `/`. No Node anywhere yet.

### Stage 2 — Stand up the SSR FE host

A new workspace package `platform/frontend-ssr/` (sibling of
`platform/frontend/`). It depends on the same plugin frontends and the
same `@junius/shell` route composition; only the *entry point* differs.

Shape:
- `server.ts` — Node HTTP server (default port `3000`). For each
  inbound request:
  1. Match the URL against the composed plugin route tree (same
     `buildRoutes` output the SPA uses).
  2. Resolve loader data — but route loaders that fetch from the BE use
     the **server transport** (§3) with the inbound cookie forwarded.
  3. `renderToPipeableStream(<App router={...} />)` → streamed HTML
     response with the route data serialised for hydration.
  4. Set the same `Cache-Control` / `Content-Type` headers as a Vite
     SSR build expects.
- `entry-server.tsx` — the SSR React entry (creates a per-request
  router with server-flavoured transport).
- `entry-client.tsx` — the hydration entry. Reuses the existing
  `@junius/shell` mount logic with `hydrateRoot` instead of
  `createRoot`.
- `vite.config.ts` — Vite SSR build (`vite build --ssr entry-server.tsx`
  + `vite build entry-client.tsx`) produces the two bundles the Node
  server needs.

Static-asset serving: Vite's SSR output includes a hashed asset manifest
the Node server reads to emit the correct `<script>` / `<link>` tags
per page. Static assets are served by the Node process from
`platform/frontend-ssr/dist/client/` (no separate CDN required for v1,
though one can be put in front).

Stage 2 verification: `pnpm --filter @junius/shell-ssr dev` boots a Node
process that SSRs the dashboard against a locally-running juniusd, and
view-source shows server-rendered HTML, not a hydration placeholder.

### Stage 3 — API base URL split + auth cookie forwarding

The Connect transport in
[packages/client](../../packages/client) takes a single base URL today.
Split it:

- **Client transport** (browser): base URL is **relative** (`/api`),
  hits the FE container, which proxies to BE. Same-origin → cookies
  attached automatically. Set via `import.meta.env.PUBLIC_API_BASE` at
  client-bundle build time (default `/api`).
- **Server transport** (Node SSR): base URL is **absolute internal**
  (e.g. `http://backend:8080/api`), reads from
  `process.env.JUNIUS_BE_INTERNAL_URL` at runtime (mandatory in
  split mode; the Node server fails fast at boot if unset).

Cookie forwarding: the server transport accepts a per-request `Cookie`
header. The SSR entry stores the inbound `Cookie` on a Node
`AsyncLocalStorage` keyed by request; the transport reads from that
store. This keeps loader code unchanged — loaders just call
`useQuery(eventsService.list)` and the right cookies show up on the
outbound call.

`/api/*` proxy: the Node server mounts a thin pass-through proxy at
`/api/*` and `/h/*` (the M02 plugin handler namespace) using
`http-proxy` or `undici`. Streaming + WebSocket-safe. Same `Cookie` /
`Set-Cookie` pass-through. The proxy target reads from
`JUNIUS_BE_INTERNAL_URL` so server-side SSR fetches and browser
fetches end up at the same BE.

Auth header forwarding for non-cookie auth (M18's admin override path
uses a header) is the same pattern: forward whatever the inbound
request carries, with an allowlist of headers the SSR layer is allowed
to forward (don't blanket-forward; an explicit allowlist avoids leaking
internal-only headers between the two tiers).

Stage 3 verification: a Playwright test that logs in as bob, visits
`/events` with JS disabled (`page.setJavaScriptEnabled(false)`), and
asserts that the HTML response contains bob's events but not alice's.
Same test with alice's session shows the opposite. Confirms cookies
flow through SSR to the BE as the requesting user.

### Stage 4 — Dev mode for split topology

`junius dev` today boots Vite + juniusd. Extend with
`junius dev --frontend ssr` (or auto-detect from
`[build] frontend = "none"` in the dev `platform.toml`):

- Boots juniusd as today.
- Boots the SSR Node server in dev mode (Vite middleware mode — HMR
  works for SSR + client, hot-reloads plugin route changes).
- Boots **no** Vite SPA server in this mode (the SSR Node server *is*
  the FE origin).
- Dev `oidc_redirect_url` points at the SSR origin
  (`http://localhost:3000/api/auth/callback`), which proxies to BE.

Dev verification: `task dev:ssr` brings up the split stack and the
events plugin's CRUD round-trips through it with HMR working on both
the FE and (via `cargo watch`) BE sides.

### Stage 5 — Deployment example + docs

New `examples/example-deployment-split/`:
- `platform.toml` with `[build] frontend = "none"` and the split-mode
  config knobs filled in.
- `docker-compose.yml` showing the two-container topology
  (`juniusd` + `frontend-ssr`) plus the dev-stack peers (postgres,
  authentik, etc.).
- A short README explaining when to pick split vs embedded.

Plugin-authoring guide: a new "SSR considerations" section listing
the small set of things plugin authors must avoid in route loaders
(direct `window` references without an `isServer` guard, `document` in
top-level module code, etc.). Most React-19-idiomatic code is already
SSR-safe; the guide flags the common gotchas.

Design doc: append a §4.5 "Frontend delivery modes" to
[docs/design/04-frontend.md](../design/04-frontend.md) describing the
two topologies. A decision-log entry in
[docs/design/14-decision-log.md](../design/14-decision-log.md) records
the choice of TanStack Start over hand-rolled SSR (with the spike
notes from §0.2).

### Stage 6 — CI

New CI job `e2e-split`: builds the headless BE + the SSR FE, brings
both up via testcontainers, runs the curated split-mode Playwright
slice from §A. Parallel with the existing `e2e` job (which keeps
covering the embedded topology). Both must pass for a merge.

The `task ci:e2e` aggregate target gains a `:split` subtask.

## Library choices — confirm with user before starting

| Concern | Proposed default | Rationale | Downstream |
|---|---|---|---|
| SSR framework | **TanStack Start** | Built atop TanStack Router (already used), shares route definitions, supports code-based routes which plugins need. | M24 (`frontend` image) |
| Alternative if Start doesn't fit | Hand-rolled React 19 `renderToPipeableStream` + TanStack Router SSR primitives | Drop-in if Start's file-based routing fights plugin-contributed routes. | — |
| Node runtime | Node 22 LTS | Standard, mature, matches Vite/TanStack-Start expectations. | M24 (`frontend` image base) |
| Reverse proxy in FE container | `undici` `Agent.dispatch` | Tiny, native, streaming-safe, no extra dep tree. | — |
| Request-scoped state for cookie forwarding | `AsyncLocalStorage` (Node std) | Avoids threading a context object through every loader; standard pattern. | — |
| Client-side env var injection | `import.meta.env.PUBLIC_API_BASE` (Vite) | Already used elsewhere; build-time substitution. | — |
| Server-side internal API URL | `JUNIUS_BE_INTERNAL_URL` env var | Runtime; container-orchestrator-friendly. | M24 (image entrypoint) |

## Risks & mitigations

- **TanStack Start file-based routing vs plugin-contributed routes.**
  Start expects routes under `app/routes/`; plugins want to ship their
  own. Mitigation: confirmed in §0.2 that Start supports code-based
  routes via its `RouteTree` API; if a spike during Stage 2 finds the
  developer-experience too rough, swap to hand-rolled SSR.
- **SSR + plugin code-splitting interaction.** Each plugin's route
  module needs to be loadable both for SSR (server-side `import()`) and
  for hydration (client-side dynamic chunk). Mitigation: Vite handles
  this natively; the dual-bundle pattern (entry-server + entry-client)
  is exactly what its SSR mode is built for. The composed-route
  generator from M04 already emits per-plugin route modules.
- **Cookie domain mismatches in production.** If FE and BE are on
  different subdomains (`app.example.com` vs `api.example.com`),
  cookies won't reach the BE from the browser. Mitigation: the
  reference example proxies `/api` through the FE container so the
  browser only sees the FE origin; the proxy forwards cookies on the
  server side. Document this; advise against direct browser→BE in
  split mode.
- **SSR doubles the read load on the BE.** Every page render now hits
  the BE both server-side (SSR) and client-side (post-hydration). The
  client side can short-circuit by hydrating the server-fetched state,
  but if a loader is mis-typed and re-fetches on the client, load
  doubles. Mitigation: TanStack Query hydration handles this when set
  up correctly; the M23 Playwright tests assert `network.requestCount`
  doesn't double-count for SSR'd pages.
- **`junius dev` complexity grows.** Now there are *two* dev modes
  (embedded SPA + Vite proxy vs. SSR Node + BE). Mitigation: keep the
  default unchanged; `--frontend ssr` is opt-in; a CI smoke test
  exercises the SSR dev mode so it doesn't bit-rot.
- **Adds a Node runtime dependency to the project.** Today the host is
  Rust-only at runtime; embedded mode is unchanged but split mode adds
  Node. Mitigation: split mode is **opt-in**; the embedded path stays
  the default deployment story; no Rust crate gains a Node-only dep.

## Out of scope

- **Replacing the embedded mode.** Embedded stays a fully supported
  v1 topology — see [M11](12-M11-deployment-workflow.md). Split mode
  is additive.
- **Per-plugin separate FE bundles served from per-plugin containers.**
  Plugins still compose into one FE artifact (one `frontend-ssr`
  container per deployment). Per-plugin runtime hosts can be a later
  milestone if there's demand.
- **Edge-runtime SSR** (Cloudflare Workers, Deno Deploy, etc.). The
  SSR host is Node-on-a-VM-or-container. Edge support is a later
  question; the TanStack Start choice keeps the door open without
  committing.
- **Streaming SSR / RSC** (React Server Components). Standard
  `renderToPipeableStream` only. RSC is an order of magnitude more
  invasive on the plugin authoring model; defer.
- **CDN in front of the FE.** Operationally trivial to add (cache HTML
  for unauthenticated pages, asset URLs are hashed) — documented in the
  example README, not implemented as part of the milestone.
- **Backwards-compatibility translation** between the two modes. A
  deployment picks one and sticks with it; switching modes is a
  redeploy, not a hot operation.

## Verification

The milestone is done when, against a fresh checkout:

1. `task ci` (incl. the new `e2e-split` job) is green.
2. `cd examples/example-deployment-split && docker compose up` boots
   both containers. Browsing `http://localhost/events`:
   - View-source shows server-rendered HTML containing event rows.
   - `Network` panel shows no client-side re-fetch of the loader data
     immediately after hydration.
   - Logging in, creating an event, refreshing — all round-trip.
3. `cd examples/example-deployment && docker compose up` (the original
   embedded example) still boots and behaves identically to before this
   milestone (regression check).
4. A bob session served by SSR shows bob's events; an alice session
   served by SSR shows alice's. (Cookie forwarding correctness.)

## Downstream doc updates

- [docs/impl/README.md](README.md) — M23 row added with ★ priority
  marker; the priority callout below the table updated.
- [docs/design/04-frontend.md](../design/04-frontend.md) — new §4.5
  "Frontend delivery modes".
- [docs/design/14-decision-log.md](../design/14-decision-log.md) —
  M23 entry recording the TanStack Start choice + the Node sidecar
  decision.
- [docs/plugin-authoring-guide.md](../plugin-authoring-guide.md) —
  new "SSR considerations" section.
- [docs/impl/26-M24-precompiled-containers.md](26-M24-precompiled-containers.md)
  — depends on the `frontend` and `backend` build modes M23 introduces.
