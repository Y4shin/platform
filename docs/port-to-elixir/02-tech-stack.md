# 02 — Current Tech Stack

> Part of the [Junius → Elixir/Phoenix report](README.md). As-is analysis.

A hybrid **Cargo workspace** (Rust backend + CLI) and **pnpm workspace** (TypeScript frontend),
glued by a proto-driven codegen pipeline and a management CLI (`junius`), all inside a **Nix**
dev shell driven by **go-task**.

The **"Portable?"** column flags whether the choice carries over to the Elixir port, is
replaced, or is dropped.

## Backend (Rust)

Edition 2024, `rustc` 1.88, AGPL-3.0. Host binary: `juniusd`.

| Concern | Choice | Why (per design docs) | Portable? |
|---|---|---|---|
| Language | **Rust** | "No JS/TS on the backend" hard constraint; single static binary | Replaced by **Elixir** |
| HTTP server | **Axum 0.8** (tokio/tower/tower-http) | Mature async stack | Replaced by **Phoenix/Bandit** |
| Primary API | **Connect-RPC** (`connectrpc` server + `buffa` protobuf) at `/rpc` | Schema-evolution discipline via Buf; first-class TS client; chosen over OpenAPI/rspc | **Dropped** (LiveView) |
| Other HTTP | plain `/h/<plugin>` routes + host REST `/api/*` | uploads, OAuth callbacks, `/api/me` | Folded into Phoenix router |
| Database | **PostgreSQL** + **sqlx 0.8** (no ORM; compile-time-checked SQL; committed `.sqlx/` cache) | Plain SQL as the contract; no ORM | Postgres **kept**; sqlx → **Ecto** |
| Schema layout | one Postgres **schema + least-privilege role per plugin** | DB-enforced plugin isolation | **Kept** (per-plugin Ecto repo + role) |
| Auth | **OIDC** via `openidconnect`; server sessions; AES-256-GCM token encryption | Never manage passwords; no tokens in localStorage | Kept; libs → `assent`/`oidcc` + Cloak |
| Jobs | **RabbitMQ** (`lapin` + `deadpool-lapin`); durable queue + DLQ per job | No Redis dependency; durable | Replaced by **Oban** (behind a behaviour) |
| Email | **lettre** (log / SMTP-mailpit / Resend) | Pluggable transactional provider | Replaced by **Swoosh** |
| Object storage | **rust-s3** behind an `ObjectStore` trait (MinIO/S3) | S3-compatible; vendor-neutral | Replaced by **ExAws.S3** |
| Telemetry | **tracing** → OpenTelemetry (export-only) → Grafana **LGTM** | Standard OTel stack | `:telemetry` + **OpenTelemetry** (same LGTM) |
| Proto codegen (Rust) | per-crate `build.rs`: `connectrpc-build` + `junius-rpc-meta` + `junius-i18n-build` | Generate server stubs + permission-witness aliases | **Dropped** with the proto layer |

The `connectrpc`/`buffa` stack replaced an earlier hand-rolled unary server and `prost`
(decision log, 2026-05-25). There is **no gRPC server and no tRPC** — Connect-RPC over plain
HTTP is the wire protocol.

## Frontend (TypeScript)

Node 24, pnpm 11. The frontend is a pnpm workspace of packages under `platform/frontend`
(`@junius/shell`), `packages/*` (`@junius/design`, `@junius/generated`, `@junius/sdk`,
`@junius/e2e`), and each plugin's `frontend/`.

