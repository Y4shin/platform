# 5. Repository & Deployment Layout

The source monorepo is the **codebase** — platform host, plugins, shared libraries, and the `junius` tool. It does **not** contain a `platform.toml`; the codebase has no opinion on which plugins ship in any particular deployment. Deployments live in separate directories (their own repos, or just a working directory on a server) and configure themselves against the source — see §5.5.

## 5.1 Source monorepo

```
junius/                 # The source codebase
├── README.md
├── Cargo.toml                      # Rust workspace root
├── Cargo.lock
├── pnpm-workspace.yaml             # pnpm workspace declaration
├── package.json                    # root scripts (delegate to junius)
├── pnpm-lock.yaml
├── buf.work.yaml                   # Buf proto-monorepo declaration
├── rust-toolchain.toml             # pinned Rust version
├── biome.json                      # FE formatter/linter
├── .editorconfig
├── .gitignore
├── .github/workflows/              # CI: junius check + breaking-check + build + test
│
├── platform/                       # The host (NOT a plugin)
│   ├── Cargo.toml                  # produces the deployment binary
│   ├── src/
│   │   ├── main.rs                 # binary entry point
│   │   ├── server.rs               # Axum + middleware + plugin mounting
│   │   ├── plugin/                 # Plugin trait, PluginContext, capability handles
│   │   ├── auth/                   # identity, sessions, tokens, RBAC primitives
│   │   ├── users/                  # user management
│   │   ├── db/                     # connection pool, migration orchestrator
│   │   ├── config/                 # config loading
│   │   ├── telemetry/              # OpenTelemetry setup
│   │   └── lib.rs
│   └── frontend/                   # The shell FE (also not a plugin)
│       ├── package.json            # @junius/shell
│       ├── src/
│       │   ├── main.tsx
│       │   ├── App.tsx             # composes plugin routes into the root router
│       │   ├── auth/               # login, session UI
│       │   └── layout/             # platform-wide chrome
│       └── index.html
│
├── plugins/                        # All plugins (in-tree or via submodule)
│   ├── speakers/
│   │   ├── plugin.toml
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   ├── proto/
│   │   ├── migrations/
│   │   └── frontend/
│   │       ├── package.json
│   │       ├── routes/
│   │       ├── lib/
│   │       └── index.ts
│   └── events/ ...
│
├── crates/                         # Shared Rust libraries (used by platform + plugins)
│   ├── junius-sdk/               # what plugins import to talk to the host
│   │                               # (Plugin trait, capability handles, common types)
│   └── ...
│
├── packages/                       # Shared FE libraries
│   ├── design-system/              # @junius/design — tokens, base components, CSS vars
│   ├── client/                     # @junius/client — Connect transport, auth helpers
│   └── ...
│
├── proto/                          # Platform-level shared protos (auth, common types)
│   └── platform/v1/
│
├── tools/
│   └── junius/                    # The management CLI (Rust binary, not deployed)
│       ├── Cargo.toml
│       └── src/
│
└── docs/                           # This design doc, ADRs, plugin authoring guide
```

## 5.2 What lives where

- **`platform/`** — the host. Provides the Axum server, the `Plugin` trait, `PluginContext`, capability handles, DB pool, config, telemetry, and the FE shell (root layout, plugin loader). **Auth, user management, and RBAC live here** (see §5.3). The deployment binary is built from this crate, with each deployment's enabled plugins linked in via `junius`'s generated glue.
- **`plugins/`** — domain plugins. Each is a self-contained plugin matching the structure in [06-plugin-shape.md](06-plugin-shape.md). Presence in this directory means "available to ship"; whether a plugin is actually shipped is decided by each deployment (§5.5).
- **`crates/`** — Rust libraries that aren't plugins but are imported by plugins. The most important is `junius-sdk` — the API surface plugins use. Plugins depend on `junius-sdk`, not on `platform/` directly. This keeps the plugin/host boundary clean and prevents plugins from reaching into host internals.
- **`packages/`** — TS analogue. `design-system` provides shared components and tokens that all plugin frontends use. `client` provides the shared Connect-RPC transport, auth headers, etc.
- **`proto/`** — platform-level shared protos (common types referenced by plugin protos, auth-related messages). Plugin-specific protos live inside each plugin's `proto/`.
- **`tools/junius/`** — the management CLI ([07-junius.md](07-junius.md)). Lives in the workspace so contributors get it via `cargo build`; not part of the deployment binary.
- **`docs/`** — this design doc, ADRs, plugin authoring guide.

## 5.3 Core stays in the host

Auth, user management, RBAC, sessions/tokens, and other foundational concerns live in **`platform/`**, not in plugins. Reasoning:

