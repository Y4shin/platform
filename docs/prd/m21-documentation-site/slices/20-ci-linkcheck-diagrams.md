---
kind: feature
title: "CI integration + link-check + diagrams"
slug: ci-linkcheck-diagrams
issue: 20
prd: ../prd.md
mode: afk
---

# Slice #20 — CI integration + link-check + diagrams

Full design: [`23-M21-documentation.md` §D + Stage 5](../../../impl/23-M21-documentation.md#d--task-targets--ci).

## What to build

Make the book self-defending: broken links and CLI-ref drift fail CI, diagrams render, and
the toolchain is reproducible.

- Add `mdbook-linkcheck` as a CI preprocessor (`follow-web-links = false`,
  `warning-policy = error`).
- Add `task docs:check` (regenerate CLI ref → `git diff --quiet` drift gate → `mdbook build`)
  as the **sixth** `task ci` job.
- Add `mdbook-mermaid` and convert the architecture map to a real diagram.
- Pin the mdbook + preprocessor versions in the toolchain manifest so CI and local match.

## Acceptance criteria

- [ ] A deliberately broken `[link](missing.md)` fails `task docs:check` citing the file.
- [ ] An un-regenerated `junius` flag change fails `task docs:check` ("CLI ref drift").
- [ ] CI installs mdbook reproducibly across runs.
- [ ] The architecture diagram renders identically in the served and built outputs.

## Blocked by

- #16 — needs the book to exist; link-check + CLI-drift gates run against the built book.
