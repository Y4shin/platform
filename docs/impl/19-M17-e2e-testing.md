# 19. M17 — Unified end-to-end testing (per-plugin Playwright)

> **Status:** 🚧 planned. Independent of [M14](16-M14-internationalization.md)/[M15](17-M15-rpc-service-macro.md)/[M16](18-M16-authoring-ergonomics.md);
> can land any time after [M13](14-M13-events-plugin.md) (which is the first plugin with enough UI —
> events list/detail/edit, invite, public pages — to make real browser E2E worth automating).

One-line goal: let **each plugin own its Playwright E2E specs** (`plugins/<name>/frontend/e2e/**`),
auto-discovered by a single **`task test:e2e`** that provisions the dev stack + a logged-in session,
and run that suite as part of **`task ci`** (in CI; opt-in locally) instead of the current
hand-gated, single-root `e2e/greetings.spec.ts`.

## Why this milestone exists

E2E is the one test layer the platform has *wired but not made routine*. Today:

- There's a single root [`e2e/`](../../e2e) dir with `playwright.config.ts` + one `greetings.spec.ts`;
  it doesn't scale to "every plugin tests its own pages."
- [`task test:e2e`](../../Taskfile.yml) only runs when `JUNIUS_E2E=1` and otherwise prints a skip — it
  needs the full dev stack **and a login**, which nothing provisions automatically.
- It is **not** in `task ci` (the gate runs lint + `test:rust` + `test:js`), so E2E never runs unless a
  human remembers to, against a stack they stood up by hand.
- The M13 dogfooding pass made the cost concrete: standing up the stack + getting a logged-in browser
  was the single biggest time sink (Authentik first-boot, WSL↔Docker networking, the no-permissions
  user, manual password setup). A plugin author should write a spec, not re-derive all that.

The deterministic component-level coverage (vitest, e.g. the widgets enabled/disabled fallback) already
runs in `test:js` and **stays there** — this milestone is specifically the *real-browser, full-stack*
layer.

## Outcome / acceptance

- A plugin author drops `plugins/<name>/frontend/e2e/foo.spec.ts` and it is **picked up automatically**
  — no central registration.
- `task test:e2e` (a) ensures the stack + app are up, (b) provides a **logged-in, permissioned**
  browser context via a shared fixture, (c) runs every discovered spec. One command, no manual login.
- `task ci` runs the E2E suite in CI (a dedicated job), green/red like any other gate; locally it stays
  opt-in (so `task ci` on a laptop without Docker doesn't block).
- Auth in E2E does **not** depend on driving the Authentik UI — a fixture seeds a session directly (the
  documented "CI seeds sessions against Postgres" approach), so specs are fast and deterministic.
- `events` ships the first real per-plugin suite (the M13 verification walk: create private→publish,
  group event, invite + slot-limit signup, public invite 404, `.ics`), proving the harness.

## Design

### 1. Discovery — specs live with the plugin

Playwright's config globs specs from every plugin plus a root dir for cross-plugin journeys:

```ts
// e2e/playwright.config.ts
testDir: '..',
testMatch: ['plugins/*/frontend/e2e/**/*.spec.ts', 'e2e/cross/**/*.spec.ts'],
```

A plugin opts in simply by having `frontend/e2e/*.spec.ts`; nothing else to register (mirrors how
`frontend/` presence already opts a plugin into FE wiring). Optionally surface the set in
`junius plugin info` and let `junius check` flag a spec that imports another plugin's internals.

### 2. Shared fixtures — a `@junius/e2e` package

A workspace package exporting Playwright fixtures so specs are declarative:

- **`baseURL`** — the running app origin (Vite `:5173` in dev; the embedded `juniusd` in CI).
- **`loginAs(user)`** — returns a browser context already authenticated as a seeded user with a chosen
  permission set (see §3). The page object starts logged in; no OIDC round-trip.
- **stack/data helpers** — seed/cleanup rows (groups, roles, memberships, plugin fixtures) over the
  platform pool, and assert side effects (e.g. a mailpit message, an `.ics` body).

```ts
import { test, expect } from '@junius/e2e';
test('alice publishes an event', async ({ page, loginAs }) => {
  await loginAs('alice', { permissions: ['events:read', 'events:write'] });
  await page.goto('/p/events'); /* … */
});
```

### 3. Auth without the Authentik UI

Driving the real OIDC flow in CI is slow and flaky (Authentik first-boot, the WSL/networking issues hit
in M13). Instead the `loginAs` fixture **seeds a session directly** — exactly what the auth tests /
[`dev/authentik/README.md`](../../dev/authentik/README.md) note CI does:

1. upsert a `platform.user` row for the test user;
2. grant permissions via a group/role/membership (the `dev/dev-seed.sql` pattern, generalized into the
   fixture);
3. insert a `platform.session` row and set the **`session` cookie** to its id — the host's session
   middleware reads the cookie as a plaintext session id (`platform/src/auth/session.rs`), so no token
   signing or Authentik interaction is needed.

This makes the **Authentik container optional for E2E** (it can be dropped from the CI stack subset),
removing the slowest, flakiest dependency. A separate, explicitly-tagged spec can still exercise the
*real* OIDC login against Authentik for coverage, gated/allowed to be slower.

### 4. Stack provisioning + app lifecycle

`task test:e2e` must be runnable from cold. It orchestrates, idempotently:

1. **Backing services** — the dev stack subset E2E needs: Postgres, RabbitMQ, MinIO, mailpit (and
   Authentik *only* for the opt-in real-login spec). Reuse `task infra:up`; wait on healthchecks.
2. **Migrate** — `junius migrate up` against the stack.
3. **App** — start the host. Two modes:
   - **dev** (local): `juniusd` (:18080) + Vite (:5173), as `junius dev` already does;
   - **CI**: the **embedded SPA** (`junius build --features embed-frontend` → run `juniusd`) — one
     origin, no Vite, closer to production and fewer moving parts. Playwright's `webServer` block can
     own start/stop + readiness.
4. **Run** Playwright; tear down on exit.

Playwright's built-in `webServer` + `globalSetup` handle (3)+(4); (1)+(2) are a small Task prelude (or
a `globalSetup` that shells `task infra:up` + migrate). The data fixtures (§2/§3) own per-test seeding.

