# 15. Open Questions — Resolution Schedule

This doc maps the nine open questions in [../design/13-open-questions.md](../design/13-open-questions.md) to the milestone where each must be resolved, with the recommended direction. **Every recommendation requires user confirmation before the gating milestone starts** — defaults are starting points, not silent decisions (see [00-approach.md](00-approach.md) §0.3).

When you confirm a direction below, also:
1. Note it in [../design/14-decision-log.md](../design/14-decision-log.md) so the source-of-truth design folder reflects the lock.
2. Update the relevant milestone doc's "Open questions resolved" block if needed.

## Resolution table

| # | Open question | Gated milestone | Recommended direction | Status |
|---|---|---|---|---|
| 1 | Testing strategy | **Before M03** | Three layers, concrete tooling — see §15.1 | proposed |
| 2 | Asset handling | **Before M04** | Per-plugin assets, Vite-bundled — see §15.2 | proposed |
| 3 | Hot reload across plugin boundaries | **Validated during M04** | Expect pnpm symlinks + Vite to give clean HMR; fallback documented — see §15.3 | proposed, validate empirically |
| 4 | Multi-tenancy | **Before M06** | Single-tenant locked for v1 — see §15.4 | proposed |
| 5 | Audit logging | **Before M06** | Host-wide `platform.audit_event`, 365-day default retention — see §15.5 | proposed |
| 6 | Trust model & threat model | **Before M07** | First-party plugins only in v1 — see §15.6 | proposed |
| 7 | Internationalization | **Before M13** | Defer; English-only with a `t()` shim — see §15.7 | proposed |
| 8 | Worked example & plugin authoring guide | **Emerges M03–M05, finalised at M13** | Use Speakers (M13) as the worked example — see §15.8 | proposed, executes through implementation |
| 9 | v0 milestone definition | **Defined by this plan** | M05 is the v0 cut — see §15.9 | locked by this plan |

## §15.1 Testing strategy (gated before M03)

**Recommended:** three layers.

- **Unit** — `cargo test -p <crate>` for Rust, `vitest` + `@testing-library/react` for TS. Each plugin owns its own unit tests.
- **Integration** — `cargo test --workspace` driving the full bound platform binary via `reqwest`; ephemeral Postgres via `testcontainers-modules` for DB-touching tests.
- **End-to-end** — `playwright` running against `junius dev` in CI.

**Locked in:** M03 establishes these as the conventions. Every later milestone's verification step uses them.

**If you'd rather:** use `wiremock` for HTTP-level mocking instead of real `reqwest` round-trips, or pick `cypress` over playwright — change M03's "Library choices" section and propagate to every later milestone's verification step.

## §15.2 Asset handling (gated before M04)

**Recommended:** per-plugin assets live under `plugins/<name>/frontend/src/assets/` and are imported via Vite (URL or inline import). All assets get bundled into the FE artifact and embedded via `rust-embed`. No separate static-hosting layer.

**Why:** keeps the "single binary" promise from [../design/01-goals-and-constraints.md](../design/01-goals-and-constraints.md); avoids CDN setup for v1.

**Locked in:** M04.

**If assets grow large** (e.g. video tutorials): break out to S3/Storage (M10) and serve via signed URLs from the binary. Not a v1 concern.

## §15.3 Hot reload across plugin boundaries (validate during M04)

**Recommended:** trust pnpm workspace symlinks + Vite to give clean HMR through plugin package boundaries. The setup is standard ("workspace package consumes another workspace package's source via symlinks") and Vite's HMR runs across symlink boundaries by default.

**Validation plan during M04:**
1. With `hello` plugin shipping a FE page, run `junius dev`.
2. Edit `plugins/hello/frontend/src/routes/pages/HelloPage.tsx` → page should hot-update without reload.
3. Edit `packages/sdk/src/auth/AuthProvider.tsx` → shell should hot-update.

If either fails, fall back options (in order of effort):
- (a) Tweak Vite config (`server.watch.usePolling`, `resolve.preserveSymlinks: false`).
- (b) Run a "watch-rebuild" pass per workspace package via `pnpm --filter ... --parallel dev`.
- (c) Last resort: per-plugin Vite build with module federation. Significant work; only chosen if (a) and (b) both fail.

**Locked in:** M04 documents the empirically chosen approach.

## §15.4 Multi-tenancy (gated before M06)

**Recommended:** single-tenant. Each organisation that uses the platform runs **its own deployment** (its own `platform.toml`, its own binary, its own database). The deployment workflow from M11 makes spinning up additional org-deployments straightforward.

**Why:**
- Aligns with the source/deployment split — orgs are deployments.
- Keeps every table simple (no `org_id` columns, no row-level security, no per-tenant grants).
- Matches the design's "political-domain platform" framing — political orgs typically aren't shared infrastructure.

