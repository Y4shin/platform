# 26. M24 — Optional precompiled container mode

> **Status:** ✅ implemented (pending first live CI run). **★ Top
> priority** — alongside [M23](25-M23-optional-ssr.md), this
> milestone jumped the queue ahead of M18–M22. Depends on M23 for the
> `frontend` (SSR Node) and `backend` (headless `juniusd`) build
> modes.
>
> No plugin-API change. The `Plugin` trait, manifests, SDK — all
> unchanged. M24 adds an **additional deployment path**: pull a
> prebuilt image instead of running `junius build`.
>
> What landed: `junius sync --bundle-all` (Stage 1); `[build] mode =
> "source" | "precompiled"` on the existing `[build]` block +
> `JUNIUS_MODE` env override + a boot-time bundle/config check that
> refuses to start on a mismatch (Stage 2); a decision-log audit
> confirming every `junius sync` artifact is plugin-set-only and
> bundle-all needs no refactor (Stage 3); a new `source-build` cargo
> feature gating `Build`/`Cache`/`Plugin {Enable,Disable}` so the
> in-container CLI carries only the runtime-relevant commands
> (Stage 4); three Dockerfiles in `docker/{full,backend,frontend}.Dockerfile`
> + a `/healthz` route on both BE and FE (Stage 5); a GHA workflow
> publishing to `ghcr.io/<owner>/junius-<variant>:{latest,<short-sha>}`
> on every push to `main`, plus two example deployments
> (`examples/example-deployment-precompiled/{full,split}/`) and
> updated design docs (Stage 6).
>
> Two doc-recorded deviations from the original spec:
> - **`[build] mode`** lives on the existing `[build]` block (not a
>   new `[platform]` section) — colocates with the M23-added
>   `[build] frontend` knob.
> - **`/healthz`** instead of the doc's `/h/health`. The `/h/<name>/`
>   prefix is reserved per-plugin; `/healthz` is the standard
>   convention and needs no rule change.
>
> Deferred into a future M25 followups doc: cosign keyless signing,
> linux/arm64 multi-arch, `junius doctor` (bundle/config diff CLI),
> nightly precompiled-E2E run.

One-line goal: ship three official Docker images (`full`, `frontend`,
`backend`) built on every push to `main`, each bundling **every plugin
in the monorepo**, configurable at runtime via `platform.toml`. Provides
a "batteries-included, no Rust toolchain required" deployment path
alongside the existing source-build flow.

## Why this milestone exists

The current deployment story
([M11](12-M11-deployment-workflow.md)) requires every deployment to
build its own binary: install `junius`, run `junius build`, wait for
`cargo build --release` + `pnpm build`, get a binary out. That is the
right model when the deployment is selecting a *subset* of plugins, or
pinning a fork, or shipping a custom plugin. It is overkill when the
deployment wants the stock set and is happy to take "whatever's on
`main`."

The proposition:

- **One bundle, three flavours.** Every plugin in `plugins/` linked
  into the binary; `[plugins].enabled` in `platform.toml` declares
  which the deployment is opting into (and must match the bundle —
  see §0.3).
- **Three image variants** to cover the two M23 topologies plus the
  classic single-binary path:
  - `full` — `juniusd` built with `embed-frontend`, all plugins, all
    FE assets baked in. The "single container, traditional SPA" story.
  - `frontend` — the SSR Node server from M23, all plugin frontends
    bundled. Paired with `backend`.
  - `backend` — `juniusd` built without `embed-frontend`, all
    plugins. Pairs with `frontend` *or* with any third-party static
    asset host.
- **Published to ghcr.io** on every `main` push, tagged
  `<variant>-latest` and `<variant>-<short-sha>`. A deployment pulls
  the image, drops in a `platform.toml`, runs.

The current source-build path stays first-class. The two paths are
side-by-side, not replacements.

## Outcome / acceptance

- Three Dockerfiles (`docker/full.Dockerfile`,
  `docker/frontend.Dockerfile`, `docker/backend.Dockerfile`) producing
  three distinct images, each with all monorepo plugins bundled.
