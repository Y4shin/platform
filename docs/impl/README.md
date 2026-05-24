# Junius — Implementation Plan

This folder is the build order for the design in [../design/](../design/). It turns 14 design docs into a sequence of milestones a solo developer can land one at a time, in order, each producing something runnable.

Start with [00-approach.md](00-approach.md) — it explains how the plan is structured, how library defaults work (they require your confirmation before each milestone, not just silent adoption), and how to keep the plan internally consistent when a default changes.

## Milestone index

Status legend: ✅ implemented · 🚧 planned · ⏳ in progress.

| # | Status | Milestone | What lands | Verify by |
|---|---|---|---|---|
| **M00** | ✅ | [Workspace skeleton](01-M00-workspace-skeleton.md) | Cargo + pnpm + buf workspaces, tooling configs, empty crates/packages | `cargo build && pnpm install` succeed; CI green |
| **M01** | ✅ | [`junius` bootstrap](02-M01-junius-bootstrap.md) | CLI shell, manifest parser, `junius check` on a fixture | `junius check --manifest fixture.toml` rejects bad input |
| **M02** | ✅ | [Platform core](03-M02-platform-core.md) | `Plugin` trait, `plugin_metadata!`, Axum binary booting with zero plugins | `cargo run -p platform` serves 200 on `/` |
| **M03** | ✅ | [First plugin](04-M03-first-plugin.md) | Hello plugin (Rust only) + `junius sync` wires it in | `curl /h/hello/ping` returns `200 pong` |
| **M04** | ✅ | [Frontend shell](05-M04-frontend-shell.md) | React/TanStack shell, `buildRoutes`, Vite, `rust-embed`, `junius dev` | `junius dev` renders `/p/hello` in a browser |
| **M05** | ✅ | [Connect-RPC](06-M05-connect-rpc.md) | proto + buf + hand-rolled Connect unary server + connect-query end-to-end | `/p/hello` shows server response via `useQuery` |
| **M06** | ✅ | [DB + auth](07-M06-db-auth.md) | Postgres, host migrations, per-plugin roles, OIDC, sessions, typed plugin config/secrets | Log in via Authentik; `GET /api/me` returns user |
| **M07** | 🚧 | [Repos + permissions](08-M07-repos-permissions.md) | `#[derive(Repository)]`, `Has<X>`, `#[derive(PluginCtx)]`, RPC `requires` enforcement | Compile-fail test rejects writes without the permission |
| **M08** | 🚧 | [Resource access](09-M08-resource-access.md) | `resource_principal`/`resource_share` recording, `viewerCanX` flags | User A's private note invisible to B until shared |
| **M09** | 🚧 | [Cross-plugin](10-M09-cross-plugin.md) | Required + optional inter-plugin deps; component registry; narrow RPC namespaces | Disabling `widgets` degrades the page gracefully |
| **M10** | 🚧 | [Infra capabilities](11-M10-infra-capabilities.md) | Jobs (apalis), Storage (S3), Email (lettre), Telemetry (OTel) | Job dispatched from RPC runs and sends an email |
| **M11** | 🚧 | [Deployment workflow](12-M11-deployment-workflow.md) | `[source]` resolution, `platform.lock`, example deployment dir, `develop`-feature CLI gating | Build a binary from outside the source repo |
| **M12** | 🚧 | [Hardening](13-M12-hardening.md) | All `junius check` rules; CI schema test; breaking-change detection | CI fails on every deliberate violation |
| **M13** | 🚧 | [Speakers plugin](14-M13-speakers-plugin.md) | First real domain plugin, end-to-end | Speakers CRUD works; `events` plugin reuses `SpeakerPicker` |

**v0 cut** = end of M05. From there onward, every milestone adds depth (persistence, permissions, ownership, cross-plugin composition, etc.) rather than new top-level capability.

## Other docs in this folder

- [00-approach.md](00-approach.md) — sequencing principles, the "defaults are not silent" rule, and the consolidated library-defaults table.
- [15-open-questions-resolution.md](15-open-questions-resolution.md) — which entries from [../design/13-open-questions.md](../design/13-open-questions.md) must be resolved before which milestone, with a recommended direction for each (still requires your confirmation).

## How to use this plan

1. Before starting each milestone, read its doc end-to-end.
2. Walk through the **"Library choices — confirm before starting"** block; for each row, confirm the default or pick an alternative. If you change a default, update every later milestone doc listed in that row's "downstream" cell so the plan stays self-consistent.
3. Implement against the milestone's **Scope (in)** list.
4. Run the milestone's **Verify** step before claiming it's done.
5. Move to the next milestone.

The plan is meant to evolve. When reality disagrees with a milestone (a library doesn't fit, an open question gets a different answer, scope shifts), edit the affected milestone docs in place. The plan reflects current intent, not a frozen historical record.