**Cost of changing later:** moving from single-tenant to multi-tenant is a full schema migration (add `org_id` everywhere, add RLS or per-tenant Postgres schemas, rewrite every query). This is a large project. The cost of the wrong choice is much higher than the cost of confirming now.

**Locked in:** M06.

## §15.5 Audit logging (gated before M06)

**Recommended:**

- **Schema** — host-wide `platform.audit_event` table with columns `id` (uuid PK), `event_kind` (TEXT, `'<plugin>:<resource>.<verb>'`), `actor_user_id` (UUID FK or NULL for system actions), `resource_kind` (TEXT, `'<plugin>:<table>'`), `resource_id` (UUID, nullable for non-resource events), `details` (JSONB), `occurred_at` (TIMESTAMPTZ). Indexed by actor, resource, and time.
- **API** — `PluginResources.audit.emit(event_kind, actor_user_id, resource_kind, resource_id, details)` (async, never panics on failure — falls back to `tracing::warn!`).
- **Capability** — plugins must declare `audit.emit` in `[requires.capabilities]` to call the API.
- **What's automatically audited** — every `authz.share`/`unshare`/`record_owner` call emits an event automatically (M08). Plugin-domain events are at the plugin author's discretion, encouraged by the authoring guide (M13).
- **Retention** — default 365 days, configurable per deployment via `[config.audit].retention_days`. Enforcement is a host job that runs daily (M10).
- **Reader access** — a "platform admin" group (created during deployment bootstrapping) has a role with `audit:read` permission; a future plugin or shell UI exposes it.

**Locked in:** M06 lands the schema + API; M08 adds the first plugin-side usage; M10 adds the retention job.

## §15.6 Trust model & threat model (gated before M07)

**Recommended:** **first-party plugins only in v1.**

- All plugin code lives in the source monorepo (in-tree or via git submodule).
- All plugin changes go through code review.
- Capability declarations (`[requires.capabilities]`) are audit-only — the platform doesn't enforce them at runtime in v1 because all plugin code is trusted.
- Plugin authors are trusted to follow the design's patterns (Repository, PluginCtx, authz). `junius check` catches accidents; malicious code is out of scope.

**Implications:**
- No plugin sandboxing.
- No code-signing for plugins.
- No per-plugin resource quotas.
- Capability-not-declared errors at runtime exist primarily to nudge plugin authors toward keeping manifests honest; they aren't a security boundary.

**If we ever want third-party plugins:** a separate design doc kicks off, layering on runtime capability enforcement, sandboxing (probably via WASM or a separate Postgres role + crate isolation), and a plugin-review process. Not in scope until then.

**Locked in:** M07, where the permission system goes live, makes this explicit.

## §15.7 Internationalization (gated before M13)

**Recommended:** **defer.** v1 ships English-only.

To keep the future seam clean:
- `@junius/sdk` exports a no-op `t(key, vars?) => interpolate(key, vars)` shim. Plugin authors write `t('Save speaker')` from M13 onward.
- Strings include English content; the keys are themselves human-readable, so the no-op behaviour is sensible.
- When real i18n lands, the shim is replaced with a real implementation (likely `lingui`, `i18next`, or `react-intl`); strings get extracted into a catalog by tooling.

**Locked in:** M13 introduces the `t()` shim.

**If you need i18n for v1:** pick a library now, swap the shim, and update M13's "Library choices" + the authoring guide.

## §15.8 Worked example & plugin authoring guide (emerges M03–M05, finalised at M13)

**Recommended:** the guide grows incrementally:
- M03 — minimal "Hello, plugin" example covering scaffolding + a single route.
- M05 — extends with proto + RPC + Connect-Query.
- M07/M08 — adds permissions + repository pattern + resource ownership.
- M13 — full worked example using Speakers as the case study; document is locked at this point.

The guide lives at `docs/plugin-authoring-guide.md` (separate from the design + impl folders, since it targets future plugin authors rather than designers/implementers).

**Locked in:** M13 finalises the guide.

## §15.9 v0 milestone definition (defined by this plan)

**Locked: v0 = end of M05.**

A v0 cut means: the architecture's shape is fully proven end-to-end (workspace → junius → platform core → first plugin → frontend shell → Connect-RPC), without persistence, permissions, ownership, or cross-plugin composition. Everything beyond M05 adds depth, not new architectural shapes — so M05 is the right point to stop, demo, and (if desired) take a beat before continuing.

The plan continues straight through to M13 because the design's promise depends on the post-v0 milestones — particularly the repository pattern + permission system (M07), resource ownership (M08), and cross-plugin composition (M09). Stopping at v0 would leave the design fundamentally unvalidated.

**Locked in:** this plan.
