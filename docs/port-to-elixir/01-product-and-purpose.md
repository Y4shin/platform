# 01 — Product & Purpose

> Part of the [Junius → Elixir/Phoenix report](README.md). As-is analysis.

## What the app is

**Junius** is a **plugin-driven monolith for political / organizational work**. From the repo
README and `docs/design/01-goals-and-constraints.md`:

> A plugin-driven monolith for political work. The platform substrate provides user management,
> authentication, and shared infrastructure; each domain use case (speakers, events, canvassing,
> …) ships as a plugin that bundles its backend, frontend, and API contract together.

It is one process, one deployable — but composed from independent plugin units.

## The problem it solves

Political organizations — committees, caucuses, working groups, campaign teams — need a variety
of tools: managing events, speakers, canvassing rounds, votes, documents. Each is a distinct use
case, but they all share:

- **One identity substrate** — the same people, in the same groups, with the same roles.
- **One permission model** — who may do what, and to which resources.
- **One deployment** — a single self-contained artifact an organization can host.

Junius answers this with a **host** (the substrate: identity, sessions, RBAC, shared infra) plus
**plugins** (the use cases). A deployment selects which plugins it wants; adding a plugin does
not require touching the host.

## Design pillars

From `docs/design/01-goals-and-constraints.md` (verbatim points, with notes for the port):

- **Plugin-driven monolith.** One process, one deployable, composed from independent plugin
  units.
- **Compile-time plugin composition.** Plugins are wired in at build time, not loaded
  dynamically. *(This is a Rust-era choice the port deliberately reverses — see
  [05](05-elixir-target-architecture.md).)*
- **Plugins bundle backend + frontend.** A plugin owns its server logic, API schema, UI routes,
  and any components it exposes to peers.
- **No JS/TS on the backend.** *(A Rust-era hard constraint. Moot for Elixir.)*
- **Significant SPA-like interactivity** alongside CRUD-style views.
- **End-to-end type safety** between backend and frontend. *(Achieved in Rust via proto/Connect
  codegen; the port trades this for LiveView's single-language boundary.)*
- **Single self-contained deployment artifact.** One binary, no separate frontend hosting.

The system leans **single-tenant per deployment** (`docs/design/13-open-questions.md`): one
deployment serves one organization.

## Shipped functionality

The repo ships two production plugins plus demos.

### `events` — the real domain plugin

The first full domain plugin (milestone M13). Feature set (from `docs/design/14-decision-log.md`
and `plugins/events/`):

- **Events owned by a user *or* a group.** Ownership kind is chosen per event instance; a
  personal event is user-owned, a committee meeting is group-owned.
- **Private or public.** Public events are world-readable (they bypass the access-control filter
  on read); private events are gated by ownership/role/shares.
- **Sign-up invite pages** with: slot limits, manual open/close, per-field visibility toggles,
  group pre-sign-up, and opt-out.
- **A confirmation-email background job** enqueued on sign-up.
- **Revocable iCalendar export**: an access-checked single-event download, key-authed
  personal/group subscription feeds, and a per-group public toggle. Feed keys are unguessable,
  stored **hashed** (SHA-256), subject-scoped, and revocable.
- **Two unauthenticated surfaces**: the public invite page (login-optional) and the ICS feeds
  (key-authed). These use "ungated" handlers where the SQL access-control layer — not a
  compile-time permission — decides access.

Its manifest (`plugins/events/plugin.toml`) declares three permissions — `events:read`,
`events:write`, `events:share` — two exposed components (`EventCard`, `EventPicker`), one exposed
table (`event`), and two required capabilities (`email.send`, `job.enqueue`). See
[03](03-architecture.md) and [04](04-plugin-interface.md) for the concrete schema and handlers.

### `admin` — manages the platform itself

A **first-party plugin**, deliberately *not* built into the host UI. This is a significant
architectural statement (milestone M18): the host has no UI of its own, and the plugin contract
is rich enough that even platform administration is "just a plugin." It manages:

- **Groups**, **group-roles**, and their **permission sets**.
- **Global user-roles** and their assignments (including the built-in `admin` role).
- **OIDC group → group/role mappings** (reconciled on login).
- A **permission catalogue** sourced from the host's compiled-in metadata, so the admin UI can
  never drift from what the system actually accepts.
- The **audit log** viewer.

It is gated by a special trusted capability, `platform.admin`, which the host's allowlist
permits **only** the `admin` plugin to declare — the host refuses to boot otherwise
(`plugins/admin/plugin.toml`, `platform/src/server.rs`).

### Demo plugins

`hello`, `greetings`, and `widgets` exercise the plugin contract itself — RPC services,
cross-plugin dependencies (required and optional), and graceful degradation when an optional
dependency is disabled. They are not domain features; they exist to prove and test the
plumbing.

## Envisioned but not shipped

The design docs and README name **speakers** and **canvassing** as intended future domains. They
are illustrative of the target: each would be a plugin like `events`, sharing the same identity
and permission substrate.

## Why this matters for the port

The product is **not** "an events app" — it is **a platform for building political-org apps as
plugins**. The port must reproduce the *substrate* (identity, RBAC, resource-scoped access
control, shared infra, the plugin contract) with the same fidelity as the domain features. Full
parity is scoped as: **base platform + events + admin** (see [README](README.md) decisions).
