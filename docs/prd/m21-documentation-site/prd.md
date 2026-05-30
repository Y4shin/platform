---
kind: feature
title: Documentation site (mdbook)
slug: m21-documentation-site
epic: developer-experience
milestone: M21
prd_issue: 15
slices: [16, 17, 18, 19, 20]
status: issues-created
---

# Documentation site (mdbook)

> Migrated from the M21 milestone doc
> [`docs/impl/23-M21-documentation.md`](../../impl/23-M21-documentation.md), which holds the
> **full design** (the `book.toml` + `SUMMARY.md` layout, the net-new page inventory, the
> `docs:*` task targets). This PRD is the planning surface; it links back rather than
> duplicating. Independent of M14–M20 — can land any time after M13. **Low-risk** (no code
> paths change) and **high-leverage** (every later milestone gets a place to land docs).

## Problem / why

The `docs/` tree has grown to ~40 files / ~9.6k lines (14 design docs, 23 milestone docs, a
567-line authoring guide, two index READMEs). It's well-organised as a folder and unreadable
as a body of work:

- **No global landing page** — a new reader must know to open `docs/design/README.md` first.
- **No search** — finding "how does `has_permission_in_group` work" means `grep -r`.
- **No nav between sections** — design ↔ impl ↔ guide are linked ad-hoc; no next/previous,
  breadcrumbs, or in-page section index.
- **No tested cross-references** — broken `[label](path)` links sit unnoticed.
- **No reader-friendly entry surfaces** — quickstart, prerequisites, CLI reference, glossary,
  troubleshooting, deployment, contributing, changelog — every one would have helped the
  build and none exist.

M21 wraps the existing tree in an mdbook (rendered, searchable, navigable) **without moving or
rewriting any existing file**, and fills in the conspicuously-missing surfaces.

## User stories

- As a contributor I run `task docs:serve` and get a navigable, searchable book at
  `localhost:3000` with live reload; `task docs:build` produces a static `docs/book/`.
- As a reader I find every existing `docs/design/`, `docs/impl/`, and
  `plugin-authoring-guide.md` file in one book — unmoved, with working relative links — plus
  new getting-started / concepts / reference / troubleshooting / ops / contributing /
  changelog pages.
- As a reader I search the book (elasticlunr) and see diagrams render (Mermaid).
- As a maintainer broken cross-doc links **fail CI** (`mdbook-linkcheck`, a sixth `task ci`
  step), and the `junius` CLI reference is **auto-generated** from `clap` help (drift-checked
  in CI like `.sqlx/`), so docs can't silently rot.
- As a reader the new `docs/README.md` is the book intro and also reads acceptably straight on
  GitHub (no mdbook-only syntax in the entry point).

## End-to-end behaviour

`book.toml` + `SUMMARY.md` + a new `docs/README.md` wrap the existing tree (with `src = "."`
so every existing path stays valid); new section dirs are scaffolded with placeholder stubs so
the nav is complete from day one. The prose fills in across stages: getting-started + glossary
+ the auto-generated CLI reference (via a new `junius docs cli` subcommand); then the concepts
pages (architecture map as a Mermaid diagram, permissions, groups & roles, public surfaces,
jobs, i18n, apps & nav) + the `plugin.toml` and HTTP-API references; then troubleshooting + ops
+ contributing + changelog. Finally CI integration: `mdbook-linkcheck` as a preprocessor, a
`task docs:check` job added to `task ci`, `mdbook-mermaid` wired, and the toolchain pinned. A
deliberately broken link or an un-regenerated CLI flag fails `task docs:check`.

## Layers touched

Full design per section in
[`23-M21-documentation.md` §Design](../../impl/23-M21-documentation.md#design).

- **Tooling** (`docs/book.toml`, toolchain manifest): mdbook + `mdbook-mermaid` /
  `-linkcheck` / `-toc` (+ optional `-admonish`), pinned for CI/local parity.
- **Book scaffold** (`docs/SUMMARY.md`, `docs/README.md`, new section dirs): wraps the
  existing tree; no existing file moves.
- **Net-new content**: getting-started (3), concepts (7), reference (CLI + `plugin.toml` +
  HTTP API + glossary), troubleshooting (3), ops (3), contributing (3), changelog.
- **CLI codegen** (`tools/junius`): a new `junius docs cli --out <dir>` subcommand
  (`clap-markdown` / `clap_mangen`) writing one fmt-clean markdown file per command.
- **Task + CI** (`Taskfile.yml`, CI): `docs:serve` / `docs:build` / `docs:cli-reference` /
  `docs:check`; `docs:check` becomes the sixth `task ci` job.

## Out of scope

- **Publishing to a public site** (GitHub Pages / Read the Docs) — local-only in v1; the
  static `docs/book/` artifact is left for a future ops milestone.
- **Auto-generating Rust API docs** (`cargo doc`) — different audience/tool.
- **Proto/gRPC reference site** — `buf` renders that separately.
- **Localising the book** — English-only; M14's i18n was for the app, not the docs.
- **Interactive examples / runnable sandboxes** — pure-static docs only.
- **Versioned docs** (per-release branches) — the book reflects `main`.
- **Migrating from any existing tool** — the tree is already plain markdown.

## Decisions

Carried from the milestone's
[*Library choices*](../../impl/23-M21-documentation.md#library-choices--confirm-with-user-before-starting)
table — **confirm before starting**:

- **Renderer:** `mdbook` (Rust-native, single binary, plain-markdown, ships search; fits the
  `nix develop` toolchain). Reject docusaurus/mkdocs/vitepress (ecosystem mismatch).
- **Diagrams:** `mdbook-mermaid`. **Link check:** `mdbook-linkcheck` (CI preprocessor).
  **TOC:** `mdbook-toc`. **Callouts:** `mdbook-admonish` opt-in only if content needs it.
- **CLI reference:** a new `junius docs cli` subcommand using `clap-markdown` (or
  `clap_mangen` + adapter); drift-checked in CI like `.sqlx/`. Reject hand-curated CLI docs.
- **`book.toml` `src`:** `"."` (book root = `docs/`) so every existing path stays valid —
  **do not** move files into `docs/src/`.
- **Theme:** mdbook default `navy`. **Search:** mdbook default (elasticlunr).
- **CI link policy:** offline only (`follow-web-links = false`) to avoid flakes.
- **CLI-ref drift:** `task ci` fails if regenerated files differ from committed.

## Open questions

Resolved at the design level (see
[§Open questions resolved](../../impl/23-M21-documentation.md#open-questions-resolved)):
mdbook is the tool; `book.toml` lives at `docs/book.toml` with `src = "."`; the existing tree
is **not** restructured (the book wraps); the CLI reference is generated with a CI
drift-check; the milestone publishes nowhere in v1. No open blockers for slicing.

## Implementation notes
<!-- appended by implement-issue as slices land; empty for now -->
