# Junius

A plugin-driven monolith for political work. The **host** (`platform/`) provides auth, user
management, and shared infrastructure; each domain use case ships as a **plugin** under
`plugins/` that bundles its backend (Rust), frontend (React/TS), and API contract (proto)
together. Plugins talk to the host only through the `junius-sdk` crate — never by reaching into
host internals.

Full design lives in [docs/design/](docs/design/) (numbered 01–14); milestone implementation
notes in [docs/impl/](docs/impl/). Read those before large changes.

## Toolchain & commands

The whole toolchain (Rust, Node 24, pnpm, biome, buf, `task`) comes from the Nix flake. Get a
shell with `nix develop`, or `direnv allow` once so it loads on `cd`. Everything below assumes
you're inside that shell.

Day-to-day work goes through `task` (see [Taskfile.yml](Taskfile.yml); `task` alone lists all):

| Command | Does |
| --- | --- |
| `task ci` | Full local gate: fmt-check, lint, Rust+JS tests, buf, i18n, E2E (E2E auto-skips without Docker) |
| `task fmt` | Apply all formatters (`cargo fmt`, biome, `buf format`) |
| `task lint` | clippy (`-D warnings`), biome, buf lint, `junius check` |
| `task test` / `test:rust` / `test:js` / `test:e2e` | Test suites; `test:rust:unit` is the DB-less subset |
| `task dev` | Run the host (`juniusd`) + Vite dev server |
| `task infra:up` / `infra:down` | Start/stop the local backing stack (Postgres, Authentik, RabbitMQ, MinIO, mailpit, LGTM) |
| `task migrate` / `sync` / `build` | Wrap the `junius` CLI against `dev/platform.toml` |

Integration and E2E tests need Docker. **Before committing, run the gate that covers your
change** (`task lint` + the relevant `task test:*`, or `task ci` for the lot).

## Where things live

- `platform/` — the host (NOT a plugin). Axum server, the `Plugin` trait + `PluginContext`,
  capability handles, DB pool, config, telemetry, and the FE shell (`platform/frontend/`,
  `@junius/shell`). **Auth, users, RBAC, sessions stay here** — they're dependencies of the
  plugin loader itself.
- `plugins/` — domain plugins (`admin`, `events`). Each self-contained: `src/`, `proto/`,
  `migrations/`, `frontend/`, `plugin.toml`.
- `crates/` — shared Rust libs. `junius-sdk` is the plugin↔host API surface (+ `junius-sdk-macros`).
  Plugins depend on `junius-sdk`, never on `platform/`.
- `packages/` — shared TS libs (`@junius/design`, the generated client, the SDK, e2e helpers).
- `proto/` — platform-level shared protos (`platform/v1`, `user/v1`). Plugin protos live in each
  plugin's `proto/`. `buf` spans the whole tree.
- `tools/junius/` — the management CLI (build/dev/migrate/sync/check). Run it via `cargo run -p junius -- <cmd>`.
- `docs/` — `design/` (architecture), `impl/` (per-milestone notes), `prd/` (in-flight planning).

Conventions live in [.claude/rules/](.claude/rules/): the git workflow and testing/quality
strategy always apply; per-language rules (Rust, frontend, proto, nix) are path-scoped and load
only when you touch matching files.

## Always

- **English** for code, comments, docs, and commits.
- **Conventional Commits** with a scope, matching history: `feat(ui): …`, `ci(buf): …`,
  `docs(prd): …`, `build(nix): …`. Branch from `main`.
- **Never hand-edit generated code** — anything under a `generated/` directory (proto/RPC
  output, compiled Lingui catalogs) is produced by `buf generate` / `lingui compile`.
- Keep the **plugin/host boundary** intact: new plugin-facing API goes through `junius-sdk`;
  host internals do not leak to plugins.

## Planning workflow

Substantial work is specced before it's built. The PRD/epic/slice lifecycle lives in
`.claude/skills/` (invoke as `/create-feature-prd`, `/create-capability-prd`, `/analyse-issue`,
`/implement-issue`, …). Committed artifacts live under `docs/prd/<slug>/`; see
`.claude/skills/*/SKILL.md` for the flow.
