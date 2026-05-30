---
kind: epic
title: Developer experience
slug: developer-experience
epic_issue: 32
prds:
  - slug: m21-documentation-site
    kind: feature
    issue: 15
    blocked_by: []
status: in-progress
---

# Developer experience

> Retrofitted epic grouping milestone **M21**. The child PRD links to
> [`docs/impl/23-M21-documentation.md`](../../../impl/23-M21-documentation.md) for the full
> design. A single-PRD epic today; it's the home for future DX work (CLI ergonomics, authoring
> guides, examples) so that strand has somewhere to accumulate.

## Problem / outcome

The `docs/` tree is well-organised as a folder and unreadable as a body of work: ~40 files /
~9.6k lines with no global landing page, no search, no inter-section navigation, no tested
cross-references, and none of the reader-friendly entry surfaces (quickstart, glossary, CLI
reference, troubleshooting, deployment, contributing, changelog).

**Outcome:** the documentation becomes a navigable, searchable, link-checked site that a
newcomer can actually onboard from — without moving or rewriting any existing file.

## Constituent plugins & surfaces

- **`docs/` tree** wrapped in an mdbook (`book.toml` + `SUMMARY.md`), rendered and searchable.
- **`docs:*` task targets** for build/serve/link-check.
- **CI** — a link-check + diagram-render gate.
- No plugin or runtime code paths change (low-risk, high-leverage).

## Shared / foundational work

None cross-cutting — this is a single self-contained PRD. The mdbook scaffold it introduces is
itself the foundation later DX work renders into.

## Per-plugin features

- **M21 — Documentation site** (`m21-documentation-site`): the mdbook scaffold wrapping the
  existing tree, the net-new reader surfaces (getting-started, glossary, CLI reference,
  concepts/references, troubleshooting/ops/contributing/changelog), and the CI link-check +
  diagram render.

## Dependency ordering

Single PRD — no internal ordering. Independent of M14–M20; can land any time after M13.

## Out of scope

Rewriting or relocating existing docs; API-doc generation; a hosted/published site deployment —
the milestone wraps what exists and fills the gaps, nothing more.

## Open questions

None outstanding — resolved per-slice in the M21 milestone doc.

## Decomposition

1. **`m21-documentation-site`** (feature, #15) — wrap the tree in mdbook, add the missing
   reader surfaces, and gate cross-references in CI.
