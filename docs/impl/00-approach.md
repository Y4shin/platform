# 0. Approach

How this plan is structured, how to use it, and the working rule for library defaults.

## 0.1 Sequencing principles

The design ([../design/](../design/)) describes a target system. This plan turns that into an order to build it in. Three rules shape the sequencing:

1. **Scaffolding before code, tools before consumers.** M00 lays down the workspace structure with no source code. M01 builds `junius` first because every later milestone uses it. M02 builds the platform host with zero plugins registered. Only then does M03 introduce the first plugin. This avoids chicken-and-egg situations where a milestone's verification step depends on infrastructure that doesn't exist yet.
2. **Each milestone must end with a runnable artifact and a concrete verification step.** No milestone is "completed" purely on the basis of code written — every doc ends with a `Verify` block specifying the curl/browser/test command that proves the milestone works end-to-end.
3. **Capability before coverage.** A new capability is introduced in a single plugin (usually `hello`) before being applied broadly. Permissions land in M07 against the hello plugin; resource ownership in M08 against the hello plugin's `note` table; cross-plugin composition in M09 with two new toy plugins. The first **real** domain plugin (Speakers) doesn't arrive until M13, when every supporting capability is in place.

## 0.2 Milestone doc template

Every milestone doc follows the same shape:

- **Goal** — one sentence outcome.
- **Why now** — what this unblocks; what this depends on.
- **Scope (in)** — concrete deliverables.
- **Scope (out)** — what's explicitly pushed to a later milestone, so the doc resists scope creep.
- **Library choices — confirm with user before starting** — every open library/approach decision relevant to this milestone, with a proposed default, one-line rationale, and the list of downstream milestones that reference the same default (so you know what to update if you pick differently).
- **Open questions resolved** — entries from [../design/13-open-questions.md](../design/13-open-questions.md) that must be decided before starting.
- **Verification** — the concrete end-to-end check.

## 0.3 The "defaults are not silent" rule

The plan proposes a default library or approach for every open choice the design left unresolved. **Defaults are not decisions.** Before starting a milestone, walk through its "Library choices" block with the user. For each row:

- Confirm the default → no action needed.
- Pick an alternative → update this milestone's doc to reflect the choice. Then update every later milestone listed in the "downstream" cell so the plan stays internally consistent.

This applies even when a default looks obvious. "Use sqlx" looks obvious because the design names it, but it's still on the table at M06 — and if you pick differently, M07/M08/M09/M12/M13 all need updating.

The consolidated defaults index lives in §0.5 of this doc. The same defaults are repeated inline in each milestone they touch, because the milestone docs are meant to be readable standalone.

## 0.4 Updating the plan as you go

The plan is a living document. When you learn something during implementation that invalidates a later milestone's assumptions, edit that milestone's doc in place. Common cases:

- **A default changes** — update the milestone where it was introduced and every milestone in the "downstream" cell. Update §0.5 below.
- **An open question gets a different answer** — update [15-open-questions-resolution.md](15-open-questions-resolution.md) and any milestone that depended on the prior assumption.
- **Scope shifts** — move items between milestones; never accumulate "TODO" notes inside a milestone, instead push them to a later milestone or strike them.
- **A milestone splits or merges** — renumber the docs and the index in [README.md](README.md).

The plan reflects current intent. It is not a historical record.

## 0.5 Consolidated library defaults

Every default below is **proposed**, not decided. Confirm before starting the milestone that introduces it.

