# Slice #24 — Swap E2E orchestrator to RustFS

**PRD:** ../prd.md · **kind:** capability · **mode:** afk

Full design: [`24-M22-rustfs-storage.md` §Stage 2b](../../../impl/24-M22-rustfs-storage.md#stage-2-only-if-stage-1-says-adopt--swap-dev--e2e--example).

> **Adopt-only.** Proceed only if Slice #22 concludes "adopt"; otherwise this slice closes
> without work.

## What to build

Update `packages/e2e/src/env.ts`'s `startStack()` to spin a RustFS container instead of MinIO
(same testcontainers shape: random-port mapping, label-tagged for the M17 sweep). Adjust the
rendered `e2e/host-config.toml.tmpl` `[config.storage.buckets.main]` endpoint shape to
whatever RustFS requires (likely identical — confirm against the spike).

**First consumer:** the M17 E2E suite — `task test:e2e` cold-starting green against RustFS.

## Acceptance criteria

- [ ] `task test:e2e` cold-starts green with RustFS instead of MinIO.
- [ ] Every M10 `ObjectStore` integration test passes against the E2E RustFS container.

## Blocked by

- #22 — needs the "adopt" verdict + the confirmed endpoint/config shape from the probe.