- The `full` and `backend` images additionally ship a **runtime-only
  `junius` CLI** at `/usr/local/bin/junius` — same binary, built
  without the `develop` *and* without the new `source-build` cargo
  features (see §3-bis). End surface in-container:
  `junius migrate {up, status}`, `junius check`, `junius i18n check`.
  Source-build commands (`build`, `cache`, `plugin {enable, disable}`)
  and source-tree-only commands (`sync`, `dev`, `new`, `rpc`) are
  compiled out. The `frontend` image doesn't ship `junius` — it has no
  DB connection and no `platform.toml` consumption.
- A new GitHub Actions workflow (`.github/workflows/images.yml`) that
  on every push to `main`:
  - Builds all three images (with layer caching).
  - Pushes to `ghcr.io/<org>/junius-<variant>:latest` and
    `ghcr.io/<org>/junius-<variant>:<short-sha>`.
  - Skips push on PRs (build-only as a smoke test).
- A new example deployment
  `examples/example-deployment-precompiled/` that uses the published
  images (compose example for both topologies — `full` standalone, and
  `frontend` + `backend` together).
- Boot-time bundle/config compatibility check: the binary verifies that
  the `[plugins].enabled` list in `platform.toml` matches the plugin
  set bundled into the image; mismatches fail fast with a clear error.
- Documentation: a new "Pick a deployment path" section in the ops
  guide laying out the source-build vs precompiled-image tradeoff.

Verified by:

- `docker pull ghcr.io/<org>/junius-full:latest && docker run ...`
  brings up a working juniusd against a stock dev `platform.toml` (with
  `[plugins].enabled` listing every monorepo plugin).
- `docker compose up` in
  `examples/example-deployment-precompiled/split/` brings up
  `frontend` + `backend`; logging in and creating an event works
  end-to-end (the M23 SSR Playwright slice run against the precompiled
  images, not just the source-built ones).
- A platform.toml missing one of the bundled plugins from `enabled`
  fails the boot check with the exact missing-plugin name.

## Design

### Stage 1 — "Bundle every plugin" build mode

Today `junius sync` writes
[platform/src/generated/plugins.rs](../../platform/src/generated/plugins.rs)
based on the deployment's `[plugins].enabled` list. For the precompiled
images, that list **is** every plugin in `plugins/`. Two ways to
arrange this:

**Option A (preferred):** a new `junius sync --bundle-all` flag that
ignores `[plugins].enabled` and generates `plugins.rs` from a
filesystem walk of `plugins/*/plugin.toml`. Stays one generated file;
junius is the source of truth. Used by the image-building workflow,
nothing else.

**Option B:** a separate `platform/src/generated/plugins_all.rs` with a
`bundled-plugins` cargo feature that selects which file gets compiled.
Slightly more flexible but adds a feature flag to test combinations.

Default to **A** — simpler, fewer code paths, matches the existing
"generated file mirrors `platform.toml`" mental model. The image build
just runs `junius sync --bundle-all` against the source tree.

### Stage 2 — Boot-time bundle/config compatibility check

When `juniusd` boots:

1. The bundled plugin set is known statically (the generated
   `plugins.rs` lists them; expose it as a `BUNDLED_PLUGINS: &[&str]`
   const via the generator).
2. `platform.toml`'s `[plugins].enabled` is parsed as usual.
3. **Check 1:** every name in `enabled` must appear in
   `BUNDLED_PLUGINS`. Otherwise: `precompiled image does not ship
   plugin "<name>"; either remove it from [plugins].enabled or build
   from source with this plugin available`. Exit non-zero, no socket
   bound.
4. **Check 2:** every name in `BUNDLED_PLUGINS` must appear in
   `enabled`. Otherwise: `precompiled image ships plugin "<name>" but
   [plugins].enabled does not list it; precompiled images require an
   exact match`. Exit non-zero.

(The user's spec calls for "error if any plugin they ship with is not
enabled" — Check 2 is that check. The asymmetric mode here is "the
bundle IS the active set"; if you want a subset, build from source.
This avoids the runtime-selection complexity wholesale.)

The bundle check is gated by a new optional `[platform] mode =
"source" | "precompiled"` field in `platform.toml`. Default `"source"`
preserves today's behaviour. The image entrypoint sets
`JUNIUS_MODE=precompiled` (overrides the toml, image is authoritative
about its own mode). Source-built deployments never trip the check.