- These are dependencies of the plugin system itself (you can't authenticate a plugin loader using a service the loader is responsible for loading).
- Plugins consume identity primitives via the typed API exposed in `junius-sdk` — they don't reach into host internals directly.
- Treating identity as a plugin creates a chicken-and-egg startup problem we'd rather not solve.

Some platform-adjacent UI (user admin, role management) ships in the FE shell under `platform/frontend/`. If a clear need emerges later for a first-party plugin to expose admin UI, we can revisit — the boundary isn't permanent, but the default is "core in the host."

## 5.4 Workspace-level files

- **`Cargo.toml` (root)** — Cargo workspace. Members: `platform`, `plugins/*`, `crates/*`, `tools/junius`. Shared dependency versions live in `[workspace.dependencies]` for consistency.
- **`pnpm-workspace.yaml`** — lists `platform/frontend`, `plugins/*/frontend`, `packages/*`.
- **`buf.work.yaml`** — lists `proto/` and `plugins/*/proto/` as buf modules. Enables cross-plugin proto imports and lets `buf breaking` operate across the whole tree.

Note what's **not** here: no `platform.toml`. The source monorepo doesn't declare which plugins are enabled in any deployment.

## 5.5 Deployments

A **deployment** is a separate directory (its own repo, or just a working directory on a server) that decides which plugins ship in *its* binary and how they're configured. The source monorepo is the codebase; the deployment is the configuration.

```
my-deployment/                 # Outside the source monorepo
├── platform.toml              # source pinning + enabled plugins + config
├── platform.lock              # exact resolved source revision (managed by junius)
├── secrets/                   # secret material (gitignored or vault references)
└── .gitignore
```

Example `platform.toml`:

```toml
[source]
repo = "git+https://github.com/org/junius.git"
rev  = "v0.5.0"                # branch, tag, or SHA; pinned in platform.lock

[plugins]
enabled = ["speakers", "events", "canvassing"]

[plugins.speakers]
max_bookings_per_day = 10      # plugin-specific config

[config]
database_url = "env:DATABASE_URL"
```

`junius build` run inside the deployment directory:

1. Resolves `[source]` — fetches/updates a local cache of the source at the pinned revision.
2. Validates that every `enabled` plugin exists in the source.
3. Generates composition glue (Rust, TS, buf) specific to this deployment's plugin set.
4. Builds the binary.
5. Outputs `./platform-bin` (or similar) — the artifact for this deployment.

The deployment directory needs only `junius` installed; the Rust/pnpm/Buf toolchains are invoked inside the cached source by `junius`.

### Why split source from deployment?

- **Reuse**: one source revision can power many independent deployments — different orgs, environments, plugin sets — without code branching.
- **Auditability**: a deployment's `platform.toml` + `platform.lock` fully describe what's running, separable from code review.
- **Boundary**: the source codebase doesn't need to know about specific deployments. Plugin authors can't accidentally hardcode org-specific config.
- **Distribution**: a deployment can be a tiny git repo (config-only) maintained by ops, while the source lives elsewhere maintained by engineering.

### Source resolution

The `[source]` block drives source resolution:

- **Git URL + revision** (default): `repo = "git+https://..."`, `rev = "<tag/branch/sha>"`.
- **Local path** (for development against a working copy): `path = "/home/me/code/junius"`.
- Multiple sources are conceivable later (different plugins from different repos) but not in v1; submodules inside the source monorepo already cover that case.

`platform.lock` pins the exact resolved revision (`rev` resolved to a SHA, source content hash). `junius build` will fail if the lock and the resolved source disagree, unless `--update-lock` is passed.

### Dev mode in the source repo

For working on the codebase itself, the source monorepo contains an example deployment under `dev/`:

```
junius/dev/
├── platform.toml         # dev sandbox: enables most/all plugins, points [source] at path = ".."
└── .gitignore
```

`junius dev` defaults to using `dev/platform.toml` if no `--config` is passed. This is a developer convenience, not a real deployment.

## 5.6 Precompiled deployment path (M24)

Source-build (§5.5) is the v1 default and remains supported. M24 adds a **second supported deployment path**: pull a prebuilt container image from `ghcr.io` and drop in a `platform.toml`. No Rust / Node / buf toolchain on the deployment host.

Three image variants are published on every push to `main`:

- `junius-full` — `juniusd` with `--features embed-frontend`, all monorepo plugins, single-container topology.
- `junius-backend` — headless `juniusd` (no embedded SPA), all monorepo plugins. Pairs with the SSR FE container.
- `junius-frontend` — the M23 SSR Node server with every plugin frontend bundled. Pairs with `junius-backend`.

Tagged `<variant>-latest` (follows `main`) and `<variant>-<short-sha>` (immutable; pin in production).

### "Bundle IS the active set"

Precompiled images bundle **every monorepo plugin**. `[plugins].enabled` in the deployment's `platform.toml` must match the bundled set exactly — the boot check (`JUNIUS_MODE=precompiled`, set by the image entrypoint) refuses to start on a mismatch and names every missing/extra plugin. Two escape hatches:

1. Add the missing plugins to `[plugins].enabled` (the usual response when the monorepo grows a new plugin and you've bumped your image tag).
2. Drop back to source-build (§5.5) if you want a subset.

Runtime plugin selection in the precompiled mode is intentionally deferred — it duplicates the cross-plugin RPC graph as a runtime check and complicates the boot path. The source-build escape hatch covers the case.

### When to pick which

| Need | Path |
|---|---|
| Custom or forked plugins; subset of the monorepo set | Source-build (§5.5) |
| Stock plugin set, no build toolchain on deployment hosts, fastest path from `git pull` to running | Precompiled `full` |
| Stock plugin set + SSR for first-paint / CDN-fronted FE / independent FE/BE scaling | Precompiled `split` (M23 topology) |

The precompiled deployment example is `examples/example-deployment-precompiled/` (split into `full/` and `split/`).
