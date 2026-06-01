---
kind: feature
title: "Troubleshooting + ops + contributing + changelog"
slug: troubleshooting-ops-contributing-changelog
issue: 19
prd: ../prd.md
mode: afk
---

# Slice #19 — Troubleshooting + ops + contributing + changelog

Full design: [`23-M21-documentation.md` §C/Stage 4](../../../impl/23-M21-documentation.md#troubleshooting).

## What to build

The remaining net-new prose surfaces.

- `troubleshooting/` (3): `common-errors.md` (the M13 friction-log "common gotcha" rows that
  aren't fixed by code), `dev-setup.md` (Authentik first-boot, WSL↔Docker networking, OIDC
  group scope, dev-seed→provisioning transition), `ci.md` (the six `task ci` jobs, how to
  reproduce failures locally).
- `ops/` (3): `deployment.md`, `database.md`, `observability.md` — documenting what already
  exists for the deployer (no new ops infra introduced).
- `contributing/` (3): `overview.md`, `milestone-plan.md`, `decision-log-workflow.md` —
  capturing the repo's currently-implicit norms.
- `changelog.md` — one section per shipped milestone (M00–M21), each a 3–5-bullet summary
  cross-linking the relevant impl doc.

## Acceptance criteria

- [ ] Every M13 friction-log row tagged "common gotcha" appears in `common-errors.md`.
- [ ] The changelog has one section per M00–M21, each linking the impl doc.
- [ ] The contributing pages match the actual repo norms (one unsigned commit per stage, etc.).

## Blocked by

- #16 — needs the book scaffold + section stubs.
