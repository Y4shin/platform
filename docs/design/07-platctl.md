# 7. The Management Tool (`junius`)

A single Rust CLI that owns the entire plugin lifecycle: composition, codegen, scaffolding, validation, build, dev mode, and inspection. The "kickstart" concept from earlier discussions is just the `compose` + `build` subset of this tool. One binary, one entry point, everything plugin-related goes through it.

## 7.1 Design principle

Manifests are declarative intent. **`junius sync` brings the project files into alignment with the manifests.** Idempotent, runnable anytime, no-ops if everything already matches. CI runs `junius check` (`sync --dry-run`) and fails on drift.

Example: a plugin author edits `plugin.toml` to add a new inter-plugin proto dependency. They run `junius sync`. The tool updates `buf.yaml`, the plugin's `Cargo.toml` and `package.json`, runs `buf generate`, and regenerates the composed glue code. One manifest edit → coherent state across five files.

## 7.2 Sync strategy

Files fall into three categories:

| Category | Edited by | Sync behavior |
|---|---|---|
| **Fully generated** (`plugins.rs`, `plugins.generated.ts`, the typed component registry, the composed router) | Never by humans | `junius` overwrites unconditionally. Header comment marks them as generated. |
| **Co-managed** (`Cargo.toml`, `package.json`, `buf.yaml`) | Plugin authors **and** `junius` | `junius` uses marker comments and only edits inside markers. Authors edit freely outside. |
| **Human-only** (plugin source code, `.proto` files, `plugin.toml`, `platform.toml`) | Authors only | `junius` reads but never writes. |

Marker convention for co-managed files:

```toml
# Cargo.toml
[dependencies]
serde = "1"

# >>> junius managed start
events-plugin   = { path = "../events" }
speakers-plugin = { path = "../speakers" }
# <<< junius managed end

[dev-dependencies]
# ...
```

If a human edits between markers, `junius sync` overwrites their changes (and `junius check` reports the drift). Outside markers, `junius` leaves the file alone.

## 7.3 v1 commands (essential)

| Command | Purpose |
|---|---|
| `junius sync` | Bring all derived files in line with manifests. `--plugin <name>` to scope. `--dry-run` to preview. |
| `junius check` | `sync --dry-run` + manifest validation + dep graph validation. CI entry point. |
| `junius new plugin <name>` | Scaffold a plugin (manifest, Cargo crate, FE package, proto skeleton, register stub). |
| `junius new component <plugin> <name>` | Add a component file, register under `[exposes.components]`, run sync. |
| `junius new rpc <plugin> <service>` | Add a service to the plugin's `.proto`, scaffold Rust handler + TS client wiring. |
| `junius new migration <plugin> <name>` | Create up/down SQL pair in the plugin's migrations dir. |
| `junius new permission <plugin> <perm>` | Add entry under `[permissions]`. |
| `junius plugin enable <name>` | Add to `platform.toml`, run sync. Refuses if required deps missing. |
| `junius plugin disable <name>` | Remove from `platform.toml`, run sync. Refuses if depended upon. |
| `junius plugin list` | Enabled / disabled / available. |
| `junius plugin info <name>` | Manifest summary + resolved deps, mount points, exposed components, permissions, capabilities. |
| `junius dev` | The one-command dev flow: Vite + `cargo run` + file watchers + `buf generate` on proto change + `sync` on manifest change. |
| `junius build` | Full production build → single self-contained Rust binary. |

## 7.4 v2 commands (introspection, deferred but planned)

| Command | Purpose |
|---|---|
| `junius deps tree` | Plugin dep graph. |
| `junius deps why <plugin>` | Explain why a plugin is enabled (transitively). |
| `junius deps unused` | Declared deps with no actual import. |
| `junius deps missing` | Imports without a corresponding manifest declaration. |
| `junius routes` | All composed FE routes. |
| `junius rpc list` | All RPC services + methods. |
| `junius registry` | The generated component registry, per consumer. |
| `junius permissions` | All permissions across enabled plugins. |
| `junius capabilities <plugin>` | What a plugin declared (and eventually, what it actually uses). |
| `junius migrate up/down/status` | Aggregated platform migrations. `--plugin <name>` for scoped. |
| `junius breaking-check` | `buf breaking` across all plugin protos vs. main. |
| `junius diff-manifest` | Per-plugin manifest diff vs. main, useful in PR review. |

## 7.5 Later (when scale demands it)

- **`junius docs generate`** — static documentation site listing every plugin's routes, RPC services, components, permissions, and capabilities. High value for compliance audits.
- **`junius release <plugin> --version X`** — atomic version bumps across manifest + Cargo + package.json + changelog. Only needed with independent plugin versioning.
- **`junius lint`** — opinionated authoring suggestions.
- **`junius i18n extract / check`** — translation pipeline.
- **Capability enforcement at runtime** — only if/when third-party plugins ever happen.

## 7.6 Design rules

- **Wrap, don't replace.** `junius` calls `buf`, `cargo`, `pnpm`, `sqlx`, etc. — it doesn't reimplement them.
- **Noun-verb subcommands.** Scales to 50+ commands cleanly (`plugin enable`, `deps tree`, `new component`).
- **All mutating commands respect `--dry-run`.** CI uses this.
- **Scoped or global.** `junius sync` does everything; `--plugin <name>` scopes to one plugin. Both produce the same end state.
- **Single Rust binary**, lives in the same workspace as everything else. No external install step for contributors.
- **Plain text output by default, `--format json` for scripting.**

## 7.7 Build artifact

`junius build` produces one self-contained Rust binary:

- Axum server
- Connect-RPC services at `/rpc/<plugin>/*`
- Plugin HTTP routes at `/h/<plugin>/*`
- Plugin SPA routes at `/p/<plugin>/*` (served as part of the embedded FE)
- Embedded FE bundle via `rust-embed` (fallthrough to `index.html` + assets)

Drop the binary on a host, point it at a database, run.

## 7.8 Why one composed FE project instead of per-plugin bundles

- Single JS runtime, single React tree, single style cascade — no iframe/microfrontend overhead.
- Shared dependencies (React, TanStack, Connect) deduplicated by Vite.
- Cross-plugin imports are real ES imports, not runtime federation.
- One bundle is dramatically easier to cache, version, and embed than many.
