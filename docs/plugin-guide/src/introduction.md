# Introduction

Junius is a **plugin-driven monolith for political work**. The *host*
(`platform/`) provides authentication, user management, RBAC, sessions and the
shared infrastructure every feature needs. Each domain use case ships as a
*plugin* under `plugins/` that bundles its backend (Rust), frontend (React/TS)
and API contract (proto) together.

This book teaches you how to build one of those plugins, start to finish.

## What you'll build

We follow the **`events` plugin** (`plugins/events/`) as the worked example.
It is the canonical reference because it exercises every platform primitive:

- per-instance **ownership** by **user *and* group** principals,
- **public/private visibility** with a per-resource ACL,
- **permission-gated RPC** with compile-time *and* runtime enforcement,
- an **unauthenticated** public surface (the invite/sign-up pages),
- **background jobs + email** (sign-up confirmations),
- **token-authed feeds** (the `.ics` calendar subscriptions),
- and full **internationalization** of every user-facing string.

Every snippet in this book has a real counterpart under `plugins/events/`. When
in doubt, read the code.

## The one-paragraph mental model

A plugin is a **Rust crate** (`plugins/<name>/`) plus an optional **frontend
package** (`plugins/<name>/frontend/`, published to the pnpm workspace as
`@junius/plugin-<name>`). The Rust side serves plain HTTP under `/h/<name>` and
Connect-RPC under `/rpc`; the frontend contributes routes under `/p/<name>`
(and, if it opts in, a public surface under `/i/<name>`) plus components other
plugins can reuse. A deployment's `platform.toml` lists which plugins are
enabled, and **`junius sync`** wires them into the host — generating the plugin
registry, the RPC permission table, the frontend route tree and the component
registry.

Plugins talk to the host **only** through the `junius-sdk` crate. They never
reach into host internals, and they never touch another plugin's database tables
unless that plugin explicitly exposes them. This boundary is what keeps the
monolith maintainable as it grows.

## How this book is organized

- **Getting started** — the architecture, your dev environment, and scaffolding
  a running plugin in a few commands.
- **Building the backend** — the manifest, migrations, permissions,
  repositories, RPC services, ownership/sharing, and public surfaces.
- **Building the frontend** — routes, calling the backend, forms, and exposing
  components to other plugins.
- **Platform capabilities** — jobs, email, object storage, and i18n.
- **Quality and shipping** — testing (the TDD workflow), the CI gate, and
  server-side rendering.
- **Reference** — the `plugin.toml` schema, the `junius` CLI, the SDK surface,
  and a troubleshooting index.

If you want the fast path, read [Getting started](./getting-started/architecture.md)
and then jump to whichever capability you need. If you want the full picture,
read straight through.

## Conventions used throughout

- Run everything inside the Nix dev shell: `nix develop` (or `direnv allow`
  once). All commands assume you're in that shell.
- The local quality gate is `task ci`. After editing a manifest or a proto,
  re-run `junius sync` (the `task sync` shortcut).
- **English** for all code, comments, docs and commits.
- Commands are written so you can copy-paste them. Where a value is
  deployment-specific it's shown as `<deployment>/platform.toml`; in this repo
  that's `dev/platform.toml`.