### 5. CI wiring

- Add a **`ci:e2e`** Task target (mirrors the existing `ci:integration` shape: it needs Docker + a
  migrated stack). It provisions the subset, builds the embedded `juniusd`, and runs the discovered
  suite headless.
- Add it as a **fifth CI job** (the workflow already runs `lint`/`check`/`integration`/`deployment` in
  parallel — see [M12](13-M12-hardening.md)); E2E is the heaviest, so it stays its own job with the
  Playwright browser cache.
- **`task ci`** (the serial local gate) gains E2E **only when opted in** (Docker present /
  `JUNIUS_E2E=1`), so a laptop run without the stack still passes lint+unit. The skip message already
  models this; M17 keeps local opt-in but makes **CI always run it**.
- Cache the Playwright browser download in CI; pin the browser via `@playwright/test`.

## Stages

Repo norm: one **unsigned** commit per stage, `task ci` green per stage, sign+push the batch at the end.

- **Stage 1 — `@junius/e2e` fixtures + session-seed auth.** The fixtures package: `baseURL`, the
  `loginAs` session-seed fixture (§3), and data seed/cleanup helpers. Port the existing root
  `e2e/greetings.spec.ts` onto it as the proof. *Verify:* `JUNIUS_E2E=1 task test:e2e` runs greetings
  green against a hand-started stack, with **no manual login**.
- **Stage 2 — Per-plugin discovery.** Point `playwright.config.ts` at
  `plugins/*/frontend/e2e/**/*.spec.ts` (+ `e2e/cross/**`); move greetings' spec under
  `plugins/greetings/frontend/e2e/`. *Verify:* discovery picks it up with zero central registration;
  adding an empty second plugin spec is found automatically.
- **Stage 3 — One-command provisioning.** Make `task test:e2e` cold-start capable: Playwright
  `webServer` (embedded `juniusd`) + a `globalSetup` that ensures infra + migrate. *Verify:* from a
  clean machine (stack down), `JUNIUS_E2E=1 task test:e2e` goes green unattended.
- **Stage 4 — `events` suite (dogfood the harness).** The M13 verification walk as real specs:
  private→publish visibility, group event, invite + slot-limit signup refusal, public invite 404
  logged-out, an `.ics` fetch. *Verify:* the suite is green and exercises the user/group × public/private
  matrix through the browser.
- **Stage 5 — CI job + `task ci` opt-in.** `ci:e2e` target, the fifth workflow job (browser cache,
  embedded build), and the opt-in hook in `task ci`. *Verify:* CI runs E2E green; a deliberately broken
  spec fails the job; local `task ci` without Docker still passes (skips E2E with the message).
- **Stage 6 — Docs.** Authoring-guide section ("writing an E2E spec for your plugin"); note the
  fixture API + the session-seed login; update this file's status.

## Risks & mitigations

- **Flakiness** (the usual E2E tax). Mitigate: session-seed auth (no UI login), Playwright
  auto-waiting + web-first assertions, per-test data isolation (unique seed rows, cleanup), retries in
  CI only, trace-on-failure artifacts.
- **CI cost/time.** It's the heaviest job; keep it parallel + cached, run the embedded single-origin
  build (no Vite), and drop Authentik from the E2E stack subset (session-seed removes the dependency).
- **Cross-plugin test ownership.** A plugin spec should test *its* surface; cross-plugin journeys live
  in `e2e/cross/`. Optionally a `junius check` rule flags a plugin spec importing another plugin's
  internals.
- **The M13 environment traps** (WSL↔Docker, Authentik boot) bit *interactive* dev; the CI harness
  (Linux runners, embedded build, no Authentik) sidesteps them. Document the local opt-in caveat.

## Out of scope

- Replacing the deterministic **vitest** component tests (they stay in `test:js`).
- Visual-regression / screenshot diffing (a possible later add-on).
- Load/perf testing.

## Downstream doc updates

- [`README.md`](README.md) milestone index — add the M17 row.
- [`../plugin-authoring-guide.md`](../plugin-authoring-guide.md) — a "writing E2E tests" section.
- [`13-M12-hardening.md`](13-M12-hardening.md) — note the CI job count went 4 → 5.
- [`../design/14-decision-log.md`](../design/14-decision-log.md) — an M17 entry (per-plugin E2E +
  session-seed auth).
