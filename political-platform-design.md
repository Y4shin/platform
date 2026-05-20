# Political Platform — Design Doc

A plugin-driven monolith for political work. The core platform provides user management, authentication, and shared infrastructure; each domain use case (e.g. speakers, events, canvassing) is delivered as a plugin that bundles its backend, frontend, and API contract together.

This document captures decisions taken so far. Open questions are listed at the end.

---

## 1. Goals & Constraints

- **Plugin-driven monolith.** One process, one deployable, but composed from independent plugin units.
- **Compile-time plugin composition.** Plugins are wired in at build time, not loaded dynamically. Adding/removing a plugin is a rebuild.
- **Plugins bundle backend + frontend.** A single plugin owns its server logic, API schema, UI routes, and any components it exposes to peers.
- **No JS/TS on the backend.** Hard constraint.
- **Significant SPA-like interactivity** alongside more CRUD-style views. The frontend stack must handle both regimes well.
- **End-to-end type safety** between backend and frontend.
- **Single self-contained deployment artifact.** One binary, no separate frontend hosting.

---

## 2. Architecture at a Glance

```
  deployment dir                ┌────────────────────────┐
  platform.toml ─────────────▶ │   platctl (mgmt tool)  │
  (which plugins, config)      │   reads config + src,  │
                               │   composes + builds    │
                                └────────────────────────┘
                                       │
            ┌──────────────────────────┼──────────────────────────┐
            ▼                          ▼                          ▼
     Rust workspace             pnpm workspace             .proto schemas
     (backend crates)           (FE packages)              (Connect-RPC)
            │                          │                          │
            │                          ▼                          │
            │              Vite build → dist/                     │
            │                          │                          │
            ▼                          ▼                          ▼
     ┌────────────────────────────────────────────────────────────┐
     │   Single Rust binary                                       │
     │   - Axum server                                            │
     │   - Connect-RPC services (/rpc/<plugin>/*)                 │
     │   - Plugin HTTP routes (/p/<plugin>/* for non-RPC)         │
     │   - Embedded FE bundle served as static assets             │
     └────────────────────────────────────────────────────────────┘
```

---

## 3. Backend

### 3.1 Language & framework

- **Rust** for all backend code.
- **Axum** as the HTTP framework.
- Cargo workspace; each plugin is a member crate.

### 3.2 Plugin contract (backend side)

Each plugin crate implements the `Plugin` trait on a top-level struct. The trait exposes a `routes(&self, resources: PluginResources) -> Router` method that builds an Axum router scoped to the plugin's resources, plus lifecycle and job hooks. Full interface specification in §11.

`platctl` (§7) generates a `plugins.rs` that constructs and registers each enabled plugin in order. Plugin dependency order is resolved from manifests (see §6.1).

### 3.3 API contract: Connect-RPC

- Each plugin owns one or more `.proto` files defining its services.
- `connect-rs` generates the Rust server stubs.
- `@connectrpc/connect-web` (+ `@bufbuild/protoc-gen-es`) generates the TS client.
- RPC routes mount under `/rpc/<plugin-name>/*`.
- Schema evolution is enforced by Buf's `buf breaking` checks in CI.

Connect-RPC was chosen over OpenAPI and rspc for:
- Strongest schema evolution discipline.
- Best long-term ecosystem stability (Buf is a serious company with a real product roadmap).
- First-class TS client with great DX (works over plain HTTP, no gRPC plumbing).

---

## 4. Frontend

### 4.1 Stack

- **React + TypeScript.**
- **TanStack Router** for routing — code-based, fully type-safe, composable.
- **TanStack Query** + `@connectrpc/connect-query` for data fetching, caching, and mutations.
- **Vite** as the build tool.
- **SPA mode** (no SSR). The platform sits behind authentication; SSR/SEO benefits don't apply.

### 4.2 Why this stack over SvelteKit / Next.js / Leptos

- **Code-based routing fits plugin composition natively.** Routes are data; each plugin exports a route subtree that `platctl` concatenates. File-based routers (SvelteKit, Next App Router, Nuxt, Remix) require build-time codegen shims to compose plugins.
- **Best-in-class ecosystem for SPA-heavy UI.** Rich text editors, data grids, drag-and-drop, complex forms — all most mature on React.
- **Largest hiring pool.**
- **Connect-RPC integration is first-party** via `connect-query`.
- **Leptos rejected** for production: 0.x churn, thin component ecosystem, niche hiring.
- **SvelteKit rejected** despite better per-component DX: file-based routing fights the plugin model.

### 4.3 Plugin contract (frontend side)

Each plugin is also a pnpm workspace package (`@platform/plugin-<name>`) that exports:

```ts
// plugins/speakers/frontend/index.ts
export { routes } from './routes';              // TanStack Router subtree
export { SpeakerPicker, SpeakerCard } from './lib'; // public components
export { createSpeakersClient } from './rpc';    // Connect-RPC client factory
export type { Speaker, SpeakerId } from './types';
```

---

## 5. Repository & Deployment Layout

The source monorepo is the **codebase** — platform host, plugins, shared libraries, and the `platctl` tool. It does **not** contain a `platform.toml`; the codebase has no opinion on which plugins ship in any particular deployment. Deployments live in separate directories (their own repos, or just a working directory on a server) and configure themselves against the source — see §5.5.

### 5.1 Source monorepo

```
political-platform/                 # The source codebase
├── README.md
├── Cargo.toml                      # Rust workspace root
├── Cargo.lock
├── pnpm-workspace.yaml             # pnpm workspace declaration
├── package.json                    # root scripts (delegate to platctl)
├── pnpm-lock.yaml
├── buf.work.yaml                   # Buf proto-monorepo declaration
├── rust-toolchain.toml             # pinned Rust version
├── biome.json                      # FE formatter/linter
├── .editorconfig
├── .gitignore
├── .github/workflows/              # CI: platctl check + breaking-check + build + test
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
│       ├── package.json            # @platform/shell
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
│   ├── platform-sdk/               # what plugins import to talk to the host
│   │                               # (Plugin trait, capability handles, common types)
│   └── ...
│
├── packages/                       # Shared FE libraries
│   ├── design-system/              # @platform/design — tokens, base components, CSS vars
│   ├── client/                     # @platform/client — Connect transport, auth helpers
│   └── ...
│
├── proto/                          # Platform-level shared protos (auth, common types)
│   └── platform/v1/
│
├── tools/
│   └── platctl/                    # The management CLI (Rust binary, not deployed)
│       ├── Cargo.toml
│       └── src/
│
└── docs/                           # This design doc, ADRs, plugin authoring guide
```

### 5.2 What lives where

- **`platform/`** — the host. Provides the Axum server, the `Plugin` trait, `PluginContext`, capability handles, DB pool, config, telemetry, and the FE shell (root layout, plugin loader). **Auth, user management, and RBAC live here** (see §5.3). The deployment binary is built from this crate, with each deployment's enabled plugins linked in via `platctl`'s generated glue.
- **`plugins/`** — domain plugins. Each is a self-contained plugin matching the structure in §6. Presence in this directory means "available to ship"; whether a plugin is actually shipped is decided by each deployment (§5.5).
- **`crates/`** — Rust libraries that aren't plugins but are imported by plugins. The most important is `platform-sdk` — the API surface plugins use. Plugins depend on `platform-sdk`, not on `platform/` directly. This keeps the plugin/host boundary clean and prevents plugins from reaching into host internals.
- **`packages/`** — TS analogue. `design-system` provides shared components and tokens that all plugin frontends use. `client` provides the shared Connect-RPC transport, auth headers, etc.
- **`proto/`** — platform-level shared protos (common types referenced by plugin protos, auth-related messages). Plugin-specific protos live inside each plugin's `proto/`.
- **`tools/platctl/`** — the management CLI (§7). Lives in the workspace so contributors get it via `cargo build`; not part of the deployment binary.
- **`docs/`** — this design doc, ADRs, plugin authoring guide.

### 5.3 Core stays in the host

Auth, user management, RBAC, sessions/tokens, and other foundational concerns live in **`platform/`**, not in plugins. Reasoning:

