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
| **M07** | ✅ | [Repos + permissions](08-M07-repos-permissions.md) | `#[repository]`, `Has<X>`, `#[derive(PluginCtx)]`, RPC `requires` enforcement | Compile-fail test rejects writes without the permission |
| **M08** | ✅ | [Resource access](09-M08-resource-access.md) | `resource_principal`/`resource_share` recording, `viewerCanX` flags | User A's private note invisible to B until shared |
| **M09** | ✅ | [Cross-plugin](10-M09-cross-plugin.md) | Required + optional inter-plugin deps; component registry; narrow RPC namespaces | Disabling `widgets` degrades the page gracefully |
| **M10** | ✅ | [Infra capabilities](11-M10-infra-capabilities.md) | Jobs (RabbitMQ), Storage (S3-compatible), Email (lettre), Telemetry (OTel→LGTM); runtime capability gating | Job dispatched from RPC runs and sends an email |
| **M11** | ✅ | [Deployment workflow](12-M11-deployment-workflow.md) | `[source]` resolution (git/path), `platform.lock`, source cache + `cache prune`, `file:` secrets, `plugin enable/disable`, example deployment dir, `develop`-feature CLI gating | Build a binary from outside the source repo |
| **M12** | ✅ | [Hardening](13-M12-hardening.md) | Full `junius check` ruleset (private-table access, exposed-table breaking changes, FE export/manifest match); four-job CI with Postgres integration + `buf breaking` + `sqlx prepare --check` | CI fails on every deliberate violation |
| **M13** | ✅ | [Events plugin](14-M13-events-plugin.md) | First real domain plugin, end-to-end | Events CRUD with user/group ownership, public/private visibility, sign-up invite pages, and revocable iCalendar export/feeds |
| **M14** | ✅ | [Internationalization](16-M14-internationalization.md) | Host-side locale storage, typed-codegen backend localizer, FE locale seam + Lingui v5, `junius i18n check` + CI gate, full events plugin retrofit (BE email job + 51 wrapped FE strings, EN + DE). | `task ci:i18n` green; events job test renders the signup email subject + body in EN + DE; the locale switcher round-trips event-list strings live |
| **M15** | ✅ | [Typed RPC handlers](17-M15-rpc-service-macro.md) | `#[rpc_service]` macro + proto-derived witnesses (single-source RPC permissions); `junius check` enforcement; `junius rpc scaffold` stub codemod | A handler restates no permissions; `junius check` fails an unguarded service; scaffold fills a missing method with `todo!()` |
| **M16** | ✅ | [Authoring ergonomics](18-M16-authoring-ergonomics.md) | The four M13 dogfooding follow-ups: `junius sync` auto-wiring (buf + host-FE dep), `Authz::forget_resource`, `User::has_permission_in_group`, blessed public-handler / plugin-nav affordances | A new proto plugin needs no manual buf/pnpm steps; delete leaves no orphaned ACL rows; `events` drops its local workarounds |
| **M17** | ✅ | [E2E testing](19-M17-e2e-testing.md) | Per-plugin Playwright specs (`plugins/*/frontend/e2e/**`) auto-discovered by `task test:e2e`; `@junius/e2e` fixtures with session-seed login (no Authentik UI); ephemeral testcontainers stack; a sixth CI job | Drop a plugin spec → it runs; `task test:e2e` provisions + logs in unattended; CI runs E2E green/red |
| **M18** | 🚧 | [Group & Role Provisioning](20-M18-group-role-provisioning.md) | How to create groups and roles and how to provision them to users | Create groups/roles via config and verify in database & e2e tests for frontend group/role creation (with database verification) |
| **M19** | 🚧 | [Core platform UIs](21-M19-platform-ui.md) | Core host surfaces: real `/` dashboard (greeting + zero-perms empty state), user menu + logout, `/me` profile, 403/500 pages — plus operator pages (audit log, jobs, plugin inventory, health) extending the M18 `admin` plugin. **Nav redesign + tile grid moved to M20.** | Fresh user lands on a dashboard; signs out from the header; `/me` shows groups/roles; bob hitting `/p/admin` lands on 403; admin sees every mutation in `/p/admin/audit` |
| **M20** | 🚧 | [App navigation](22-M20-app-navigation.md) | Plugins declare `[[plugin.apps]]` (1..N apps each, optional sub-nav); `junius sync` emits `plugin-apps.ts`; host frontend grows a left-rail picker (⌘K-openable) + sub-nav rail with current-location highlighting; M19's dashboard tiles re-source from `APPS` | Picker opens, lists permission-filtered apps grouped by section; navigating into Events shows Overview/Calendar/Invites/Feeds sub-nav with active item bar-highlighted |
| **M21** | 🚧 | [Documentation site](23-M21-documentation.md) | `mdbook` wraps the existing `docs/` tree (design + impl + plugin guide) and fills in the missing surfaces: getting-started, concepts (permissions, groups, jobs, i18n, apps), auto-generated CLI reference, `plugin.toml` reference, HTTP API, glossary, troubleshooting, ops, contributing, changelog; CI link-check + CLI-drift checks | `task docs:serve` opens a navigable, searchable book; broken links + CLI-help drift fail CI; new contributor goes clone → logged-in app from the quickstart alone |

**v0 cut** = end of M05. From there onward, every milestone adds depth (persistence, permissions, ownership, cross-plugin composition, etc.) rather than new top-level capability.

**★ Top priority:** M00–M13 ship **English-only**; **M14 (Internationalization) is the prioritized next milestone** — the first thing tackled after M13. i18n is no longer an open-ended "post-v1" punt.

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
