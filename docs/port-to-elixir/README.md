# Porting Junius to Elixir/Phoenix — Analysis & Re-implementation Strategy

This report analyzes the current **Junius** project (a Rust + React plugin-driven monolith) and
lays out a clean-slate **Elixir/Phoenix** re-implementation strategy. It exists to inform a
rewrite in a new repository.

It answers five questions:

1. **What app does this repo ship, and what is its purpose?** → [01-product-and-purpose.md](01-product-and-purpose.md)
2. **What is the current tech stack?** → [02-tech-stack.md](02-tech-stack.md)
3. **What is the current architecture** (with concrete interfaces)? → [03-architecture.md](03-architecture.md)
4. **How do plugins work, and how is the interface informed by the choice of Rust?** → [04-plugin-interface.md](04-plugin-interface.md)
5. **What else does a rewrite need to know?** → the Elixir target design and everything below.

## Reading order

| # | File | What it covers |
|---|---|---|
| — | [README.md](README.md) (this) | Executive summary, decisions, glossary |
| 01 | [01-product-and-purpose.md](01-product-and-purpose.md) | The app, its domain, shipped features |
| 02 | [02-tech-stack.md](02-tech-stack.md) | Current stack with versions + rationale |
| 03 | [03-architecture.md](03-architecture.md) | Request path, composition, data model + ACL (quoted) |
| 04 | [04-plugin-interface.md](04-plugin-interface.md) | The plugin contract + how it's tied to Rust |
| 05 | [05-elixir-target-architecture.md](05-elixir-target-architecture.md) | Proposed Elixir design + worked example plugin |
| 06 | [06-plugin-distribution-and-assets.md](06-plugin-distribution-and-assets.md) | Third-party plugins + the asset problem |
| 07 | [07-mapping-and-tradeoffs.md](07-mapping-and-tradeoffs.md) | Rust→Elixir mapping, what's gained/lost |
| 08 | [08-sequencing-and-open-questions.md](08-sequencing-and-open-questions.md) | Build order, open questions, verification |

Files 01–04 are **as-is analysis** of the current system. Files 05–08 are the **forward-looking
strategy**. If you only read two files, read [04](04-plugin-interface.md) (the plugin contract,
which is where the language choice bites hardest) and [05](05-elixir-target-architecture.md)
(the proposed replacement).

## Executive summary

**Junius** is a single-deployable, plugin-driven web platform for political/organizational work.
A **host** provides identity (OIDC), sessions, RBAC, and shared infrastructure; each domain use
case ships as a **plugin** bundling its backend, database schema, API contract, and frontend.
The shipped plugins are **events** (a full domain plugin) and **admin** (manages the platform
itself).

The current implementation is **Rust** (Axum + Connect-RPC + PostgreSQL/sqlx) with a **React**
frontend (TanStack + Vite + Tailwind), composed at **compile time**: a CLI (`junius`) reads the
enabled-plugin list and generates a static registry and wiring, producing one self-contained
binary.

The single most important finding for the port: **the plugin interface is deeply tied to Rust's
type system** — type-level permission witnesses, compile-time manifest-to-type generation, and
macro-enforced SQL confinement give guarantees that *unauthorized code won't compile*. **None of
this has an Elixir equivalent.** But the codebase already carries the **runtime half** of every
one of these mechanisms (runtime permission checks, a runtime capability gate, and a SQL-level
access-control function). So the honest strategy is: **keep the runtime layer, drop the type
layer**, and re-add safety through runtime checks + boot-time validation + tests. The
security-critical **SQL access-control layer is language-neutral and ports verbatim.**

The Elixir target is a **Phoenix umbrella/poncho** where the host is one app and each plugin is
an **OTP application** registered at runtime, with a **LiveView** frontend (replacing the React
SPA + Connect-RPC entirely), **Oban** for jobs, per-plugin **Ecto repos** on per-plugin
Postgres roles, and the SQL access-control function ported as-is.

## Confirmed decisions

These were decided with the requester up front and constrain the whole strategy.

| Area | Decision |
|---|---|
| **Frontend** | **Phoenix LiveView.** Drop the React SPA + Connect-RPC client entirely. Collaborative/live surfaces are in scope (e.g. a **live conference manager** — live votes, speaker lists — via Phoenix PubSub). |
| **Plugin model** | **Runtime / OTP.** Plugins are OTP applications registered at runtime via a behaviour + registry; no compile-time static linking. |
| **Third-party plugins** | **First-class**, including plugins that ship their own static assets. |
| **Jobs** | **Oban**, behind a swappable `Junius.Jobs` behaviour so other backends can return. |
| **Cross-plugin access** | **Context-function APIs** (not raw SQL table sharing), with authorization enforced inside every exposed function. |
| **Tenancy** | **Single-tenant per deployment.** |
| **DB isolation** | **Keep per-plugin Postgres roles** (DB-level least-privilege). |
| **OIDC** | **Authentik** (behind an adapter). |
| **Email / storage** | **Dev stack** — SMTP (mailpit / self-hosted) and MinIO/self-hosted S3, behind adapters. |
| **Scope** | **Full parity**: base platform + the **events** and **admin** plugins. |
| **License** | **AGPL-3.0**, carried forward. |
| **Approach** | Clean slate: capture the essential behaviour + domain/data model; re-architect the mechanism idiomatically (no literal 1:1 port). |

## Glossary

- **Host** — the platform substrate (`platform/` in the Rust repo). Owns auth, users, sessions,
  RBAC, and shared infra. **Not** a plugin.
- **Plugin** — a self-contained domain use case bundling backend + DB schema + API + frontend.
  Composed into the deployment. Examples: `events`, `admin`.
- **Capability** — a coarse host-service permission a plugin declares in its manifest
  (`db.read`, `email.send`, `job.enqueue`, `storage.read/write`, `platform.admin`). Gates access
  to *host-provided handles*.
- **Permission** — a fine-grained, plugin-defined action string (`events:read`, `events:write`).
  Held by users via roles. Answers "*what kind* of action may this user perform?"
- **Resource scope** — *which specific resources* a user may act on. Resolved separately from
  permissions, by ownership + shares, via the `platform.user_can_access` SQL function.
- **Group** — a real-world organizational unit (committee, caucus, team). Users are members.
- **Group role** — a role defined *within* a group; owns a set of permissions. A user holds at
  most one role per group.
- **User role** — a *global-scope* role (added in milestone M18). The built-in `admin` user role
  holds the wildcard permission `*`.
- **Tenant** — an isolated organization. Junius is **single-tenant per deployment**: one
  deployment serves one organization.

> Source references in files 03–05 cite real paths in the Rust repo (e.g.
> `crates/junius-sdk/src/plugin.rs`) so every quoted interface can be verified against source.