| Concern | Choice | Why (per design docs) | Portable? |
|---|---|---|---|
| Framework | **React + TypeScript** | Largest ecosystem + hiring pool for SPA-heavy UI | **Dropped** (LiveView) |
| Routing | **TanStack Router** (code-based) | Code-based routing composes plugin route subtrees natively; file-based routers fight the plugin model | Replaced by Phoenix Router |
| Data | **TanStack Query** + `@connectrpc/connect-query` | First-class Connect integration | Dropped (LiveView is stateful server-side) |
| Build | **Vite 6** | Fast SPA bundler | Replaced by esbuild+tailwind (Phoenix) |
| Styling | **Tailwind v4** + Radix, wrapped in `@junius/design` | Design system; plugins never write raw HTML controls | **Tailwind kept**; `@junius/design` → a shared UI OTP app |
| Transport | **Connect-Web** against `/rpc` | Type-safe client from proto | Dropped |
| Codegen (TS) | **buf** `protoc-gen-es` v2 → `@junius/generated` | Same protos generate the TS client | Dropped |
| SSR (optional) | hand-rolled `renderToPipeableStream` Node server (M23) | Optional SSR without TanStack Start (routeTree is codegen'd) | Dropped (LiveView is server-rendered) |
| i18n | **Lingui** (gettext `.po`, explicit key msgids; en/de/pseudo) | Shared catalog format BE+FE | Replaced by **Gettext** (also `.po` — msgids reusable) |

**Key enabler to note:** TanStack's *code-based* routing is what makes compile-time plugin
composition possible on the frontend — each plugin exports a `buildRoutes(parent)` factory that
`junius sync` concatenates into one route tree. The Elixir port reproduces this idea with
Phoenix router composition (see [05](05-elixir-target-architecture.md)).

## Tooling, build & ops

| Concern | Choice | Notes | Portable? |
|---|---|---|---|
| Management CLI | **`junius`** (clap; feature-tiered) | `check`, `sync`, `build`, `dev`, `migrate`, `plugin`, `provision`, `oidc`, `i18n`, `new`, `rpc`, `cache` | Replaced by **Mix tasks** + a deploy-time assembler |
| Toolchain | **Nix flake** | Provides Rust, Node, pnpm, biome, buf, protoc, go-task | Replaced by Elixir/OTP + Mix (Nix optional) |
| Task runner | **go-task** (`task ci` = full gate) | Mirrors CI jobs | Replaced by Mix aliases / Makefile |
| Hooks | **lefthook** (pre-push) | Verifies flake FOD hashes | Optional |
| Quality gates | **clippy** (`-D warnings`), **biome**, **buf** (lint + breaking), committed `.sqlx` | CI enforces zero warnings/skips | Replaced by **Credo/Dialyzer** + `mix format` + tests |
| Composition | `junius sync` generates static registries + wiring from `platform.toml` | The load-bearing compile-time mechanism | Replaced by a **runtime registry** (+ optional assembler for assets) |

## Deployment & backing services

- **Deployment topologies**: (a) source-built single binary; (b) precompiled `ghcr.io` images in
  three variants — `full` (embedded SPA + all plugins), `backend` (headless), `frontend` (SSR
  Node server). There's a "source vs deployment" split: a deployment directory holds only
  `platform.toml` + secrets + a `platform.lock` (blake3 hashes) and resolves the platform source
  by path or git rev.
- **Backing services** (`dev/docker-compose.yml`): **Postgres 17** (app DB), **Authentik**
  (OIDC IdP), **RabbitMQ** (jobs), **MinIO** (S3 storage), **mailpit** (SMTP), **Grafana
  otel-lgtm** (telemetry).

For the Elixir port these become: an **OTP release** (`mix release`) + Docker; the same backing
services (Postgres, Authentik, MinIO, mailpit, LGTM) minus RabbitMQ, which **Oban** replaces with
a Postgres-backed queue. See [05](05-elixir-target-architecture.md) and
[06](06-plugin-distribution-and-assets.md).

## What the stack choices tell us for the port

- The **portable core** is: PostgreSQL, per-plugin schemas + roles, the SQL access-control
  function, the manifest/permission *model*, OIDC + server sessions, and Tailwind.
- The **Rust-specific machinery** — Connect-RPC/proto codegen, sqlx compile-time queries, the
  `junius sync` static-composition step, and the whole type-level permission/repository system —
  is what gets replaced, and is analyzed in [04](04-plugin-interface.md).
