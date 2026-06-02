# Server-side rendering

Junius deployments can run in two frontend topologies: **embedded** (the default
— the browser bundle is served by the host and hydrates client-side) and
**split SSR** (a Node host renders on the server, the browser hydrates). Plugin
frontends are double-bundled for both. You rarely need to *target* SSR — you just
need to avoid a handful of gotchas so your plugin works in either topology.

## The rules of thumb

- **No top-level `window` / `document`.** Module-level access to browser globals
  throws during SSR. Wrap them in `useEffect`, guard with
  `typeof window !== 'undefined'`, or put them inside an event handler.

  ```tsx
  // ✗ throws on the server
  const width = window.innerWidth;

  // ✓ runs only in the browser
  useEffect(() => { setWidth(window.innerWidth); }, []);
  ```

- **No browser-only APIs in route loaders.** Loaders run on **both** sides.
  `localStorage`, `IntersectionObserver`, `window.matchMedia` belong inside
  components behind effect hooks, never in a loader.

- **Idempotent renders.** Server and client must produce the same HTML for the
  same inputs, or hydration mismatches show up as a visible flicker plus a
  console warning. Use `useId` for stable ids; never `Math.random()` in render.

- **Cookies and auth just work.** The SSR layer forwards the inbound `Cookie`
  header through an `AsyncLocalStorage` to a Connect interceptor, so
  `useQuery(service.method, …)` runs as the requesting user on the server with
  **no plugin-side wiring**. You call the backend the same way in both topologies
  ([Calling the backend](../frontend/calling-the-backend.md)).

## You don't have to opt in

Embedded mode is still the default, and plugins don't *target* SSR. Follow the
rules above and the SSR path works for any deployment that turns on split mode —
without you doing anything topology-specific. If you stick to standard React 19
patterns (effects for browser APIs, `useId` for ids, no module-level side
effects), you're already SSR-safe.

## Catching mistakes

The split-SSR E2E topology (`task ci:e2e:split`) exercises your plugin under
server rendering, so a hydration mismatch or a server-side `window` access shows
up in CI. If a spec passes embedded but fails split, look first for one of the
gotchas above.