| Concern | Proposed default | Introduced | Also used by | Revisit if |
|---|---|---|---|---|
| Rust toolchain | Pin to current stable in `rust-toolchain.toml` | M00 | All Rust milestones | We need a nightly-only feature |
| FE linter/formatter | `biome` | M00 | All FE milestones | Major tooling shift |
| CLI framework | `clap` derive | M01 | junius only | We need plugin-style CLI extensibility |
| TOML parser | `toml` crate via `serde` | M01 | M02, M07 | — |
| CLI snapshot tests | `insta` | M01 | junius only | — |
| HTTP framework | `axum` | M02 | All backend milestones | Locked by design |
| Async runtime | `tokio` multi-thread, `full` features | M02 | All backend milestones | — |
| Async trait machinery | `async-trait` | M02 | All backend milestones | Stable object-safe async-fn-in-trait lands |
| Proc-macro deps | `syn 2 + quote + proc-macro2` | M02 | M07 | — |
| Error types | `thiserror` (SDK) / `anyhow` (plugins) | M02 | All backend milestones | — |
| In-process HTTP test client | `tower::ServiceExt::oneshot` + `reqwest` for full-binary tests | M03 | All backend milestones | — |
| Embedder | `rust-embed` | M04 | M11 (deployment artifact) | Locked by design |
| React | Latest stable | M04 | All FE milestones | — |
| TanStack Router + Query | Latest stable | M04 | All FE milestones | Locked by design |
| Dev-mode process supervisor | Custom (`tokio::process` + `select!`) inside `junius` | M04 | — | Becomes more than ~100 LoC |
| FE testing | `vitest` + `@testing-library/react` | M04 | All FE milestones | — |
| E2E testing | `playwright` against `junius dev` | M04 | All FE milestones | — |
| Connect (server) | `connect-rs` | M05 | M07, M09, M13 | Locked by design |
| Connect (client) | `@connectrpc/connect-web` + `@connectrpc/connect-query` | M05 | All FE milestones from M05 | Locked by design |
| TS proto codegen | `@bufbuild/protoc-gen-es` + `@connectrpc/protoc-gen-connect-es` | M05 | All FE milestones from M05 | — |
| Buf vendoring | `junius` downloads a pinned `bufbuild/buf` release | M05 | M12 (`buf breaking` in CI) | Team prefers system install |
| DB driver | `sqlx` with offline `.sqlx/` cache committed | M06 | M07, M08, M09, M12, M13 | Locked by design |
| Migration runner | Custom inside `junius` (parses `-- @requires`, topo-sorts) | M06 | M11, M12 | Topo-sort needs become exotic |
| Cookies middleware | `tower-cookies` | M06 | — | tower retires it |
| Session token encryption | `aes-gcm` (RustCrypto) | M06 | — | KMS-managed keys preferred |
| OIDC client | `openidconnect` crate | M06 | — | Authentik drops standard OIDC flows we need |
| JWT (if needed directly) | `jsonwebtoken` | M06 | — | — |
| Containers (tests) | `testcontainers-modules` with `postgres` feature | M06 | M12 | — |
| Compile-fail tests | `trybuild` | M07 | — | — |
| Compile-time query check | `cargo sqlx prepare --check` in CI | M07 | M12 | — |
| Job queue | `apalis` with Postgres backend | M10 | Subsequent plugins | Cron-style scheduling needed |
| Object storage SDK | `aws-sdk-s3` | M10 | Subsequent plugins | Compile time painful → `rust-s3` |
| Email | `lettre` + per-deployment `Transport` impl | M10 | Subsequent plugins | Need richer templating |
| Email prod transport | Resend | M10 | Subsequent plugins | Cost or compliance constraints |
| Telemetry | `tracing` + `tracing-subscriber` + `tracing-opentelemetry` + `opentelemetry-otlp` | M10 | All later milestones | — |
| Dev mail capture | `mailpit` in docker-compose | M10 | — | — |
| Git library (junius) | `gix` (pure-Rust, no system git) | M11 | — | `gix` lacks an auth scheme we need |
| Source content hash | `blake3` | M11 | — | — |
| SQL parser (junius check) | `sqlparser-rs` (PostgreSQL dialect) | M12 | — | — |
| FE forms | `react-hook-form` + `zod` | M13 | Subsequent plugins | TanStack Form catches up |
| Dev stack (compose) | `postgres`, `authentik`, `minio`, `mailpit` | M06 (pg+authentik) extended in M10 (minio+mailpit) | — | Devcontainers preferred |
| CI runner | GitHub Actions | M00 | All later milestones | — |

## 0.6 What this plan does **not** cover

Out of scope, deliberately:

- **Production operations** — backup strategy, monitoring runbooks, on-call setup, incident response. These belong to a future ops doc, not the implementation plan.
- **Specific deployment infrastructure** — Kubernetes manifests, Terraform, container images. The plan ends at "single binary built". How a deployment runs the binary is the deployment's problem.
- **Authentik deployment** — the plan assumes someone has run an Authentik instance the platform can OIDC against. Running Authentik is out of scope; integrating with it is in scope.
- **Performance work** — no load testing, no benchmarks, no optimization passes. Correctness first; performance work follows real usage data.
- **UX design** — the design system in M04/M13 covers the component shape, not the visual design. Real UX work comes after the platform exists.
