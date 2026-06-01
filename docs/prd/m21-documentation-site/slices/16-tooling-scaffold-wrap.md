---
kind: feature
title: "Tooling + scaffold + existing-content wrap"
slug: tooling-scaffold-wrap
issue: 16
prd: ../prd.md
mode: hitl
---

# Slice #16 — Tooling + scaffold + existing-content wrap

Full design: [`23-M21-documentation.md` §A–B + Stage 1](../../../impl/23-M21-documentation.md#a--tooling).

## What to build

Stand up the book so `task docs:serve` opens a complete, navigable shell over the existing
tree — content stubs in place so the nav works from day one.

- `docs/book.toml` (mdbook config, `src = "."`, navy theme, search on), `docs/SUMMARY.md`
  (the book TOC), and a new `docs/README.md` intro page that also reads acceptably on GitHub.
- Every existing file in `docs/design/`, `docs/impl/`, and `plugin-authoring-guide.md` linked
  in the nav — **unmoved, unrewritten**; every existing relative link still resolves.
- New section dirs (`getting-started/`, `concepts/`, `reference/`, `troubleshooting/`,
  `ops/`, `contributing/`, `changelog.md`) created with **placeholder stubs**.
- `task docs:serve` (live reload) and `task docs:build` (static `docs/book/`, gitignored).

## Acceptance criteria

- [ ] `task docs:serve` opens the book; every nav entry resolves.
- [ ] Search finds an arbitrary M13 phrase.
- [ ] The design and impl index pages render with their existing tables intact.
- [ ] No existing file moved; existing relative links still work.

## Blocked by

- None — can start immediately.
