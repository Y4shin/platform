# 23. M21 — Documentation site (mdbook)

> **📦 Migrated to a PRD.** Planning of this (still-unbuilt) milestone moved to the PRD
> workflow — see [`docs/prd/m21-documentation-site/prd.md`](../prd/m21-documentation-site/prd.md)
> and the [MIGRATION.md](../../MIGRATION.md) rollout. This doc stays as the **full design
> reference**; track active work via the PRD's tracking issue
> ([#15](https://github.com/Y4shin/platform/issues/15)), not this doc.

> **Status:** 🚧 planned. Independent of M14–M20 — can land any time after
> M13. The existing `docs/` tree (design, impl, plugin-authoring-guide)
> is kept in place; M21 wraps it in a navigable, locally-servable book
> and adds the missing pieces (getting-started, CLI reference, glossary,
> troubleshooting, ops, contributing, changelog).

One-line goal: `task docs:serve` opens a browser at the project's
documentation site, with the existing 9.6k lines of markdown organised
into a single navigable book plus the net-new docs the project keeps
working around the absence of.

## Why this milestone exists

The `docs/` tree has grown to ~40 files and ~9.6k lines:

- 14 design docs (`docs/design/`)
- 23 milestone docs (`docs/impl/`)
- A 567-line plugin-authoring guide
- Two folder-level READMEs that act as indexes

It's well-organised *as a folder structure* and unreadable *as a body
of work*. There is:

- **No global landing page** — `docs/README.md` doesn't exist; a new
  reader has to know to open `docs/design/README.md` or
  `docs/impl/README.md` first.
- **No search.** Finding "how does `has_permission_in_group` work" or
  "what does the M13 friction log say about buf" means `grep -r`.
- **No nav between sections.** Design ↔ impl ↔ guide are linked
  ad-hoc; there is no consistent next/previous, no breadcrumbs, no
  section index in-page.
- **No tested cross-references.** Broken `[label](path)` links sit
  unnoticed (the M13 friction log already accumulated a few during
  the M14 doc rename).
- **No reader-friendly entry surfaces** — quickstart, prerequisites,
  CLI reference, glossary, troubleshooting, deployment guide,
  contributor guide, changelog — every one of these would have helped
  the build at some point and none exist.

This milestone introduces mdbook to render the existing tree as a
book and fills in the conspicuously-missing surfaces. It is
**low-risk** (no code paths change) and **high-leverage** (every
subsequent milestone benefits from a place to land its docs).

## Outcome / acceptance

- `task docs:serve` builds the book and opens it at
  `http://localhost:3000` (mdbook's default), with live reload on
  source changes.
- `task docs:build` produces a static site under `docs/book/`
  (`.gitignore`d).
- The book renders **every existing markdown file** from `docs/design/`,
  `docs/impl/`, and `docs/plugin-authoring-guide.md` — without moving
  or rewriting them — plus the new content listed below.
- Search works out of the box (mdbook ships elasticlunr).
- Diagrams render (Mermaid via `mdbook-mermaid`).
- Broken cross-doc links fail CI (`mdbook-linkcheck`); this becomes a
  sixth `task ci` step.
- `junius` CLI reference is **auto-generated** from `clap`'s help
  output by a `task docs:cli-reference` target run during the book
  build (no hand-curated CLI docs drifting from reality).
- A new `docs/README.md` is the book's introduction page and works
  acceptably when read straight in GitHub (no mdbook-only syntax in
  the entry point).

## Design

### A — Tooling

- **mdbook** as the renderer. Rust-native, single binary, native to
  the project's `nix develop` shell. Tree of pure `.md` files works
  as-is.
- **`mdbook-mermaid`** for runtime / architecture diagrams. Currently
  every diagram in the docs is ASCII; selective Mermaid lets the
  architecture map and the request lifecycle pages render visually
  while ASCII stays where it reads better (the M19/M20 mockups).
- **`mdbook-linkcheck`** as a CI-only preprocessor. Catches broken
  cross-doc links + missing anchors before merge.
- **`mdbook-toc`** for per-page tables-of-contents in long docs
  (M13's plugin guide, M10's infra capabilities).
- **`mdbook-admonish`** for `> Note: …` / `> Warning: …` callouts.
  Optional — adopt only if existing prose uses callouts often enough
  to justify it; otherwise skip.

All preprocessors install via `cargo install` and get pinned in
`rust-toolchain.toml` / a `.tool-versions` style manifest so CI and
local match.

### B — Layout: keep existing files in place; add a thin wrapper

mdbook accepts arbitrary paths from `SUMMARY.md`. The wrapper:

```
docs/
├── book.toml                                     # NEW — mdbook config
├── SUMMARY.md                                    # NEW — book TOC
├── README.md                                     # NEW — book intro / landing page
├── design/                                       # UNCHANGED
│   ├── README.md
│   └── 01-…14-…
├── impl/                                         # UNCHANGED
│   ├── README.md
│   ├── 00-approach.md
│   ├── 01-M00-… 22-M20-… 23-M21-…
│   └── 14-M13-events-plugin.friction.md
├── plugin-authoring-guide.md                     # UNCHANGED
├── getting-started/                              # NEW
│   ├── quickstart.md
│   ├── prerequisites.md
│   └── first-plugin-tour.md
├── concepts/                                     # NEW
│   ├── architecture-map.md
│   ├── permissions.md
│   ├── groups-and-roles.md
│   ├── public-surfaces.md
│   ├── background-jobs.md
│   ├── i18n.md
│   └── apps-and-navigation.md
├── reference/                                    # NEW
│   ├── cli/
│   │   ├── README.md
│   │   ├── junius-sync.md           ← auto-generated
│   │   ├── junius-check.md          ← auto-generated
│   │   ├── junius-dev.md            ← auto-generated
│   │   ├── junius-migrate.md        ← auto-generated
│   │   ├── junius-new.md            ← auto-generated
│   │   ├── junius-plugin.md         ← auto-generated
│   │   ├── junius-provision.md      ← auto-generated
│   │   └── …                                     # one per top-level command
│   ├── plugin-toml.md
│   ├── http-api.md
│   └── glossary.md
├── troubleshooting/                              # NEW
│   ├── common-errors.md
│   ├── dev-setup.md
│   └── ci.md
├── ops/                                          # NEW
│   ├── deployment.md
│   ├── database.md
│   └── observability.md
├── contributing/                                 # NEW
│   ├── overview.md
│   ├── milestone-plan.md
│   └── decision-log-workflow.md
└── changelog.md                                  # NEW
```

`book.toml`:

```toml
[book]
title    = "Junius"
authors  = ["Junius contributors"]
language = "en"
src      = "."             # all paths in SUMMARY.md are relative to docs/

[output.html]
default-theme = "navy"
git-repository-url = "<set by deployment, optional>"
edit-url-template = "<...>/{path}"   # GitHub edit links

[output.html.search]
enable = true
limit-results = 30
use-boolean-and = true

[preprocessor.mermaid]
command = "mdbook-mermaid"

[preprocessor.toc]
command = "mdbook-toc"

[output.linkcheck]
follow-web-links = false             # offline; only check intra-book + filesystem
warning-policy   = "error"
```

`SUMMARY.md` is the only file that *must* be authored explicitly; it's
the book's nav. Sketch:

```markdown
# Summary

[Introduction](README.md)

# Getting started
- [Quickstart](getting-started/quickstart.md)
- [Prerequisites & toolchain](getting-started/prerequisites.md)
- [Your first plugin (tour)](getting-started/first-plugin-tour.md)

# Concepts
- [Architecture map](concepts/architecture-map.md)
- [Permissions](concepts/permissions.md)
- [Groups & roles](concepts/groups-and-roles.md)
- [Public surfaces](concepts/public-surfaces.md)
- [Background jobs](concepts/background-jobs.md)
- [Internationalization](concepts/i18n.md)
- [Apps & navigation](concepts/apps-and-navigation.md)

# Plugin author's guide
- [Authoring guide](plugin-authoring-guide.md)

# Reference
- [`junius` CLI](reference/cli/README.md)
  - [`junius sync`](reference/cli/junius-sync.md)
  - [`junius check`](reference/cli/junius-check.md)
  - [`junius dev`](reference/cli/junius-dev.md)
  - [`junius migrate`](reference/cli/junius-migrate.md)
  - [`junius new`](reference/cli/junius-new.md)
  - [`junius plugin`](reference/cli/junius-plugin.md)
  - [`junius provision`](reference/cli/junius-provision.md)
- [`plugin.toml` reference](reference/plugin-toml.md)
- [Host HTTP API](reference/http-api.md)
- [Glossary](reference/glossary.md)

# Troubleshooting
- [Common errors](troubleshooting/common-errors.md)
- [Local dev setup pitfalls](troubleshooting/dev-setup.md)
- [CI failures](troubleshooting/ci.md)

# Operating Junius
- [Deployment](ops/deployment.md)
- [Database operations](ops/database.md)
- [Observability](ops/observability.md)

# Contributing
- [Overview](contributing/overview.md)
- [The milestone plan](contributing/milestone-plan.md)
- [Decision-log workflow](contributing/decision-log-workflow.md)

# Design
- [Design docs index](design/README.md)
  - [Goals & constraints](design/01-goals-and-constraints.md)
  - [Architecture](design/02-architecture.md)
  - [Backend](design/03-backend.md)
  - [Frontend](design/04-frontend.md)
  - [Repository & deployment layout](design/05-repository-and-deployment-layout.md)
  - [Plugin shape](design/06-plugin-shape.md)
  - [The management tool (`junius`)](design/07-platctl.md)
  - [Cross-plugin composition](design/08-cross-plugin-composition.md)
  - [Build & dev workflow](design/09-build-and-dev-workflow.md)
  - [Infrastructure & data](design/10-infrastructure-and-data.md)
  - [Backend plugin interface](design/11-backend-plugin-interface.md)
  - [Frontend plugin interface](design/12-frontend-plugin-interface.md)
  - [Open questions](design/13-open-questions.md)
  - [Decision log](design/14-decision-log.md)

# Implementation
- [Implementation plan index](impl/README.md)
  - [Sequencing approach](impl/00-approach.md)
  - [M00 — Workspace skeleton](impl/01-M00-workspace-skeleton.md)
  - … (all the way through M21)
  - [M13 friction log](impl/14-M13-events-plugin.friction.md)
  - [Open questions resolution](impl/15-open-questions-resolution.md)

# Changelog
- [Changelog](changelog.md)
```

Critical: **no existing file is moved**. Every relative link in
`docs/design/*.md`, `docs/impl/*.md`, and `docs/plugin-authoring-guide.md`
continues to resolve. The book wraps; it doesn't restructure.

### C — Net-new content (what to add and why)

These are the gaps the build hit but had nowhere to land. Each
section below is a single page unless noted. **Each is a stub in
Stage 1 (just a placeholder + headings) so the nav works; the prose
fills in across Stages 2–4.** Splitting "structure" from "content"
keeps each stage shippable.

#### Getting started

- **[`getting-started/quickstart.md`]** — Clone, `nix develop`,
  `docker compose -f dev/docker-compose.yml up -d`, log in to
  Authentik once, `task dev`. Five-minute path to "Alice logged into
  Events." Currently spread across `dev/authentik/README.md`,
  `dev/dev-seed.sql`, and the M06 verification block.
- **[`getting-started/prerequisites.md`]** — What `nix develop`
  provides (Rust, pnpm, buf, mdbook, …); what Docker is for
  (Postgres, Authentik, MinIO, mailpit, RabbitMQ); WSL / mac
  particulars; the env-vars contract from `dev/authentik/README.md`.
- **[`getting-started/first-plugin-tour.md`]** — Lighter-weight
  prelude to the plugin-authoring guide: "here's what a plugin
  *looks* like; here's the smallest interesting change you can
  make." For someone exploring the codebase, not yet building.

#### Concepts

The design docs explain **why**; the impl docs explain **when**; a
concepts page explains **what it is, today, in one place**. Each
page is a synthesis with cross-links rather than a duplicate.

- **`concepts/architecture-map.md`** — One Mermaid diagram:
  Browser → Vite (dev) / rust-embed (prod) → Axum host → per-plugin
  schemas in Postgres → MinIO/RabbitMQ/SMTP. Annotated. The
  reference picture every other doc points at.
- **`concepts/permissions.md`** — The whole permission story in one
  place: manifest declaration, the proto `requires` gate, the SDK
  witness type, the SQL ACL (`platform.user_can_access`), the
  M16 `has_permission_in_group`, the M18 admin wildcard. Currently
  scattered across M06–M08, M16, M18.
- **`concepts/groups-and-roles.md`** — `platform.group`,
  `group_role`, `role_permission`, `group_membership` + M18's
  `user_role*` + OIDC mapping + config provisioning, in one
  reader-friendly walkthrough.
- **`concepts/public-surfaces.md`** — The login-optional `/i/<plugin>`
  pattern + the token-authed feed pattern (M13). The single page
  M13 friction row #38 and #49 both wished for.
- **`concepts/background-jobs.md`** — M10 in plain language: when to
  use a job, the `Jobs::disabled` test harness, the email-on-signup
  worked example.
- **`concepts/i18n.md`** — Lingui + the backend localizer; how a
  plugin opts in; what `task ci:i18n` checks.
- **`concepts/apps-and-navigation.md`** — `[[plugin.apps]]` (M20),
  sub-nav, the picker, `useActiveApp`. Written *after* M20 lands;
  stubbed earlier.

#### Reference

- **`reference/cli/*.md`** — One page per top-level `junius`
  command, auto-generated from `clap`'s help output. The build
  step calls `junius help <command> --markdown` (or equivalent
  via the `clap_mangen` / `clap-markdown` crate) and writes the
  output into `reference/cli/<cmd>.md`. The files are
  **regenerated as part of `task docs:build`** and committed —
  CI checks for drift like the `.sqlx/` offline cache (M07).
- **`reference/plugin-toml.md`** — Every key in `plugin.toml`,
  what it means, when it's required, where it's read. Currently
  the source of truth is the `junius-manifest` crate and a
  scattering of examples; this page is the user-facing index.
- **`reference/http-api.md`** — Every `/api/*` route the host
  exposes (`/api/me`, `/api/auth/login`, `/api/auth/callback`,
  `/api/auth/logout`, `/api/me/locale` after M14, `/api/sessions/<id>`
  after M19) — request/response shape + which session/role gates
  it.
- **`reference/glossary.md`** — One-line definitions: *host*,
  *plugin*, *capability*, *principal*, *ownership*, *witness*,
  *exposed table*, *manifest*, *ACL*, *Authz*, *PluginCtx*,
  *PluginResources*, *PluginDb*, *managed_by* (M18), *app* (M20),
  *user-role* (M18), *group-role*, *deployment* (vs source). The
  page everyone's first link is.

#### Troubleshooting

- **`troubleshooting/common-errors.md`** — The collected M13
  friction-log "common gotcha" rows that aren't fixed by code:
  - `use junius_sdk::permissions` collides with the generated
    module → use the fully-qualified macro.
  - Custom-enum `query!` ergonomics: `col AS "col: Type"` +
    `$N::schema.enum`.
  - `biome check --write` vs `biome format --write` for import
    sorting.
  - `clippy::struct_excessive_bools` on toggle DTOs.
  - `buf` lint `RPC_RESPONSE_STANDARD_NAME` per-RPC empty messages.
  - Missing `zod` direct dep on plugins authoring zod schemas.
  - Plugin sub-route nav typing (until M16-D ships).
- **`troubleshooting/dev-setup.md`** — Authentik first-boot
  (~2–3 min), WSL ↔ Docker networking, the OIDC group claim
  scope (after M18), the dev-seed → provisioning transition.
- **`troubleshooting/ci.md`** — The six `task ci` jobs (lint,
  rust unit, rust integration, deployment, e2e after M17,
  link-check after M21), what each catches, how to reproduce
  failures locally, what `task fmt` fixes.

#### Operating Junius

The plan calls production ops "out of scope" (per
[00-approach.md §0.6](00-approach.md#06-what-this-plan-does-not-cover)).
These pages **don't** introduce ops infrastructure; they document
what already exists for the deployer's benefit.

- **`ops/deployment.md`** — What `junius build` produces, where to
  put it, the env vars it reads (matched against `dev/platform.toml`
  `[config]`), the secret-loading model (`env:`/`file:` indirection
  from M06/M11), running migrations + provisioning before first
  boot.
- **`ops/database.md`** — The migrator role vs plugin roles
  (M06), the no-down-migrations rule, the `meta.migrations` table,
  the `.sqlx/` offline cache invariant.
- **`ops/observability.md`** — OTLP endpoint config, the local
  LGTM stack from M10, what to grep / dashboard for (request
  latency, job failure rate, audit-event volume). Pointer to the
  `/p/admin/health` page (M19) and `/p/admin/jobs` (M19) for
  in-app introspection.

#### Contributing

- **`contributing/overview.md`** — How to land a change: read the
  active milestone doc end-to-end, work the *Library choices*
  block with the user, implement against *Scope (in)*, run
  `task ci`, sign+push the batch. Mirrors the unwritten norms in
  the repo.
- **`contributing/milestone-plan.md`** — The model. What a
  milestone *is*; what "defaults are not silent" means; how the
  friction log feeds into later milestones; when scope shifts
  trigger a doc rewrite. Currently the implicit reading of
  `impl/00-approach.md`.
- **`contributing/decision-log-workflow.md`** — When to add a
  decision-log entry; the format conventions; the relationship
  between the open-questions file and the decision log.

#### Changelog

- **`changelog.md`** — Hand-curated, one section per milestone tag
  (`M00` … `M21`), each section a 3–5-bullet summary of what
  landed. Cross-links to the relevant impl doc. The "what shipped
  in M14, again?" answer.

### D — Task targets + CI

```yaml
# Taskfile.yml additions
tasks:
  docs:serve:
    desc: Serve the docs site locally with live reload.
    cmds:
      - task: docs:cli-reference
      - mdbook serve docs --open

  docs:build:
    desc: Build the static docs site to docs/book/.
    cmds:
      - task: docs:cli-reference
      - mdbook build docs

  docs:cli-reference:
    desc: Regenerate reference/cli/*.md from junius's clap help.
    cmds:
      - cargo run -p junius -- docs cli --out docs/reference/cli

  docs:check:
    desc: Build the book and assert no broken links / drift in CLI ref.
    cmds:
      - task: docs:cli-reference
      - git diff --quiet docs/reference/cli || (echo "CLI ref drift; commit the regenerated files" && exit 1)
      - mdbook build docs
```

`task ci` gains `docs:check` as its sixth job (mirrors the M17 E2E
addition pattern). CI installs the pinned mdbook + preprocessors
from the toolchain manifest.

### E — `junius docs cli` subcommand

A new `cargo run -p junius -- docs cli --out <dir>` walks every
top-level + sub-command and writes one markdown file per
`junius <command>` invocation. Implementation leans on
`clap_complete` / `clap-markdown` (existing community crates).
Each file is fmt-clean and re-runnable — same drift model the
M11/M13 generated files already use.

## Stages

Repo norm: one **unsigned** commit per stage, `task ci` green per
stage, sign+push the batch at the end. The stages are
content-driven (each stage ships a coherent slice of book) rather
than tooling-driven (it'd be tempting to land tooling first +
content later, but a tooling-only stage produces no shippable
value).

- **Stage 1 — Tooling + scaffold + existing-content wrap.**
  `book.toml`, `SUMMARY.md`, `docs/README.md` introduction page,
  every existing file linked in the nav, `task docs:serve` /
  `task docs:build` working. New section dirs (`getting-started/`,
  `concepts/`, `reference/`, etc.) created with **placeholder
  stubs** so the nav is complete from day one. *Verify:*
  `task docs:serve` opens the book; clicking any nav entry
  resolves; search finds an arbitrary M13 phrase; the design and
  impl indexes both render with their existing tables intact.

- **Stage 2 — Getting-started + glossary + CLI reference.** Fill
  in `quickstart.md`, `prerequisites.md`, `first-plugin-tour.md`,
  `reference/glossary.md`. Implement `junius docs cli` and wire
  it into `docs:build`; populate `reference/cli/*.md`. *Verify:*
  the quickstart walks a fresh contributor from clone to logged-
  in app in under five minutes; the glossary defines every term
  the design docs use without definition; `task docs:check`
  detects drift if a `junius` flag changes.

- **Stage 3 — Concepts + plugin.toml + HTTP API reference.** Fill
  in the seven `concepts/*.md` pages (architecture map with
  Mermaid; permissions; groups & roles; public surfaces;
  background jobs; i18n; apps & nav — the last one stubbed if M20
  hasn't shipped). Fill in `reference/plugin-toml.md` and
  `reference/http-api.md`. *Verify:* the architecture map renders
  as a diagram; the permissions concept page is the single page
  a new author needs (no need to chase M06–M08 + M16 + M18);
  the `plugin-toml` reference matches the manifest crate fields
  one-to-one.

- **Stage 4 — Troubleshooting + ops + contributing + changelog.**
  Fill in the three troubleshooting pages (the M13 friction
  log's common-error rows become permanent guidance), three ops
  pages, three contributing pages, and the changelog (one
  section per shipped milestone, M00–M21). *Verify:* every M13
  friction-log row tagged "common gotcha" appears in
  `common-errors.md`; the changelog cross-links to every impl
  doc; the contributing pages match the actual repo norms (one
  unsigned commit per stage, etc.).

- **Stage 5 — CI integration + link-check + diagrams.** Add
  `mdbook-linkcheck` as a CI preprocessor; add `task docs:check`
  to `task ci`; add `mdbook-mermaid` and convert the
  architecture map to a real diagram; pin the mdbook + preproc
  versions in the toolchain manifest. *Verify:* a deliberately
  broken `[link](missing.md)` fails CI; CI installs mdbook
  reproducibly across runs; the architecture diagram renders
  identically in the served and built outputs.

## Out of scope

- **Publishing to a public docs site** (GitHub Pages,
  Read the Docs, …). The book is local-only in v1; `mdbook build`
  produces a static `docs/book/` artifact that a future ops
  milestone could deploy. The book.toml stays generic so a
  deployment can pick its host.
- **Auto-generating Rust API docs** (`cargo doc`). Different
  audience, different tool. The user-facing book references the
  SDK by *concept*; the `cargo doc` HTML is the developer-facing
  surface. A future Stage could publish both side-by-side; not
  this milestone.
- **Proto / gRPC reference site.** `buf` already renders a
  schema reference; if we want to publish it, that's a separate
  pipeline.
- **Localising the book itself.** The book is English-only. M14
  shipped i18n for *the app*; localising the docs is a much
  bigger undertaking with a different translator audience.
- **Interactive examples / runnable sandboxes** (StackBlitz,
  WebContainer). Out of scope; pure-static docs only.
- **Versioned docs** (per-release branches). The book reflects
  `main`. A future ops milestone can introduce a versioning
  scheme once releases are tagged formally.
- **Migration from any existing tool.** The current `docs/` tree
  is plain markdown; nothing to migrate.

## Library choices — confirm with user before starting

| Choice | Proposed default | Rationale | Notes |
|---|---|---|---|
| **Renderer** | `mdbook` | Native Rust, single binary, plain-markdown content, ships search; matches `nix develop` toolchain | Alternatives: `docusaurus` (React, heavier, mismatched stack); `mkdocs` (Python, adds a runtime); `vitepress` (Vue, mismatched). All rejected for ecosystem fit. |
| **Diagrams** | `mdbook-mermaid` | One config line; renders client-side; no second toolchain | Alternative: PlantUML (Java). Rejected — heavier, server-side rendering needed. |
| **Link check** | `mdbook-linkcheck` | mdbook-native, runs as a preprocessor in CI | Alternative: lychee. Rejected — second crate to install, overlaps with what mdbook-linkcheck does for free. |
| **TOC** | `mdbook-toc` | Per-page in-page TOC for the long docs (M13 plugin guide, M10 infra) | Skipping is fine if the editorial answer is "split long docs instead." |
| **Callouts** | `mdbook-admonish` (opt-in) | `> Note:` / `> Warning:` boxes if existing prose has many; install if Stage-2 content needs them | Otherwise skip; the project's prose voice favours full sentences over icon-prefixed callouts. |
| **CLI reference codegen** | A new `junius docs cli` subcommand using `clap-markdown` (or `clap_mangen` + a small adapter) | Single source of truth; drift-checked in CI like `.sqlx/` | Alternative: hand-curated CLI docs. Rejected — they will drift. |
| **`book.toml` `src`** | `"."` (book root = `docs/`) | Keeps every existing file path stable; only `book.toml` + `SUMMARY.md` + new dirs are added | Alternative: move everything into `docs/src/`. Rejected — breaks every relative link in the existing tree. |
| **Theme** | mdbook default `"navy"` | Looks good in dark; readable in light; no custom CSS needed in v1 | A project-branded theme is later polish. |
| **Search** | mdbook default (elasticlunr) | Built in; client-side; no infra | — |
| **CI link policy** | Offline only (`follow-web-links = false`) | Avoids flakes; we don't gate on external availability | A nightly that also checks web links is a future add. |
| **CLI ref drift** | `task ci` fails if regenerated files differ from committed | Same posture as `.sqlx/` offline cache (M07) and the M11/M12 generated artifacts | A `--check` mode on the codegen would be cleaner if `clap-markdown` exposes one; otherwise the `git diff` shape works. |
| **Mermaid theme** | Default | One less knob; can revisit if the diagrams need branding | — |

## Open questions resolved

- **Static-site tool** → mdbook (Rust-native, fits toolchain, plain
  markdown).
- **Where does `book.toml` live?** → `docs/book.toml`, with
  `src = "."` so existing paths stay valid.
- **Does the existing docs tree get restructured?** → **No.** mdbook
  wraps; nothing moves; every existing relative link continues to
  work.
- **CLI reference: hand-curated vs generated?** → Generated, with
  CI drift-check.
- **Does the milestone publish anywhere?** → Local-only in v1;
  deployment hosting is a separate (future) concern.

## Downstream doc updates

- [`README.md`](README.md) milestone index — add the M21 row.
- [`../README.md`](../README.md) (top of the repo) — once the book
  exists, the repo README can shrink to "see the docs site
  (`task docs:serve`) or browse `docs/`."
- [`14-M13-events-plugin.friction.md`](14-M13-events-plugin.friction.md)
  — the rows about authoring-guide content gaps (rows tagged
  `wish`/`papercut` that asked for documentation) close as "fixed
  in M21 — see `concepts/` and `troubleshooting/common-errors.md`."
- [`../design/14-decision-log.md`](../design/14-decision-log.md) —
  M21 entry: mdbook + the layout decision + auto-CLI ref.
- The `task ci` shape: M12 had 4 jobs; M17 makes it 5; M21 makes
  it 6.

## Verification

```bash
# Tooling install (one-time; future runs come from nix develop).
cargo install mdbook mdbook-mermaid mdbook-linkcheck mdbook-toc
# (Pinned versions live in rust-toolchain.toml / a tool-versions file.)

# Stage 1 — scaffold.
task docs:serve         # opens http://localhost:3000
#  - Browse SUMMARY.md → every section present (Getting started, Concepts,
#    Reference, Troubleshooting, Ops, Contributing, Design, Implementation,
#    Changelog).
#  - Open any design doc (e.g. /design/10-infrastructure-and-data) — renders
#    intact; every relative link in the page still works.
#  - Search "user_can_access" — finds at least 5 hits across M07/M08/M13/M18.

# Stage 2 — getting-started + glossary + CLI ref.
task docs:cli-reference
git diff docs/reference/cli/   # initial generation; commit.
task docs:serve
#  - /getting-started/quickstart — walks clone → docker → task dev → login.
#  - /reference/cli/junius-sync — generated content matches `junius sync --help`.
#  - /reference/glossary — every header term used elsewhere defined.

# Stage 3 — concepts.
task docs:serve
#  - /concepts/architecture-map — Mermaid diagram renders.
#  - /concepts/permissions — single page covers manifest→proto→SDK→SQL ACL.
#  - /reference/plugin-toml — every key from `crates/junius-manifest` present.

# Stage 4 — troubleshooting + ops + contributing + changelog.
task docs:serve
#  - /troubleshooting/common-errors — lists the M13 friction-log gotchas.
#  - /changelog — one section per M00–M21, each linking the impl doc.

# Stage 5 — CI integration.
# Add a deliberate broken link in docs/concepts/permissions.md ([x](missing.md)).
task docs:check                  # fails with linkcheck error citing the file.
# Bump a junius flag's help text without regenerating the CLI ref.
task docs:check                  # fails with "CLI ref drift; commit the regenerated files".
# Revert both → task docs:check passes.

# Full gate.
task ci                          # green; the new docs:check job included.
```

End of M21. The project's documentation is no longer a folder
of `.md` files that you `grep`; it is a searchable, navigable
book that a new contributor can read end-to-end, with the
reference + troubleshooting + ops surfaces the build kept
working around the absence of. Future milestones land their
docs into this structure without negotiating where things go.
