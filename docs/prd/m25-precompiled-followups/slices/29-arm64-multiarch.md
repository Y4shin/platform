---
kind: capability
title: "linux/arm64 multi-arch images"
slug: arm64-multiarch
issue: 29
prd: ../prd.md
mode: afk
---

# Slice #29 — linux/arm64 multi-arch images

Full design: [`27-M25-precompiled-followups.md` §3](../../../impl/27-M25-precompiled-followups.md#3-linuxarm64-multi-arch).

> Land **only after** amd64 is proven stable — this doubles CI minutes per push.

## What to build

Add `linux/arm64` alongside the existing `linux/amd64` image builds.

- `platforms: linux/amd64,linux/arm64` on the `docker/build-push-action` step.
- Confirm the rust-builder stage cross-compiles cleanly (it should — no native deps in the
  workspace today).

**First consumer:** an arm64 image pull + boot (Apple Silicon / cloud arm64 hosts).

## Acceptance criteria

- [ ] The workflow publishes both `linux/amd64` and `linux/arm64` for each variant.
- [ ] An arm64 image pulls and boots green.

## Blocked by

- None — independent of the other M25 items (but sequence after amd64 is proven stable).