### Stage 3 — Runtime config the binary needs that `junius build` baked in today

`juniusd` already reads `platform.toml` at runtime end-to-end
([platform/src/main.rs:28-29](../../platform/src/main.rs#L28-L29) +
the full `HostConfig::load_from_toml` path). The audit step for M24:
walk every consumer of `junius build`'s generated output and confirm
nothing **else** gets baked in besides the plugin list. Specifically:

- [platform/src/generated/plugins.rs](../../platform/src/generated/plugins.rs)
  — plugin registry. Bundle-all handles it (Stage 1).
- [platform/src/generated/rpc_requires.rs](../../platform/src/generated/rpc_requires.rs)
  — cross-plugin RPC require graph. Bundle-all generates the full
  graph; boot-time `enabled` check (Stage 2) ensures the active set
  matches.
- [platform/frontend/src/generated/routes.ts](../../platform/frontend/src/generated/routes.ts)
  — composed plugin route tree. Bundle-all emits the full tree; SSR
  and SPA both walk it.
- [crates/junius-sdk/src/generated/domains.rs](../../crates/junius-sdk/src/generated/domains.rs)
  — generated typed domains. Bundle-all emits the full set.
- `packages/generated/src/proto/<plugin>/` — TS proto codegen.
  Bundle-all emits all plugins'.

The audit is itself a deliverable: a one-page "what `junius build`
generates and where it gets consumed" note in
[docs/design/14-decision-log.md](../design/14-decision-log.md), so
that a future generator addition doesn't silently break the precompiled
mode.

If the audit surfaces anything that *is* configuration-dependent (not
plugin-set-dependent), it has to move from compile-time to runtime.
Stage 3's deliverable is the audit + any such refactors; if the audit
finds nothing, Stage 3 is just the note.

### Stage 3-bis — Trim the in-container `junius` CLI

`junius` today has a `default = ["develop"]` feature
([tools/junius/Cargo.toml](../../tools/junius/Cargo.toml)) gating the
source-tree-only commands (`sync`, `dev`, `new`, `rpc`). That leaves
`check`, `build`, `migrate`, `plugin`, `cache`, `i18n` in a
non-`develop` build — but `build` + `cache` are themselves source-build
machinery (resolving `[source]`, hashing the source tree, managing
`~/.cache/junius`) and have nothing to do inside a precompiled image.
`plugin enable / disable` are similar — they mutate the deployment's
`platform.toml` based on `plugins/*/plugin.toml` reads from the source
tree, which the image doesn't have.

Add a second feature flag:

```toml
[features]
default = ["develop", "source-build"]
develop = []
source-build = []  # gates `Build`, `Cache`, `Plugin::{Enable, Disable}`
```

Gate the relevant subcommands with `#[cfg(feature = "source-build")]`
in [tools/junius/src/cli.rs](../../tools/junius/src/cli.rs) and the
matching dispatches in
[tools/junius/src/commands/mod.rs](../../tools/junius/src/commands/mod.rs).
The image builds junius with `--no-default-features` — the resulting
binary's `--help` lists only:

- `junius check` — validate a `platform.toml`.
- `junius migrate up` / `migrate status` — run / inspect host +
  plugin migrations against the configured database.
- `junius plugin list` / `plugin info <name>` — introspection
  (read-only; safe in a container).
- `junius i18n check` — validate per-plugin i18n catalogs.

A source-tree build remains unchanged (both features default-on), so
no contributor workflow shifts. The non-`develop`-only build that
deployment hosts use today (M11) continues to ship `build`/`cache`
because `source-build` is also default-on; only the in-container junius
strips both.

Stage 3-bis verification: `cargo build -p junius --no-default-features`
succeeds; the resulting binary's `--help` doesn't mention `build`,
`cache`, `sync`, `dev`, `new`, `rpc`, or `plugin {enable, disable}`;
`junius migrate status --config /etc/junius/platform.toml` works
unchanged inside the `full` and `backend` images.

### Stage 4 — Three Dockerfiles

`docker/full.Dockerfile`:
- Multi-stage. Builder: rust + node + pnpm + buf, runs
  `junius sync --bundle-all`,
  `cargo build --release -p platform --features embed-frontend`,
  `cargo build --release -p junius --no-default-features` (the
  trimmed CLI from §3-bis), and `pnpm --filter @junius/shell build`.
- Runtime: distroless or `debian:slim` (decide in §0.2). Copies
  `juniusd` to `/usr/local/bin/juniusd` and the trimmed `junius` to
  `/usr/local/bin/junius`. `ENTRYPOINT ["/usr/local/bin/juniusd"]`.
  Env: `JUNIUS_MODE=precompiled`,
  `JUNIUS_CONFIG=/etc/junius/platform.toml`. Volume `/etc/junius/`.
- Operators run migrations via `docker run --rm <image> junius
  migrate up --config /etc/junius/platform.toml`, then start the
  long-running container as normal.

`docker/backend.Dockerfile`:
- Builder: rust + buf only (no node — no FE bundle).
  `junius sync --bundle-all`,
  `cargo build --release -p platform` (no `embed-frontend`),
  `cargo build --release -p junius --no-default-features`.
- Runtime: same base. Same envs. Same entrypoint. Same trimmed
  `junius` shipped alongside `juniusd`.

`docker/frontend.Dockerfile`:
- Builder: node + pnpm only. Runs
  `pnpm --filter @junius/shell-ssr build` (the M23 SSR build target).
- Runtime: `node:22-slim` (matching the M23 runtime choice). Copies
  the built `dist/` + `node_modules`. `ENTRYPOINT ["node",
  "dist/server.js"]`. Env: `JUNIUS_BE_INTERNAL_URL` (mandatory, from
  M23). Volume `/etc/junius/` for any frontend-specific config.
- **No `junius` CLI inside.** The FE container holds no DB connection
  and consumes no `platform.toml`; the BE container owns the
  migrate/check surface.

Shared concerns:
- Reproducibility: pinned base image digests, deterministic build args
  (`SOURCE_DATE_EPOCH`), Cargo + pnpm lockfiles authoritative.
- Layer caching: cargo's target dir + pnpm's store cached across CI
  runs via the buildx cache backend (`type=gha`).
- Healthchecks: each image declares `HEALTHCHECK` hitting `/h/health`
  (BE) or `/healthz` (FE).
- Labels: OCI labels include source commit SHA, build time, and the
  list of bundled plugins (for `docker inspect` introspection).

### Stage 5 — GitHub Actions workflow

`.github/workflows/images.yml`:
- Triggers: `push` to `main`, `pull_request` (build-only, no push),
  `workflow_dispatch` (manual rebuild).
- Permissions: `packages: write`, `contents: read`,
  `id-token: write` (for image signing — see §0.2).
- Steps per variant (matrixed):
  1. Checkout, set up buildx.
  2. Compute tags: `:latest`, `:<short-sha>`.
  3. `docker/login-action` against `ghcr.io` using
     `${{ secrets.GITHUB_TOKEN }}`.
  4. `docker/build-push-action`: build, push on `main`, skip-push on
     PR. Cache `type=gha,scope=<variant>`.
  5. Sign the pushed digest with `cosign` (keyless via GH OIDC).
- A summary job that posts a single comment on PRs listing all three
  image digests + sizes (for PR review visibility).
- A separate `images-smoke` job that, on `main`, pulls the just-pushed
  `full` image and runs `docker run --rm <image> --check-config`
  against a fixture `platform.toml` from the example. Catches a
  silently-broken image before it sits at `:latest`.

### Stage 6 — Precompiled deployment examples + docs

`examples/example-deployment-precompiled/`:
- `full/` — single-container compose using `ghcr.io/.../junius-full:latest`.
  Just postgres + authentik + the image.
- `split/` — two-container compose using `frontend` + `backend`. The
  M23 split example's twin, but with `image: ghcr.io/...` instead of
  `build:`.
- `README.md` — when to pick precompiled vs source-built; how tags
  work (`:latest` vs pinned SHA); how to verify the cosign signature.

Ops guide: a new "Choosing a deployment path" section. Three paths
side-by-side:
1. **Source-built** (M11) — full plugin selection, custom plugins,
   pinned forks. Needs Rust + node + buf on the build host.
2. **Precompiled full** (M24) — all plugins, single container, simplest
   ops. No build toolchain on the deployment host.
3. **Precompiled split** (M23 + M24) — all plugins, two containers,
   SSR for first paint and operational separation.

Decision-log entry: the asymmetric "bundle IS the active set" choice —
why the precompiled mode doesn't do runtime plugin selection, and the
escape hatch (source-build) for deployments that need a subset.

### Stage 7 — CI integration

- The existing CI (`.github/workflows/ci.yml`) is unchanged; the new
  workflow is separate.
- The M17 E2E suite (and the M23 split-mode suite) gain a CI variant
  that runs against the precompiled images instead of source-built
  ones, on a nightly schedule (not per-PR — too slow). Catches drift
  between the image build and the source-built artifacts.
- `junius doctor` (a small new CLI subcommand, or fold into `junius
  check`) verifies a deployment's `platform.toml` against a target
  mode (`source` or `precompiled`) — checks the bundle/config match
  *before* pulling a multi-hundred-MB image.

## Library choices — confirm with user before starting

| Concern | Proposed default | Rationale | Downstream |
|---|---|---|---|
| Runtime base image (BE) | `gcr.io/distroless/cc-debian12` | Smallest viable, no shell, cuts attack surface. | — |
| Runtime base image (FE) | `node:22-slim` | M23 default; smallest official Node image with a working glibc. | M23 |
| Image registry | `ghcr.io` | Spec; same org as the repo; free for public projects. | — |
| Image signing | `cosign` keyless via GH OIDC | Sigstore standard; verifiable without pre-shared keys. | — |
| Tag scheme | `<variant>-latest` + `<variant>-<short-sha>` | Spec; matches common conventions. | — |
| Multi-arch | linux/amd64 + linux/arm64 | Apple Silicon dev hosts + cloud arm64; buildx handles it. | — |
| Bundle-all mechanism | `junius sync --bundle-all` (Option A in §1) | One generated file; matches existing mental model. | — |
| In-container `junius` feature gating | New `source-build` cargo feature, default-on, gating `build` + `cache` + `plugin {enable, disable}`. Image builds with `--no-default-features`. | Smallest viable runtime CLI (migrate / check / list / info / i18n check) without breaking the existing source-tree CLI surface. | — |
| Mode flag | `[platform] mode = "source" \| "precompiled"` in `platform.toml`, overridable by `JUNIUS_MODE` env | Image authoritative about its own mode; toml is a default for source-built deployments. | — |
| Layer-cache backend | `type=gha` | GH-native, no extra infra. | — |
| Healthcheck endpoint (BE) | `/h/health` | Matches platform helper namespace from M02; needs to exist (add if missing). | — |
| Healthcheck endpoint (FE) | `/healthz` | Common SSR convention; the M23 Node server exposes it. | M23 |

## Risks & mitigations

- **Image size.** Bundling every plugin's FE and BE pushes the `full`
  image past current single-plugin-set builds. Mitigation: aggressive
  multi-stage (no toolchains in the runtime layer), distroless base,
  pnpm `--prod` install, Rust release profile with `strip = "symbols"`
  and `lto = "thin"`. Target: `full` under 200 MB, `backend` under
  150 MB, `frontend` under 250 MB (Node + node_modules dominates).
- **Cold-start build time in CI.** Three images × multi-arch =
  significant CI minutes. Mitigation: GHA cache backend; matrix
  parallelism; PRs skip the push step (build-only); the nightly
  precompiled-E2E run carries the cost of validation, not per-PR.
- **Drift between source-built and image-built behaviour.** The image
  is built from the same `cargo build` but with a different generated
  `plugins.rs` (bundle-all vs deployment-specific). Mitigation:
  bundle-all is the only difference; the M17 E2E suite runs nightly
  against the precompiled images to catch drift early; a
  `junius check --bundle-all` mode validates the bundled set passes
  the same rules as a hand-picked set.
- **"Asymmetric bundle = active set" surprises users.** Someone tries
  the precompiled image, sets `[plugins].enabled = ["events"]`, and
  hits the boot error. Mitigation: the error message names the exact
  missing plugins *and* points at the two escape hatches (add them to
  `enabled`, or switch to source-build); the example deployments
  ship with a complete `enabled` list pre-filled.
- **ghcr.io rate limits / availability.** A deployment relying on
  `:latest` pulls is at the mercy of registry uptime. Mitigation: the
  example doc strongly recommends pinning to a SHA tag, not `:latest`,
  for production deployments; `:latest` is for dev/CI convenience.
- **Plugin compile time.** Bundling all plugins makes the BE image
  build slower than a typical 2–3-plugin deployment. Mitigation: same
  as the source-build path (parallel codegen, sccache via the GHA
  cache, release-profile knobs); doesn't affect deployment-time
  performance.
- **A new plugin gets added to `plugins/` and immediately breaks every
  precompiled deployment.** The first `main` push after the merge
  produces an image whose `BUNDLED_PLUGINS` includes the new one, and
  every existing `platform.toml` now fails Check 2. Mitigation: the
  release notes / changelog convention (add to M21's docs scope) flags
  bundle-set changes prominently; deployments that pin
  `:<short-sha>` are unaffected until they choose to bump; the doctor
  CLI command tells you the bundle diff before you upgrade.

## Out of scope

- **Runtime plugin selection in the precompiled image.** The bundle is
  the active set; subsetting requires the source-build path. (Could
  revisit later; deferred deliberately — keeps M24 small and avoids
  duplicating the cross-plugin RPC graph as a runtime check.)
- **Third-party plugins in precompiled images.** Custom plugins go via
  source-build (or a fork that publishes its own images using the
  same Dockerfiles as a template).
- **Helm charts / Kubernetes manifests.** Compose examples only.
  Real K8s deployments roll their own from the images.
- **Image vulnerability scanning in this milestone.** Worth doing as a
  follow-up (Trivy / Grype in CI) but adds CI surface; defer.
- **Release versioning / semver tags.** Just `latest` + SHA in this
  milestone. Tagged releases (`v0.1.0`) come with a versioning policy
  that doesn't exist yet — handle in a later milestone.
- **An "official" Postgres image.** Deployments bring their own
  Postgres; the precompiled images are app-only.

## Verification

The milestone is done when:

1. A push to `main` produces three signed images at
   `ghcr.io/<org>/junius-{full,frontend,backend}:latest` and
   `:<short-sha>`, each verifiable with `cosign verify`.
2. `docker run --rm ghcr.io/<org>/junius-full:<sha> --check-config`
   against the precompiled example's `platform.toml` succeeds.
3. `cd examples/example-deployment-precompiled/full && docker compose up`
   boots, login works, the events plugin round-trips an event.
4. `cd examples/example-deployment-precompiled/split && docker compose up`
   (M23-dependent) boots, SSR HTML is returned, login + events work.
5. A `platform.toml` with `enabled = ["events"]` against the `full`
   image fails the boot check with a clear missing-plugins error
   naming every plugin from the bundle that wasn't enabled.
6. `docker run --rm ghcr.io/<org>/junius-full:<sha> junius --help`
   lists only `check`, `migrate`, `plugin {list, info}`, `i18n` —
   no `build`, `cache`, `sync`, `dev`, `new`, `rpc`, or
   `plugin {enable, disable}`.
7. `docker run --rm ghcr.io/<org>/junius-backend:<sha> junius migrate
   up --config /etc/junius/platform.toml` applies pending migrations
   against a reachable database; same image's `juniusd` then boots
   green.
8. The nightly precompiled-image E2E run is green.

## Downstream doc updates

- [docs/impl/README.md](README.md) — M24 row added, ★ priority
  callout updated.
- [docs/design/05-repository-and-deployment-layout.md](../design/05-repository-and-deployment-layout.md)
  — append §5.6 describing the precompiled deployment path alongside
  the source-build path from §5.5.
- [docs/design/14-decision-log.md](../design/14-decision-log.md) —
  M24 entry covering (a) the bundle-IS-the-active-set decision,
  (b) the choice of distroless + node:22-slim base images,
  (c) the cosign keyless signing decision.
- [docs/impl/12-M11-deployment-workflow.md](12-M11-deployment-workflow.md)
  — add a "see also M24" pointer at the top so deployment-path
  decisions surface the alternative.
- [docs/impl/25-M23-optional-ssr.md](25-M23-optional-ssr.md) —
  cross-reference: the `frontend` image M24 ships is the SSR Node
  server defined in M23.
