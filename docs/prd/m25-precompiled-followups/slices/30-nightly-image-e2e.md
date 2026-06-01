---
kind: capability
title: "Nightly precompiled-image E2E run"
slug: nightly-image-e2e
issue: 30
prd: ../prd.md
mode: afk
---

# Slice #30 — Nightly precompiled-image E2E run

Full design: [`27-M25-precompiled-followups.md` §4](../../../impl/27-M25-precompiled-followups.md#4-nightly-precompiled-image-e2e).

## What to build

Run the existing E2E specs against the **published precompiled images** nightly, to catch
drift between source-built and image-built behaviour (the only intentional difference is
bundle-all vs the deployment's `enabled` list — anything else is a bug).

- `.github/workflows/images-nightly.yml` with `schedule: '0 3 * * *'`, pulling
  `<variant>:latest` and running `task ci:e2e` (+ `ci:e2e:split`) with
  `JUNIUS_E2E_USE_IMAGE=1`.
- A small change to the E2E orchestrators to skip the `cargo build` / `pnpm build` steps when
  an image is supplied.

**First consumer:** the existing `e2e/` and `e2e/split/` specs, run against the published
images.

## Acceptance criteria

- [ ] The nightly workflow pulls `<variant>:latest` and runs both E2E suites against them.
- [ ] The orchestrators skip the source build when `JUNIUS_E2E_USE_IMAGE=1`.
- [ ] A green nightly run exercises the same specs as the source-built CI E2E.

## Blocked by

- None — independent of the other M25 items.