- These are dependencies of the plugin system itself (you can't authenticate a plugin loader using a service the loader is responsible for loading).
- Plugins consume identity primitives via the typed API exposed in `platform-sdk` — they don't reach into host internals directly.
- Treating identity as a plugin creates a chicken-and-egg startup problem we'd rather not solve.

Some platform-adjacent UI (user admin, role management) ships in the FE shell under `platform/frontend/`. If a clear need emerges later for a first-party plugin to expose admin UI, we can revisit — the boundary isn't permanent, but the default is "core in the host."

### 5.4 Workspace-level files

- **`Cargo.toml` (root)** — Cargo workspace. Members: `platform`, `plugins/*`, `crates/*`, `tools/platctl`. Shared dependency versions live in `[workspace.dependencies]` for consistency.
- **`pnpm-workspace.yaml`** — lists `platform/frontend`, `plugins/*/frontend`, `packages/*`.
- **`buf.work.yaml`** — lists `proto/` and `plugins/*/proto/` as buf modules. Enables cross-plugin proto imports and lets `buf breaking` operate across the whole tree.

Note what's **not** here: no `platform.toml`. The source monorepo doesn't declare which plugins are enabled in any deployment.

### 5.5 Deployments

A **deployment** is a separate directory (its own repo, or just a working directory on a server) that decides which plugins ship in *its* binary and how they're configured. The source monorepo is the codebase; the deployment is the configuration.

```
my-deployment/                 # Outside the source monorepo
├── platform.toml              # source pinning + enabled plugins + config
├── platform.lock              # exact resolved source revision (managed by platctl)
├── secrets/                   # secret material (gitignored or vault references)
└── .gitignore
```

Example `platform.toml`:

```toml
[source]
repo = "git+https://github.com/org/political-platform.git"
rev  = "v0.5.0"                # branch, tag, or SHA; pinned in platform.lock

[plugins]
enabled = ["speakers", "events", "canvassing"]

[plugins.speakers]
max_bookings_per_day = 10      # plugin-specific config

[config]
database_url = "env:DATABASE_URL"
```

`platctl build` run inside the deployment directory:

1. Resolves `[source]` — fetches/updates a local cache of the source at the pinned revision.
2. Validates that every `enabled` plugin exists in the source.
3. Generates composition glue (Rust, TS, buf) specific to this deployment's plugin set.
4. Builds the binary.
5. Outputs `./platform-bin` (or similar) — the artifact for this deployment.

The deployment directory needs only `platctl` installed; the Rust/pnpm/Buf toolchains are invoked inside the cached source by `platctl`.

#### Why split source from deployment?

- **Reuse**: one source revision can power many independent deployments — different orgs, environments, plugin sets — without code branching.
- **Auditability**: a deployment's `platform.toml` + `platform.lock` fully describe what's running, separable from code review.
- **Boundary**: the source codebase doesn't need to know about specific deployments. Plugin authors can't accidentally hardcode org-specific config.
- **Distribution**: a deployment can be a tiny git repo (config-only) maintained by ops, while the source lives elsewhere maintained by engineering.

#### Source resolution

The `[source]` block drives source resolution:

- **Git URL + revision** (default): `repo = "git+https://..."`, `rev = "<tag/branch/sha>"`.
- **Local path** (for development against a working copy): `path = "/home/me/code/political-platform"`.
- Multiple sources are conceivable later (different plugins from different repos) but not in v1; submodules inside the source monorepo already cover that case.

`platform.lock` pins the exact resolved revision (`rev` resolved to a SHA, source content hash). `platctl build` will fail if the lock and the resolved source disagree, unless `--update-lock` is passed.

#### Dev mode in the source repo

For working on the codebase itself, the source monorepo contains an example deployment under `dev/`:

```
political-platform/dev/
├── platform.toml         # dev sandbox: enables most/all plugins, points [source] at path = ".."
└── .gitignore
```

`platctl dev` defaults to using `dev/platform.toml` if no `--config` is passed. This is a developer convenience, not a real deployment.

---

## 6. Plugin Shape

A plugin is a single directory containing:

```
plugins/speakers/
├── plugin.toml           # Manifest: name, route prefix, deps, exposed components
├── Cargo.toml            # Rust crate
├── src/                  # Rust backend (lib.rs exposes register())
├── proto/                # Connect-RPC .proto files
└── frontend/
    ├── package.json      # @platform/plugin-speakers
    ├── routes/           # TanStack Router subtree (mounted under /p/speakers)
    ├── lib/              # Public components for other plugins
    ├── rpc/              # Generated Connect-RPC client (build artifact)
    └── index.ts          # Public exports
```

### 6.1 Manifest

Each plugin has a `plugin.toml` at its root. This file is the **only** thing `platctl` reads to decide how to compose a plugin into the platform — everything else (`Cargo.toml`, `package.json`, `buf.yaml`, generated code) derives from it.

**Format**: TOML. Schema defined as a Rust struct in `platctl`, deserialized via serde. The `manifest_schema` field is the only versioning concept retained (it lets the manifest format evolve without breaking older plugins).

```toml
# plugins/speakers/plugin.toml

# --- Identity -------------------------------------------------------------
[plugin]
name = "speakers"                     # unique platform-wide
display_name = "Speakers"
description = "Manage public speakers and their booking history."
manifest_schema = 1                   # manifest format version

# --- Mount points (auto-derived from name if omitted) ---------------------
[mount]
route_prefix = "/p/speakers"          # SPA routes
rpc_prefix   = "/rpc/speakers"        # Connect-RPC
http_prefix  = "/h/speakers"          # non-RPC HTTP (uploads, OAuth callbacks)

# --- Inter-plugin dependencies --------------------------------------------
# All plugins live in this monorepo (directly or via git submodule); the
# workspace tree IS the version. There is no semver and no resolver.
# `platctl check` only validates that required deps are enabled and that
# imports/usages match declarations.

[dependencies.identity]
optional = false

[dependencies.venues]
optional = true                       # graceful degradation via typed registry

# --- What this plugin exposes to other plugins ----------------------------
[exposes.components.SpeakerCard]
module      = "./frontend/lib/SpeakerCard"
description = "Compact speaker summary card."

[exposes.components.SpeakerPicker]
module      = "./frontend/lib/SpeakerPicker"
description = "Searchable speaker selector."

# --- Permissions defined by this plugin -----------------------------------
[permissions]
"speakers:read"  = "View speakers and their booking history."
"speakers:write" = "Create, edit, and delete speakers."
"speakers:book"  = "Book a speaker for an event."

# --- Platform capabilities required (audit-only in v1) --------------------
[requires]
capabilities = ["db.read", "db.write", "storage.write", "email.send"]
```

#### Design choices locked in

- **Manifest is the single source of truth for plugin-level metadata.** Language-native files (`Cargo.toml`, `package.json`) own language-native concerns (Rust deps, npm deps); the manifest only declares **inter-plugin** relationships and platform-facing facts.
- **Plugin layout is fixed by convention** (see §6). No `[paths]` block; every plugin uses the same directory structure. Plugins that don't need a section (no RPC, no migrations, etc.) simply omit the directory.
- **Plugin enablement lives in the deployment's `platform.toml` (§5.5), not in each plugin's manifest.** A plugin doesn't decide whether it's enabled — the deployment does. The source monorepo has no opinion on which plugins ship in any given binary.
- **Permissions are declared in the manifest, not in Rust code.** Makes them auditable without running code, drives the platform's RBAC tables, and lets `platctl docs generate` enumerate them statically. Plugins still *enforce* permissions in code; the manifest only *declares* them.
- **Capabilities are audit-only in v1.** The `[requires.capabilities]` list is informational; `platctl plugin info` displays it, admins can review before enabling. Runtime enforcement (refusing to give a `db` handle to a plugin that didn't declare `db.write`) is deferred until/unless we ever accept untrusted third-party plugins.
- **No per-plugin versioning, no dependency version constraints.** Single-version monorepo (including git submodules) makes semver ceremony rather than mechanism. `manifest_schema` is the only versioning concept retained. The platform's deployment artifact has its own version (from build metadata), but plugins do not. If a plugin is ever published externally, versioning becomes a deliberate addition at that point.
- **Cross-repo plugins use git submodules.** A submodule shows up under `plugins/<name>/` like any in-tree plugin; `platctl` treats it identically. The submodule's pinned revision in the parent repo's git index is the implicit version.
- **`Cargo.toml` and `package.json` use workspace protocols** for cross-plugin deps (`path = "../<plugin>"`, `workspace:*`). Their own `version` fields are sentinels (e.g. `0.0.0`) — never consumed by a resolver. `platctl sync` writes these inside marker comments; plugin authors don't touch them.
- **TOML, not YAML/JSON.** Matches Cargo, easy to read, no YAML footguns.
- **Mount paths default from `plugin.name`** but can be overridden — useful for renames and migrations.

---

## 7. The Management Tool (`platctl`)

A single Rust CLI that owns the entire plugin lifecycle: composition, codegen, scaffolding, validation, build, dev mode, and inspection. The "kickstart" concept from earlier discussions is just the `compose` + `build` subset of this tool. One binary, one entry point, everything plugin-related goes through it.

### 7.1 Design principle

Manifests are declarative intent. **`platctl sync` brings the project files into alignment with the manifests.** Idempotent, runnable anytime, no-ops if everything already matches. CI runs `platctl check` (`sync --dry-run`) and fails on drift.

Example: a plugin author edits `plugin.toml` to add a new inter-plugin proto dependency. They run `platctl sync`. The tool updates `buf.yaml`, the plugin's `Cargo.toml` and `package.json`, runs `buf generate`, and regenerates the composed glue code. One manifest edit → coherent state across five files.

### 7.2 Sync strategy

Files fall into three categories:

| Category | Edited by | Sync behavior |
|---|---|---|
| **Fully generated** (`plugins.rs`, `plugins.generated.ts`, the typed component registry, the composed router) | Never by humans | `platctl` overwrites unconditionally. Header comment marks them as generated. |
| **Co-managed** (`Cargo.toml`, `package.json`, `buf.yaml`) | Plugin authors **and** `platctl` | `platctl` uses marker comments and only edits inside markers. Authors edit freely outside. |
| **Human-only** (plugin source code, `.proto` files, `plugin.toml`, `platform.toml`) | Authors only | `platctl` reads but never writes. |

Marker convention for co-managed files:

```toml
# Cargo.toml
[dependencies]
serde = "1"

# >>> platctl managed start
events-plugin   = { path = "../events" }
speakers-plugin = { path = "../speakers" }
# <<< platctl managed end

[dev-dependencies]
# ...
```

If a human edits between markers, `platctl sync` overwrites their changes (and `platctl check` reports the drift). Outside markers, `platctl` leaves the file alone.

### 7.3 v1 commands (essential)

| Command | Purpose |
|---|---|
| `platctl sync` | Bring all derived files in line with manifests. `--plugin <name>` to scope. `--dry-run` to preview. |
| `platctl check` | `sync --dry-run` + manifest validation + dep graph validation. CI entry point. |
| `platctl new plugin <name>` | Scaffold a plugin (manifest, Cargo crate, FE package, proto skeleton, register stub). |
| `platctl new component <plugin> <name>` | Add a component file, register under `[exposes.components]`, run sync. |
| `platctl new rpc <plugin> <service>` | Add a service to the plugin's `.proto`, scaffold Rust handler + TS client wiring. |
| `platctl new migration <plugin> <name>` | Create up/down SQL pair in the plugin's migrations dir. |
| `platctl new permission <plugin> <perm>` | Add entry under `[permissions]`. |
| `platctl plugin enable <name>` | Add to `platform.toml`, run sync. Refuses if required deps missing. |
| `platctl plugin disable <name>` | Remove from `platform.toml`, run sync. Refuses if depended upon. |
| `platctl plugin list` | Enabled / disabled / available. |
| `platctl plugin info <name>` | Manifest summary + resolved deps, mount points, exposed components, permissions, capabilities. |
| `platctl dev` | The one-command dev flow: Vite + `cargo run` + file watchers + `buf generate` on proto change + `sync` on manifest change. |
| `platctl build` | Full production build → single self-contained Rust binary. |

### 7.4 v2 commands (introspection, deferred but planned)

| Command | Purpose |
|---|---|
| `platctl deps tree` | Plugin dep graph. |
| `platctl deps why <plugin>` | Explain why a plugin is enabled (transitively). |
| `platctl deps unused` | Declared deps with no actual import. |
| `platctl deps missing` | Imports without a corresponding manifest declaration. |
| `platctl routes` | All composed FE routes. |
| `platctl rpc list` | All RPC services + methods. |
| `platctl registry` | The generated component registry, per consumer. |
| `platctl permissions` | All permissions across enabled plugins. |
| `platctl capabilities <plugin>` | What a plugin declared (and eventually, what it actually uses). |
| `platctl migrate up/down/status` | Aggregated platform migrations. `--plugin <name>` for scoped. |
| `platctl breaking-check` | `buf breaking` across all plugin protos vs. main. |
| `platctl diff-manifest` | Per-plugin manifest diff vs. main, useful in PR review. |

### 7.5 Later (when scale demands it)

- **`platctl docs generate`** — static documentation site listing every plugin's routes, RPC services, components, permissions, and capabilities. High value for compliance audits.
- **`platctl release <plugin> --version X`** — atomic version bumps across manifest + Cargo + package.json + changelog. Only needed with independent plugin versioning.
- **`platctl lint`** — opinionated authoring suggestions.
- **`platctl i18n extract / check`** — translation pipeline.
- **Capability enforcement at runtime** — only if/when third-party plugins ever happen.

### 7.6 Design rules

- **Wrap, don't replace.** `platctl` calls `buf`, `cargo`, `pnpm`, `sqlx`, etc. — it doesn't reimplement them.
- **Noun-verb subcommands.** Scales to 50+ commands cleanly (`plugin enable`, `deps tree`, `new component`).
- **All mutating commands respect `--dry-run`.** CI uses this.
- **Scoped or global.** `platctl sync` does everything; `--plugin <name>` scopes to one plugin. Both produce the same end state.
- **Single Rust binary**, lives in the same workspace as everything else. No external install step for contributors.
- **Plain text output by default, `--format json` for scripting.**

### 7.7 Build artifact

`platctl build` produces one self-contained Rust binary:

- Axum server
- Connect-RPC services at `/rpc/<plugin>/*`
- Plugin HTTP routes at `/h/<plugin>/*`
- Plugin SPA routes at `/p/<plugin>/*` (served as part of the embedded FE)
- Embedded FE bundle via `rust-embed` (fallthrough to `index.html` + assets)

Drop the binary on a host, point it at a database, run.

### 7.8 Why one composed FE project instead of per-plugin bundles

- Single JS runtime, single React tree, single style cascade — no iframe/microfrontend overhead.
- Shared dependencies (React, TanStack, Connect) deduplicated by Vite.
- Cross-plugin imports are real ES imports, not runtime federation.
- One bundle is dramatically easier to cache, version, and embed than many.

---

## 8. Cross-Plugin Composition

### 8.1 Required dependencies

Required deps are normal ES imports between plugin packages:

```svelte
import { SpeakerPicker } from '@platform/plugin-speakers';
```

`platctl check` validates at build time that the importing plugin declares the dep in its manifest. Missing required deps → build fails.

### 8.2 Optional dependencies

Optional deps cannot be imported directly because the dependency may be disabled. `platctl` generates a **typed component registry**:

```ts
// generated by platctl based on enabled plugins
export const registry = {
  'speakers.SpeakerPicker': SpeakerPicker,
  'venues.VenuePicker': VenuePicker,
  // keys for disabled plugins are absent
} as const;

export function getComponent<K extends keyof typeof registry>(key: K): typeof registry[K];
export function getComponent<K extends string>(key: K): ComponentType | undefined;
```

Consumer usage:

```tsx
const VenuePicker = getComponent('venues.VenuePicker');

return VenuePicker
  ? <VenuePicker value={venueId} onChange={setVenueId} />
  : <input value={venueName} onChange={e => setVenueName(e.target.value)} placeholder="Venue (free text)" />;
```

`platctl` types each consumer's view of the registry based on its manifest:
- Required deps → key is present, component type is concrete.
- Optional deps → key is `Component | undefined`.
- Undeclared deps → key absent (compile error to reference).

---

## 9. Build & Dev Workflow

All workflows go through `platctl`. See §7 for the full command reference.

### 9.1 Production build

```
platctl check     # validate manifests + dep graph + drift
platctl build     # → single binary at target/release/platform
```

### 9.2 Dev mode

```
platctl dev
```

Orchestrates: Vite dev server (with HMR) + `cargo run` (with file watching) + `buf generate` on `.proto` changes + `platctl sync` on `plugin.toml` changes. Vite proxies `/rpc/*` and `/h/*` to the backend. pnpm workspace symlinks make cross-plugin FE imports hot-reload.

(Dev mode ergonomics need real-world validation — flagged as open question.)

---

## 10. Infrastructure & Data

The host (`platform/`) provides a small set of shared infrastructure capabilities. Plugins consume them through typed handles on `PluginContext` (exposed via `platform-sdk`), gated by `[requires.capabilities]` in the plugin's manifest.

### 10.1 Provided infrastructure

| Capability | Handle | v1 backing | Notes |
|---|---|---|---|
| Database | `Db` | PostgreSQL (single instance per deployment) | Per-plugin schema + per-plugin Postgres role (§10.2 / §10.4). |
| Object storage | `Storage` | S3-compatible (MinIO locally; S3/R2/etc. in prod) | Per-plugin bucket prefix. |
| Background jobs | `Jobs` | Postgres-backed queue (e.g. apalis) | No Redis dependency in v1. |
| Email | `Email` | Pluggable transactional provider (SES / Resend / Postmark) | Backend chosen per deployment. |
| Config / Secrets | `Config` | Typed access to deployment config + env-var secrets | See §5.5 deployment block. |
| Telemetry | `Telemetry` | OpenTelemetry (tracer, meter, logger) | Standard across PLAI codebases. |

**Deferred to v2**: cache (Redis), search (Meilisearch/Elasticsearch), realtime/WebSockets, formal outbound-HTTP capability.

### 10.2 Database

- **PostgreSQL**, single instance per deployment.
- **sqlx** for access: compile-time-checked queries (`query!` / `query_as!`), async, native connection pool. No ORM — plain SQL is the contract.
- Each plugin commits its `.sqlx/` prepared-query cache; `cargo sqlx prepare --check` runs in `platctl check`.
- **One Postgres schema per plugin**, named after the plugin (`speakers.*`, `events.*`). The host owns `platform.*` (users, sessions, RBAC) and `meta.*` (migration bookkeeping).
- **Connection pools owned by the host.** Plugins never instantiate `PgPool` directly. The host runs one pool per Postgres role (§10.4); `PluginContext.db()` returns the plugin's scoped pool.
- Capability gates: `db.read` grants a read-only handle; `db.write` grants a full handle.

### 10.3 Cross-plugin data access

Always-apply migrations (§10.5) mean every plugin's schema exists in every deployment. This makes cross-plugin data access uniform whether the dep is required or optional.

**Declaring the surface**:

```toml
# plugins/speakers/plugin.toml — owner declares public tables
[exposes.tables.speaker]
schema = "speakers"
description = "Speaker records — stable public schema."

# plugins/events/plugin.toml — consumer declares what it uses
[dependencies.speakers]
optional = false
tables = ["speaker"]
```

**Reads (SELECT / JOIN)**: allowed against any table in `B.[exposes.tables]` that the consumer declared in its `tables = [...]` list.

**Writes (INSERT / UPDATE / DELETE)**: allowed against the same surface. Plugin authors are trusted to use SQL directly. If B needs to enforce invariants regardless of who writes, B defines **Postgres triggers** on its own tables — triggers fire for every writer.

**FKs**:
- Across required deps: `NOT NULL` permitted.
- Across optional deps: **must be nullable** — the dep's code may not be running and won't be creating rows to reference. `platctl check` rejects `NOT NULL` FKs targeting an optional-dep schema.

**Forbidden**:
- `ON DELETE CASCADE` / `ON UPDATE CASCADE` on cross-plugin FKs — silently mutates other plugins' tables and bypasses coordination. `platctl check` rejects them.
- Any SQL reference to a non-public table in another plugin's schema. Enforced at runtime by Postgres roles (§10.4) and at PR time by `platctl check`.

### 10.4 Postgres role enforcement

Each plugin runs its queries as its own Postgres role. Grants are computed from manifests and applied by the migration runner — convention becomes database-enforced.

For a plugin `events` that declares `[dependencies.speakers].tables = ["speaker"]`:

```sql
CREATE ROLE role_events NOINHERIT;

-- Own schema: full access
GRANT USAGE, CREATE ON SCHEMA events TO role_events;
GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA events TO role_events;
ALTER DEFAULT PRIVILEGES IN SCHEMA events GRANT ALL ON TABLES TO role_events;

-- Declared cross-plugin deps: full DML on declared exposed tables
GRANT USAGE ON SCHEMA speakers TO role_events;
GRANT SELECT, INSERT, UPDATE, DELETE ON speakers.speaker TO role_events;

-- Host public surfaces: read-only
GRANT USAGE ON SCHEMA platform TO role_events;
GRANT SELECT ON platform.user TO role_events;
```

The host maintains one `PgPool` per role; `PluginContext.db()` returns the plugin's pool. A plugin trying to query outside its grants gets a Postgres permission error.

**Migration runner role**: a privileged role (e.g. `platform_migrator`) that owns all schemas and can issue GRANT statements. The only role with broad schema-modification rights. Used exclusively by `platctl migrate`.

### 10.5 Migrations

**Layout**: each plugin has `plugins/<name>/migrations/<ts>_<name>.up.sql`. Down migrations are optional and discouraged — forward-fix is the recommended discipline.

**Bookkeeping** lives in `meta.migrations`:

```sql
CREATE SCHEMA meta;

CREATE TABLE meta.migrations (
    id              BIGSERIAL    PRIMARY KEY,            -- apply order across deployment lifetime
    plugin          TEXT         NOT NULL,               -- 'platform' for host migrations
    migration_name  TEXT         NOT NULL,               -- e.g. '0007_create_speaker'
    checksum        TEXT         NOT NULL,               -- SHA-256 of .up.sql contents
    applied_at      TIMESTAMPTZ  NOT NULL DEFAULT now(),
    UNIQUE (plugin, migration_name)
);
```

`id BIGSERIAL` gives true apply-order; `checksum` lets `platctl` detect post-apply file edits.

**Always-apply**: every deployment runs the full migration set from its pinned source revision regardless of which plugins are *enabled*. A disabled plugin still has its schema and Postgres role; only its router / RPC / code paths are absent. Enable/disable is purely a code concern.

**Ordering**:

1. **Host migrations first.** All migrations under `platform/migrations/` apply before any plugin migration.
2. **Plugin migrations** then apply as a topo sort over a DAG with edges from:
   - **Within a plugin**: migration N → migration N+1 (implicit, from filename order).
   - **`@requires` declarations**: explicit edges between any two migrations.

   Plugin-level manifest dependencies **do not** infer migration ordering. Migrations declare their cross-plugin order directly in the SQL.

**`@requires` syntax** — SQL header comment, parsed before any executable SQL:

```sql
-- @requires speakers:0007_create_speaker
-- @requires platform:0003_users

CREATE TABLE events.event (
  id           UUID PRIMARY KEY,
  speaker_id   UUID NULL REFERENCES speakers.speaker(id),
  organizer_id UUID NOT NULL REFERENCES platform.user(id),
  ...
);
```

Format: `-- @requires <plugin>:<migration_name>` where `<migration_name>` is the filename without `.up.sql`. The `@` prefix is reserved for `platctl` directives; future additions (`@breaking-change`, etc.) go here.

**Edges flow in either direction.** Cleanup migrations naturally reverse the dep arrow:

```sql
-- plugins/events/migrations/0050_drop_speaker_email_cache.up.sql
ALTER TABLE events.event DROP COLUMN cached_speaker_email;
```

```sql
-- plugins/speakers/migrations/0051_drop_speaker_email.up.sql
-- @requires events:0050_drop_speaker_email_cache
ALTER TABLE speakers.speaker DROP COLUMN email;
```

Topo sort puts `events:0050` before `speakers:0051`. This is exactly why we don't infer ordering from manifest deps — cleanup flows against them.

**`platctl check` enforces**:
- Every `@requires` reference resolves to a real migration.
- The DAG has no cycles.
- Every cross-plugin SQL reference (FK or schema-qualified table reference) has a matching `@requires` for the target migration.
- No `CASCADE` clauses on cross-plugin FKs.
- `checksum` matches `meta.migrations` for already-applied migrations.
- `cargo sqlx prepare --check` is clean for every plugin.

### 10.6 Schema evolution & compatibility

The cross-plugin compatibility surface is `[exposes.tables]`. Rules mirror `buf breaking` for protos, applied to SQL:

| Change to an exposed table | Compatibility |
|---|---|
| Add column (nullable or with default) | Compatible |
| Add table to exposed set | Compatible |
| Drop `NOT NULL` | Compatible |
| Widening type change (`varchar(50)` → `text`, `int` → `bigint`) | Compatible |
| Add index | Compatible |
| Drop column | **Breaking** |
| Rename column | **Breaking** |
| Drop table or remove from exposed set | **Breaking** |
| Narrowing type change | **Breaking** |
| Add `NOT NULL` (when existing rows could violate) | **Breaking** |
| Change PK / drop unique constraint targeted by FK | **Breaking** |

Private tables (anything not in `[exposes.tables]`) can change freely — no consumer dependencies.

**Coordinated breaking changes**: a breaking change to an exposed table must ship in the same source revision (same PR) as matching migrations and code updates in every consumer plugin.

**Enforcement in v1** uses two layers:

**Layer 1 — `platctl check` (static, fast)**:
- Parses migrations; flags `ALTER` / `DROP` / `RENAME` on tables listed in any plugin's `[exposes.tables]` as "touches public schema."
- For known-breaking patterns (`DROP COLUMN`, `RENAME COLUMN`, `DROP TABLE`): verifies that every consumer plugin (declared via `[dependencies.b].tables`) either drops the table from its declared deps or has a matching migration in this PR.
- Rejects `CASCADE` on cross-plugin FKs.

**Layer 2 — CI integration test**:
- Ephemeral Postgres (testcontainers).
- `platctl migrate up` against a representative deployment configuration.
- Run all plugin test suites against the migrated DB.
- Catches anything Layer 1 missed: actual FK violations on apply, query failures, type errors, runtime test failures.

**Layer 3 — schema snapshot diff (deferred)**: per-plugin `exposed-schema.sql` snapshot diffed by `platctl`, with breaking changes requiring an `@breaking-change` annotation. Adopt when manual coordination starts missing things at scale.

### 10.7 Authentication & Authorization

The host owns identity, sessions, groups, roles, and resource-level access checks. Plugins consume these via `Auth`, `Users`, and authorization helpers on `PluginResources` (§11.3); they never roll their own auth.

#### 10.7.1 Identity: OIDC + Authentik, server sessions

- Authentication is via **OIDC against Authentik** (configurable per deployment). The platform never manages passwords.
- After a successful OIDC flow, the platform creates a **server-side session** in `platform.session` (Postgres). Sessions are identified by an HTTP-only, Secure, SameSite=Lax cookie carrying only the session ID.
- OIDC access/refresh tokens are stored encrypted in the session for downstream calls if needed (proxying to other Authentik-protected services); otherwise unused.
- The host's auth middleware exchanges the cookie for a `User` on every request before handler logic runs.

Token-in-localStorage is explicitly not supported (XSS exposure).

#### 10.7.2 Identity schema

```sql
CREATE TABLE platform.user (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    oidc_sub      TEXT NOT NULL UNIQUE,         -- Authentik subject
    email         TEXT NOT NULL,
    display_name  TEXT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE platform.session (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id       UUID NOT NULL REFERENCES platform.user(id) ON DELETE CASCADE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at    TIMESTAMPTZ NOT NULL,
    oidc_tokens   BYTEA                          -- encrypted, optional
);
CREATE INDEX session_user_idx    ON platform.session(user_id);
CREATE INDEX session_expires_idx ON platform.session(expires_at);
```

#### 10.7.3 Groups, roles, memberships

- A **group** is a real-world organizational unit (committee, caucus, working group, campaign team).
- A **role** is defined *within* a group; the same role name in two groups is two distinct rows. ("Chair of Committee X" is a different role from "Chair of Caucus Y".)
- A user has **at most one role per group**. Multiple roles are modeled as a promotion (replace the existing membership row), not as a multi-role membership.
- Roles own the **capability** permissions — the strings declared in plugin manifests' `[permissions]` blocks (§6.1).
- **Groups have no relations among themselves.** No hierarchy, no parent/child, no inheritance. A user is either a member of a group or they aren't.

```sql
CREATE TABLE platform.group (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name          TEXT NOT NULL,
    description   TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE platform.group_role (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    group_id      UUID NOT NULL REFERENCES platform.group(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    UNIQUE (group_id, name)
);

CREATE TABLE platform.role_permission (
    role_id       UUID NOT NULL REFERENCES platform.group_role(id) ON DELETE CASCADE,
    permission    TEXT NOT NULL,                 -- e.g. 'speakers:read'
    PRIMARY KEY (role_id, permission)
);

CREATE TABLE platform.group_membership (
    user_id       UUID NOT NULL REFERENCES platform.user(id) ON DELETE CASCADE,
    group_id      UUID NOT NULL REFERENCES platform.group(id) ON DELETE CASCADE,
    role_id       UUID NOT NULL REFERENCES platform.group_role(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, group_id)              -- one role per (user, group)
);
```

#### 10.7.4 Resource ownership and sharing

Capability permissions (`speakers:read`) declare *what kind* of action. **Scope** — which specific resources a user may act on — is resolved separately via ownership and explicit shares.

**Ownership is per-resource-type AND per-instance.** Some resource types are *conceptually* group-owned: a meeting in a speakers-list app belongs to the committee that scheduled it. Others are *conceptually* user-owned: a personal draft document, a vote ballot cast by an individual. The platform supports both via a single `resource_principal` table where exactly one of `owner_user_id` or `owner_group_id` is set; **the plugin decides at creation time** which principal kind is appropriate for the resource being created. There is no manifest-level constraint forcing all instances of a resource type to one ownership kind — though plugins are encouraged to be consistent within a resource type for predictability.

**Owners have implicit full access** to their resources, regardless of role permissions. Otherwise users could create resources they can't subsequently see — a footgun.

**Sharing in v1**: only owners may share their resources. A finer-grained "who can share what" model is deferred. Explicit shares grant a specific permission to a principal (another user, a group, or the platform-wide public).

**Public resources**: opt-in per resource via a `resource_share` row with `principal_kind = 'public'`. Useful for things like committee-wide announcements or organization-wide documents.

```sql
-- Ownership: exactly one of user_id / group_id is set.
CREATE TABLE platform.resource_principal (
    resource_kind   TEXT NOT NULL,               -- '<plugin>:<table>', e.g. 'speakers:speaker'
    resource_id     UUID NOT NULL,
    owner_user_id   UUID REFERENCES platform.user(id)  ON DELETE CASCADE,
    owner_group_id  UUID REFERENCES platform.group(id) ON DELETE CASCADE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (resource_kind, resource_id),
    CHECK ((owner_user_id IS NULL) <> (owner_group_id IS NULL))
);

-- Explicit per-resource grants.
CREATE TABLE platform.resource_share (
    resource_kind         TEXT NOT NULL,
    resource_id           UUID NOT NULL,
    principal_kind        TEXT NOT NULL,         -- 'user' | 'group' | 'public'
    principal_user_id     UUID REFERENCES platform.user(id)  ON DELETE CASCADE,
    principal_group_id    UUID REFERENCES platform.group(id) ON DELETE CASCADE,
    permission            TEXT NOT NULL,
    granted_by_user_id    UUID NOT NULL REFERENCES platform.user(id),
    granted_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at            TIMESTAMPTZ,
    CHECK (
        (principal_kind = 'user'   AND principal_user_id  IS NOT NULL AND principal_group_id IS NULL) OR
        (principal_kind = 'group'  AND principal_group_id IS NOT NULL AND principal_user_id  IS NULL) OR
        (principal_kind = 'public' AND principal_user_id  IS NULL     AND principal_group_id IS NULL)
    )
);
```

`resource_kind` uses the **`<plugin>:<table>`** format (e.g. `speakers:speaker`, `events:meeting`). Plugins document their resource kinds in the authoring guide.

#### 10.7.5 The access-check function

```sql
CREATE OR REPLACE FUNCTION platform.user_can_access(
    p_resource_kind TEXT,
    p_resource_id   UUID,
    p_user_id       UUID,
    p_permission    TEXT
) RETURNS BOOLEAN
LANGUAGE plpgsql STABLE
AS $$
DECLARE
    v_owner_user_id  UUID;
    v_owner_group_id UUID;
BEGIN
    -- 0. Resolve ownership.
    SELECT owner_user_id, owner_group_id
    INTO v_owner_user_id, v_owner_group_id
    FROM platform.resource_principal
    WHERE resource_kind = p_resource_kind AND resource_id = p_resource_id;

    -- 1. Owner has implicit full access.
    IF v_owner_user_id = p_user_id THEN RETURN true; END IF;

    -- 2. Owned by a group where the user's role grants p_permission.
    IF v_owner_group_id IS NOT NULL AND EXISTS (
        SELECT 1
        FROM platform.group_membership gm
        JOIN platform.role_permission rp ON rp.role_id = gm.role_id
        WHERE gm.user_id    = p_user_id
          AND gm.group_id   = v_owner_group_id
          AND rp.permission = p_permission
    ) THEN RETURN true; END IF;

    -- 3. Explicit share to this user (active).
    IF EXISTS (
        SELECT 1 FROM platform.resource_share rs
        WHERE rs.resource_kind = p_resource_kind
          AND rs.resource_id   = p_resource_id
          AND rs.permission    = p_permission
          AND rs.principal_user_id = p_user_id
          AND (rs.expires_at IS NULL OR rs.expires_at > now())
    ) THEN RETURN true; END IF;

    -- 4. Explicit share to a group the user is in (active).
    IF EXISTS (
        SELECT 1
        FROM platform.resource_share rs
        JOIN platform.group_membership gm ON gm.group_id = rs.principal_group_id
        WHERE rs.resource_kind = p_resource_kind
          AND rs.resource_id   = p_resource_id
          AND rs.permission    = p_permission
          AND gm.user_id       = p_user_id
          AND (rs.expires_at IS NULL OR rs.expires_at > now())
    ) THEN RETURN true; END IF;

    -- 5. Public share (active).
    IF EXISTS (
        SELECT 1 FROM platform.resource_share rs
        WHERE rs.resource_kind = p_resource_kind
          AND rs.resource_id   = p_resource_id
          AND rs.permission    = p_permission
          AND rs.principal_kind = 'public'
          AND (rs.expires_at IS NULL OR rs.expires_at > now())
    ) THEN RETURN true; END IF;

    RETURN false;
END;
$$;
```

#### 10.7.6 Two-layer authorization in plugin code

Both layers apply on every authenticated request:

**Layer 1 — capability gate at the handler / repo method** (already specified in §11.5, §11.6). The `Has<X>` bound ensures the user has the capability *somewhere* in their group memberships. Coarse, fast, compile-time enforced.

**Layer 2 — resource-scoped filter at the query.** Every repository method that returns or operates on resources joins through `platform.user_can_access(...)`:

```rust
#[impl_repository(SpeakerRepo)]
impl<P: Has<SpeakersRead>> SpeakerRepo<P> {
    pub async fn list(&self) -> Result<Vec<Speaker>, RepoError> {
        sqlx::query_as!(Speaker, "
            SELECT s.*
            FROM speakers.speaker s
            WHERE platform.user_can_access('speakers:speaker', s.id, $1, 'speakers:read')
        ", self.user_id)
        .fetch_all(self.pool())
        .await
        .map_err(Into::into)
    }

    pub async fn get(&self, id: SpeakerId) -> Result<Option<Speaker>, RepoError> {
        sqlx::query_as!(Speaker, "
            SELECT s.*
            FROM speakers.speaker s
            WHERE s.id = $1
              AND platform.user_can_access('speakers:speaker', s.id, $2, 'speakers:read')
        ", id, self.user_id)
        .fetch_optional(self.pool())
        .await
        .map_err(Into::into)
    }
}
```

Rows the user can't access simply don't appear in results — no "permission denied" mid-query, no leaked existence.

#### 10.7.7 Plugin integration: recording ownership and sharing

**On create**: plugins record ownership atomically with the insert, via an `authz` helper on `PluginResources` (or via a derive that generates the boilerplate):

```rust
#[impl_repository(SpeakerRepo)]
impl<P: Has<SpeakersWrite>> SpeakerRepo<P> {
    pub async fn create(&self, input: NewSpeaker) -> Result<Speaker, RepoError> {
        let mut tx = self.pool().begin().await?;

        let s = sqlx::query_as!(Speaker,
            "INSERT INTO speakers.speaker (...) VALUES (...) RETURNING *",
            /* ... */
        ).fetch_one(&mut *tx).await?;

        // Plugin chooses ownership kind per resource type:
        //   - Speakers (personal contacts):     Principal::User(self.user_id)
        //   - Meetings (committee-scheduled):   Principal::Group(meeting.committee_id)
        //   - Ballots (cast by a user):         Principal::User(self.user_id)
        self.authz
            .record_owner(&mut tx, "speakers:speaker", s.id, Principal::User(self.user_id))
            .await?;

        tx.commit().await?;
        Ok(s)
    }
}
```

A `#[owned_by(user)]` / `#[owned_by(group_from = "...")]` attribute on the repo method could automate this; deferred to implementation.

**Sharing**: plugins expose mutation RPCs (e.g. `SpeakerService.ShareSpeaker`) that delegate to the host's `authz.share(...)` API. The host enforces "only the current owner can share" and writes to `platform.resource_share`.

#### 10.7.8 Frontend `User` and per-resource flags

The frontend `User` object (from `@platform/sdk`) carries memberships and their capability permissions:

```ts
interface User {
  id: UserId;
  email: string;
  displayName: string;
  memberships: ReadonlyArray<{
    groupId:     GroupId;
    groupName:   string;
    role:        { id: RoleId; name: string };
    permissions: ReadonlySet<string>;
  }>;
}
```

`useHasPermission('speakers:read')` returns true if **any** of the user's memberships grants it. This is the FE analog of Layer 1 — gates "Create" buttons, navigation items, etc.

For per-resource decisions ("can I edit *this* speaker?"), the server includes computed flags on each returned resource — e.g. `Speaker { ..., viewerCanEdit: bool, viewerCanShare: bool }`. The FE reads these flags rather than recomputing access. **The server is the source of truth**; the client just reflects.

---

## 11. Backend Plugin Interface

This section defines the Rust API surface a plugin author writes against. It lives in the `platform-sdk` crate (§5.1) and is consumed by every plugin crate. Cross-references: capabilities and DB access in §10; manifest schema in §6.1; cross-plugin composition in §8.

### 11.1 The `Plugin` trait

Every plugin crate implements `Plugin` once on a top-level struct.

```rust
#[async_trait]
pub trait Plugin: Send + Sync + 'static {
    /// Static metadata, produced by the plugin_metadata! macro from plugin.toml.
    fn metadata(&self) -> &'static PluginMetadata;

    /// Receive the raw, pre-scoped resources for this plugin (Postgres pool
    /// configured for the plugin's role, object storage scoped to the plugin's
    /// bucket prefix, etc.) and return an Axum Router. The plugin sets up its
    /// Axum state internally — typically by passing `resources` directly via
    /// `.with_state(resources)` and using a plugin-defined `#[derive(PluginCtx)]`
    /// context type as the per-request extractor (§11.6).
    /// The host nests the returned router under the plugin's prefixes.
    fn routes(&self, resources: PluginResources) -> Router;

    /// Lifecycle: after migrations, before traffic. Default: no-op.
    async fn on_startup(&self, _resources: &PluginResources) -> Result<(), PluginError> {
        Ok(())
    }

    /// Lifecycle: when the server begins graceful shutdown. Default: no-op.
    async fn on_shutdown(&self, _resources: &PluginResources) -> Result<(), PluginError> {
        Ok(())
    }

    /// Background workers to register with the job queue. Default: none.
    fn jobs(&self) -> Vec<JobHandler> {
        Vec::new()
    }
}
```

**Convention**: every plugin exposes `pub fn new(...) -> Self`. The constructor may take arguments (eager state, host-injected configuration); `platctl`-generated glue calls it. Object-safe via `async_trait` so the host can hold `Vec<Box<dyn Plugin>>` — the per-plugin context type is not on the trait, so generics over it don't break object safety.

### 11.2 Metadata via macro (not source-tree codegen)

The plugin's `lib.rs` invokes a proc-macro that reads the crate's `plugin.toml` at compile time and expands to the static metadata plus typed permission markers:

```rust
// plugins/speakers/src/lib.rs

platform_sdk::plugin_metadata!();
// expands to (conceptually):
//   pub static METADATA: PluginMetadata = PluginMetadata { name: "speakers", ... };
//   pub mod permissions {
//       pub struct SpeakersRead;  impl Permission for SpeakersRead  { const NAME: &str = "speakers:read"; }
//       pub struct SpeakersWrite; impl Permission for SpeakersWrite { const NAME: &str = "speakers:write"; }
//       pub struct SpeakersBook;  impl Permission for SpeakersBook  { const NAME: &str = "speakers:book"; }
//   }

pub struct SpeakersPlugin { /* ... */ }

impl SpeakersPlugin {
    pub fn new() -> Self { Self { /* ... */ } }
}

#[async_trait]
impl Plugin for SpeakersPlugin {
    fn metadata(&self) -> &'static PluginMetadata { &METADATA }
    fn routes(&self, resources: PluginResources) -> Router { /* see §11.7 */ }
}
```

**Why a macro, not codegen into source:**
- No generated `.rs` files in plugin source trees.
- `plugin.toml` edits propagate at the next `cargo build` automatically.
- The macro is the single mapping from manifest → type system: permission types declared in the manifest **are exactly** the types available in code. Referencing an undeclared permission is a compile error — no separate `platctl check` pass for permission name validity.

The macro lives in a `platform-sdk-macros` proc-macro crate.

**`platctl check` enforcement**: scans every plugin's crate root and verifies that `plugin_metadata!()` is invoked exactly once. Missing or duplicate invocations fail with a clear error.

### 11.3 `PluginResources` and `PluginContext<S, P>`

The host hands every plugin a `PluginResources` bundle — the raw, pre-scoped primitives it needs. `PluginContext<S, P>` is a generic type in `platform-sdk` that plugins parameterize with their own per-request state `S` (typically a struct of typed repositories) and a permission witness `P`.

```rust
// In platform-sdk:

/// Raw, pre-scoped resources. The DB pool is configured for the plugin's
/// Postgres role; storage for its bucket prefix; telemetry pre-tagged with
/// `plugin = "<name>"`. Plugins receive this in `Plugin::routes` and lifecycle
/// hooks and pass it through to their own Axum state.
#[derive(Clone)]
pub struct PluginResources {
    pub(crate) db: PluginDb,            // opaque; data access via Repository only
    pub(crate) storage: PluginStorage,  // opaque; file access via Bucket only
    pub jobs: Jobs,
    pub email: Email,
    pub config: PluginConfig,
    pub telemetry: Telemetry,
    pub auth: Auth,
    pub users: Users,
}

/// Generic per-request context. Plugins parameterize S (their own state, often
/// a bundle of typed repositories) and P (permission witness — what permissions
/// the current request has been proven to hold).
#[derive(Clone)]
pub struct PluginContext<S, P = ()>
where
    S: Clone + Send + Sync + 'static,
{
    pub state: S,                    // plugin-defined: repos, caches, audit context, ...
    pub user: Option<User>,
    pub resources: PluginResources,  // host-provided primitives, always available
    _phantom: PhantomData<P>,
}
```

Note what's still hidden:
- **`PluginDb` is opaque** — not in the public API of `platform-sdk` in a queryable form. The only way to use it is via a `#[derive(Repository)]` type that internally owns a `ScopedDb` derived from it (§11.5).
- **`PluginStorage` is opaque** the same way — only usable via a `#[derive(Bucket)]` type.

This means the repository pattern enforcement from §11.5 survives unchanged: `sqlx::query!()` cannot compile outside a repository, regardless of how the plugin constructs its context.

What's freely accessible on `resources`:
- `jobs`, `email`, `config`, `telemetry`, `auth`, `users` — all the non-DB / non-storage handles. Capability gating is **runtime**: calling `resources.email.send(...)` without declaring `email.send` returns `Err(PluginError::CapabilityNotDeclared("email.send"))`.

Type aliases per plugin make handler signatures readable:

```rust
// In each plugin:
pub type SpeakersCtx<P = ()> = PluginContext<SpeakersState<P>, P>;
```

Examples:
- `SpeakersCtx<()>` — no permissions proven; used in lifecycle hooks or system contexts.
- `SpeakersCtx<SpeakersRead>` — read permission proven.
- `SpeakersCtx<permissions!(SpeakersRead & SpeakersWrite)>` — both proven.

`PluginContext` is `Clone` (resources and state are `Arc`-wrapped internally) so plugin services and RPC handlers can take owned copies without lifetime gymnastics.

### 11.4 Typed permission system

```rust
// In platform-sdk:

pub trait Permission: Send + Sync + 'static {
    const NAME: &'static str;
}

/// Conjunction of two permissions.
pub struct And<A, B>(PhantomData<(A, B)>);

/// "P contains permission X" — implemented for X itself and transitively through And.
pub trait Has<X: Permission> {}
impl<X: Permission> Has<X> for X {}
impl<X: Permission, B> Has<X> for And<X, B> {}
impl<X: Permission, A: Permission> Has<X> for And<A, X> {}
// (full transitivity worked out via sealed helper traits in implementation)

/// Trait listing permissions for runtime introspection (used by the extractor).
pub trait PermissionList {
    fn names() -> &'static [&'static str];
}
impl<P: Permission> PermissionList for P {
    fn names() -> &'static [&'static str] { &[P::NAME] }
}
// + impls for And<A, B>, etc.
```

The `permissions!` macro builds the type expression:

```rust
permissions!(SpeakersRead & SpeakersWrite)
// expands to: And<SpeakersRead, SpeakersWrite>

permissions!(SpeakersRead & SpeakersWrite & EventsRead)
// expands to: And<SpeakersRead, And<SpeakersWrite, EventsRead>>
```

Permission markers come from each plugin's `plugin_metadata!()` expansion (§11.2): one zero-sized struct per declared permission. **Referencing an undeclared permission is a compile error** because the type doesn't exist.

OR-style combinators (`Or<A, B>` + `HasAny<X>`) deferred until a real handler needs them.

### 11.5 Repositories — `platform-sdk`-enforced, permission-gated

Data access **must** go through a repository. `platform-sdk` enforces this by hiding the raw `sqlx::PgPool` type — it's not in `platform-sdk`'s public API, so `sqlx::query!()` etc. cannot compile outside a repository's impl.

```rust
// plugins/speakers/src/repo.rs

use platform_sdk::Repository;

#[derive(Repository, Clone)]
pub struct SpeakerRepo<P = ()> {
    // The derive macro injects:
    //   - a private `db: ScopedDb` field
    //   - a private `user: Option<User>` field
    //   - the PhantomData<P>
    //   - a `pub fn new(db: &PluginDb, user: Option<User>) -> Self` constructor
    //     (called by the plugin's #[derive(PluginCtx)] machinery — §11.6)
    //   - sealed accessors usable only inside #[impl_repository] blocks below
}

#[impl_repository(SpeakerRepo)]     // attribute macro that enables sqlx access within
impl<P: Has<SpeakersRead>> SpeakerRepo<P> {
    pub async fn get(&self, id: SpeakerId) -> Result<Speaker, RepoError> {
        // Inside this impl block, `self.pool()` is available (provided by the macro).
        // Outside such blocks, the pool is unreachable.
        sqlx::query_as!(Speaker, "SELECT * FROM speakers.speaker WHERE id = $1", id)
            .fetch_one(self.pool())
            .await
            .map_err(Into::into)
    }

    pub async fn list(&self) -> Result<Vec<Speaker>, RepoError> { /* ... */ }
}

#[impl_repository(SpeakerRepo)]
impl<P: Has<SpeakersRead> + Has<SpeakersWrite>> SpeakerRepo<P> {
    pub async fn create(&self, input: NewSpeaker) -> Result<Speaker, RepoError> { /* ... */ }
    pub async fn update(&self, id: SpeakerId, patch: SpeakerPatch) -> Result<Speaker, RepoError> { /* ... */ }
}

#[impl_repository(SpeakerRepo)]
impl<P: Has<SpeakersRead> + Has<SpeakersBook>> SpeakerRepo<P> {
    pub async fn book(&self, id: SpeakerId, slot: BookingSlot) -> Result<Booking, RepoError> { /* ... */ }
}
```

Usage from a route handler — repos are pre-instantiated in the per-plugin context (§11.6), so handlers don't construct them:

```rust
async fn get_speaker(
    ctx: SpeakersCtx<SpeakersRead>,
    Path(id): Path<SpeakerId>,
) -> Result<Json<Speaker>, ApiError> {
    let s = ctx.state.speakers.get(id).await?;
    Ok(Json(s))
}
```

**Properties:**

- **Repositories are mandatory.** `PluginResources` exposes no queryable `PluginDb` API; the only public type that can run SQL is one annotated with `#[derive(Repository)]`. No escape hatch for ad-hoc queries — new queries are new repository methods. Deliberate guardrail.
- **Permission types flow through `Has<X>` bounds** — same machinery used by the per-plugin extractor (§11.6). A handler that extracted `SpeakersCtx<SpeakersRead>` cannot call `ctx.state.speakers.create(...)` because `SpeakersWrite` isn't proven. Compile-time guarantee.
- **Platform-sdk supplies the primitives** (`PluginDb`, `ScopedDb`, `Repository` derive, `#[impl_repository(...)]`, `Has<X>`, `Permission`); plugins compose them. The derive + attribute macros are the only authorized path to a `sqlx` executor.
- **Cross-plugin reads** (§10.3) still go through a repository — typically a consumer plugin defines a read-only view repo wrapping the dep's tables, with its own permission gates.

**`platctl check` additionally verifies**:
- Every plugin crate declaring `db.read` or `db.write` capability has at least one `#[derive(Repository)]` struct.
- No `use sqlx::PgPool` or `use sqlx::query` outside `#[impl_repository(...)]` blocks (regex-scan; redundant with the type-hiding but produces clearer errors).

Object storage follows the same pattern: a `Bucket` derive for typed file access; raw `S3Client` not exposed.

### 11.6 Per-plugin state & extractor via `#[derive(PluginCtx)]`

Each plugin defines its own state type — typically a bundle of its typed repositories. A `#[derive(PluginCtx)]` macro generates the Axum extractor, including the permission check and per-request repository construction.

```rust
// plugins/speakers/src/lib.rs

use platform_sdk::PluginCtx;

/// Per-request state for the speakers plugin. The PluginCtx derive generates
/// the Axum extractor for SpeakersCtx<P> (= PluginContext<SpeakersState<P>, P>).
#[derive(Clone, PluginCtx)]
pub struct SpeakersState<P = ()> {
    #[repo] pub speakers: SpeakerRepo<P>,
    #[repo] pub bookings: BookingRepo<P>,
    // Additional plugin-specific per-request fields can go here. The macro
    // leaves non-#[repo] fields to be constructed via a #[from_request] hook.
}

pub type SpeakersCtx<P = ()> = platform_sdk::PluginContext<SpeakersState<P>, P>;
```

The derive expands (conceptually) to:

```rust
// generated by #[derive(PluginCtx)]
#[async_trait]
impl<P: PermissionList + 'static>
    FromRequestParts<PluginResources> for SpeakersCtx<P>
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, resources: &PluginResources)
        -> Result<Self, ApiError>
    {
        let user = extract_user(parts).await?;
        for perm in P::names() {
            if !user.has_permission(perm) {
                return Err(ApiError::forbidden(perm));
            }
        }
        let state = SpeakersState {
            speakers: SpeakerRepo::new(&resources.db, Some(user.clone())),
            bookings: BookingRepo::new(&resources.db, Some(user.clone())),
        };
        Ok(PluginContext {
            state,
            user: Some(user),
            resources: resources.clone(),
            _phantom: PhantomData,
        })
    }
}
```

The macro handles three responsibilities in one place: permission verification, repository instantiation with the request's user, and context assembly. Plugin authors write the struct; the macro writes the boilerplate.

Usage:

```rust
async fn create_speaker(
    ctx: SpeakersCtx<permissions!(SpeakersRead & SpeakersWrite)>,
    Json(input): Json<NewSpeaker>,
) -> Result<Json<Speaker>, ApiError> {
    let s = ctx.state.speakers.create(input).await?;   // compiles: SpeakersWrite proven by P
    Ok(Json(s))
}
```

A handler extracting `SpeakersCtx<SpeakersRead>` cannot call `ctx.state.speakers.create(...)`. The type system rejects it because `SpeakerRepo<SpeakersRead>` has no `create` method (it's only on `impl<P: Has<SpeakersWrite>> SpeakerRepo<P>`).

**Custom per-request state**: if a plugin needs more than repositories (e.g. a request-scoped cache, an audit-trail builder), it can add non-`#[repo]` fields and provide a `#[from_request]` hook that the derive calls. For v1 we expect ~all plugins to be repo-only; the hook lives in `#[derive(PluginCtx)]` as a deferred extension point (§11.12).

### 11.7 RPC + HTTP route registration

`Plugin::routes(&self, resources: PluginResources) -> Router` returns an **unprefixed** Axum router. The plugin attaches `resources` as Axum state (which the `#[derive(PluginCtx)]` extractor reads from). The host nests the returned router under the plugin's prefixes (RPC under `/rpc/<plugin>`, non-RPC HTTP under `/h/<plugin>`).

For Connect-RPC, plugins implement buf-generated service traits. The service implementation holds the resources (or a derived state) and the auto-generated `connect_rs::router_for` helper wires it up:

```rust
struct SpeakerServiceImpl {
    resources: PluginResources,
}

#[async_trait]
impl SpeakerService for SpeakerServiceImpl {
    async fn get_speaker(
        &self,
        ctx: SpeakersCtx<SpeakersRead>,        // extracted from the request — same machinery as HTTP
        req: GetSpeakerRequest,
    ) -> Result<Speaker, ConnectError> {
        let s = ctx.state.speakers.get(req.id.into()).await?;
        Ok(s.into())
    }
    /* ... */
}

fn routes(&self, resources: PluginResources) -> Router {
    let svc = SpeakerServiceImpl { resources: resources.clone() };
    Router::new()
        .merge(connect_rs::router_for(svc))      // RPC routes under /rpc/speakers (after host nesting)
        .route("/upload", post(upload_handler))  // non-RPC under /h/speakers
        .with_state(resources)                   // Axum state for HTTP extractors
}
```

Non-RPC handlers extract `SpeakersCtx<P>` exactly the same way as RPC handlers.

### 11.8 Permissions on RPC methods (proto-declared, codegen-enforced)

Permission requirements for RPC methods are declared as method options in `.proto` files:

```proto
import "platform/v1/annotations.proto";

service SpeakerService {
  rpc GetSpeaker (GetSpeakerRequest) returns (Speaker) {
    option (platform.requires) = "speakers:read";
  }
  rpc CreateSpeaker (CreateSpeakerRequest) returns (Speaker) {
    option (platform.requires) = "speakers:read,speakers:write";
  }
}
```

What this delivers:
- **Server-side enforcement**: a `connect_rs` interceptor wired by the host reads each method's annotations and rejects requests whose user lacks the listed permissions, before the handler runs. Plugin authors write no Rust permission code for RPC.
- **Type-safe handler bodies**: codegen produces method signatures taking the plugin's own `<PluginName>Ctx<P>` (per §11.6) where `P` is the type-level expansion of the annotated permission list. Repository gates from §11.5 apply transparently inside RPC handlers — the plugin name is known from the proto's enclosing manifest, so codegen synthesizes the right context type.
- **Frontend awareness**: the same annotations are surfaced in the TS client, letting the FE gate UI elements (hide a "Create" button if the user lacks `speakers:write`).
- **Manifest cross-check**: `platctl check` parses proto annotations and verifies every referenced permission appears in the relevant plugin's manifest `[permissions]` block. (Rust gets this for free via the type system; proto needs an explicit check because strings aren't types yet.)

### 11.9 Background jobs

```rust
fn jobs(&self) -> Vec<JobHandler> {
    vec![
        JobHandler::new::<SendBookingConfirmation>("speakers.send_booking_confirmation"),
        JobHandler::new::<SyncExternalSpeakers>("speakers.sync_external"),
    ]
}
```

`JobHandler` wraps a typed job (input struct + async handler). Plugins enqueue:

```rust
resources.jobs.enqueue(SendBookingConfirmation { booking_id }).await?;
```

Job names are namespaced `<plugin>.<job_name>` to prevent collisions. Jobs run with a synthetic system identity. If a job needs to invoke permission-gated repository methods, the platform supplies a `system_context::<SpeakersState<P>>(resources)` helper that constructs a privileged `SpeakersCtx<P>` with all permissions granted (bypassing the request-scoped permission check). Mechanism detail deferred to implementation.

### 11.10 Errors

```rust
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("capability not declared: {0}")]
    CapabilityNotDeclared(&'static str),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error(transparent)]
    External(#[from] anyhow::Error),
}
```

Plugin authors typically use `anyhow::Result` internally and convert at the boundary. RPC handlers convert to `ConnectError`; HTTP handlers convert to `ApiError` mapping to RFC 7807 problem details.

### 11.11 Host startup / shutdown sequence

1. **Migrations** — `platctl migrate up` applies host + all plugin migrations (§10.5).
2. **Resources construction** — host builds one `PluginResources` per plugin (scoped DB pool, storage prefix, telemetry pre-tagged with `plugin = "<name>"`, etc.). Per-request `PluginContext<S, P>` instances are built later by each plugin's `#[derive(PluginCtx)]` extractor.
3. **`on_startup`** — for each plugin in `plugins_generated` order: `plugin.on_startup(&resources).await?`. Fail-fast on error; host aborts.
4. **Job registration** — for each plugin: register `plugin.jobs()` with the queue, supplying the plugin's `PluginResources` so handlers can construct `system_context()` contexts when invoked.
5. **Route registration** — for each plugin: `plugin.routes(resources)` returns a Router (the plugin has internally `.with_state(resources)`-attached); host nests it under the plugin's prefixes (`/rpc/<plugin>`, `/h/<plugin>`, `/p/<plugin>`).
6. **Serve** — server starts accepting traffic.
7. **Graceful shutdown** — stop accepting → drain in-flight → for each plugin in reverse order: `plugin.on_shutdown(&resources).await` (errors logged, don't abort shutdown).

### 11.12 Implementation details still to work out

These are sequenced after this section lands; they don't block the design.

- Exact proc-macro implementation for `plugin_metadata!()`.
- Exact derive + attribute-macro implementation for `#[derive(Repository)]` and `#[impl_repository(...)]`.
- Exact derive implementation for `#[derive(PluginCtx)]` including the `#[repo]` field marker, the generated `FromRequestParts` impl, and the `#[from_request]` hook for non-repo fields.
- Full transitivity of `Has<X>` impls through nested `And` (sealed helper traits).
- `Bucket` derive for object storage — parallel to `Repository`.
- Job system identity model (`system_context::<S>(resources)` helper).
- The buf custom-options plugin needed for `option (platform.requires)` to round-trip through Rust + TS codegen, including synthesizing the plugin's `<PluginName>Ctx<P>` type in handler signatures.
- `connect_rs` interceptor wiring for proto-declared permission enforcement.
- OR-style permission combinators (`Or<A, B>` + `HasAny<X>`).

---

## 12. Frontend Plugin Interface

This section defines the TypeScript/React surface a frontend plugin author writes against. It mirrors §11's structure for the backend, but is meaningfully lighter — TypeScript lacks proc macros, React's component model already handles per-component lifecycle, and most of the contract is "export a fixed shape and `platctl` composes."

### 12.1 Overview & philosophy

A frontend plugin is a pnpm workspace package `@platform/plugin-<name>` that exports a fixed surface from `src/index.ts`. There is no `Plugin` interface to implement — the contract is the named exports.

All generated TypeScript code lives in a separate `@platform/generated` package (§12.2). Plugin source trees contain **no generated files** — analogous to the backend's macro-based approach where nothing generated lives inside `plugins/<name>/`.

The shell app (`platform/frontend/`) composes plugins into one React/TanStack Router application. Composition happens at build time via `platctl`-generated aggregators that import from each plugin's package.

### 12.2 The `@platform/generated` package

All TS codegen lives in one workspace package with subpath exports per plugin:

```
packages/generated/                 # managed by platctl; "do not edit" header
├── package.json                    # subpath exports per plugin (see below)
├── shared/
│   └── proto-types.ts              # cross-plugin proto-generated TS types
└── plugins/
    ├── speakers/
    │   ├── index.ts                # barrel: re-exports rpc, METADATA, Permission
    │   ├── rpc.ts                  # narrow Connect-RPC namespace for this plugin
    │   ├── permissions.ts          # Permission union type
    │   └── metadata.ts             # METADATA constant
    └── events/ ...
```

Subpath exports in `package.json`:

```json
{
  "name": "@platform/generated",
  "exports": {
    "./shared/proto": "./dist/shared/proto-types.js",
    "./speakers":     "./dist/plugins/speakers/index.js",
    "./speakers/rpc": "./dist/plugins/speakers/rpc.js",
    "./events":       "./dist/plugins/events/index.js",
    "./events/rpc":   "./dist/plugins/events/rpc.js"
  }
}
```

Plugin code imports:

```ts
import { rpc, METADATA }   from '@platform/generated/events';
import type { Permission } from '@platform/generated/events';
import type { Speaker }    from '@platform/generated/shared/proto';
```

`platctl sync` regenerates this package from every plugin's `plugin.toml`. `platctl check` validates:
- Every `@platform/generated/<other-plugin>/*` import in a plugin's source is justified by a manifest dep on that other plugin.
- The package's `exports` field matches the plugin set in the source.

**Why a single generated package**: keeps generated content out of plugin source trees (matching backend's macro approach), reduces sync surface (one package to regenerate), and avoids per-plugin `src/generated/` mixing with hand-written code.

**Note**: there is no `@platform/permissions` aggregate package. The source monorepo doesn't know which plugins are enabled in any deployment (§5.5), so deployment-bound aggregates can't live in the source. Each plugin's `Permission` type is locally scoped.

### 12.3 Plugin source tree & public exports

```
plugins/speakers/frontend/
├── package.json                # @platform/plugin-speakers; depends on @platform/generated, @platform/sdk, @platform/design
├── tsconfig.json
├── src/
│   ├── index.ts                # public exports — all hand-written
│   ├── routes/
│   │   ├── index.ts            # exports buildRoutes(parent)
│   │   └── pages/
│   ├── lib/                    # public components (declared in plugin.toml [exposes.components])
│   ├── hooks/                  # plugin-internal hooks
│   └── types.ts
```

No `src/generated/`. The plugin's `src/index.ts`:

```ts
// plugins/speakers/frontend/src/index.ts

export { buildRoutes } from './routes';

// Public components (must match plugin.toml [exposes.components])
export { SpeakerCard }   from './lib/SpeakerCard';
export { SpeakerPicker } from './lib/SpeakerPicker';

// Domain types other plugins may want
export type { Speaker, SpeakerId } from './types';

// No `rpc` export — consumers import from @platform/generated/<this-plugin>/rpc.
// No `permissions` export — same reason.
```

`platctl check` verifies that the named component exports match `plugin.toml`'s `[exposes.components]` declarations.

### 12.4 Routes composition

Each plugin exports a `buildRoutes(parent)` function that takes its mount point and returns a TanStack Router subtree:

```ts
// plugins/speakers/frontend/src/routes/index.ts
import { Route, type AnyRoute } from '@tanstack/react-router';
import { requirePermissions } from '@platform/sdk';
import type { Permission } from '@platform/generated/speakers';
import { SpeakersListPage, SpeakerDetailPage, SpeakerEditPage } from './pages';

export function buildRoutes(parent: AnyRoute) {
  const list = new Route({
    getParentRoute: () => parent,
    path: '/',
    component: SpeakersListPage,
  });

  const detail = new Route({
    getParentRoute: () => parent,
    path: '/$speakerId',
    component: SpeakerDetailPage,
    loader: ({ params }) => /* prefetch via TanStack Query */,
  });

  const edit = new Route({
    getParentRoute: () => parent,
    path: '/$speakerId/edit',
    beforeLoad: ({ context }) => requirePermissions<Permission>(context, ['speakers:write']),
    component: SpeakerEditPage,
  });

  return [list, detail, edit];
}
```

`platctl` generates the shell's composed router:

```ts
// platform/frontend/src/generated/routes.ts — generated
import { rootRoute } from '../router/root';
import { buildRoutes as buildSpeakers } from '@platform/plugin-speakers';
import { buildRoutes as buildEvents   } from '@platform/plugin-events';

const speakersParent = new Route({ getParentRoute: () => rootRoute, path: '/p/speakers' });
speakersParent.addChildren(buildSpeakers(speakersParent));

const eventsParent = new Route({ getParentRoute: () => rootRoute, path: '/p/events' });
eventsParent.addChildren(buildEvents(eventsParent));

export const routeTree = rootRoute.addChildren([speakersParent, eventsParent]);
```

### 12.5 Permissions

Each plugin's typed `Permission` union lives in `@platform/generated/<plugin>`:

```ts
// @platform/generated/speakers/permissions.ts (generated)
export type Permission =
  | 'speakers:read'
  | 'speakers:write'
  | 'speakers:book';
```

`@platform/sdk` hooks are generic over the permission union:

```ts
export function useHasPermission<P extends string>(p: P): boolean;
export function useHasAllPermissions<P extends string>(perms: P[]): boolean;
export function useHasAnyPermission<P extends string>(perms: P[]): boolean;
export function requirePermissions<P extends string>(ctx: RouterContext, perms: P[]): void;
```

In plugin code, the caller parameterizes with its own union (or with an imported dep's union for cross-plugin checks):

```ts
import { useHasPermission } from '@platform/sdk';
import type { Permission as MyPerm } from '@platform/generated/speakers';

const canEdit = useHasPermission<MyPerm>('speakers:write');   // ✓
const typo    = useHasPermission<MyPerm>('spekers:write');     // ✗ TS error
```

For cross-plugin permission checks (rare — typically each plugin guards its own):

```ts
// In plugin events, which declared a dep on speakers:
import type { Permission as SpeakersPerm } from '@platform/generated/speakers';
const canBook = useHasPermission<SpeakersPerm>('speakers:book');
```

`platctl check` enforces that any `@platform/generated/<other-plugin>` import has a corresponding `[dependencies.<other-plugin>]` declaration in the importer's manifest.

Three places permissions are checked, mirroring the backend's three (extractor / repo / RPC):

1. **Route guards** via `requirePermissions` in `beforeLoad`.
2. **Component-level hooks** (`useHasPermission`).
3. **RPC method calls** — surfaced from `option (platform.requires)` in `.proto` (§11.8). v1: server-side rejection only; client-side pre-check is a v2 UX optimization.

The runtime side: the auth context provides `User.permissions: ReadonlySet<string>`. Hooks check set membership. Compile-time safety comes from the typed unions at each call site.

### 12.6 RPC — narrow per-plugin client over a single shell transport

The shell owns the Connect transport. Each plugin gets a generated narrow `rpc` namespace exposing only the methods declared in its manifest (its own services plus cross-plugin methods declared via `[dependencies.<dep>].rpc_methods`).

Manifest declaration:

```toml
# plugins/events/plugin.toml
[dependencies.speakers]
optional     = false
tables       = ["speaker"]
rpc_methods  = ["SpeakerService.GetSpeaker", "SpeakerService.ListSpeakers"]
```

Generated narrow namespace:

```ts
// @platform/generated/events/rpc.ts (generated)
import {
  EventService_GetEvent, EventService_CreateEvent,
  SpeakerService_GetSpeaker, SpeakerService_ListSpeakers,
} from '@platform/generated/shared/proto';

export const rpc = {
  EventService: {
    getEvent:    EventService_GetEvent,
    createEvent: EventService_CreateEvent,
  },
  SpeakerService: {
    // Only the two declared methods. createSpeaker, book, etc. are absent.
    getSpeaker:   SpeakerService_GetSpeaker,
    listSpeakers: SpeakerService_ListSpeakers,
  },
} as const;
```

Plugin code via Connect-Query:

```ts
import { rpc } from '@platform/generated/events';
import { useQuery } from '@connectrpc/connect-query';

function VenueSpeakersList({ venueId }: { venueId: VenueId }) {
  const { data } = useQuery(rpc.SpeakerService.listSpeakers, { venueId });
  // rpc.SpeakerService.createSpeaker is undefined at the type level → compile error
}
```

The shell sets up the Connect transport once. Plugins don't see or construct it — they just call Connect-Query hooks against `rpc.*` method descriptors. The transport handles auth headers, base URL, retries.

This mirrors the backend's repository pattern (§11.5): a single source of truth (shell transport + shared proto schemas), with each plugin's compile-time surface narrowed to its declared usage.

**v1 enforcement** of the cross-plugin import allowlist is via `platctl check` (convention) scanning imports against each plugin's manifest. Per-plugin `tsconfig.json` `paths` allowlists are an option to upgrade to if drift becomes a problem.

### 12.7 Cross-plugin components

See §8 for the full mechanism. FE-specific patterns:

**Required deps**: regular ES imports.

```ts
import { SpeakerPicker } from '@platform/plugin-speakers';
import type { SpeakerId } from '@platform/plugin-speakers';
```

**Optional deps**: typed registry from `@platform/sdk`.

```ts
import { getComponent } from '@platform/sdk';
const VenuePicker = getComponent('venues.VenuePicker'); // typed; `undefined` if disabled
```

The registry is populated by `platctl`-generated code in the shell.

### 12.8 Auth & user context

`@platform/sdk` provides:

```ts
interface User {
  id: UserId;
  email: string;
  displayName: string;
  permissions: ReadonlySet<string>;  // populated from the host's RBAC at login
}

export function useUser(): User | null;
export function useIsAuthenticated(): boolean;
export function useCurrentUser(): User;  // throws/redirects if not authenticated
```

The shell wraps the app in an `AuthProvider` that fetches/refreshes the session and handles login/logout. Plugin code never touches auth state directly — only the hooks.

### 12.9 Design system

`@platform/design` package, built on **Tailwind + Radix UI primitives**:

- **Tailwind** for utility-class styling. One `tailwind.config.ts` at the workspace root applied to all plugin source files. `platctl sync` maintains the `content` paths to include `plugins/*/frontend/src/**/*.{ts,tsx}`.
- **Radix UI** primitives (`@radix-ui/react-*`) for unstyled accessible behavior (Dialog, Dropdown, Tooltip, Popover, etc.).
- **`@platform/design` wrappers** combine the two into the platform's vocabulary: `Button`, `Input`, `Card`, `Stack`, `Grid`, `Modal`, `Form`. Plugins import from here for any visual component; never write raw `<button>` or HTML form elements directly.

```ts
import { Button, Card, Stack } from '@platform/design';

function SpeakerCard({ speaker }: { speaker: Speaker }) {
  return (
    <Card>
      <Stack gap="md">
        <h3 className="text-lg font-semibold">{speaker.name}</h3>
        <Button variant="primary">View details</Button>
      </Stack>
    </Card>
  );
}
```

Tokens (colors, spacing, typography, radii) are Tailwind theme extensions in the workspace config + exposed as CSS variables for runtime theming. Plugins reference tokens through Tailwind class names (`bg-surface-1`, `text-primary`) — never raw color values. Keeps theming centralized.

### 12.10 Host shell composition

`platctl` generates a thin shell entry point with all the providers:

```tsx
// platform/frontend/src/main.tsx (mostly generated)
import { RouterProvider, createRouter } from '@tanstack/react-router';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { TransportProvider } from '@connectrpc/connect-query';
import { createConnectTransport } from '@connectrpc/connect-web';

import { routeTree } from './generated/routes';
import { componentRegistry } from './generated/component-registry';
import { ComponentRegistryProvider, AuthProvider } from '@platform/sdk';

const transport = createConnectTransport({ baseUrl: '/' });
const queryClient = new QueryClient();
const router = createRouter({ routeTree, context: { auth: undefined! } });

function App() {
  return (
    <AuthProvider onReady={(auth) => router.update({ context: { auth } })}>
      <TransportProvider transport={transport}>
        <QueryClientProvider client={queryClient}>
          <ComponentRegistryProvider registry={componentRegistry}>
            <RouterProvider router={router} />
          </ComponentRegistryProvider>
        </QueryClientProvider>
      </TransportProvider>
    </AuthProvider>
  );
}
```

The shell is intentionally tiny — ~50 LoC of bootstrapping. Everything else lives in plugins.

### 12.11 Implementation details still to work out

These don't block the design.

- Exact codegen pipeline for `@platform/generated` (when to regenerate, gitignored vs. committed, hot-reload story in dev).
- pnpm `exports` field templating for many plugins (`platctl sync` writes this).
- TypeScript-side enforcement of the cross-plugin import allowlist via per-plugin `tsconfig.json` `paths` — adopt if convention drift becomes a problem.
- Connect-Query method descriptor codegen consistency in `@platform/generated/shared/proto`.
- `AuthProvider` session refresh strategy (cookies, OAuth flows, etc.) — depends on the auth open question (§13).
- Component registry type shape for required vs. optional dep keys.
- Tailwind config composition: workspace-root vs. per-plugin theme extensions.
- `Form` component shape: react-hook-form integration? TanStack Form? Defer until the first complex form appears.
- Sub-layout patterns: plugin authoring guide should document the "parent route with `Layout` component" pattern.

---

## 13. Open Questions

These are decisions not yet taken. Each will need its own short doc or discussion before implementation.

- **Testing strategy.** Per-plugin unit tests are obvious. Integration tests across plugins? End-to-end browser tests? Contract tests for `.proto` files?
- **Asset handling.** Per-plugin static assets (images, fonts) — bundled with the FE or served separately?
- **Internationalization.** Where do translations live? Per plugin? Shared catalog?
- **Hot reload across plugin boundaries in dev mode.** Needs validation on a toy two-plugin setup before committing.
- **Multi-tenancy.** Does one platform binary serve one organization or many? Single-tenant (each org runs its own deployment) is the leaning default — matches the source/deployment split, keeps the data model simple — but needs to be made explicit. Multi-tenant changes every table (`org_id`), every Postgres role (RLS), and every query.
- **Audit logging.** Political-domain platforms need a verifiable "who did what, when" trail — for incident response, legal discovery, compliance. Decisions: what events are auditable, where stored (separate table per plugin? a host-wide `platform.audit_event`? a separate DB?), retention policy, who can read it, how plugins emit events through `platform-sdk`.
- **Trust model & threat model.** v1 implicitly assumes first-party plugins (all code is trusted, lives in the source monorepo or its submodules). This should be stated explicitly near §1, along with what changes if third-party plugins enter the picture later (capability enforcement at runtime, plugin sandboxing, code review process for external contributions).
- **Worked example & plugin authoring guide.** The doc currently references `docs/plugin-authoring-guide.md` in several places but it doesn't exist yet. A minimal "Hello, plugin" end-to-end (manifest, Rust handler, RPC, FE route, permission check, dev-mode run) would both validate the design and onboard plugin authors.
- **Implementation sequencing / v0 milestone.** Many moving parts (`platctl`, `platform-sdk`, `@platform/sdk`, `@platform/generated`, `@platform/design`, Postgres roles, Connect-RPC interceptors, derive macros). A concrete v0 — one trivial plugin end-to-end with the minimum plumbing — would prove the architecture and prevent scope creep. Worth picking the v0 cut explicitly.

---

## 14. Decision Log

| Date       | Decision                                                            |
|------------|---------------------------------------------------------------------|
| 2026-05-19 | Backend: Rust + Axum. Compile-time plugins via Cargo workspace.     |
| 2026-05-19 | API contract: Connect-RPC (Buf), per-plugin `.proto` files.         |
| 2026-05-19 | Frontend: React + TypeScript + TanStack Router + TanStack Query + Vite. SPA mode. |
| 2026-05-19 | Plugins are dual-packaged (Cargo crate + pnpm package + proto).     |
| 2026-05-19 | Cross-plugin component sharing via ES imports (required) + typed registry (optional). |
| 2026-05-19 | `platctl` composes FE into one Vite project; build artifact is embedded in the Rust binary via `rust-embed`. |
| 2026-05-19 | Deployment artifact: one self-contained Rust binary.                |
| 2026-05-19 | Plugin manifest is `plugin.toml`, TOML format. Single source of truth for inter-plugin metadata. v1 schema in §6.1. |
| 2026-05-19 | Permissions declared in manifest, enforced in code. Audit-friendly without code analysis. |
| 2026-05-19 | Plugin capabilities (`db.read`, `email.send`, etc.) are audit-only in v1. Runtime enforcement deferred. |
| 2026-05-19 | Management CLI named `platctl`. Owns composition, codegen, scaffolding, validation, build, dev. Replaces the "kickstart" concept; kickstart is now just a subset (`compose` + `build`). |
| 2026-05-19 | Sync strategy: `platctl` fully overwrites generated files; uses marker comments to manage co-edited files (`Cargo.toml`, `package.json`, `buf.yaml`); never touches human-only files. |
| 2026-05-19 | `platctl dev` is the one-command dev flow (Vite + cargo + watchers + buf-on-change + sync-on-manifest-change). |
| 2026-05-19 | Plugin directory layout is fixed by convention; `[paths]` block removed from the manifest. Plugins omit directories they don't need. |
| 2026-05-19 | No per-plugin versioning and no dependency version constraints. Single-version monorepo policy; `manifest_schema` is the only versioning concept retained. |
| 2026-05-19 | Cross-repo plugins are integrated via git submodules under `plugins/<name>/`; `platctl` treats them identically to in-tree plugins. |
| 2026-05-19 | Auth, user management, and RBAC live in the host (`platform/`), not as plugins. Plugins consume identity primitives via `platform-sdk`. |
| 2026-05-19 | Repository / deployment split: the source monorepo is the codebase (no `platform.toml`); deployments live in separate directories (their own repos or server dirs) that pin the source revision and declare enabled plugins. `platctl build` in a deployment dir produces that deployment's binary. |
| 2026-05-19 | Source resolution: deployment's `[source]` block points at a git URL+rev or a local path; `platform.lock` records the resolved revision. |
| 2026-05-19 | Dev sandbox: `dev/platform.toml` inside the source monorepo for local development. Not a real deployment — a developer convenience used by `platctl dev`. |
| 2026-05-19 | Provided platform infra (v1): Database, Object Storage, Background Jobs, Email, Config/Secrets, Telemetry. Exposed as typed `PluginContext` handles gated by `[requires.capabilities]`. Cache/search/realtime deferred to v2. |
| 2026-05-19 | Database: PostgreSQL + sqlx with committed `.sqlx/` prepared-query cache. One Postgres schema per plugin; host owns `platform.*` and `meta.*`. |
| 2026-05-19 | Always-apply migrations: every deployment runs the full migration set from its pinned source revision regardless of which plugins are enabled. Enable/disable is purely a code concern; schemas always exist. |
| 2026-05-19 | Cross-plugin FKs and JOINs allowed across both required and optional deps. FKs into optional-dep schemas must be nullable. Cross-plugin writes via SQL allowed; invariants enforced via Postgres triggers when needed. `CASCADE` on cross-plugin FKs forbidden. |
| 2026-05-19 | `[exposes.tables]` declares public R+W tables; private tables (everything else) evolve freely. Consumer plugins declare `[dependencies.<dep>].tables = [...]`. |
| 2026-05-19 | Postgres role enforcement from day one: each plugin runs as `role_<plugin>` with grants computed from manifests by `platctl migrate`. One pool per role. Migration runner uses privileged `platform_migrator` role. |
| 2026-05-19 | Migration ordering: host first; plugin migrations as a topo sort over (within-plugin filename order + explicit `-- @requires <plugin>:<migration>` edges). Manifest plugin deps do NOT infer migration ordering. |
| 2026-05-19 | `meta.migrations` records applied migrations with `BIGSERIAL id`, `plugin`, `migration_name`, `checksum` (SHA-256), `applied_at`. |
| 2026-05-19 | Down migrations optional and discouraged; forward-fix is the recommended discipline. |
| 2026-05-19 | Schema evolution: `[exposes.tables]` is the compatibility surface; breaking changes coordinated in the same source revision across all consumers. v1 enforcement = Layer 1 (`platctl check` static) + Layer 2 (CI integration test with ephemeral Postgres). Layer 3 (snapshot diff with `@breaking-change` annotation) deferred. |
| 2026-05-19 | Authentication: OIDC against Authentik (configurable per deployment). Platform creates server-side sessions in `platform.session` (Postgres); session cookies are HTTP-only, Secure, SameSite=Lax. OIDC tokens stored encrypted in the session for downstream proxying if needed. Tokens-in-localStorage explicitly unsupported. |
| 2026-05-19 | Groups and roles are first-class. `platform.group` = real-world unit (committee, caucus, etc.). `platform.group_role` is defined *within* a group; "Chair of Committee X" and "Chair of Caucus Y" are distinct rows. A user has **at most one role per group**. Roles own the capability-permission set in `platform.role_permission`, referencing strings declared in manifests' `[permissions]`. |
| 2026-05-19 | Groups have no relations among themselves — no hierarchy, no parent/child, no inheritance. Flat membership only. |
| 2026-05-19 | Two-layer authorization: (1) **capability gate** via manifest-declared permission strings, enforced at handler / repo method via `Has<X>` bounds — user must hold the capability in *some* group; (2) **resource-scoped filter** via `platform.user_can_access(...)` joined into every repo query — resolves ownership, group-role grants, explicit shares, and public visibility. Inaccessible rows don't appear in results. |
| 2026-05-19 | Resource ownership recorded in `platform.resource_principal`; exactly one of `owner_user_id` or `owner_group_id`. **Ownership kind is per-resource-instance**, chosen by the plugin at creation time — some resource types are conceptually group-owned (committee meetings), others user-owned (personal documents, vote ballots cast). Owners have implicit full access regardless of role permissions. |
| 2026-05-19 | Sharing v1: only the current owner can share a resource. `platform.resource_share` rows grant a specific permission to a user, a group, or `principal_kind = 'public'` (platform-wide visibility, opt-in per resource). Finer-grained "who can share what" rules deferred. |
| 2026-05-19 | `resource_kind` strings in `platform.resource_principal` and `platform.resource_share` use the `<plugin>:<table>` format (e.g. `speakers:speaker`, `events:meeting`). |
| 2026-05-19 | Frontend `User` carries `memberships[]` with each group's role and capability-permission set. `useHasPermission(p)` returns true if any membership grants `p`. Per-resource access decisions come from server-supplied flags on resource payloads (e.g. `viewerCanEdit`); the client does not recompute access locally. |
| 2026-05-19 | Backend plugin interface: `Plugin` trait — object-safe, async, `metadata()` + `routes()` required, `on_startup`/`on_shutdown`/`jobs()` defaulted no-op. v1 schema in §11. |
| 2026-05-19 | Plugin convention: every plugin exposes `pub fn new(...) -> Self`; constructor may take host-injected args. `platctl`-generated glue calls it. |
| 2026-05-19 | Plugin metadata produced by `platform_sdk::plugin_metadata!()` proc-macro reading `plugin.toml` at compile time. No source-tree codegen for metadata. |
| 2026-05-19 | `platctl check` verifies every plugin crate invokes `plugin_metadata!()` exactly once. |
| 2026-05-19 | Permission system: macro-generated zero-sized marker types per declared permission. Undeclared permissions are compile errors. `permissions!()` macro combines markers via `And<A, B>`. OR combinators deferred. |
| 2026-05-19 | `PluginContext<S, P>` is generic in `platform-sdk` over plugin-defined state `S` and permission witness `P`; `Clone` (resources `Arc`-wrapped). Plugin authors define a per-plugin type alias `<PluginName>Ctx<P>` and the `#[derive(PluginCtx)]` extractor verifies `P` while assembling state. |
| 2026-05-19 | Repository pattern enforced by `platform-sdk`: no public `db()` accessor on `PluginContext`; raw `PgPool` not in public API. Data access only via `#[derive(Repository)]` types within `#[impl_repository(...)]` blocks. Methods gated by `Has<X>` bounds. Same pattern (`Bucket`) for object storage. |
| 2026-05-19 | Capability gating runtime-only in v1: undeclared capability handles return `Err(PluginError::CapabilityNotDeclared(...))`. Compile-time gating deferred. |
| 2026-05-19 | Connect-RPC routing: auto-generated via `connect_rs::router_for(service)`. |
| 2026-05-19 | RPC method permissions declared in `.proto` via `option (platform.requires) = "..."`. Enforced server-side by host interceptor; codegen produces `<PluginName>Ctx<P>` handler signatures (the per-plugin type alias from §11.6); surfaced in TS client; `platctl check` validates strings against manifest `[permissions]`. |
| 2026-05-19 | `Plugin::routes` returns an unprefixed `Router`; host nests under `/rpc/<plugin>` / `/h/<plugin>` based on metadata prefixes. |
| 2026-05-19 | Background jobs registered via `Plugin::jobs() -> Vec<JobHandler>`; job names namespaced `<plugin>.<job_name>`; jobs invoke permission-gated repos via a `system_context()` helper (mechanism TBD). |
| 2026-05-19 | `PluginResources` (raw, pre-scoped DB pool, storage, jobs, email, etc.) is what `Plugin::routes` and lifecycle hooks receive. `PluginContext<S, P>` becomes a generic struct in `platform-sdk` parameterized by plugin-defined per-request state `S` (typically a bundle of typed repositories) and permission witness `P`. Each plugin defines its own `<PluginName>Ctx<P> = PluginContext<<PluginName>State<P>, P>` type alias. |
| 2026-05-19 | `#[derive(PluginCtx)]` macro generates each plugin's per-request extractor, performing permission verification, repository instantiation (with the request's user), and context assembly. Replaces `SpeakerRepo::from_ctx(&ctx)` in handler bodies; handlers access repos directly as `ctx.state.<repo>`. |
| 2026-05-19 | Frontend plugin contract: plugin is a pnpm workspace package (`@platform/plugin-<name>`) exporting a fixed surface from `src/index.ts` (`buildRoutes`, public components, domain types). No `Plugin` interface to implement; composition is via named exports + `platctl`-generated shell aggregators. |
| 2026-05-19 | All TypeScript codegen lives in a single workspace package `@platform/generated` with subpath exports per plugin (`@platform/generated/<plugin>`, `@platform/generated/<plugin>/rpc`, `@platform/generated/shared/proto`). Plugin source trees contain zero generated files — mirroring the backend's macro-based approach. No deployment-bound aggregate packages (e.g. no `@platform/permissions`) since enablement isn't bound to the source repo. |
| 2026-05-19 | Routes composition: each plugin exports `buildRoutes(parent: AnyRoute)` returning a TanStack Router subtree; `platctl` generates the shell's composed route tree from each plugin's `buildRoutes` plus the plugin's `route_prefix` from `plugin.toml`. |
| 2026-05-19 | Permissions on the frontend: per-plugin `Permission` union from `@platform/generated/<plugin>`; `@platform/sdk` hooks (`useHasPermission`, `requirePermissions`, etc.) are generic over the union; cross-plugin permission imports require a declared manifest dep, enforced by `platctl check`. |
| 2026-05-19 | RPC on the frontend: shell owns the single Connect transport; each plugin gets a narrow `rpc` namespace generated in `@platform/generated/<plugin>/rpc` containing only methods declared in its manifest (own services + `[dependencies.<dep>].rpc_methods`). Mirrors the backend's repository pattern: single source of truth, per-consumer narrowed compile-time surface. v1 enforcement via `platctl check`; tsconfig `paths` allowlist deferred. |
| 2026-05-19 | Design system: Tailwind + Radix UI primitives wrapped in `@platform/design` (Button, Card, Stack, Modal, Form, etc.). One workspace-root `tailwind.config.ts`; tokens via Tailwind theme + CSS variables. Plugins never write raw `<button>` or HTML form elements. |
| 2026-05-19 | Host shell composition: thin `main.tsx` (~50 LoC) wrapping `AuthProvider`, `TransportProvider`, `QueryClientProvider`, `ComponentRegistryProvider`, `RouterProvider`. Routes + component registry imported from `platctl`-generated aggregators in `src/generated/`. |
